use std::{io, net::SocketAddr};

use crate::{
    codec::{ShroomCodec, ShroomTransport},
    NetResult,
};

use bytes::BytesMut;
use futures::{SinkExt, Stream, StreamExt};
use shroom_pkt::{
    opcode::{HasOpcode, NetOpcode},
    util::packet_buf::PacketBuf,
    EncodePacket, PacketWriter, ShroomPacketData,
};
use tokio::io::{ReadHalf, WriteHalf};
use tokio_util::codec::{FramedRead, FramedWrite};

pub struct ShroomConn<C: ShroomCodec> {
    r: FramedRead<ReadHalf<C::Transport>, C::Decoder>,
    w: FramedWrite<WriteHalf<C::Transport>, C::Encoder>,
    // TODO remove that buf later
    local_buf: BytesMut,
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
}

impl<C> ShroomConn<C>
where
    C: ShroomCodec + Unpin,
{
    /// Create a new session from the `io` and
    pub fn new(io: C::Transport, (enc, dec): (C::Encoder, C::Decoder)) -> Self {
        let local_addr = io.local_addr().unwrap();
        let peer_addr = io.peer_addr().unwrap();
        let (r, w) = tokio::io::split(io);
        Self {
            r: FramedRead::new(r, dec),
            w: FramedWrite::new(w, enc),
            local_addr,
            peer_addr,
            local_buf: BytesMut::with_capacity(4096),
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn read_packet(&mut self) -> NetResult<ShroomPacketData> {
        match self.r.next().await {
            Some(Ok(pkt)) => Ok(pkt),
            Some(Err(e)) => Err(e),
            None => Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
        }
    }

    pub async fn send_packet_buffer(&mut self, buf: &PacketBuf) -> NetResult<()> {
        // It is required to send the packets one-by-one, because the client doesn't support
        // other ways
        for pkt in buf.packets() {
            self.send_packet(pkt).await?;
        }

        Ok(())
    }

    pub async fn send_packet(&mut self, data: &[u8]) -> NetResult<()> {
        self.w.send(data).await?;
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
        self.local_buf.clear();
        self.local_buf.reserve(4096);

        // Encode the packet onto the buffer
        let mut pw = PacketWriter::new(&mut self.local_buf);
        pw.write_opcode(op)?;
        data.encode_packet(&mut pw)?;

        self.w.send(&self.local_buf).await?;
        Ok(())
    }

    pub async fn close(mut self) -> NetResult<()> {
        self.w.close().await?;
        Ok(())
    }
}

impl<C: ShroomCodec> Stream for ShroomConn<C> {
    type Item = NetResult<ShroomPacketData>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.get_mut().r.poll_next_unpin(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
    };
    use turmoil::net::{TcpListener, TcpStream};

    use crate::{
        codec::{
            legacy::{handshake_gen::BasicHandshakeGenerator, LegacyCodec},
            ShroomCodec,
        },
        crypto::SharedCryptoContext,
    };

    const PORT: u16 = 1738;

    async fn bind() -> std::result::Result<TcpListener, std::io::Error> {
        TcpListener::bind((IpAddr::from(Ipv4Addr::UNSPECIFIED), PORT)).await
    }

    #[test]
    fn echo() -> anyhow::Result<()> {
        const ECHO_DATA: [&'static [u8]; 4] = [&[0xFF; 4096], &[1, 2], &[], &[0x0; 1024]];

        let legacy = Arc::new(LegacyCodec::<turmoil::net::TcpStream>::new(
            SharedCryptoContext::default(),
            BasicHandshakeGenerator::v83(),
        ));

        let mut sim = turmoil::Builder::new().build();

        sim.host("server", || async move {
            let listener = bind().await?;

            let legacy = LegacyCodec::<turmoil::net::TcpStream>::new(
                SharedCryptoContext::default(),
                BasicHandshakeGenerator::v83(),
            );
            loop {
                let socket = listener.accept().await?.0;
                let mut sess = legacy.create_server_session(socket).await?;
                // Echo
                while let Ok(pkt) = sess.read_packet().await {
                    sess.send_packet(pkt.as_ref()).await?;
                }
            }
        });

        sim.client("client", async move {
            let socket = TcpStream::connect(("server", PORT)).await?;
            let mut sess = legacy.create_client_session(socket).await?;
            for (i, data) in ECHO_DATA.iter().enumerate() {
                sess.send_packet(data).await?;
                let pkt = sess.read_packet().await?;
                assert_eq!(pkt.as_ref(), *data, "failed at: {i}");
            }

            Ok(())
        });

        sim.run().unwrap();

        Ok(())
    }
}
