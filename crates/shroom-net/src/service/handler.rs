use std::fmt::Debug;

use async_trait::async_trait;
use futures::{future, Future};
use shroom_pkt::{DecodePacket, PacketReader, ShroomPacketData};

use crate::{NetError, SessionTransport, ShroomSession};

use super::{
    resp::{IntoResponse, Response},
    server_sess::SharedSessionHandle,
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
        sess: &mut ShroomSession<Self::Transport>,
        handle: SharedSessionHandle,
    ) -> Result<Self::Handler, Self::Error>;
}

/// Session handler trait, which used to handle packets and handle messages
#[async_trait]
pub trait ShroomSessionHandler: Sized {
    type Transport: SessionTransport;
    type Error: From<NetError> + Debug;
    type Msg: Send;

    /// Handle an incoming packet
    async fn handle_packet(
        &mut self,
        packet: ShroomPacketData,
        session: &mut ShroomSession<Self::Transport>,
    ) -> Result<SessionHandleResult, Self::Error>;

    /// Handle a passed message
    async fn handle_msg(
        &mut self,
        session: &mut ShroomSession<Self::Transport>,
        msg: Self::Msg,
    ) -> Result<(), Self::Error>;

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
pub async fn call_handler_fn<'session, F, Req, Fut, Trans, State, Resp, Err>(
    state: &'session mut State,
    session: &'session mut ShroomSession<Trans>,
    mut pr: PacketReader<'session>,
    mut f_handler: F,
) -> Result<SessionHandleResult, Err>
where
    Req: DecodePacket<'session>,
    Resp: IntoResponse,
    Err: From<NetError>,
    Fut: Future<Output = Result<Resp, Err>>,
    F: FnMut(&'session mut State, Req) -> Fut,
    Trans: SessionTransport + Send + Unpin,
{
    let req = Req::decode_packet(&mut pr).map_err(NetError::from)?;
    let resp = f_handler(state, req).await?.into_response();
    Ok(resp.send(session).await?)
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
    ($fname:ident, $state:ty, $session:ty, $err:ty, $default_handler:expr, $($req:ty => $handler_fn:expr),* $(,)?) => {
        async fn $fname<'session>(state: &'session mut $state, session: &'session mut $session, mut pr: shroom_pkt::PacketReader<'session>) ->  Result<SessionHandleResult, $err> {
            let recv_op = pr.read_opcode()?;
            match recv_op {
                $(
                    <$req as shroom_pkt::opcode::HasOpcode>::OPCODE  => $crate::service::handler::call_handler_fn(state, session, pr, $handler_fn).await,
                )*
                _ =>   $default_handler(state, recv_op, pr).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::io;

    use shroom_pkt::{opcode::WithOpcode, PacketReader, PacketWriter};

    use crate::{
        crypto::SharedCryptoContext,
        service::{BasicHandshakeGenerator, HandshakeGenerator},
        ShroomSession,
    };

    use super::SessionHandleResult;

    pub type Req1 = WithOpcode<0, u16>;

    #[derive(Debug, Default)]
    struct State {
        req1: Req1,
    }

    impl State {
        async fn handle_req1(&mut self, req: Req1) -> anyhow::Result<()> {
            self.req1 = req;
            Ok(())
        }

        async fn handle_default(
            &mut self,
            _op: u16,
            _pr: PacketReader<'_>,
        ) -> anyhow::Result<SessionHandleResult> {
            Ok(SessionHandleResult::Ok)
        }
    }

    fn get_fake_session() -> ShroomSession<std::io::Cursor<Vec<u8>>> {
        let io = std::io::Cursor::new(vec![]);
        let hshake = BasicHandshakeGenerator::v83().generate_handshake();
        ShroomSession::from_client_handshake(io, SharedCryptoContext::default(), hshake)
    }

    #[tokio::test]
    async fn router() {
        let mut sess = get_fake_session();
        let mut state = State::default();

        let mut pw = PacketWriter::default();
        pw.write_opcode(0u16).expect("Encode");
        pw.write_u16(123).expect("Encode");

        let pkt = pw.into_packet();

        shroom_router_fn!(
            handle,
            State,
            ShroomSession<io::Cursor<Vec<u8>>>,
            anyhow::Error,
            State::handle_default,
            Req1 => State::handle_req1,
        );

        handle(&mut state, &mut sess, pkt.into_reader())
            .await
            .unwrap();

        assert_eq!(state.req1.0, 123);
    }
}
