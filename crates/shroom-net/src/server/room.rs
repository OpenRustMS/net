use std::{
    hash::Hash,
    pin::pin,
    sync::{atomic::AtomicUsize, Arc},
    task::Poll,
};

use futures::Stream;
use indexmap::IndexMap;
use pin_project::pin_project;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};

use tokio_stream::{
    wrappers::{BroadcastStream, ReceiverStream},
    StreamExt,
};

use super::tick::Tick;

// TODO: handle client being out of capacity

pub type BroadcastMsg<Key, Msg> = (Option<Key>, Msg);

/// A set of clients in a room
#[derive(Debug)]
pub struct RoomSet<Key, Msg> {
    clients: IndexMap<Key, mpsc::Sender<Msg>>,
    broadcast_tx: broadcast::Sender<(Option<Key>, Msg)>,
}

impl<Msg, Key> RoomSet<Key, Msg>
where
    Key: Hash + Eq + PartialEq + Clone,
    Msg: Clone,
{
    fn new(broadcast_tx: broadcast::Sender<BroadcastMsg<Key, Msg>>) -> Self {
        Self {
            clients: IndexMap::default(),
            broadcast_tx,
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

    /// Broadcasts a message to all clients
    pub fn broadcast(&self, msg: Msg) -> anyhow::Result<()> {
        // If it fails no clients are there to listen, which is a non-problem
        let _ = self.broadcast_tx.send((None, msg));
        Ok(())
    }

    /// Broadcasts a message to all clients except the source
    pub fn broadcast_filter(&self, msg: Msg, src: &Key) -> anyhow::Result<()> {
        // If it fails no clients are there to listen, which is a non-problem
        let _ = self.broadcast_tx.send((Some(src.clone()), msg));
        Ok(())
    }
}

/// The state of a room processes incoming messages
/// and maintains the room
pub trait RoomState: Sized {
    type Key: PartialEq + Eq + std::hash::Hash + Send + Sync + Clone + 'static;
    type ConnMsg: Send + Sync + Clone + 'static;
    type Msg: Send + Sync + 'static;
    type JoinData: Send + Sync + 'static;
    type CreateData;

    fn create(
        create_data: Self::CreateData,
        conns: RoomSet<Self::Key, Self::ConnMsg>,
    ) -> anyhow::Result<Self>;

    fn sessions(&self) -> &RoomSet<Self::Key, Self::ConnMsg>;
    fn session_mut(&mut self) -> &mut RoomSet<Self::Key, Self::ConnMsg>;

    #[allow(unused_variables)]
    fn handle_join(&mut self, src: Self::Key, data: Self::JoinData) -> anyhow::Result<()> {
        Ok(())
    }
    #[allow(unused_variables)]
    fn handle_leave(&mut self, src: Self::Key) -> anyhow::Result<()> {
        Ok(())
    }
    fn handle_msg(&mut self, src: Option<Self::Key>, msg: Self::Msg) -> anyhow::Result<()>;
    fn handle_tick(&mut self) -> anyhow::Result<()>;
}

pub enum RoomMsg<S: RoomState> {
    ConnJoin {
        id: S::Key,
        join_data: S::JoinData,
        tx_conn: mpsc::Sender<S::ConnMsg>,
        tx: oneshot::Sender<()>,
    },
    ConnLeave(S::Key, oneshot::Sender<()>),
    RoomMsg((Option<S::Key>, S::Msg)),
}

#[pin_project]
pub struct RoomStream<S: RoomState> {
    id: S::Key,
    #[pin]
    rx_conn: ReceiverStream<S::ConnMsg>,
    #[pin]
    rx_br: BroadcastStream<BroadcastMsg<S::Key, S::ConnMsg>>,
}

impl<S: RoomState> std::fmt::Debug for RoomStream<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomStream")
            .field("rx_br", &self.rx_br)
            .finish()
    }
}

impl<S: RoomState> Stream for RoomStream<S> {
    type Item = S::ConnMsg;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        //TODO maybe later allow to rewind but for now
        // we just send None if client is out of sync
        let mut me = self.project();
        use Poll::*;

        let mut done = true;

        match me.rx_conn.poll_next(cx) {
            Ready(Some(val)) => return Ready(Some(val)),
            Ready(None) => {}
            Pending => done = false,
        }

        loop {
            match me.rx_br.as_mut().poll_next(cx) {
                // only take message If it's not filtered
                Ready(Some(Ok((src, msg)))) => match src {
                    // Message is not filtered
                    None => return Ready(Some(msg)),
                    // Message is not filtered for this client
                    Some(src) if src != *me.id => return Ready(Some(msg)),
                    // Message is filtered for this client skip it
                    _ => {
                        continue;
                    }
                },
                Ready(Some(Err(_))) | Ready(None) => {
                    break;
                }
                Pending => {
                    done = false;
                    break;
                }
            }
        }

