use std::{io, net::SocketAddr};

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_util::codec::Framed;

use crate::{
    opcode::{HasOpcode, NetOpcode},
    EncodePacket, NetResult, PacketWriter, ShroomPacket,
};

use super::{
    codec::{handshake::Handshake, packet_codec::PacketCodec},
    crypto::ShroomCryptoKeys,
    service::packet_buffer::PacketBuffer,
};

pub trait SessionTransport: AsyncWrite + AsyncRead {}
impl<T> SessionTransport for T where T: AsyncWrite + AsyncRead {}

pub struct ShroomSession<T> {
    codec: Framed<T, PacketCodec>,
    encode_buffer: BytesMut,
}

impl<T> ShroomSession<T>
where
    T: SessionTransport + Unpin,
{
    pub fn new(io: T, codec: PacketCodec) -> Self {
        Self {
            codec: Framed::new(io, codec),
            encode_buffer: BytesMut::new(),
        }
    }

    /// Initialize a server session, by sending out the given handshake
    pub async fn initialize_server_session(
        mut io: T,
        keys: &ShroomCryptoKeys,
        handshake: Handshake,
    ) -> NetResult<Self> {
        handshake.write_handshake_async(&mut io).await?;
        Ok(Self::from_server_handshake(io, keys, handshake))
    }

    pub async fn initialize_client_session(
        mut io: T,
        keys: &ShroomCryptoKeys,
    ) -> NetResult<(Self, Handshake)> {
        let handshake = Handshake::read_handshake_async(&mut io).await?;
        let sess = Self::from_client_handshake(io, keys, handshake.clone());

        Ok((sess, handshake))
    }

    pub fn from_server_handshake(io: T, keys: &ShroomCryptoKeys, handshake: Handshake) -> Self {
        let codec = PacketCodec::from_server_handshake(keys, handshake);
        Self::new(io, codec)
    }

    pub fn from_client_handshake(io: T, keys: &ShroomCryptoKeys, handshake: Handshake) -> Self {
        let codec = PacketCodec::from_client_handshake(keys, handshake);
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
            self.send_raw_packet(pkt).await?;
        }

        Ok(())
    }

    pub async fn send_raw_packet(&mut self, data: &[u8]) -> NetResult<()> {
        self.codec.send(data).await?;
        Ok(())
    }

    pub async fn send_packet_with_opcode<P: EncodePacket>(
        &mut self,
        opcode: impl NetOpcode,
        data: P,
    ) -> NetResult<()> {
        self.encode_buffer.clear();
        let mut pw = PacketWriter::new(&mut self.encode_buffer);
        pw.write_opcode(opcode)?;
        data.encode_packet(&mut pw)?;

        self.codec.send(&self.encode_buffer).await?;
        Ok(())
    }

    pub async fn send_packet<P: EncodePacket + HasOpcode>(&mut self, data: P) -> NetResult<()> {
        self.send_packet_with_opcode(P::OPCODE, data).await
    }

    pub async fn close(&mut self) -> NetResult<()> {
        self.codec.close().await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> NetResult<()> {
        self.codec.flush().await?;
        Ok(())
    }
}

impl ShroomSession<TcpStream> {
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.codec.get_ref().peer_addr()
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.codec.get_ref().local_addr()
    }

    pub async fn connect(
        addr: SocketAddr,
        keys: &ShroomCryptoKeys,
    ) -> NetResult<(Self, Handshake)> {
        let socket = TcpStream::connect(addr).await?;
        Self::initialize_client_session(socket, keys).await
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use arrayvec::ArrayString;
    use turmoil::net::{TcpListener, TcpStream};

    use crate::net::{
        codec::handshake::Handshake,
        crypto::{RoundKey, ShroomCryptoKeys},
        ShroomSession,
    };

    const PORT: u16 = 1738;

    async fn bind() -> std::result::Result<TcpListener, std::io::Error> {
        TcpListener::bind((IpAddr::from(Ipv4Addr::UNSPECIFIED), PORT)).await
    }

    #[test]
    fn echo() -> anyhow::Result<()> {
        let mut sim = turmoil::Builder::new().build();
        const ECHO_DATA: [&'static [u8]; 4] = [&[0xFF; 4096], &[1, 2], &[], &[0x0; 1024]];
        const V: u16 = 83;

        const DEFAULT_KEYS: ShroomCryptoKeys = ShroomCryptoKeys::with_default_keys();

        sim.host("server", || async move {
            let handshake = Handshake {
                version: V,
                subversion: ArrayString::try_from("1").unwrap(),
                iv_enc: RoundKey::zero(),
                iv_dec: RoundKey::zero(),
                locale: 1,
            };

            let listener = bind().await?;

            loop {
                let socket = listener.accept().await?.0;
                let mut sess = ShroomSession::initialize_server_session(
                    socket,
                    &DEFAULT_KEYS,
                    handshake.clone(),
                )
                .await?;

                // Echo
                loop {
                    match sess.read_packet().await {
                        Ok(pkt) => {
                            sess.send_raw_packet(&pkt.data).await?;
                        }
                        _ => {
                            break;
                        }
                    }
                }
            }
        });

        sim.client("client", async move {
            let socket = TcpStream::connect(("server", PORT)).await?;
            let (mut sess, handshake) =
                ShroomSession::initialize_client_session(socket, &DEFAULT_KEYS).await?;
            assert_eq!(handshake.version, V);

            for data in ECHO_DATA.iter() {
                sess.send_raw_packet(data).await?;
                let pkt = sess.read_packet().await?;
                assert_eq!(pkt.data.as_ref(), *data);
            }

            Ok(())
        });

        sim.run().unwrap();

        Ok(())
    }
}
