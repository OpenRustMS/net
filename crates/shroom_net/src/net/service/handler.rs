use std::fmt::Debug;

use async_trait::async_trait;
use futures::{future, Future};

use crate::{
    net::{SessionTransport, ShroomSession},
    DecodePacket, NetError, PacketReader, ShroomPacket,
};

use super::{SessionHandleResult, ShroomContext, SharedSessionHandle};

/// Handler creator
#[async_trait]
pub trait MakeServerSessionHandler {
    type Transport: SessionTransport;
    type Error: From<NetError> + Debug;
    type Handler: ShroomSessionHandler<Transport = Self::Transport, Error = Self::Error>;

    /// Create a new handler for the given `session`
    /// and the shared session `handle`
    async fn make_handler(
        &mut self,
        sess: ShroomSession<Self::Transport>,
        handle: SharedSessionHandle,
    ) -> Result<ShroomContext<Self::Handler>, Self::Error>;
}

/// Session handler trait, which used to handle packets and handle messages
#[async_trait]
pub trait ShroomSessionHandler: Sized {
    type Transport: SessionTransport + Send;
    type Error: From<NetError> + Debug;
    type Msg: Send;

    /// Handle an incoming packet
    async fn handle_packet(
        ctx: &mut ShroomContext<Self>,
        packet: ShroomPacket,
    ) -> Result<SessionHandleResult, Self::Error>;

    /// Handle a passed message
    async fn handle_msg(ctx: &mut ShroomContext<Self>, msg: Self::Msg) -> Result<(), Self::Error>;

    /// Poll a message to archive an actor like message passing
    /// per default that's a never ending future
    async fn poll_msg(&mut self) -> Result<Self::Msg, Self::Error> {
        future::pending::<()>().await;
        unreachable!()
    }

    async fn finish(self, _is_migrating: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Call a the specified handler function `f` and process the returned response
pub async fn call_handler_fn<'session, F, Req, Fut, Err, H: ShroomSessionHandler>(
    ctx: &'session mut ShroomContext<H>,
    mut pr: PacketReader<'session>,
    mut f_handler: F,
) -> Result<(), Err>
where
    H: 'session,
    Req: DecodePacket<'session>,
    Err: From<NetError>,
    Fut: Future<Output = Result<(), Err>>,
    F: FnMut(&'session mut ShroomContext<H>, Req) -> Fut,
{
    let req = Req::decode_packet(&mut pr)?;
    f_handler(ctx, req).await
}

/// Declares an async router fn
/// which routes the packet to the matching handler
/// by reading the Opcode and checking It against the `OPCODE` from the `HasOpcode` Trait
/// Example:
///
/// shroom_router_fn!(
///     handle, // name
///     State,  // State type
///     ShroomSession<TcpStream>,  // Session type
///     anyhow::Error, // Error type
///     State::handle_default, // fallback handler
///     PacketReq => State::handle_req, // Handle PacketReq with handle_req
/// );
#[macro_export]
macro_rules! shroom_router_fn {
    ($fname:ident, $handler:ty, $err:ty, $default_handler:expr, $($req:ty => $handler_fn:expr),* $(,)?) => {
        async fn $fname<'session>(ctx: &'session mut ShroomContext<$handler>, mut pr: $crate::PacketReader<'session>) ->  Result<(), $err> {
            let recv_op = pr.read_opcode()?;
            match recv_op {
                $(
                    <$req as $crate::HasOpcode>::OPCODE  => $crate::net::service::handler::call_handler_fn(ctx, pr, $handler_fn).await,
                )*
                _ =>   $default_handler(ctx, recv_op, pr).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{
        crypto::SharedCryptoContext,
        net::{
            service::{
                BasicHandshakeGenerator, HandshakeGenerator, SessionHandleResult, ShroomContext, SharedSessionHandle,
            },
            ShroomSession,
        },
        opcode::WithOpcode,
        PacketReader, PacketWriter, ShroomPacket,
    };

    use super::ShroomSessionHandler;

    pub type Req1 = WithOpcode<0, u16>;
    pub type Req2 = WithOpcode<1, ()>;

    type Ctx = ShroomContext<Handler>;
    #[derive(Debug, Default)]
    struct Handler {
        req1: Req1,
    }

    impl Handler {
        async fn handle_req1(ctx: &mut Ctx, req: Req1) -> anyhow::Result<()> {
            ctx.state.req1 = req;
            Ok(())
        }

        async fn handle_double(ctx: &mut Ctx, _req: WithOpcode<1, ()>) -> anyhow::Result<()> {
            Ok(ctx.send(WithOpcode::<1, u16>(ctx.state.req1.0 * 2)).await?)
        }

        async fn handle_default(
            _ctx: &mut Ctx,
            op: u16,
            _pr: PacketReader<'_>,
        ) -> anyhow::Result<()> {
            panic!("Invalid opcode: {op}");
        }
    }

    #[async_trait::async_trait]
    impl ShroomSessionHandler for Handler {
        type Transport = std::io::Cursor<Vec<u8>>;
        type Error = anyhow::Error;
        type Msg = ();

        /// Handle an incoming packet
        async fn handle_packet(
            _ctx: &mut ShroomContext<Self>,
            _packet: ShroomPacket,
        ) -> Result<SessionHandleResult, Self::Error> {
            todo!();
        }

        /// Handle a passed message
        async fn handle_msg(
            _ctx: &mut ShroomContext<Self>,
            _msg: Self::Msg,
        ) -> Result<(), Self::Error> {
            todo!()
        }
    }

    fn get_fake_session() -> ShroomSession<std::io::Cursor<Vec<u8>>> {
        let io = std::io::Cursor::new(vec![]);
        let hshake = BasicHandshakeGenerator::v83().generate_handshake();
        ShroomSession::from_client_handshake(io, SharedCryptoContext::default(), hshake)
    }

    #[tokio::test]
    async fn router() {
        let sess = get_fake_session();

        let mut pw = PacketWriter::default();
        pw.write_opcode(0u16).expect("Encode");
        pw.write_u16(123).expect("Encode");
        let pkt_req1 = pw.into_packet();

        let mut pw = PacketWriter::default();
        pw.write_opcode(1u16).expect("Encode");
        let pkt_req2 = pw.into_packet();

        shroom_router_fn!(
            handle,
            Handler,
            anyhow::Error,
            Handler::handle_default,
            Req1 => Handler::handle_req1,
            Req2 => Handler::handle_double,
        );

        let session_handle = SharedSessionHandle::new();

        let mut ctx = ShroomContext::new(sess, Handler::default(), session_handle.0);
        handle(&mut ctx, pkt_req1.into_reader()).await.unwrap();
        assert_eq!(ctx.state.req1.0, 123);

        handle(&mut ctx, pkt_req2.into_reader()).await.unwrap();
    }
}
