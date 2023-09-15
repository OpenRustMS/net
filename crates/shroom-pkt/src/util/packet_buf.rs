use itertools::Itertools;
use std::iter;

use crate::{opcode::HasOpcode, EncodePacket, PacketResult, PacketWriter};

/// Buffer to allow to encode multiple packets onto one buffer
/// while still allowing to iterate over the encoded packets
#[derive(Debug, Default)]
pub struct PacketBuf {
    buf: Vec<u8>,
    ix: Vec<usize>,
}

impl PacketBuf {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            ix: Vec::new(),
        }
    }

    /// Encode a packet onto the buffer
    pub fn encode_packet<T: EncodePacket + HasOpcode>(&mut self, pkt: T) -> PacketResult<()> {
        // Store the previous index
        let ix = self.buf.len();
        let mut pw = PacketWriter::new(&mut self.buf);

        // If an error occurs reset the index
        if let Err(err) = pw.write_opcode(T::OPCODE) {
            self.buf.truncate(ix);
            return Err(err);
        }

        // If an error occurs reset the index
        if let Err(err) = pkt.encode_packet(&mut pw) {
            self.buf.truncate(ix);
            return Err(err);
        }

        // Store the ix of the current packet
        self.ix.push(ix);
        Ok(())
    }

    /// Iterator over the written packet frames
    pub fn packets(&self) -> impl Iterator<Item = &[u8]> + '_ {
        self.ix
            .iter()
            .cloned()
            .chain(iter::once(self.buf.len()))
            .tuple_windows()
            .map(|(l, r)| &self.buf[l..r])
    }

    /// Clears the buffer
    pub fn clear(&mut self) {
        self.buf.truncate(0);
        self.ix.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::opcode::WithOpcode;

    use super::PacketBuf;

    #[test]
    fn packet_buf() -> anyhow::Result<()> {
        let mut buf = PacketBuf::default();
        buf.encode_packet(WithOpcode::<1, u8>(1))?;
        buf.encode_packet(WithOpcode::<1, u8>(2))?;
        buf.encode_packet(WithOpcode::<1, u8>(3))?;

        itertools::assert_equal(buf.packets(), [[1, 0, 1], [1, 0, 2], [1, 0, 3]]);

        buf.clear();

        assert_eq!(buf.packets().count(), 0);
        buf.encode_packet(WithOpcode::<1, u8>(1))?;
        itertools::assert_equal(buf.packets(), [[1, 0, 1]]);

        Ok(())
    }
}
