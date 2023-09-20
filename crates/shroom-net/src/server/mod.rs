pub mod handler;
pub mod resp;
pub mod room;
pub mod runtime;
pub mod server_conn;
pub mod tick;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::Stream;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinHandle,
};
use tokio_stream::{wrappers::TcpListenerStream, StreamExt};

use crate::{codec::ShroomCodec, NetError, NetResult};

use self::{
    room::{Room, RoomState},
    server_conn::ShroomConnHandler,
    tick::Tick,
};

pub use server_conn::ServerConnCtx;
pub use server_conn::ServerHandleResult;

pub type ClientId = usize;

#[derive(Debug)]
pub struct SharedConnHandle<Msg> {
    pub id: ClientId,
    pub tx: mpsc::Sender<Msg>,
}

impl<Msg> Clone for SharedConnHandle<Msg> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            tx: self.tx.clone(),
        }
    }
}

#[derive(Debug)]
pub struct ServerConnHandle<H: ShroomConnHandler> {
    pub conn: SharedConnHandle<H::Msg>,
    kill: JoinHandle<Result<(), H::Error>>,
}

impl<H: ShroomConnHandler> Drop for ServerConnHandle<H> {
    fn drop(&mut self) {
        self.kill.abort();
    }
}

pub struct ShroomServer<H: ShroomConnHandler> {
    codec: Arc<H::Codec>,
    make_state: Arc<H::MakeState>,
    clients: HashMap<ClientId, ServerConnHandle<H>>,
    next_id: AtomicUsize,
    tick: Tick,
}

impl<H: ShroomConnHandler> std::fmt::Debug for ShroomServer<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShroomServer")
            .field("next_id", &self.next_id)
            .field("ticker", &self.tick)
            .finish()
    }
}

impl<H: ShroomConnHandler> ShroomServer<H> {
    pub fn new(codec: Arc<H::Codec>, make_state: H::MakeState, tick: Tick) -> Self {
        Self {
            codec,
            clients: HashMap::new(),
            next_id: AtomicUsize::new(0),
            make_state: Arc::new(make_state),
            tick,
        }
    }

    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn spawn_room<State: RoomState + Send + 'static>(&self, state: State) -> Room<State> {
        Room::spawn(state, self.tick.clone())
    }
}

impl<H> ShroomServer<H>
where
    H: ShroomConnHandler + Send + 'static,
    H::Codec: Send + Sync + 'static,
{
    async fn init_conn(
        mh: Arc<H::MakeState>,
        codec: Arc<H::Codec>,
        io: <H::Codec as ShroomCodec>::Transport,
        rx: mpsc::Receiver<H::Msg>,
        handle: SharedConnHandle<H::Msg>,
        tick: Tick,
    ) -> Result<(), H::Error> {
        let session = codec.create_server_session(io).await?;
        let mut ctx = ServerConnCtx::new(session, rx, Duration::from_secs(30), tick);
        let mut handler = H::make_handler(&mh, &mut ctx, handle).await?;
        let res = ctx.exec(&mut handler).await;

        match res {
            Ok(migrate) => {
                handler.finish(migrate).await?;
            }
            Err(err) => {
                // TODO error
                log::error!("conn error: {:?}", err);
                handler.finish(false).await?;
            }
        }
        Ok(())
    }

    pub async fn serve(
        &mut self,
        mut io_stream: impl Stream<Item = NetResult<<H::Codec as ShroomCodec>::Transport>> + Unpin,
    ) -> NetResult<()> {
        loop {
            match io_stream.next().await {
                Some(Ok(io)) => {
                    let id = self.next_id();
                    let (tx, rx) = mpsc::channel(16);
                    let handle = SharedConnHandle::<H::Msg> { id, tx };
                    let kill = tokio::spawn(Self::init_conn(
                        self.make_state.clone(),
                        self.codec.clone(),
                        io,
                        rx,
                        handle.clone(),
                        self.tick.clone(),
                    ));

                    self.clients
                        .insert(id, ServerConnHandle { kill, conn: handle });
                }
                Some(Err(err)) => {
                    log::error!("Error while accepting connection: {}", err);
                }
                None => break,
            }
        }

        Ok(())
    }
}

impl<H> ShroomServer<H>
where
    H: ShroomConnHandler + Send + 'static,
    H::Codec: ShroomCodec<Transport = TcpStream> + Send + Sync + 'static,
{
    pub async fn serve_tcp(mut self, addr: impl Into<SocketAddr>) -> NetResult<()> {
        let listener = TcpListener::bind(addr.into()).await?;
        let stream = TcpListenerStream::new(listener).map(|io| io.map_err(NetError::from));
        self.serve(stream).await
    }
}