        if done {
            Ready(None)
        } else {
            Pending
        }
    }
}

#[derive(Debug)]
pub struct RoomJoinHandle<S: RoomState> {
    pub tx_room: mpsc::Sender<RoomMsg<S>>,
    pub id: S::Key,
    rx: RoomStream<S>,
    left: bool,
    force_leave_tx: mpsc::Sender<S::Key>,
}

impl<S: RoomState> RoomJoinHandle<S>
where
    S: 'static,
{
    /// Helper function which allows leaving without consuming the Handle
    async fn inner_leave(&mut self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx_room
            .send(RoomMsg::ConnLeave(self.id.clone(), tx))
            .await?;
        rx.await?;
        self.left = true;
        Ok(())
    }

    /// Sends a message to the room
    pub async fn send(&self, msg: S::Msg) -> anyhow::Result<()> {
        self.tx_room
            .send(RoomMsg::RoomMsg((Some(self.id.clone()), msg)))
            .await?;
        Ok(())
    }

    pub fn stream(&mut self) -> &mut RoomStream<S> {
        &mut self.rx
    }

    pub async fn recv(&mut self) -> anyhow::Result<S::ConnMsg> {
        self.rx
            .next()
            .await
            .ok_or_else(|| anyhow::format_err!("Room closed"))
    }

    /// Allows to switch this handle to another room
    /// by leaving the old room first and then joining the new room
    pub async fn switch_to(
        &mut self,
        other_room: &Room<S>,
        join_data: S::JoinData,
    ) -> anyhow::Result<()>
    where
        S: Send,
    {
        // Leave the room
        self.inner_leave().await?;
        // Attempt to get the new room handle
        let handle = other_room
            .join_with_channel(self.id.clone(), join_data)
            .await?;
        let _ = std::mem::replace(self, handle);
        Ok(())
    }

    /// Consumes the handle and leaves the room
    pub async fn leave(mut self) -> anyhow::Result<()> {
        self.inner_leave().await?;
        Ok(())
    }
}

/// Last resort option to leave the room, when the handle is dropped
impl<S: RoomState> Drop for RoomJoinHandle<S> {
    fn drop(&mut self) {
        if !self.left {
            self.force_leave_tx
                .try_send(self.id.clone())
                .expect("force leave");
        }
    }
}

#[derive(Debug)]
pub struct Room<S: RoomState> {
    kill: JoinHandle<anyhow::Result<()>>,
    tx: mpsc::Sender<RoomMsg<S>>,
    broadcast_tx: broadcast::Sender<BroadcastMsg<S::Key, S::ConnMsg>>,
    session_count: Arc<AtomicUsize>,
    force_leave_tx: mpsc::Sender<S::Key>,
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
    pub fn conn_count(&self) -> usize {
        self.session_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Spawns this room, returning a handle to this room
    pub fn spawn(
        create: S::CreateData,
        tick: Tick,
        room_cap: usize,
        broadcast_cap: usize,
    ) -> anyhow::Result<Self> {
        let (br_tx, _) = broadcast::channel(broadcast_cap);
        let (tx, rx) = mpsc::channel(room_cap);
        let (force_leave_tx, force_leave_rx) = mpsc::channel(128);

        let room_set = RoomSet::new(br_tx.clone());
        let state = S::create(create, room_set)?;
        let session_count = Arc::new(AtomicUsize::new(0));
        let kill = tokio::spawn(Self::exec(
            state,
            tick,
            rx,
            force_leave_rx,
            session_count.clone(),
        ));
        Ok(Self {
            kill,
            tx,
            session_count,
            force_leave_tx,
            broadcast_tx: br_tx,
        })
    }

    /// Joins the room with the given sender
    pub async fn join(
        &self,
        id: S::Key,
        join_data: S::JoinData,
        tx_conn: mpsc::Sender<S::ConnMsg>,
        rx_conn: mpsc::Receiver<S::ConnMsg>,
    ) -> anyhow::Result<RoomJoinHandle<S>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RoomMsg::ConnJoin {
                tx,
                tx_conn,
                join_data,
                id: id.clone(),
            })
            .await?;

        rx.await?;

        let rx_broadcast = self.broadcast_tx.subscribe();

