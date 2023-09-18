use indexmap::IndexMap;

use super::{ClientId, ShroomSessionHandle};

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
