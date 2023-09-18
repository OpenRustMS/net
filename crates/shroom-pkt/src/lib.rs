pub mod analyzer;
pub mod error;
pub mod opcode;
pub mod proto;
pub mod reader;
pub mod test_util;
pub mod util;
pub mod writer;

pub use error::Error;
pub use util::SizeHint;

pub type PacketResult<T> = Result<T, error::Error>;

use bytes::{Bytes, BytesMut};

/// Export the reader and writer here
pub use reader::PacketReader;
pub use writer::PacketWriter;

// Re-export proto
pub use proto::*;

pub use opcode::*;

/// Decode a `u128` from the given byte array
pub(crate) fn shroom128_from_bytes(data: [u8; 16]) -> u128 {
    // u128 are stored as 4 u32 little endian encoded blocks
    // but the blocks are itself in LE order aswell
    // so we have to reverse it
    let mut data: [u32; 4] = bytemuck::cast(data);
    data.reverse();
    u128::from_le_bytes(bytemuck::cast(data))
}

/// Encode a `u128` into a byte array
pub(crate) fn shroom128_to_bytes(v: u128) -> [u8; 16] {
    let mut blocks: [u32; 4] = bytemuck::cast(v.to_le_bytes());
    blocks.reverse();
    bytemuck::cast(blocks)
}

/// Required length to encode this string
pub(crate) fn packet_str_len(s: &str) -> usize {
    // len(u16) + data
    2 + s.len()
}

#[derive(Clone, Default, Debug)]
pub struct ShroomPacketData(Bytes);

impl ShroomPacketData {
    pub fn from_data(data: Bytes) -> Self {
        Self(data)
    }

    pub fn from_writer(pw: PacketWriter<BytesMut>) -> Self {
        Self::from_data(pw.buf.freeze())
    }

    pub fn into_reader(&self) -> PacketReader<'_> {
        PacketReader::new(&self.0)
    }

    pub fn read_opcode(&self) -> PacketResult<u16> {
        self.into_reader().read_u16()
    }
}

impl AsRef<Bytes> for ShroomPacketData {
    fn as_ref(&self) -> &Bytes {
        &self.0
    }
}

pub use shroom_pkt_derive::*;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_shroom128(v: u128) {
            let bytes = shroom128_to_bytes(v);
            let v2 = shroom128_from_bytes(bytes);
            assert_eq!(v, v2);
        }
    }
}
