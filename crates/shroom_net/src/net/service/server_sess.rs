use std::{fmt::Debug, io, marker::PhantomData, sync::Arc, time::Duration};

use futures::{Stream, StreamExt};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_stream::wrappers::TcpListenerStream;

use crate::{
    crypto::SharedCryptoContext,
    net::{codec::handshake::Handshake, service::SessionHandleResult, ShroomSession},
    util::framed_pipe::FramedPipeReceiver,
    NetError, ShroomPacket,
};

use super::{
    handler::{MakeServerSessionHandler, ShroomSessionHandler},
    HandshakeGenerator, SharedSessionHandle, ShroomContext,
};

#[derive(Debug)]
pub struct ShroomSessionHandle<H: ShroomSessionHandler> {
    pub handle: tokio::task::JoinHandle<Result<(), H::Error>>,
    _handler: PhantomData<H>,
}

impl<H> ShroomSessionHandle<H>
where
    H: ShroomSessionHandler + Send,
{
    /// Check whether the session is still active
    pub fn is_active(&self) -> bool {
        !self.handle.is_finished()
    }
}

pub struct ShroomServerSession<H: ShroomSessionHandler> {
    cfg: Arc<ShroomServerConfig>,
    session_rx: FramedPipeReceiver,
    pending_ping: bool,
    ctx: ShroomContext<H>,
}

impl<H> ShroomServerSession<H>
where
    H: ShroomSessionHandler + Send,
    H::Transport: Unpin,
{
    pub fn new(
        cfg: Arc<ShroomServerConfig>,
        session_rx: FramedPipeReceiver,
        ctx: ShroomContext<H>,
    ) -> Self {
        Self {
            cfg,
            session_rx,
            pending_ping: false,
            ctx,
        }
    }
    /// Handle migration by finishing the handler and then closing the session
    /// after the migration delay
    async fn finish(self, migrate: bool) -> Result<(), H::Error> {
        log::trace!("Session closing(migrate={migrate})");
        let ShroomContext { state, session, .. } = self.ctx;
        state.finish(migrate).await?;
        if migrate {
            // Socket has to be kept open cause the client doesn't support
            // reading a packet when the socket is closed
            tokio::time::sleep(self.cfg.migrate_delay).await;
        }
        session.close().await?;
        Ok(())
    }

    /// Handle the next ping
    async fn handle_ping_tick(&mut self) -> Result<(), H::Error> {
        // Check if previous ping was responded
        if self.pending_ping {
            log::trace!("Ping Timeout");
            return Err(NetError::PingTimeout.into());
        }

        // Elsewise send a new ping packet
        self.pending_ping = true;
        self.ctx
            .session
            .send_packet(self.cfg.ping_packet.as_ref())
            .await?;
        Ok(())
    }

    /// Handle incoming pong
    fn handle_pong(&mut self) {
        // Reset flag
        self.pending_ping = false;
    }

    async fn exec_loop(&mut self) -> Result<bool, H::Error> {
        let mut ping_interval = tokio::time::interval(self.cfg.ping_interval);

        loop {
            tokio::select! {
                biased;
                // Handle next incoming packet
                p =  self.ctx.session.read_packet() => {
                    let res = H::handle_packet(&mut self.ctx, p?).await?;
                    // Handling the handle result
                    match res {
                        SessionHandleResult::Migrate => {
                            return Ok(true);
                        },
                        SessionHandleResult::Pong => {
                            self.handle_pong();
                        },
                        SessionHandleResult::Ok => ()
                    }
                },
                _ = ping_interval.tick() => {
                    self.handle_ping_tick().await?;
                },
                //Handle external Session packets
                p = self.session_rx.next() => {
                    // note tx is never dropped, so there'll be always a packet here
                    // TODO handle error here
                    let p = p.expect("Session packet").unwrap();
                    self.ctx.session.send_packet(&p).await?;
                },
                msg = H::poll_msg(&mut self.ctx.state) => {
                    H::handle_msg(&mut self.ctx, msg?).await?;
                },
                _ = self.ctx.session_handle.ct.cancelled() => {
                    break Ok(false);
                },

            };
        }
    }

    pub async fn exec(mut self) -> Result<(), H::Error> {
        let res = self.exec_loop().await;

        match res {
            Ok(true) => {
                self.finish(true).await?;
            }
            Ok(false) => {
                self.finish(false).await?;
            }
            Err(e) => {
                log::error!("Session error: {e:?}");
                self.finish(false).await?;
            }
        }
        Ok(())
    }
}

