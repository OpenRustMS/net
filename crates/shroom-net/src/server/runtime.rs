use std::{
    net::{IpAddr, SocketAddr},
    ops::RangeInclusive,
    sync::Arc,
    time::Duration,
};

use futures::Future;
use tokio::{net::TcpStream, task::JoinHandle};

use crate::{codec::ShroomCodec, NetResult};

use super::{
    server_conn::ShroomConnHandler,
    tick::{Tick, Ticker},
    ShroomServer,
};

#[derive(Debug)]
pub struct ShroomServerConfig {
    pub server_name: String,
    pub external_ip: IpAddr,
    pub listen_ip: IpAddr,
    pub login_port: u16,
    pub game_ports: RangeInclusive<u16>,
    pub tick_duration: Duration,
}

#[async_trait::async_trait]
pub trait ShroomServerHandler {
    type Codec: ShroomCodec + Send + Sync + 'static;
    type GameHandler: ShroomConnHandler<Codec = Self::Codec> + Send + 'static;
    type LoginHandler: ShroomConnHandler<Codec = Self::Codec> + Send + 'static;

    type Services;

    async fn build_services(
        &self,
        ticker: &Ticker,
        cfg: Arc<ShroomServerConfig>,
    ) -> anyhow::Result<Self::Services>;

    fn make_login_handler(
        &self,
        services: Arc<Self::Services>,
        tick: Tick,
    ) -> anyhow::Result<<Self::LoginHandler as ShroomConnHandler>::MakeState>;

    fn make_game_handler(
        &self,
        services: Arc<Self::Services>,
        tick: Tick,
        channel_id: usize,
    ) -> anyhow::Result<<Self::GameHandler as ShroomConnHandler>::MakeState>;
}

#[derive(Debug)]
pub struct ShroomServerRuntime<S: ShroomServerHandler> {
    codec: Arc<S::Codec>,
    cfg: Arc<ShroomServerConfig>,
    ticker: Ticker,
    game_servers: Vec<JoinHandle<()>>,
    login_server: Option<JoinHandle<()>>,
    services: Arc<S::Services>,
    handler: S,
}

impl<S> ShroomServerRuntime<S>
where
    S: ShroomServerHandler,
{
    pub async fn create(
        codec: S::Codec,
        cfg: ShroomServerConfig,
        handler: S,
    ) -> anyhow::Result<Self> {
        let cfg = Arc::new(cfg);
        let ticker = Ticker::spawn(cfg.tick_duration);
        let services = handler.build_services(&ticker, cfg.clone()).await?;
        Ok(Self {
            codec: Arc::new(codec),
            cfg,
            ticker,
            game_servers: Vec::new(),
            login_server: None,
            services: Arc::new(services),
            handler,
        })
    }

    fn spawn_supervised<F>(label: &'static str, fut: F) -> JoinHandle<()>
    where
        F: Future<Output = NetResult<()>> + Send + 'static,
    {
        tokio::spawn(async move {
            match fut.await {
                Ok(()) => (),
                Err(err) => log::error!("Error for server({label}): {err}"),
            }
        })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        //TODO find a better way to execute the server
        self.login_server.unwrap().await?;
        Ok(())
    }
}

impl<S> ShroomServerRuntime<S>
where
    S: ShroomServerHandler,
    S::Codec: ShroomCodec<Transport = TcpStream>,
{
    pub async fn spawn_login_server(&mut self) -> anyhow::Result<()> {
        if self.login_server.is_some() {
            anyhow::bail!("Login server already started");
        }

        let login_make = self
            .handler
            .make_login_handler(self.services.clone(), self.ticker.get_tick())?;

        let login_server = ShroomServer::<S::LoginHandler>::new(
            self.codec.clone(),
            login_make,
            self.ticker.get_tick(),
        );

        self.login_server = Some(Self::spawn_supervised(
            "login",
            login_server.serve_tcp(SocketAddr::new(self.cfg.listen_ip, self.cfg.login_port)),
        ));

        Ok(())
    }

    pub async fn spawn_game_servers(&mut self) -> anyhow::Result<()> {
        if !self.game_servers.is_empty() {
            anyhow::bail!("Game servers already started");
        }

        for (channel_id, port) in self.cfg.game_ports.clone().enumerate() {
            let game_make = self.handler.make_game_handler(
                self.services.clone(),
                self.ticker.get_tick(),
                channel_id,
            )?;
            let game_server = ShroomServer::<S::GameHandler>::new(
                self.codec.clone(),
                game_make,
                self.ticker.get_tick(),
            );

            self.game_servers.push(Self::spawn_supervised(
                "game",
                game_server.serve_tcp(SocketAddr::new(self.cfg.listen_ip, port)),
            ));
        }

        Ok(())
    }
}
