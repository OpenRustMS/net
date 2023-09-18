#![allow(non_upper_case_globals)]

pub mod legacy;
pub mod session;

use shroom_pkt::ShroomPacketData;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{NetError, NetResult};

use tokio_util::codec::{Decoder, Encoder};

use self::session::ShroomSession;

pub trait ShroomTransport: AsyncWrite + AsyncRead + Unpin + Send + 'static {}

impl<T: AsyncWrite + AsyncRead + Unpin + Send + 'static> ShroomTransport for T {}

/// Codec trait
#[async_trait::async_trait]
pub trait ShroomCodec: Sized + Unpin {
    type Encoder: for<'a> Encoder<&'a [u8], Error = NetError> + Send + 'static;
    type Decoder: Decoder<Item = ShroomPacketData, Error = NetError> + Send + 'static;
    type Transport: ShroomTransport;

    async fn create_client_session(&self, tran: Self::Transport) -> NetResult<ShroomSession<Self>>;
    async fn create_server_session(&self, trans: Self::Transport)
        -> NetResult<ShroomSession<Self>>;
}
