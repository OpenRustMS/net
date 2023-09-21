pub mod handler;
pub mod resp;
pub mod room;
pub mod runtime;
pub mod server_conn;
pub mod tick;

use std::{
    collections::HashMap,
    fmt::Debug,
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
    time::sleep,
};
use tokio_stream::{wrappers::TcpListenerStream, StreamExt};

use crate::{codec::ShroomCodec, NetError, NetResult};

use self::{server_conn::ShroomConnHandler, tick::Tick};

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

pub struct ShroomServerConfig<H: ShroomConnHandler> {
    pub codec: Arc<H::Codec>,
    pub make_state: H::MakeState,
    pub tick: Tick,
    pub msg_cap: usize,
    pub ping_dur: Duration,
}

impl<H: ShroomConnHandler> Debug for ShroomServerConfig<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShroomServerConfig")
            .field("tick", &self.tick)
            .field("msg_cap", &self.msg_cap)
            .field("ping_dur", &self.ping_dur)
            .finish()
    }
}

pub struct ShroomServer<H: ShroomConnHandler> {
    cfg: Arc<ShroomServerConfig<H>>,
    clients: HashMap<ClientId, ServerConnHandle<H>>,
    next_id: AtomicUsize,
}

impl<H: ShroomConnHandler> std::fmt::Debug for ShroomServer<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShroomServer")
            .field("next_id", &self.next_id)
            .field("cfg", &self.cfg)
            .finish()
    }
}

impl<H: ShroomConnHandler> ShroomServer<H> {
    pub fn new(cfg: ShroomServerConfig<H>) -> Self {
        Self {
            cfg: Arc::new(cfg),
            clients: HashMap::new(),
            next_id: AtomicUsize::new(0),
        }
    }

    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl<H> ShroomServer<H>
where
    H: ShroomConnHandler + Send + 'static,
    H::Codec: Send + Sync + 'static,
{
    async fn init_conn(
        cfg: Arc<ShroomServerConfig<H>>,
        io: <H::Codec as ShroomCodec>::Transport,
        rx: mpsc::Receiver<H::Msg>,
        handle: SharedConnHandle<H::Msg>,
    ) -> Result<(), H::Error> {
        let session = cfg.codec.create_server_session(io).await?;
        let mut ctx = ServerConnCtx::new(session, rx, Duration::from_secs(30), cfg.tick.clone());
        let mut handler = H::make_handler(&cfg.make_state, &mut ctx, handle).await?;
        let res = ctx.exec(&mut handler).await;

        let migrate = match res {
            Ok(v) => v,
            Err(err) => {
                log::error!("conn error: {:?}", err);
                false
            }
        };
        handler.finish(migrate).await?;
        sleep(Duration::from_secs(10)).await; //TODO migrate delay
        let _ = ctx.close().await;
        // Close the connection here
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
                    let kill =
                        tokio::spawn(Self::init_conn(self.cfg.clone(), io, rx, handle.clone()));

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