        Ok(RoomJoinHandle {
            id: id.clone(),
            tx_room: self.tx.clone(),
            rx: RoomStream {
                id,
                rx_conn: ReceiverStream::new(rx_conn),
                rx_br: BroadcastStream::new(rx_broadcast),
            },
            left: false,
            force_leave_tx: self.force_leave_tx.clone(),
        })
    }

    /// Joins the room and creates a new channel to receive the messages
    pub async fn join_with_channel(
        &self,
        id: S::Key,
        join_data: S::JoinData,
    ) -> anyhow::Result<RoomJoinHandle<S>> {
        let (tx, rx) = mpsc::channel(32);
        self.join(id, join_data, tx, rx).await
    }

    /// Internal execution loop for this room
    async fn exec(
        mut state: S,
        mut tick: Tick,
        mut rx: mpsc::Receiver<RoomMsg<S>>,
        mut force_leave_rx: mpsc::Receiver<S::Key>,
        session_count: Arc<AtomicUsize>,
    ) -> anyhow::Result<()> {
        loop {
            let sessions = state.session_mut();

            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(RoomMsg::ConnJoin { id, join_data, tx_conn, tx }) => {
                            sessions.add(id.clone(), tx_conn);
                            let _ = tx.send(());
                            session_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            state.handle_join(id, join_data)?;
                        }
                        Some(RoomMsg::ConnLeave(id, tx)) => {
                            sessions.remove(&id);
                            let _ = tx.send(());
                            session_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                            state.handle_leave(id)?;
                        }
                        Some(RoomMsg::RoomMsg((src, msg))) => {
                            state.handle_msg(src, msg)?;
                        }
                        None => {
                            return Ok(());
                        }
                    }
                }
                _ = tick.next() => {
                    // Clean crashes sessions
                    while let Ok(id) = force_leave_rx.try_recv() {
                        state.session_mut().remove(&id);
                        session_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        state.handle_leave(id)?;
                    }
                    state.handle_tick()?;
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
        AddBroad(u32),
    }

    #[derive(Debug)]
    pub struct RoomState {
        v: u32,
        conns: RoomSet<ClientId, u32>,
    }

    impl super::RoomState for RoomState {
        type Key = ClientId;
        type ConnMsg = u32;
        type Msg = RoomMsg;
        type JoinData = ();
        type CreateData = u32;

        fn handle_msg(&mut self, src: Option<Self::Key>, msg: Self::Msg) -> anyhow::Result<()> {
            match msg {
                RoomMsg::Add(v) => self.v += v,
                RoomMsg::Sub(v) => self.v -= v,
                RoomMsg::AddBroad(v) => {
                    self.v += v;
                    self.conns.broadcast_filter(self.v, &src.unwrap())?;
                    return Ok(());
                }
            };

            self.conns.broadcast(self.v)?;
            Ok(())
        }

        fn handle_tick(&mut self) -> anyhow::Result<()> {
            self.v += 1;
            self.conns.broadcast(self.v)?;
            Ok(())
        }

        fn sessions(&self) -> &RoomSet<Self::Key, Self::ConnMsg> {
            &self.conns
        }

        fn session_mut(&mut self) -> &mut RoomSet<Self::Key, Self::ConnMsg> {
            &mut self.conns
        }

        fn create(
            create_data: Self::CreateData,
            conns: RoomSet<Self::Key, Self::ConnMsg>,
        ) -> anyhow::Result<Self> {
            Ok(Self {
                v: create_data,
                conns,
            })
        }
    }

    #[tokio::test]
    async fn simple_room() {
        let ticker = Ticker::spawn_from_millis(10);
        let room = Room::<RoomState>::spawn(0, ticker.get_tick(), 16, 16).unwrap();
        assert_eq!(room.conn_count(), 0);

        let mut r = room.join_with_channel(1, ()).await.unwrap();
        assert_eq!(room.conn_count(), 1);

        r.send(RoomMsg::Add(10)).await.unwrap();
        assert_eq!(r.recv().await.unwrap(), 10);

        r.send(RoomMsg::Sub(5)).await.unwrap();
        assert_eq!(r.recv().await.unwrap(), 5);

        // we should not recv this message
        r.send(RoomMsg::AddBroad(5)).await.unwrap();
        r.send(RoomMsg::Sub(5)).await.unwrap();
        assert_eq!(r.recv().await.unwrap(), 5);

        ticker.get_tick().next().await;
        assert_eq!(r.recv().await.unwrap(), 6);

        r.leave().await.unwrap();
        assert_eq!(room.conn_count(), 0);
    }

    #[tokio::test]
    async fn drop_leave() {
        let ticker = Ticker::spawn_from_millis(10);
        let room = Room::<RoomState>::spawn(0, ticker.get_tick(), 16, 16).unwrap();
        assert_eq!(room.conn_count(), 0);

        let r = room.join_with_channel(1, ()).await.unwrap();
        assert_eq!(room.conn_count(), 1);

        std::mem::drop(r);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(room.conn_count(), 0);
    }
}