/// Config for a server
#[derive(Debug)]
pub struct ShroomServerConfig {
    /// Crypto context which contains the keys
    pub crypto_ctx: SharedCryptoContext,
    /// Duration for how long the transport is kept alive after receiving a Migration Response
    pub migrate_delay: Duration,
    /// Ping packet
    pub ping_packet: ShroomPacket,
    /// Ping interval
    pub ping_interval: Duration,
}

/// Server which can host multiple Session
/// `MH` is used to create the handler
/// `H` is the Handshake generator
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
    /// Creates a new server with the given config
    pub fn new(cfg: ShroomServerConfig, handshake_gen: H, make_handler: MH) -> Self {
        Self {
            cfg: Arc::new(cfg),
            handshake_gen,
            make_handler,
            handles: Vec::new(),
        }
    }

    /// Removes all closed sesison handles
    fn remove_closed_handles(&mut self) {
        self.handles.retain(|handle| handle.is_active());
    }

    /// Adds a handle
    fn add_handle(&mut self, handle: ShroomSessionHandle<MH::Handler>) {
        // TODO: there should be an upper limit for active connections

        self.remove_closed_handles();
        self.handles.push(handle);
    }
}

impl<MH, H> ShroomServer<MH, H>
where
    H: HandshakeGenerator,
    MH: MakeServerSessionHandler + Send + Clone + 'static,
    MH::Error: From<io::Error> + Send + 'static,
    MH::Handler: Send + 'static,
    MH::Transport: Send + Unpin + 'static,
{
    /// Spawn a incoming `io` Transport
    fn spawn(
        io: MH::Transport,
        cfg: Arc<ShroomServerConfig>,
        mut mk: MH,
        handshake: Handshake,
    ) -> ShroomSessionHandle<MH::Handler> {
        // Spawn the future
        let handle = tokio::spawn(async move {
            // Using a block here so we can capture the result and log It later
            let res = async move {
                // Initialize the session with the handshake
                let session =
                    ShroomSession::initialize_server_session(io, cfg.crypto_ctx.clone(), handshake)
                        .await?;

                // Create the shared session handle and context
                let (session_handle, session_rx) = SharedSessionHandle::new();

                // Create the session handler
                let ctx = mk.make_handler(session, session_handle).await?;

                // Create the session and execute It
                let server_session = ShroomServerSession::new(cfg, session_rx, ctx);

                server_session.exec().await
            };

            // Await the block
            let res = res.await;
            // Print the error If there's one
            if let Err(ref err) = res {
                log::error!("Session error: {:?}", err);
            }

            // Forward the result
            res
        });

        ShroomSessionHandle {
            handle,
            _handler: PhantomData,
        }
    }

    /// Handles an incoming `io` Transport
    fn handle_incoming(&mut self, io: MH::Transport)
    where
        MH: Send + Clone + 'static,
        MH::Error: From<io::Error> + Send + 'static,
        MH::Handler: Send + 'static,
        MH::Transport: Send + Unpin + 'static,
    {
        // Generate the handshake here
        let handshake = self.handshake_gen.generate_handshake();
        // Spawn the connection
        let handle = Self::spawn(io, self.cfg.clone(), self.make_handler.clone(), handshake);
        // Add the handle to the interal collection
        self.add_handle(handle);
    }

    fn is_connection_error(e: &io::Error) -> bool {
        matches!(
            e.kind(),
            io::ErrorKind::ConnectionRefused
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionReset
        )
    }

    /// Run the server on an incoming Stream of Transports
    /// for example a `TcpListenerStream`
    pub async fn run<S>(&mut self, mut io: S) -> Result<(), MH::Error>
    where
        S: Stream<Item = std::io::Result<MH::Transport>> + Unpin,
    {
        while let Some(io) = io.next().await {
            let io = io?;
            self.handle_incoming(io);
        }

        Ok(())
    }
}

impl<MH, H> ShroomServer<MH, H>
where
    H: HandshakeGenerator,
    MH: MakeServerSessionHandler<Transport = TcpStream> + Send + Clone + 'static,
    MH::Handler: Send + 'static,
    MH::Error: From<io::Error> + Send + 'static,
{
    /// Serve with the given `addr` via Tcp as Transprot
    pub async fn serve_tcp(&mut self, addr: impl ToSocketAddrs) -> Result<(), MH::Error> {
        let listener = TcpListener::bind(addr).await?;
        self.run(TcpListenerStream::new(listener).filter(|io| {
            std::future::ready(match io {
                // Skip connection errors, just log them
                Err(err) if Self::is_connection_error(err) => {
                    log::trace!("Server Connection error: {}", err);
                    false
                }
                _ => true,
            })
        }))
        .await
    }
}
