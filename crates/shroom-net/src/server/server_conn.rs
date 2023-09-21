use shroom_pkt::{opcode::HasOpcode, EncodePacket, ShroomPacketData};
use std::time::Duration;
use tokio::{sync::mpsc, time::Instant};
use tokio_stream::StreamExt;

use crate::{codec::ShroomCodec, NetError, ShroomConn};

use super::{
    resp::{IntoResponse, Response},
    tick::{Tick, TickUnit},
    SharedConnHandle,
};

/// Conn handle result
pub enum ServerHandleResult {
    /// Indicates this handler finished succesfully
    Ok,
    /// Indicates the session to start a migration
    Migrate,
    /// Signalling a Pong response was received
    Pong,
}

pub trait IntoServerHandleResult {
    fn into_handle_result(self) -> ServerHandleResult;
}

impl IntoServerHandleResult for () {
    fn into_handle_result(self) -> ServerHandleResult {
        ServerHandleResult::Ok
    }
}

impl IntoServerHandleResult for ServerHandleResult {
    fn into_handle_result(self) -> ServerHandleResult {
        self
    }
}

pub enum ShroomConnEvent<Msg> {
    IncomingPacket(ShroomPacketData),
    Message(Msg),
    Ping,
    Tick(TickUnit),
}

/// Conn handler trait, which used to handle packets and handle messages
#[async_trait::async_trait]
pub trait ShroomConnHandler: Sized {
    type Codec: ShroomCodec + Send + Sync + 'static;
    type Error: From<NetError> + std::fmt::Debug + Send + 'static;
    type Msg: Send + 'static;
    type MakeState: Send + Sync + 'static;

    async fn recv_msg(&mut self) -> Option<Self::Msg> {
        futures::future::pending().await
    }

    async fn make_handler(
        make_state: &Self::MakeState,
        ctx: &mut ServerConnCtx<Self>,
        handle: SharedConnHandle<Self::Msg>,
    ) -> Result<Self, Self::Error>;

    /// Handle a new event
    async fn handle_msg(
        &mut self,
        ctx: &mut ServerConnCtx<Self>,
        msg: ShroomConnEvent<Self::Msg>,
    ) -> Result<ServerHandleResult, Self::Error>;

    async fn finish(self, is_migrating: bool) -> Result<(), Self::Error>;
}

pub struct ServerConnCtx<H: ShroomConnHandler> {
    session: ShroomConn<H::Codec>,
    rx: mpsc::Receiver<H::Msg>,
    tick: Tick,
    pending_ping: bool,
    ping: tokio::time::Interval,
}

impl<H> ServerConnCtx<H>
where
    H: ShroomConnHandler + Send,
{
    pub(crate) fn new(
        session: ShroomConn<H::Codec>,
        rx: mpsc::Receiver<H::Msg>,
        tick: Tick,
        ping_dur: Duration,
    ) -> Self {
        let ping = tokio::time::interval_at(Instant::now() + ping_dur, ping_dur);
        Self {
            session,
            rx,
            tick,
            pending_ping: false,
            ping,
        }
    }

    pub async fn close(self) -> Result<(), H::Error> {
        self.session.close().await?;

        Ok(())
    }

    pub fn session(&self) -> &ShroomConn<H::Codec> {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut ShroomConn<H::Codec> {
        &mut self.session
    }

    pub async fn send<P: EncodePacket + HasOpcode>(&mut self, p: P) -> Result<(), H::Error> {
        Ok(self.session.send_encode_packet(p).await?)
    }

    pub async fn reply<R: IntoResponse + Send>(&mut self, resp: R) -> Result<(), H::Error> {
        Ok(resp.into_response().send(&mut self.session).await?)
    }

    pub(crate) async fn exec(&mut self, handler: &mut H) -> Result<bool, H::Error> {
        loop {
            let res = tokio::select! {
                Some(packet) = self.session.next() => {
                    let packet = packet?;
                    handler.handle_msg(self, ShroomConnEvent::IncomingPacket(packet)).await?
                }
                Some(msg) = self.rx.recv() => {
                    handler.handle_msg(self, ShroomConnEvent::Message(msg)).await?
                }
                Some(msg) = handler.recv_msg() => {
                    handler.handle_msg(self, ShroomConnEvent::Message(msg)).await?
                }
                tick = self.tick.next() => {
                    handler.handle_msg(self, ShroomConnEvent::Tick(tick)).await?
                }
                _ = self.ping.tick() => {
                    if self.pending_ping {
                        return Err(NetError::PingTimeout.into());
                    }
                    self.pending_ping = true;
                    handler.handle_msg(self, ShroomConnEvent::Ping).await?
                }

                else => {
                    //TODO eof error
                    return Ok(false)
                },
            };

            match res {
                // Continue to run session
                ServerHandleResult::Ok => (),
                // Migrate quits the session
                ServerHandleResult::Migrate => return Ok(true),
                // Reset the pending ping
                ServerHandleResult::Pong => {
                    self.pending_ping = false;
                }
            }
        }
    }
}
