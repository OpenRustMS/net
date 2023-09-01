pub mod handler;
pub mod handshake_gen;
pub mod resp;
pub mod server_sess;
pub mod session_set;

pub use handshake_gen::*;
use tokio_util::sync::CancellationToken;
use std::{time::Duration, ops::{DerefMut, Deref}};

use crate::{EncodePacket, HasOpcode, util::framed_pipe::{FramedPipeSender, self, FramedPipeReceiver}, PacketBuffer};

use self::{handler::ShroomSessionHandler, resp::{IntoResponse, Response}};

use super::ShroomSession;

pub const DEFAULT_MIGRATE_DELAY: Duration = Duration::from_millis(7500);

/// Session handle result
pub enum SessionHandleResult {
    /// Indicates the session to start a migration
    Migrate,
    /// Indicates this handler finished succesfully
    Ok,
    /// Signalling a Pong response was received
    Pong,
}

#[derive(Debug, Clone)]
pub struct SharedSessionHandle {
    ct: CancellationToken,
    tx: FramedPipeSender,
}

impl SharedSessionHandle {
    /// Attempt to send a packet buffer to the session
    pub fn try_send_pkt_buf(&mut self, pkt_buf: &PacketBuffer) -> anyhow::Result<()> {
        Ok(self.tx.clone().try_send_all(pkt_buf.packets())?)
    }

    /// Attempt to send a single packet to the buffer
    pub fn try_send_pkt(&self, pkt: impl AsRef<[u8]>) -> anyhow::Result<()> {
        Ok(self.tx.clone().try_send(pkt)?)
    }
}

impl SharedSessionHandle {
    pub fn new() -> (Self, FramedPipeReceiver) {
        let (tx, rx) = framed_pipe::framed_pipe(8 * 1024, 128);
        (
            Self {
                ct: CancellationToken::new(),
                tx,
            },
            rx,
        )
    }
}

pub struct ShroomContext<H: ShroomSessionHandler> {
    session: ShroomSession<H::Transport>,
    state: H,
    migrate: bool,
    pub session_handle: SharedSessionHandle,
}

impl<H: ShroomSessionHandler> ShroomContext<H> {
    pub fn new(session: ShroomSession<H::Transport>, state: H, session_handle: SharedSessionHandle) -> Self {
        Self {
            session,
            state,
            migrate: false,
            session_handle
        }
    }
}

impl<H: ShroomSessionHandler + Send> ShroomContext<H> {
    pub async fn send<P: EncodePacket + HasOpcode>(&mut self, p: P) -> Result<(), H::Error> {
        Ok(self.session.send_encode_packet(p).await?)
    }


    pub async fn reply<R: IntoResponse + Send>(&mut self, resp: R) -> Result<(), H::Error> {
        Ok(resp.into_response().send(self).await?)
    }

    pub fn set_migrate(&mut self, migrate: bool) {
        self.migrate = migrate;
    }

    pub fn is_migrating(&self) -> bool {
        self.migrate
    }
}

impl<H: ShroomSessionHandler> Deref for ShroomContext<H> {
    type Target = H;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}


impl<H: ShroomSessionHandler> DerefMut for ShroomContext<H> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}