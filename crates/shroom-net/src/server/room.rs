use std::{
    hash::Hash,
    sync::{atomic::AtomicUsize, Arc},
};

use indexmap::IndexMap;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::tick::Tick;

/*  TODO:
    - Session force leave(Drop) must always send a message without blocking
    - Handle client being out of capacity(kick the client from the field)
*/

/// A set of clients in a room
#[derive(Debug)]
pub struct RoomSet<Key, Msg> {
    clients: IndexMap<Key, mpsc::Sender<Msg>>,
}

impl<Key, Msg> Default for RoomSet<Key, Msg>
where
    Key: Hash + Eq + PartialEq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Msg, Key> RoomSet<Key, Msg>
where
    Key: Hash + Eq + PartialEq,
{
    /// Creates a new roomset
    pub fn new() -> Self {
        Self {
            clients: IndexMap::new(),
        }
    }

    /// Adds a new client
    pub fn add(&mut self, key: Key, tx: mpsc::Sender<Msg>) {
        self.clients.insert(key, tx);
    }

    /// Removes a client with the given client id
    pub fn remove(&mut self, key: &Key) {
        self.clients.remove(key);
    }

    /// Send a message to a specific client by their id
    pub fn send_to(&self, to: &Key, msg: Msg) -> anyhow::Result<()> {
        self.get(to)?
            .try_send(msg)
            .map_err(|_| anyhow::format_err!("Unable to send message"))?;

        Ok(())
    }

    /// Gets a session handle by key
    pub fn get(&self, key: &Key) -> anyhow::Result<&mpsc::Sender<Msg>> {
        self.clients
            .get(key)
            .ok_or_else(|| anyhow::format_err!("Unable to find session"))
    }
}

impl<Key, Msg> RoomSet<Key, Msg>
where
    Key: Hash + Eq + PartialEq,
    Msg: Clone,
{
    /// Broadcasts a message to all clients
    pub fn broadcast(&self, msg: Msg) -> anyhow::Result<()> {
        for sess in self.clients.values() {
            let _ = sess.try_send(msg.clone());
        }
        Ok(())
    }

    /// Broadcasts a message to all clients except the source
    pub fn broadcast_filter(&self, msg: Msg, source: &Key) -> anyhow::Result<()> {
        for (key, sess) in self.clients.iter() {
            if source == key {
                continue;
            }
            let _ = sess.try_send(msg.clone());
        }
        Ok(())
    }
}

/// The state of a room processes incoming messages
/// and maintains the room
pub trait RoomState {
    type Key: PartialEq + Eq + std::hash::Hash + Send + Sync + Clone + 'static;
    type SessionMsg: Send + Sync + 'static;
    type Msg: Send + Sync + 'static;
    type JoinData: Send + Sync + 'static;

    fn sessions(&self) -> &RoomSet<Self::Key, Self::SessionMsg>;
    fn session_mut(&mut self) -> &mut RoomSet<Self::Key, Self::SessionMsg>;

    #[allow(unused_variables)]
    fn handle_join(&mut self, id: Self::Key, data: Self::JoinData) -> anyhow::Result<()> {
        Ok(())
    }
    #[allow(unused_variables)]
    fn handle_leave(&mut self, id: Self::Key) -> anyhow::Result<()> {
        Ok(())
    }
    fn handle_msg(&mut self, msg: Self::Msg) -> anyhow::Result<()>;
    fn handle_tick(&mut self) -> anyhow::Result<()>;
}

pub enum RoomMsg<S: RoomState> {
    SessionJoin {
        id: S::Key,
        join_data: S::JoinData,
        tx_session: mpsc::Sender<S::SessionMsg>,
        tx: oneshot::Sender<()>,
    },
    SessionLeave(S::Key, oneshot::Sender<()>),
    SessionForceLeave(S::Key),
    RoomMsg(S::Msg),
}

#[derive(Debug)]
pub struct RoomJoinHandle<S: RoomState> {
    tx_room: mpsc::Sender<RoomMsg<S>>,
    id: S::Key,
    left: bool,
}

impl<S: RoomState> RoomJoinHandle<S>
where
    S: 'static,
{
    /// Sends a message to the room
    pub async fn send(&self, msg: S::Msg) -> anyhow::Result<()> {
        self.tx_room.send(RoomMsg::RoomMsg(msg)).await?;
        Ok(())
    }

    /// Consumes the handle and leaves the room
    pub async fn leave(mut self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx_room
            .send(RoomMsg::SessionLeave(self.id.clone(), tx))
            .await?;
        rx.await?;
        self.left = true;
        Ok(())
    }
}

/// Last resort option to leave the room, when the handle is dropped
impl<S: RoomState> Drop for RoomJoinHandle<S> {
    fn drop(&mut self) {
        if !self.left {
            //TODO see note at the top
            let _ = self
                .tx_room
                .try_send(RoomMsg::SessionForceLeave(self.id.clone()));
            self.left = true;
        }
    }
}

#[derive(Debug)]
pub struct Room<S: RoomState> {
    kill: JoinHandle<()>,
    tx: mpsc::Sender<RoomMsg<S>>,
    session_count: Arc<AtomicUsize>,
}

