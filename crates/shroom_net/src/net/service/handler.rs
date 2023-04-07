use std::{fmt::Debug, time::Duration};

use async_trait::async_trait;
use futures::{future, Future};
use tokio::sync::mpsc;

use crate::{
    net::{ShroomSession, SessionTransport},
    DecodePacket, NetError, PacketReader, ShroomPacket,
};

use super::{
    resp::{IntoResponse, Response},
    session_svc::SharedSessionHandle,
};

pub type BroadcastSender = mpsc::Sender<ShroomPacket>;

pub enum SessionHandleResult {
    Ok,
    Migrate,
    Pong,
}

#[async_trait]
pub trait ShroomSessionHandler: Sized {
    type Transport: SessionTransport;
    type Error: From<NetError> + Debug;

    async fn handle_packet(
        &mut self,
        packet: ShroomPacket,
        session: &mut ShroomSession<Self::Transport>,
    ) -> Result<SessionHandleResult, Self::Error>;

    async fn poll_broadcast(&mut self) -> Result<Option<ShroomPacket>, Self::Error> {
        future::pending::<()>().await;
        unreachable!()
    }

    async fn finish(self, _is_migrating: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[async_trait]
pub trait ShroomServerSessionHandler: ShroomSessionHandler {
    fn get_ping_interval() -> Duration;
    fn get_ping_packet(&mut self) -> Result<ShroomPacket, Self::Error>;
}

#[async_trait]
pub trait MakeServerSessionHandler {
    type Transport: SessionTransport;
    type Error: From<NetError> + Debug;
    type Handler: ShroomServerSessionHandler<Transport = Self::Transport, Error = Self::Error>;

    async fn make_handler(
        &mut self,
        sess: &mut ShroomSession<Self::Transport>,
        handle: SharedSessionHandle,
    ) -> Result<Self::Handler, Self::Error>;
}

// TODO: sooner or later there should be a proper service/handler trait for this
// Prior attempts to define a service trait failed for several reasons
// 1. Unable to reuse the session to send the response after the handler was called
// 2. Lifetime 'a in DecodePacket<'a> is close to impossible to express while implementing the trait for a FnMut
// If you have better ideas how to implement this I'm completely open to this
// Also the current design is not final, It'd probably make sense to store the state
// in the session to avoid having 2 mut references, however It'd be quiet a challenge to call self methods
// on the state, cause you'd still like to have a session to send packets

pub async fn call_handler_fn<'session, F, Req, Fut, Trans, State, Resp, Err>(
    state: &'session mut State,
    session: &'session mut ShroomSession<Trans>,
    mut pr: PacketReader<'session>,
    mut f: F,
) -> Result<SessionHandleResult, Err>
where
    Trans: SessionTransport + Send + Unpin,
    F: FnMut(&'session mut State, Req) -> Fut,
    Fut: Future<Output = Result<Resp, Err>>,
    Req: DecodePacket<'session>,
    Resp: IntoResponse,
    Err: From<NetError>,
{
    let req = Req::decode_packet(&mut pr)?;
    let resp = f(state, req).await?.into_response();
    Ok(resp.send(session).await?)
}

#[macro_export]
macro_rules! shroom_router_handler {
    ($name: ident, $state:ty, $session:ty, $err:ty, $default_handler:expr, $($req:ty => $handler_fn:expr),* $(,)?) => {
        async fn $name<'session>(state: &'session mut $state, session: &'session mut $session, mut pr: $crate::PacketReader<'session>) ->  Result<SessionHandleResult, $err> {
            let recv_op = pr.read_opcode()?;
            match recv_op {
                $(
                    <$req as $crate::HasOpcode>::OPCODE  => $crate::net::service::handler::call_handler_fn(state, session, pr, $handler_fn).await,
                )*
                _ =>   $default_handler(state, recv_op, pr).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::{
        net::{service::{BasicHandshakeGenerator, HandshakeGenerator}, ShroomSession, crypto::ShroomCryptoKeys},
        opcode::WithOpcode,
        PacketReader, PacketWriter,
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
        let keys = ShroomCryptoKeys::default();
        ShroomSession::from_client_handshake(io, &keys, hshake)
    }

    #[tokio::test]
    async fn router() {
        let mut sess = get_fake_session();
        let mut state = State::default();

        let mut pw = PacketWriter::default();
        pw.write_opcode(0u16).expect("Encode");
        pw.write_u16(123).expect("Encode");

        let pkt = pw.into_packet();

        shroom_router_handler!(
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
