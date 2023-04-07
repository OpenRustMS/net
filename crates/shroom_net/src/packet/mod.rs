pub mod analyzer;
pub mod proto;
pub mod reader;
pub mod writer;

use crate::NetResult;
use bytes::{Bytes, BytesMut};

pub use reader::PacketReader;
pub use writer::PacketWriter;

// Re-export proto
pub use proto::*;


/// Decode a `u128` from the given byte array
pub(crate) fn shroom128_from_bytes(data: [u8; 16]) -> u128 {
    let mut data: [u32; 4] = bytemuck::cast(data);
    data.reverse();
    u128::from_le_bytes(bytemuck::cast(data))
}

/// Encode a `u128` into a byte array
pub(crate) fn shroom128_to_bytes(v: u128) -> [u8; 16] { 
    let mut raw_flags: [u32; 4] = bytemuck::cast(v.to_le_bytes());
    raw_flags.reverse();
    bytemuck::cast(raw_flags)
}


pub(crate) fn packet_str_len(s: &str) -> usize {
    // Len + data
    2 + s.len()
}

#[derive(Clone, Default, Debug)]
pub struct ShroomPacket {
    pub data: Bytes,
}

impl ShroomPacket {
    pub fn from_data(data: Bytes) -> Self {
        Self { data }
    }

    pub fn from_writer(pw: PacketWriter<BytesMut>) -> Self {
        Self::from_data(pw.buf.freeze())
    }

    pub fn into_reader(&self) -> PacketReader<'_> {
        PacketReader::new(&self.data)
    }

    pub fn read_opcode(&self) -> NetResult<u16> {
        self.into_reader().read_u16()
    }
}
