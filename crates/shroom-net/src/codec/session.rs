use std::io;

use crate::NetResult;

use futures::{SinkExt, Stream, StreamExt};
use shroom_pkt::{
    opcode::{HasOpcode, NetOpcode},
    util::packet_buf::PacketBuf,
    EncodePacket, PacketWriter, ShroomPacketData,
};
use tokio::io::{ReadHalf, WriteHalf};
use tokio_util::codec::{FramedRead, FramedWrite};

use super::{ShroomCodec, ShroomTransport};

pub struct ShroomSession<C: ShroomCodec, T> {
    r: FramedRead<ReadHalf<T>, C::Decoder>,
    w: FramedWrite<WriteHalf<T>, C::Encoder>,
}

impl<C, T> ShroomSession<C, T>
where
    C: ShroomCodec + Unpin,
    T: ShroomTransport,
{
    /// Create a new session from the `io` and
    pub fn new(io: T, (dec, enc): (C::Decoder, C::Encoder)) -> Self {
        let (r, w) = tokio::io::split(io);
        Self {
            r: FramedRead::new(r, dec),
            w: FramedWrite::new(w, enc),
        }
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
        let buf = self.w.write_buffer_mut();
        buf.clear();
        buf.reserve(4096);

        // Encode the packet onto the buffer
        let mut pw = PacketWriter::new(buf);
        pw.write_opcode(op)?;
        data.encode_packet(&mut pw)?;

        self.w.flush().await?;
        Ok(())
    }

    pub async fn close(mut self) -> NetResult<()> {
        self.w.close().await?;
        Ok(())
    }
}

impl<C: ShroomCodec, T: ShroomTransport> Stream for ShroomSession<C, T> {
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
    use std::net::{IpAddr, Ipv4Addr};
    use turmoil::net::{TcpListener, TcpStream};

    use crate::codec::legacy::LocaleCode;

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