impl<State: RoomState> Drop for Room<State> {
    fn drop(&mut self) {
        self.kill.abort();
    }
}

impl<S: RoomState> Room<S>
where
    S: Send + 'static,
{
    /// Gets the current session count in this room
    pub fn session_count(&self) -> usize {
        self.session_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Spawns this room, returning a handle to this room
    pub fn spawn(state: S, tick: Tick) -> Self {
        let (tx, rx) = mpsc::channel(128);
        let session_count = Arc::new(AtomicUsize::new(0));
        let kill = tokio::spawn(Self::exec(state, tick, rx, session_count.clone()));
        Self {
            kill,
            tx,
            session_count,
        }
    }

    /// Joins the room with the given sender
    pub async fn join(
        &self,
        id: S::Key,
        join_data: S::JoinData,
        tx_session: mpsc::Sender<S::SessionMsg>,
    ) -> anyhow::Result<RoomJoinHandle<S>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RoomMsg::SessionJoin {
                tx,
                tx_session,
                join_data,
                id: id.clone(),
            })
            .await?;

        rx.await?;

        Ok(RoomJoinHandle {
            tx_room: self.tx.clone(),
            id,
            left: false,
        })
    }

    /// Joins the room and creates a new channel to receive the messages
    pub async fn join_with_channel(
        &self,
        id: S::Key,
        join_data: S::JoinData,
    ) -> anyhow::Result<(RoomJoinHandle<S>, mpsc::Receiver<S::SessionMsg>)> {
        let (tx, rx) = mpsc::channel(16);
        Ok((self.join(id, join_data, tx).await?, rx))
    }

    /// Internal execution loop for this room
    async fn exec(
        mut state: S,
        mut tick: Tick,
        mut rx: mpsc::Receiver<RoomMsg<S>>,
        session_count: Arc<AtomicUsize>,
    ) {
        loop {
            let sessions = state.session_mut();
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(RoomMsg::SessionJoin { id, join_data, tx_session, tx }) => {
                            sessions.add(id.clone(), tx_session);
                            let _ = tx.send(());
                            session_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            state.handle_join(id, join_data).unwrap();
                        }
                        Some(RoomMsg::SessionLeave(id, tx)) => {
                            sessions.remove(&id);
                            let _ = tx.send(());
                            session_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Some(RoomMsg::SessionForceLeave(id)) => {
                            sessions.remove(&id);
                            session_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Some(RoomMsg::RoomMsg(msg)) => {
                            state.handle_msg(msg).unwrap();
                        }
                        None => {
                            return;
                        }
                    }
                }
                _ = tick.next() => {
                    state.handle_tick().unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::server::{tick::Ticker, ClientId};

    use super::*;

    #[derive(Clone, Debug)]
    pub enum RoomMsg {
        Add(u32),
        Sub(u32),
    }

    #[derive(Default, Debug)]
    pub struct RoomState {
        v: u32,
        sessions: RoomSet<ClientId, u32>,
    }

    impl super::RoomState for RoomState {
        type Key = ClientId;
        type SessionMsg = u32;
        type Msg = RoomMsg;
        type JoinData = ();

        fn handle_msg(&mut self, msg: Self::Msg) -> anyhow::Result<()> {
            match msg {
                RoomMsg::Add(v) => self.v += v,
                RoomMsg::Sub(v) => self.v -= v,
            };

            self.sessions.broadcast(self.v)?;
            Ok(())
        }

        fn handle_tick(&mut self) -> anyhow::Result<()> {
            self.v += 1;
            self.sessions.broadcast(self.v)?;
            Ok(())
        }

        fn sessions(&self) -> &RoomSet<Self::Key, Self::SessionMsg> {
            &self.sessions
        }

        fn session_mut(&mut self) -> &mut RoomSet<Self::Key, Self::SessionMsg> {
            &mut self.sessions
        }
    }

    #[tokio::test]
    async fn simple_room() {
        let ticker = Ticker::spawn_from_millis(10);
        let room = Room::spawn(RoomState::default(), ticker.get_tick());
        assert_eq!(room.session_count(), 0);

        let (r, mut rx) = room.join_with_channel(1, ()).await.unwrap();
        assert_eq!(room.session_count(), 1);

        r.send(RoomMsg::Add(10)).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), 10);

        r.send(RoomMsg::Sub(5)).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), 5);

        ticker.get_tick().next().await;
        assert_eq!(rx.recv().await.unwrap(), 6);

        r.leave().await.unwrap();
        assert_eq!(room.session_count(), 0);
    }

    #[tokio::test]
    async fn drop_leave() {
        let ticker = Ticker::spawn_from_millis(10);
        let room = Room::spawn(RoomState::default(), ticker.get_tick());
        assert_eq!(room.session_count(), 0);

        let (r, _) = room.join_with_channel(1, ()).await.unwrap();
        assert_eq!(room.session_count(), 1);

        std::mem::drop(r);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(room.session_count(), 0);
    }
}
