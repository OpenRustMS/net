use std::{io, net::SocketAddr};

use bytes::BytesMut;
use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, ToSocketAddrs},
};
use tokio_util::codec::Framed;

use crate::{
    crypto::SharedCryptoContext, EncodePacket, HasOpcode, NetError, NetOpcode, NetResult,
    PacketBuffer, PacketWriter, ShroomPacket,
};

use super::codec::{handshake::Handshake, packet_codec::PacketCodec};

/// Marker for traits which implement `AsyncWrite`, `AsyncRead` and `Unpin`
pub trait SessionTransport: AsyncWrite + AsyncRead + Unpin {}
impl<T> SessionTransport for T where T: AsyncWrite + AsyncRead + Unpin {}

pub struct ShroomSession<T> {
    codec: Framed<T, PacketCodec>,
    encode_buffer: BytesMut,
}

impl<T> ShroomSession<T>
where
    T: SessionTransport + Unpin,
{
    /// Create a new session from the `io` and
    pub fn new(io: T, codec: PacketCodec) -> Self {
        Self {
            codec: Framed::new(io, codec),
            encode_buffer: BytesMut::new(),
        }
    }

    /// Initialize a server session, by sending out the given handshake
    pub async fn initialize_server_session(
        mut io: T,
        ctx: SharedCryptoContext,
        handshake: Handshake,
    ) -> NetResult<Self> {
        handshake.write_handshake_async(&mut io).await?;
        Ok(Self::from_server_handshake(io, ctx, handshake))
    }

    /// Initialize a client session, by reading a handshake first
    pub async fn initialize_client_session(
        mut io: T,
        ctx: SharedCryptoContext,
    ) -> NetResult<(Self, Handshake)> {
        let handshake = Handshake::read_handshake_async(&mut io).await?;
        let sess = Self::from_client_handshake(io, ctx, handshake.clone());

        Ok((sess, handshake))
    }

    /// Create a server session from a handshake
    pub fn from_server_handshake(io: T, ctx: SharedCryptoContext, handshake: Handshake) -> Self {
        let codec = PacketCodec::from_server_handshake(ctx, handshake);
        Self::new(io, codec)
    }

    /// Create a client session from a handshake
    pub fn from_client_handshake(io: T, ctx: SharedCryptoContext, handshake: Handshake) -> Self {
        let codec = PacketCodec::from_client_handshake(ctx, handshake);
        Self::new(io, codec)
    }

    pub async fn read_packet(&mut self) -> NetResult<ShroomPacket> {
        match self.codec.next().await {
            Some(p) => Ok(p?),
            None => Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
        }
    }

    pub async fn send_packet_buffer(&mut self, buf: &PacketBuffer) -> NetResult<()> {
        // It is required to send the packets one-by-one, because the client doesn't support
        // other ways
        for pkt in buf.packets() {
            self.send_packet(pkt).await?;
        }

        Ok(())
    }

    pub async fn send_packet(&mut self, data: &[u8]) -> NetResult<()> {
        self.codec.send(data).await?;
        Ok(())
    }

    pub async fn send_encode_packet<P: EncodePacket + HasOpcode>(
        &mut self,
        data: P,
    ) -> NetResult<()> {
        self.send_encode_packet_with_opcode(P::OPCODE, data).await
    }

    pub async fn send_encode_packet_with_opcode(
        &mut self,
        op: impl NetOpcode,
        data: impl EncodePacket,
    ) -> NetResult<()> {
        self.encode_buffer.clear();
        self.encode_buffer.reserve(4096);

        // Encode the packet onto the buffer
        let mut pw = PacketWriter::new(&mut self.encode_buffer);
        pw.write_opcode(op)?;
        data.encode_packet(&mut pw)?;

        self.codec.send(&self.encode_buffer).await?;
        Ok(())
    }

    pub async fn close(mut self) -> NetResult<()> {
        self.codec.close().await?;
        Ok(())
    }
}

impl<T: SessionTransport> Stream for ShroomSession<T> {
    type Item = NetResult<ShroomPacket>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.get_mut().codec.poll_next_unpin(cx)
    }
}

impl<B: AsRef<[u8]>, T: SessionTransport> Sink<B> for ShroomSession<T> {
    type Error = NetError;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.get_mut().codec.poll_close_unpin(cx)
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: B) -> Result<(), Self::Error> {
        self.get_mut().codec.start_send_unpin(item.as_ref())
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.get_mut().codec.poll_flush_unpin(cx)
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.get_mut().codec.poll_close_unpin(cx)
    }
}

impl ShroomSession<TcpStream> {
    /// Get the peer address of the socket
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.codec.get_ref().peer_addr()
    }

    /// Get the local address of the socket
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.codec.get_ref().local_addr()
    }

    /// Connect to the given addr with the crypto context
    pub async fn connect(
        addr: impl ToSocketAddrs,
        ctx: SharedCryptoContext,
    ) -> NetResult<(Self, Handshake)> {
        let socket = TcpStream::connect(addr).await?;
        Self::initialize_client_session(socket, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use turmoil::net::{TcpListener, TcpStream};

    use crate::{
        crypto::SharedCryptoContext,
        net::{
            codec::handshake::LocaleCode,
            service::{BasicHandshakeGenerator, HandshakeGenerator},
            ShroomSession,
        },
    };

    const PORT: u16 = 1738;

    async fn bind() -> std::result::Result<TcpListener, std::io::Error> {
        TcpListener::bind((IpAddr::from(Ipv4Addr::UNSPECIFIED), PORT)).await
    }

    #[test]
    fn echo() -> anyhow::Result<()> {
        const ECHO_DATA: [&'static [u8]; 4] = [&[0xFF; 4096], &[1, 2], &[], &[0x0; 1024]];
        const V: u16 = 83;
        const LOCALE: LocaleCode = LocaleCode::Global;

        let mut sim = turmoil::Builder::new().build();

        sim.host("server", || async move {
            let crypto_ctx = SharedCryptoContext::default();
            let hshake_gen = BasicHandshakeGenerator::new(V, "1", LOCALE);
            let handshake = hshake_gen.generate_handshake();
            let listener = bind().await?;

            loop {
                let socket = listener.accept().await?.0;
                let mut sess = ShroomSession::initialize_server_session(
                    socket,
                    crypto_ctx.clone(),
                    handshake.clone(),
                )
                .await?;

                // Echo
                while let Ok(pkt) = sess.read_packet().await {
                    sess.send_packet(pkt.as_ref()).await?;
                }
            }
        });

        sim.client("client", async move {
            let socket = TcpStream::connect(("server", PORT)).await?;
            let (mut sess, handshake) =
                ShroomSession::initialize_client_session(socket, SharedCryptoContext::default())
                    .await?;
            assert_eq!(handshake.version, V);

            for data in ECHO_DATA.iter() {
                sess.send_packet(data).await?;
                let pkt = sess.read_packet().await?;
                assert_eq!(pkt.as_ref(), *data);
            }

            Ok(())
        });

        sim.run().unwrap();

        Ok(())
    }
}
