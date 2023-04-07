use std::{fmt::Debug, io, marker::PhantomData, sync::Arc, time::Duration};

use futures::{Stream, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    time::Interval,
};
use tokio_util::sync::CancellationToken;

use crate::{
    net::{
        codec::handshake::Handshake, crypto::ShroomCryptoKeys,
        service::handler::SessionHandleResult, ShroomSession,
    },
    util::framed_pipe::{framed_pipe, FramedPipeReceiver, FramedPipeSender},
    NetError,
};

use super::{
    handler::{MakeServerSessionHandler, ShroomServerSessionHandler, ShroomSessionHandler},
    packet_buffer::PacketBuffer,
    HandshakeGenerator,
};

pub const DEFAULT_MIGRATE_DELAY: Duration = Duration::from_millis(7500);

#[derive(Debug, Clone)]
pub struct SharedSessionHandle {
    pub ct: CancellationToken,
    pub tx: FramedPipeSender,
}

impl SharedSessionHandle {
    pub fn try_send_buf(&mut self, pkt_buf: &PacketBuffer) -> anyhow::Result<()> {
        Ok(self.tx.try_send_all(pkt_buf.packets())?)
    }

    pub fn try_send(&mut self, item: &[u8]) -> anyhow::Result<()> {
        Ok(self.tx.try_send(item)?)
    }
}

impl SharedSessionHandle {
    pub fn new() -> (Self, FramedPipeReceiver) {
        let (tx, rx) = framed_pipe(8 * 1024, 128);
        (
            Self {
                ct: CancellationToken::new(),
                tx,
            },
            rx,
        )
    }
}

#[derive(Debug)]
pub struct ShroomSessionHandle<H: ShroomSessionHandler> {
    pub handle: tokio::task::JoinHandle<Result<(), H::Error>>,
    _handler: PhantomData<H>,
}

impl<H> ShroomSessionHandle<H>
where
    H: ShroomSessionHandler + Send,
{
    pub fn is_running(&self) -> bool {
        !self.handle.is_finished()
    }
}

pub struct ShroomServerSession<H: ShroomSessionHandler> {
    session: ShroomSession<H::Transport>,
    migrate_delay: Duration,
    handler: H,
    session_handle: SharedSessionHandle,
    session_rx: FramedPipeReceiver,
    pending_ping: bool,
    ping_interval: Interval,
}

impl<H> ShroomServerSession<H>
where
    H: ShroomServerSessionHandler + Send,
    H::Transport: Unpin,
{
    pub fn new(
        session: ShroomSession<H::Transport>,
        migrate_delay: Duration,
        handler: H,
        session_handle: SharedSessionHandle,
        session_rx: FramedPipeReceiver,
    ) -> Self {
        Self {
            session,
            migrate_delay,
            handler,
            session_handle,
            session_rx,
            pending_ping: false,
            ping_interval: tokio::time::interval(H::get_ping_interval()),
        }
    }

    pub async fn migrate(mut self) -> Result<(), H::Error> {
        log::trace!("Session migrated");
        self.handler.finish(true).await?;
        // Socket has to be kept open cause the client doesn't support
        // reading a packet when the socket is closed
        // TODO: make this configurable
        tokio::time::sleep(self.migrate_delay).await;
        self.session.close().await?;
        Ok(())
    }

    pub async fn handle_ping_tick(&mut self) -> Result<(), H::Error> {
        // Check if previous ping was responded
        if self.pending_ping {
            log::trace!("Ping Timeout");
            return Err(NetError::PingTimeout.into());
        }

        // Elsewise send a new ping packet
        log::trace!("Sending ping...");
        self.pending_ping = true;
        let ping_packet = self.handler.get_ping_packet()?;
        self.session.send_raw_packet(ping_packet.as_ref()).await?;
        Ok(())
    }

    fn handle_pong(&mut self) {
        // Reset flag
        self.pending_ping = false;
    }

    pub async fn exec(mut self) -> Result<(), H::Error> {
        self.ping_interval.tick().await;

        loop {
            //TODO might need some micro-optimization to ensure no future gets stalled
            tokio::select! {
                biased;
                // Handle next incoming packet
                p = self.session.read_packet() => {
                    let p = p?;
                    let res = self.handler.handle_packet(p, &mut self.session).await?;
                    // Handling the handle result
                    match res {
                        SessionHandleResult::Migrate => {
                            return self.migrate().await;
                        },
                        SessionHandleResult::Pong => {
                            self.handle_pong();
                        },
                        SessionHandleResult::Ok => ()
                    }
                },
                _ = self.ping_interval.tick() => {
                    self.handle_ping_tick().await?;
                },
                //Handle external Session packets
                p = self.session_rx.next() => {
                    // note tx is never dropped, so there'll be always a packet here
                    let p = p.expect("Session packet");
                    self.session.send_raw_packet(&p).await?;
                },
                p = self.handler.poll_broadcast() => {
                    let p = p?.expect("Must contain packet");
                    self.session.send_raw_packet(p.as_ref()).await?;
                },
                _ = self.session_handle.ct.cancelled() => {
                    break;
                },

            };
        }

        // Finish the handler
        self.handler.finish(false).await?;
        self.session.close().await?;

        // Normal cancellation by timeout or cancellation
        Ok(())
    }
}

