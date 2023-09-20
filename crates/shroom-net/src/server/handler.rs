use futures::Future;
use shroom_pkt::{DecodePacket, PacketReader};

use crate::NetError;

use super::server_conn::{
    IntoServerHandleResult, ServerConnCtx, ServerHandleResult, ShroomConnHandler,
};

/// Call a the specified handler function `f` and process the returned response
pub async fn call_handler_fn<'conn, F, Req, Resp, Fut, H, Err>(
    mut f_handler: F,
    handler: &'conn mut H,
    ctx: &'conn mut ServerConnCtx<H>,
    mut pr: PacketReader<'conn>,
) -> Result<ServerHandleResult, Err>
where
    H: ShroomConnHandler,
    Req: DecodePacket<'conn>,
    Err: From<NetError>,
    Resp: IntoServerHandleResult,
    Fut: Future<Output = Result<Resp, Err>>,
    F: FnMut(&'conn mut H, &'conn mut ServerConnCtx<H>, Req) -> Fut,
{
    let req = Req::decode_packet(&mut pr).map_err(NetError::from)?;
    Ok(f_handler(handler, ctx, req).await?.into_handle_result())
}

/// Declares an async router fn
/// which routes the packet to the matching handler
/// by reading the Opcode and checking It against the `OPCODE` from the `HasOpcode` Trait
/// Example:
///
/// shroom_router_fn!(
///     handle, // name
///     State,  // State type
///     ShroomConn<TcpStream>,  // Conn type
///     anyhow::Error, // Error type
///     State::handle_default, // fallback handler
///     PacketReq => State::handle_req, // Handle PacketReq with handle_req
/// );
#[macro_export]
macro_rules! shroom_router_fn {
    ($fname:ident, $handler:ty, $err:ty, $default_handler:expr, $($req:ty => $handler_fn:expr),* $(,)?) => {
        async fn $fname<'conn>(handler: &'conn mut $handler,
                                    ctx: &'conn mut $crate::server::ServerConnCtx<$handler>,
                                    mut pr: shroom_pkt::PacketReader<'conn>)
                                    ->  Result<$crate::server::ServerHandleResult, $err>
        {
            let recv_op = pr.read_opcode()?;
            match recv_op {
                $(
                    <$req as shroom_pkt::opcode::HasOpcode>::OPCODE  => $crate::server::handler::call_handler_fn($handler_fn, handler, ctx, pr).await,
                )*
                _ =>   $default_handler(handler, ctx, recv_op, pr).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, time::Duration};

    use shroom_pkt::{opcode::WithOpcode, PacketReader, PacketWriter};
    use tokio::sync::mpsc;

    use crate::{
        codec::{conn::ShroomConn, legacy::LegacyCodec, LocalShroomTransport},
        server::{
            server_conn::{
                IntoServerHandleResult, ServerConnCtx, ServerHandleResult, ShroomConnEvent,
                ShroomConnHandler,
            },
            tick::Ticker,
            SharedConnHandle,
        },
    };

    pub type Req1 = WithOpcode<0, u16>;
    pub type Trans = LocalShroomTransport<Cursor<Vec<u8>>>;
    pub type Codec = LegacyCodec<Trans>;

    #[derive(Debug, Default)]
    struct TestHandler {
        req1: Req1,
    }

    #[async_trait::async_trait]
    impl ShroomConnHandler for TestHandler {
        type Codec = Codec;
        type Error = anyhow::Error;
        type Msg = ();
        type MakeState = ();

        async fn make_handler(
            _make_state: &Self::MakeState,
            _ctx: &mut ServerConnCtx<Self>,
            _handle: SharedConnHandle<Self::Msg>,
        ) -> Result<Self, Self::Error> {
            Ok(Self::default())
        }

        async fn handle_msg(
            &mut self,
            _ctx: &mut ServerConnCtx<Self>,
            _msg: ShroomConnEvent<Self::Msg>,
        ) -> Result<ServerHandleResult, Self::Error> {
            Ok(ServerHandleResult::Ok)
        }

        async fn finish(self, _is_migrating: bool) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    impl TestHandler {
        async fn handle_req1(
            &mut self,
            _ctx: &mut ServerConnCtx<Self>,
            req: Req1,
        ) -> anyhow::Result<()> {
            self.req1 = req;
            Ok(())
        }

        async fn handle_default(
            &mut self,
            _ctx: &mut ServerConnCtx<Self>,
            _op: u16,
            _pr: PacketReader<'_>,
        ) -> anyhow::Result<ServerHandleResult> {
            Ok(().into_handle_result())
        }
    }

    fn get_fake_session() -> ShroomConn<Codec> {
        let io = LocalShroomTransport(std::io::Cursor::new(vec![]));
        let cdc: LegacyCodec<Cursor<Vec<u8>>> = LegacyCodec::default();
        ShroomConn::new(io, cdc.create_mock_client_codec())
    }

    #[tokio::test]
    async fn router() {
        let tick_gen = Ticker::spawn(Duration::from_secs(60));
        let (_tx, rx) = mpsc::channel(16);
        let mut ctx = ServerConnCtx::new(
            get_fake_session(),
            rx,
            Duration::from_secs(60),
            tick_gen.get_tick(),
        );
        let mut state = TestHandler::default();

        let mut pw = PacketWriter::default();
        pw.write_opcode(0u16).expect("Encode");
        pw.write_u16(123).expect("Encode");

        let pkt = pw.into_packet();

        shroom_router_fn!(
            handle,
            TestHandler,
            anyhow::Error,
            TestHandler::handle_default,
            Req1 => TestHandler::handle_req1,
        );

        handle(&mut state, &mut ctx, pkt.into_reader())
            .await
            .unwrap();

        assert_eq!(state.req1.0, 123);
    }
}
