use indexmap::IndexMap;
use std::hash::Hash;
use std::sync::RwLock;

use crate::{EncodePacket, HasOpcode, PacketWriter, ShroomPacket};

use super::SharedSessionHandle;

#[derive(Debug)]
pub struct SessionSet<Key>(RwLock<IndexMap<Key, SharedSessionHandle>>);

impl<Key: Hash + Eq> Default for SessionSet<Key> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Key: Hash + Eq> SessionSet<Key> {
    pub fn new() -> Self {
        Self(RwLock::default())
    }

    pub fn add(&self, key: Key, session: SharedSessionHandle) {
        self.0.write().expect("Session add").insert(key, session);
    }

    pub fn remove(&self, key: Key) {
        self.0.write().expect("Session remove").remove(&key);
    }

    pub fn send_packet_to(&self, session_key: Key, pkt: ShroomPacket) -> anyhow::Result<()> {
        self.0
            .read()
            .expect("Session send to")
            .get(&session_key)
            .ok_or_else(|| anyhow::format_err!("Unable to find session"))?
            .try_send_pkt(pkt.as_ref())?;

        Ok(())
    }

    pub fn broadcast_packet(&self, pkt: ShroomPacket, src: Key) -> anyhow::Result<()> {
        for (key, sess) in self.0.read().expect("Session broadcast").iter() {
            if src == *key {
                continue;
            }
            let _ = sess.try_send_pkt(pkt.as_ref());
        }
        Ok(())
    }

    pub fn broadcast_pkt<T: EncodePacket + HasOpcode>(
        &self,
        pkt: T,
        src: Key,
    ) -> anyhow::Result<()> {
        let mut pw = PacketWriter::default();
        pw.write_opcode(T::OPCODE)?;
        pkt.encode_packet(&mut pw)?;

        self.broadcast_packet(pw.into_packet(), src)?;
        Ok(())
    }

    pub async fn send_pkt_to<T: EncodePacket + HasOpcode>(
        &self,
        session_key: Key,
        pkt: T,
    ) -> anyhow::Result<()> {
        let mut pw = PacketWriter::default();
        pw.write_opcode(T::OPCODE)?;
        pkt.encode_packet(&mut pw)?;

        self.send_packet_to(session_key, pw.into_packet())
    }
}
