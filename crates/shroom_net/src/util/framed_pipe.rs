use std::{ops::DerefMut, pin::Pin, sync::Arc, task::Poll};

use bytes::{BufMut, Bytes, BytesMut};
use futures::{channel::mpsc, ready, Sink, Stream};
use thiserror::Error;

// TODO: add signaling mechanismn that pipe was full and reader should quit

#[derive(Debug, Error)]
pub enum FramedPipeError {
    #[error("Send Error: {0}")]
    SendError(#[from] mpsc::SendError),
    #[error("Out of capacity")]
    OutOfCapacity,
    #[error("Capacity limit was reached")]
    CapacityLimitReached,
}

/// A `Pipe` which works on frames
#[derive(Debug, Clone)]
struct FramedPipeBuf {
    buf: BytesMut,
    cap: usize,
    missed: usize,
}

impl FramedPipeBuf {
    /// Create a new buffer with the given capacity
    fn new(cap: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(cap),
            cap,
            missed: 0,
        }
    }

    /// Take a frame from the buffer
    fn take(&mut self, n: usize) -> Bytes {
        self.buf.split_to(n).freeze()
    }

    /// Checks if there's enough space on the buffer
    fn check_capacity(&mut self, frame: &[u8]) -> Result<(), FramedPipeError> {
        // Check if there's enough capacity
        if frame.len() > self.buf.capacity() {
            self.buf.reserve(frame.len());

            // TODO: find a better way to do this
            // this not being 100% correct doesn't matter too much for now, as in a session would simply consume abit more memory
            // but for later this should be correct
            // sadly this will also allocate one time unnecessarely

            // If buffer is reallocated the space is usually doubled
            // so this check should be sufficient for the current use-case
            if self.buf.capacity() + self.buf.len() > self.cap {
                self.missed += 1;
                return Err(FramedPipeError::OutOfCapacity);
            }
        }

        Ok(())
    }

    /// Put the frame onto the buffer
    fn put(&mut self, frame: &[u8]) {
        self.buf.put_slice(frame)
    }
}

/// Shared handle for Sender and Receiver
type SharedFramedPipeBuf = Arc<parking_lot::Mutex<FramedPipeBuf>>;

/// A sender for the `FramedPipe` can be cloned and used a `Sink`
#[derive(Debug, Clone)]
pub struct FramedPipeSender {
    tx: mpsc::Sender<usize>,
    buf: SharedFramedPipeBuf,
}

impl FramedPipeSender {
    /// Use try send to attempt to send a frame
    fn try_push(
        frame: &[u8],
        buf: &mut FramedPipeBuf,
        tx: &mut mpsc::Sender<usize>,
    ) -> Result<(), FramedPipeError> {
        buf.check_capacity(frame.as_ref())?;
        tx.try_send(frame.as_ref().len())
            .map_err(|err| FramedPipeError::SendError(err.into_send_error()))?;
        buf.put(frame);
        Ok(())
    }

    /// Helper method for the Sink impl
    fn push(&mut self, frame: &[u8]) -> Result<(), FramedPipeError> {
        let mut buf = self.buf.lock();
        buf.check_capacity(frame)?;
        self.tx.start_send(frame.len())?;
        buf.put(frame);
        Ok(())
    }

    /// Try to send a frame onto the pipe
    pub fn try_send<B: AsRef<[u8]>>(&mut self, item: B) -> Result<(), FramedPipeError> {
        let mut buf = self.buf.lock();
        Self::try_push(item.as_ref(), buf.deref_mut(), &mut self.tx)
    }

    /// Try to send all frames onto a pipe
    /// May send send some frames and then cancel
    pub fn try_send_all<B: AsRef<[u8]>>(
        &mut self,
        items: impl Iterator<Item = B>,
    ) -> Result<(), FramedPipeError> {
        let mut buf = self.buf.lock();
        for item in items {
            Self::try_push(item.as_ref(), buf.deref_mut(), &mut self.tx)?
        }
        Ok(())
    }
}

impl<B: AsRef<[u8]>> Sink<B> for FramedPipeSender {
    type Error = FramedPipeError;

    fn poll_ready(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.tx.poll_ready(cx).map_err(FramedPipeError::SendError)
    }

    fn start_send(mut self: std::pin::Pin<&mut Self>, item: B) -> Result<(), Self::Error> {
        Pin::new(&mut self).push(item.as_ref())
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.tx)
            .poll_flush(cx)
            .map_err(FramedPipeError::SendError)
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.tx)
            .poll_close(cx)
            .map_err(FramedPipeError::SendError)
    }
}

/// Receiver end for the `FramedPipe`, there's at most one reader
#[derive(Debug)]
pub struct FramedPipeReceiver {
    rx: mpsc::Receiver<usize>,
    buf: SharedFramedPipeBuf,
}

/// Stream impl for the reader, wait on the channel
impl Stream for FramedPipeReceiver {
    type Item = Bytes;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // There's only one reader so we can just wait on the channel
        // and then read the frame of the buffer
        let next_frame = ready!(Pin::new(&mut self.rx).poll_next(cx));
        Poll::Ready(next_frame.map(|frame| self.buf.lock().take(frame)))
    }
}

/// Create a framed pipe
/// `buf_cap` describes the maximum capacity in bytes for the buffer
/// `frame_cap` describes the maximum capacity in terms of frames
pub fn framed_pipe(buf_cap: usize, frame_cap: usize) -> (FramedPipeSender, FramedPipeReceiver) {
    let buf = Arc::new(parking_lot::Mutex::new(FramedPipeBuf::new(buf_cap)));
    let (tx, rx) = mpsc::channel(frame_cap);

    (
        FramedPipeSender {
            buf: buf.clone(),
            tx,
        },
        FramedPipeReceiver { buf, rx },
    )
}

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};

    use super::*;

    // Test with multiple echo data
    #[tokio::test]
    async fn echo_pipe() {
        let (tx, mut rx) = framed_pipe(1024 * 8, 128);

        const ECHO_DATA: [&'static [u8]; 4] = [&[0xFF; 4096], &[1, 2], &[], &[0x0; 1024]];

        for _ in 0..100 {
            for data in ECHO_DATA {
                tx.clone().send(data).await.unwrap();
            }

            for data in ECHO_DATA {
                let rx_data = rx.next().await.unwrap();
                assert_eq!(&rx_data, data);
            }
        }
    }

    // Test to ensure the buffer stays at the 4096 bytes capacity
    #[tokio::test]
    async fn reclaim_echo_pipe() {
        let (tx, mut rx) = framed_pipe(1024 * 4, 128);

        const ECHO_DATA: [&'static [u8]; 4] = [&[0xFF; 4096], &[1, 2], &[], &[0x0; 1024]];

        for _ in 0..100 {
            for data in ECHO_DATA {
                tx.clone().send(data).await.unwrap();
                let rx_data = rx.next().await.unwrap();
                assert_eq!(&rx_data, data);
            }
        }
    }
}
