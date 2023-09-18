use indexmap::IndexMap;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{tick::Tick, ClientId, ShroomSessionHandle};

#[derive(Debug)]
pub struct SessionSet<Msg>(IndexMap<ClientId, ShroomSessionHandle<Msg>>);

impl<Msg> Default for SessionSet<Msg> {
    fn default() -> Self {
        Self(IndexMap::default())
    }
}

impl<Msg> SessionSet<Msg> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, key: ClientId, session: ShroomSessionHandle<Msg>) {
        self.0.insert(key, session);
    }

    pub fn remove(&mut self, key: ClientId) {
        self.0.remove(&key);
    }

    pub fn send_to(&self, to: ClientId, msg: Msg) -> anyhow::Result<()> {
        self.0
            .get(&to)
            .ok_or_else(|| anyhow::format_err!("Unable to find session"))?
            .tx
            .try_send(msg)
            .map_err(|_| anyhow::format_err!("Unable to send message"))?;

        Ok(())
    }
}

impl<Msg> SessionSet<Msg>
where
    Msg: Clone,
{
    pub fn broadcast(&self, msg: Msg, src: Option<ClientId>) -> anyhow::Result<()> {
        for (key, sess) in self.0.iter() {
            if src == Some(*key) {
                continue;
            }
            let _ = sess.tx.clone().try_send(msg.clone());
        }
        Ok(())
    }
}

pub enum RoomMsg<SessionMsg, Msg> {
    SessionJoin {
        handle: ShroomSessionHandle<SessionMsg>,
        tx: oneshot::Sender<SessionRoomHandle<SessionMsg, Msg>>,
    },
    SessionLeave(ClientId, oneshot::Sender<()>),
    SessionForceLeave(ClientId),
    RoomMsg(Msg),
}

#[derive(Debug)]
pub struct SessionRoomHandle<SessionMsg, Msg> {
    tx: mpsc::Sender<RoomMsg<SessionMsg, Msg>>,
    id: ClientId,
    left: bool,
}

impl<SessionMsg, Msg> SessionRoomHandle<SessionMsg, Msg>
where
    SessionMsg: Send + Sync + 'static,
    Msg: Send + Sync + 'static,
{
    pub async fn send(&self, msg: Msg) -> anyhow::Result<()> {
        self.tx.send(RoomMsg::RoomMsg(msg)).await?;
        Ok(())
    }

    pub async fn leave(mut self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RoomMsg::SessionLeave(self.id, tx)).await?;
        rx.await?;
        self.left = true;
        Ok(())
    }
}

impl<SessionMsg, Msg> Drop for SessionRoomHandle<SessionMsg, Msg> {
    fn drop(&mut self) {
        if !self.left {
            let _ = self.tx.try_send(RoomMsg::SessionForceLeave(self.id));
            self.left = true;
        }
    }
}

pub trait SessionRoomState {
    type SessionMsg: Send + Sync + 'static;
    type Msg: Send + Sync + 'static;

    fn handle_msg(
        &mut self,
        sessions: &SessionSet<Self::SessionMsg>,
        msg: Self::Msg,
    ) -> anyhow::Result<()>;

    fn handle_tick(&mut self, sessions: &SessionSet<Self::SessionMsg>) -> anyhow::Result<()>;
}

pub struct SessionRoom<State: SessionRoomState> {
    kill: JoinHandle<()>,
    tx: mpsc::Sender<RoomMsg<State::SessionMsg, State::Msg>>,
}

impl<State: SessionRoomState> Drop for SessionRoom<State> {
    fn drop(&mut self) {
        self.kill.abort();
    }
}

impl<State: SessionRoomState> SessionRoom<State>
where
    State: Send + 'static,
{
    pub fn spawn(state: State, tick: Tick) -> Self {
        let (tx, rx) = mpsc::channel(128);
        let kill = tokio::spawn(Self::exec(state, tick, tx.clone(), rx));
        Self { kill, tx }
    }

    pub async fn join(
        &self,
        handle: ShroomSessionHandle<State::SessionMsg>,
    ) -> anyhow::Result<SessionRoomHandle<State::SessionMsg, State::Msg>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RoomMsg::SessionJoin { handle, tx }).await?;
        Ok(rx.await?)
    }

    pub async fn exec(
        mut state: State,
        mut tick: Tick,
        tx_room: mpsc::Sender<RoomMsg<State::SessionMsg, State::Msg>>,
        mut rx: mpsc::Receiver<RoomMsg<State::SessionMsg, State::Msg>>,
    ) {
        let mut sessions = SessionSet::new();

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(RoomMsg::SessionJoin { handle, tx }) => {
                            let id = handle.id;
                            sessions.add(handle.id, handle);
                            let _ = tx.send(SessionRoomHandle { id, tx: tx_room.clone(), left: false });
                        }
                        Some(RoomMsg::SessionLeave(id, tx)) => {
                            sessions.remove(id);
                            let _ = tx.send(());
                        }
                        Some(RoomMsg::SessionForceLeave(id)) => {
                            sessions.remove(id);
                        }
                        Some(RoomMsg::RoomMsg(msg)) => {
                            state.handle_msg(&sessions, msg).unwrap();
                        }
                        None => {
                            return;
                        }
                    }
                }
                _ = tick.next() => {
                    state.handle_tick(&sessions).unwrap();
                }
            }
        }
    }
}