#[derive(Debug)]
pub struct ShroomServerConfig {
    /// Crypto keys
    pub keys: ShroomCryptoKeys,
    /// Duration for how long the transport is kept alive after receiving a Migration Response
    pub migrate_delay: Duration,
}

#[derive(Debug)]
pub struct ShroomServer<MH, H>
where
    MH: MakeServerSessionHandler,
{
    cfg: Arc<ShroomServerConfig>,
    handshake_gen: H,
    make_handler: MH,
    handles: Vec<ShroomSessionHandle<MH::Handler>>,
}

impl<MH, H> ShroomServer<MH, H>
where
    H: HandshakeGenerator,
    MH: MakeServerSessionHandler,
    MH::Handler: Send,
{
    pub fn new(cfg: ShroomServerConfig, handshake_gen: H, make_handler: MH) -> Self {
        Self {
            cfg: Arc::new(cfg),
            handshake_gen,
            make_handler,
            handles: Vec::new(),
        }
    }

    fn remove_closed_handles(&mut self) {
        self.handles.retain(|handle| handle.is_running());
    }

    fn add_handle(&mut self, handle: ShroomSessionHandle<MH::Handler>) {
        // TODO: there should be an upper limit for active connections

        self.remove_closed_handles();
        self.handles.push(handle);
    }
}

impl<MH, H> ShroomServer<MH, H>
where
    MH: MakeServerSessionHandler + Send + Clone + 'static,
    MH::Error: From<io::Error> + Send + 'static,
    MH::Handler: Send + 'static,
    MH::Transport: Send + Unpin + 'static,
    H: HandshakeGenerator,
{
    pub fn spawn(
        io: MH::Transport,
        cfg: Arc<ShroomServerConfig>,
        mut mk: MH,
        handshake: Handshake,
    ) -> Result<ShroomSessionHandle<MH::Handler>, <MH::Handler as ShroomSessionHandler>::Error>
    {
        let handle = tokio::spawn(async move {
            let res = async move {
                let mut session =
                    ShroomSession::initialize_server_session(io, &cfg.keys, handshake).await?;

                let (session_handle, session_rx) = SharedSessionHandle::new();
                let handler = mk
                    .make_handler(&mut session, session_handle.clone())
                    .await?;

                let res = ShroomServerSession::new(
                    session,
                    cfg.migrate_delay,
                    handler,
                    session_handle,
                    session_rx,
                )
                .exec()
                .await;
            
                if let Err(ref err) = res {
                    log::info!("Session exited with error: {:?}", err);
                }

                Ok(())
            };

            let res = res.await;
            if let Err(ref err) = res {
                log::error!("Session error: {:?}", err);
            }

            res
        });

        Ok(ShroomSessionHandle {
            handle,
            _handler: PhantomData,
        })
    }

    fn handle_incoming(&mut self, io: MH::Transport) -> Result<(), MH::Error>
    where
        MH: Send + Clone + 'static,
        MH::Error: From<io::Error> + Send + 'static,
        MH::Handler: Send + 'static,
        MH::Transport: Send + Unpin + 'static,
    {
        let handshake = self.handshake_gen.generate_handshake();
        let handle = Self::spawn(io, self.cfg.clone(), self.make_handler.clone(), handshake)?;
        self.add_handle(handle);

        Ok(())
    }

    pub async fn run<S>(&mut self, mut io: S) -> Result<(), MH::Error>
    where
        S: Stream<Item = std::io::Result<MH::Transport>> + Unpin,
    {
        while let Some(io) = io.next().await {
            let io = io.map_err(NetError::IO)?;
            self.handle_incoming(io)?;
        }

        Ok(())
    }
}

impl<MH, H> ShroomServer<MH, H>
where
    H: HandshakeGenerator,
    MH::Error: From<io::Error> + Send + 'static,
    MH::Handler: Send + 'static,
    MH::Transport: Send + Unpin + 'static,
    MH: MakeServerSessionHandler<Transport = TcpStream> + Send + Clone + 'static,
    MH::Error: From<io::Error> + Send + 'static,
{
    pub async fn serve_tcp(&mut self, addr: impl ToSocketAddrs) -> Result<(), MH::Error> {
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (io, _) = listener.accept().await?;
            self.handle_incoming(io)?;
        }
    }
}
