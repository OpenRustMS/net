#![allow(non_upper_case_globals)]

pub mod conn;
pub mod legacy;

use std::pin::Pin;

use shroom_pkt::ShroomPacketData;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{NetError, NetResult};

use tokio_util::codec::{Decoder, Encoder};

use self::conn::ShroomConn;

pub trait ShroomTransport: AsyncWrite + AsyncRead + Unpin + Send + 'static {
    fn peer_addr(&self) -> NetResult<std::net::SocketAddr>;
    fn local_addr(&self) -> NetResult<std::net::SocketAddr>;
}

pub struct LocalShroomTransport<T>(pub T);

impl<T> ShroomTransport for LocalShroomTransport<T>
where
    T: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    fn peer_addr(&self) -> NetResult<std::net::SocketAddr> {
        Ok(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            0,
        ))
    }

    fn local_addr(&self) -> NetResult<std::net::SocketAddr> {
        Ok(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            0,
        ))
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for LocalShroomTransport<T> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();
        Pin::new(&mut this.0).poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.get_mut();
        Pin::new(&mut this.0).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.get_mut();
        Pin::new(&mut this.0).poll_shutdown(cx)
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for LocalShroomTransport<T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.0).poll_read(cx, buf)
    }
}

impl ShroomTransport for tokio::net::TcpStream {
    fn peer_addr(&self) -> NetResult<std::net::SocketAddr> {
        self.peer_addr().map_err(|e| e.into())
    }

    fn local_addr(&self) -> NetResult<std::net::SocketAddr> {
        self.local_addr().map_err(|e| e.into())
    }
}

#[cfg(test)]
impl ShroomTransport for turmoil::net::TcpStream {
    fn peer_addr(&self) -> NetResult<std::net::SocketAddr> {
        self.peer_addr().map_err(|e| e.into())
    }

    fn local_addr(&self) -> NetResult<std::net::SocketAddr> {
        self.local_addr().map_err(|e| e.into())
    }
}

/// Codec trait
#[async_trait::async_trait]
pub trait ShroomCodec: Sized + Unpin {
    type Encoder: for<'a> Encoder<&'a [u8], Error = NetError> + Send + 'static;
    type Decoder: Decoder<Item = ShroomPacketData, Error = NetError> + Send + 'static;
    type Transport: ShroomTransport;

    async fn create_client_session(&self, tran: Self::Transport) -> NetResult<ShroomConn<Self>>;
    async fn create_server_session(&self, trans: Self::Transport) -> NetResult<ShroomConn<Self>>;
}
