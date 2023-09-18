pub mod handler;
pub mod resp;
pub mod room;
pub mod server_session;
pub mod session_set;
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
    server_session::{ShroomSessionCtx, ShroomSessionHandler},
    tick::{Tick, Ticker},
};

pub type ClientId = usize;

#[derive(Debug)]
pub struct ShroomSessionHandle<Msg> {
    pub id: ClientId,
    pub tx: mpsc::Sender<Msg>,
}

impl<Msg> Clone for ShroomSessionHandle<Msg> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            tx: self.tx.clone(),
        }
    }
}

pub struct ServerClientHandle<H: ShroomSessionHandler> {
    pub session: ShroomSessionHandle<H::Msg>,
    kill: JoinHandle<Result<(), H::Error>>,
}

impl<H: ShroomSessionHandler> Drop for ServerClientHandle<H> {
    fn drop(&mut self) {
        self.kill.abort();
    }
}

pub struct ShroomServer<H: ShroomSessionHandler> {
    codec: Arc<H::Codec>,
    make_state: Arc<H::MakeState>,
    clients: HashMap<ClientId, ServerClientHandle<H>>,
    next_id: AtomicUsize,
    ticker: Ticker,
}

impl<H: ShroomSessionHandler> ShroomServer<H> {
    pub fn new(codec: H::Codec, make_state: H::MakeState, tick_dur: Duration) -> Self {
        Self {
            codec: Arc::new(codec),
            clients: HashMap::new(),
            next_id: AtomicUsize::new(0),
            make_state: Arc::new(make_state),
            ticker: Ticker::spawn(tick_dur),
        }
    }

    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn spawn_room<State: RoomState + Send + 'static>(&self, state: State) -> Room<State> {
        Room::spawn(state, self.ticker.get_tick())
    }
}

impl<H> ShroomServer<H>
where
    H: ShroomSessionHandler + Send + Sync + 'static,
    H::Codec: Send + Sync + 'static,
{
    async fn init_session(
        mh: Arc<H::MakeState>,
        codec: Arc<H::Codec>,
        io: <H::Codec as ShroomCodec>::Transport,
        rx: mpsc::Receiver<H::Msg>,
        handle: ShroomSessionHandle<H::Msg>,
        tick: Tick,
    ) -> Result<(), H::Error> {
        let session = codec.create_server_session(io).await?;
        let mut ctx = ShroomSessionCtx::new(session, rx, Duration::from_secs(30), tick);
        let mut handler = H::make_handler(&mh, &mut ctx, handle).await?;
        let res = ctx.exec(&mut handler).await;

        match res {
            Ok(b) => {
                // TODO migrate
                handler.finish(b).await?;
            }
            Err(err) => {
                // TODO error
                log::error!("Session error: {:?}", err);
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
                    let codec = self.codec.clone();
                    let (tx, rx) = mpsc::channel(16);
                    let handle = ShroomSessionHandle::<H::Msg> { id, tx };
                    let mh = self.make_state.clone();
                    let tick = self.ticker.get_tick();
                    let kill =
                        tokio::spawn(Self::init_session(mh, codec, io, rx, handle.clone(), tick));

                    self.clients.insert(
                        id,
                        ServerClientHandle {
                            kill,
                            session: handle,
                        },
                    );
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
    H: ShroomSessionHandler + Send + Sync + 'static,
    H::Codec: ShroomCodec<Transport = TcpStream> + Send + Sync + 'static,
{
    pub async fn serve_tcp(&mut self, addr: impl Into<SocketAddr>) -> NetResult<()> {
        let listener = TcpListener::bind(addr.into()).await?;
        let stream = TcpListenerStream::new(listener).map(|io| io.map_err(NetError::from));
        self.serve(stream).await
    }
}
