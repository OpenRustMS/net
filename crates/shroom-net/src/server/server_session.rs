use shroom_pkt::{opcode::HasOpcode, EncodePacket, ShroomPacketData};
use std::time::Duration;
use tokio::{sync::mpsc, time::Instant};
use tokio_stream::StreamExt;

use crate::{
    codec::{session::ShroomSession, ShroomCodec},
    NetError,
};

use super::{
    resp::{IntoResponse, Response},
    tick::{Tick, TickUnit},
    ShroomSessionHandle,
};

/// Session handle result
pub enum SessionHandleResult {
    /// Indicates this handler finished succesfully
    Ok,
    /// Indicates the session to start a migration
    Migrate,
    /// Signalling a Pong response was received
    Pong,
}

pub trait IntoHandleResult {
    fn into_handle_result(self) -> SessionHandleResult;
}

impl IntoHandleResult for () {
    fn into_handle_result(self) -> SessionHandleResult {
        SessionHandleResult::Ok
    }
}

impl IntoHandleResult for SessionHandleResult {
    fn into_handle_result(self) -> SessionHandleResult {
        self
    }
}

pub enum ShroomSessionEvent<Msg> {
    IncomingPacket(ShroomPacketData),
    Message(Msg),
    Ping,
    Tick(TickUnit),
}

/// Session handler trait, which used to handle packets and handle messages
#[async_trait::async_trait]
pub trait ShroomSessionHandler: Sized {
    type Codec: ShroomCodec + Send + 'static;
    type Error: From<NetError> + std::fmt::Debug + Send + 'static;
    type Msg: Send + 'static;
    type MakeState: Send + Sync + 'static;

    async fn make_handler(
        make_state: &Self::MakeState,
        sess: &mut ShroomSession<Self::Codec>,
        handle: ShroomSessionHandle<Self::Msg>,
    ) -> Result<Self, Self::Error>;

    /// Handle a new event
    async fn handle_msg(
        &mut self,
        session: &mut ShroomSession<Self::Codec>,
        msg: ShroomSessionEvent<Self::Msg>,
    ) -> Result<SessionHandleResult, Self::Error>;

    async fn finish(self, is_migrating: bool) -> Result<(), Self::Error>;
}

pub struct ShroomSessionCtx<H: ShroomSessionHandler> {
    session: ShroomSession<H::Codec>,
    rx: mpsc::Receiver<H::Msg>,
    tick: Tick,
    pending_ping: bool,
    ping: tokio::time::Interval,
}

impl<H> ShroomSessionCtx<H>
where
    H: ShroomSessionHandler,
{
    pub(crate) fn new(
        session: ShroomSession<H::Codec>,
        rx: mpsc::Receiver<H::Msg>,
        ping_dur: Duration,
        tick: Tick,
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
                    handler.handle_msg(&mut self.session, ShroomSessionEvent::IncomingPacket(packet)).await?
                }
                Some(msg) = self.rx.recv() => {
                    handler.handle_msg(&mut self.session, ShroomSessionEvent::Message(msg)).await?
                }
                tick = self.tick.next() => {
                    handler.handle_msg(&mut self.session, ShroomSessionEvent::Tick(tick)).await?
                }
                _ = self.ping.tick() => {
                    if self.pending_ping {
                        // TODO timeout error
                        return Ok(false)
                    }
                    self.pending_ping = true;
                    handler.handle_msg(&mut self.session, ShroomSessionEvent::Ping).await?
                }

                else => {
                    //TODO eof error
                    return Ok(false)
                },
            };

            match res {
                // Continue to run session
                SessionHandleResult::Ok => (),
                // Migrate quits the session
                SessionHandleResult::Migrate => return Ok(true),
                // Reset the pending ping
                SessionHandleResult::Pong => {
                    self.pending_ping = false;
                }
            }
        }
    }
}
