pub mod packet_data_context;
pub mod proto;
pub mod reader;
pub mod writer;

use crate::NetResult;
use bytes::{Bytes, BytesMut};

/// Export the reader and writer here
pub use reader::PacketReader;
pub use writer::PacketWriter;

// Re-export proto
pub use proto::*;

/// Decode a `u128` from the given byte array
pub(crate) fn shroom128_from_bytes(data: [u8; 16]) -> u128 {
    // u128 are actually somewhat weird, because they are little endian u32 blocks,
    // but the blocks are ordered in reversed order
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
pub struct ShroomPacket(Bytes);

impl ShroomPacket {
    pub fn from_data(data: Bytes) -> Self {
        Self(data)
    }

    pub fn from_writer(pw: PacketWriter<BytesMut>) -> Self {
        Self::from_data(pw.buf.freeze())
    }

    pub fn into_reader(&self) -> PacketReader<'_> {
        PacketReader::new(&self.0)
    }

    pub fn read_opcode(&self) -> NetResult<u16> {
        self.into_reader().read_u16()
    }
}

impl AsRef<Bytes> for ShroomPacket {
    fn as_ref(&self) -> &Bytes {
        &self.0
    }
}

pub mod test_util {
    use bytes::BytesMut;

    use crate::{EncodePacket, DecodePacket, PacketWriter};

    use super::DecodePacketOwned;

    pub fn test_encode_decode_owned<T>(data: T)
    where
        T: EncodePacket + DecodePacketOwned + PartialEq + std::fmt::Debug,
    {
        let mut pw = PacketWriter::new(BytesMut::new());
        data.encode_packet(&mut pw).expect("must encode");

        let inner = pw.into_inner();
        let cmp = T::decode_from_data_complete(&inner).expect("must decode complete");
        assert_eq!(data, cmp);
    }

    pub fn test_encode_decode_owned_all<T>(data: impl IntoIterator<Item = T>)
    where
        T: EncodePacket + DecodePacketOwned + PartialEq + std::fmt::Debug,
    {
        for v in data {
            test_encode_decode_owned(v);
        }
    }

    pub fn test_encode_decode<'de, T>(data: T, buf: &'de mut BytesMut)
    where
        T: EncodePacket + DecodePacket<'de> + PartialEq + std::fmt::Debug,
    {
        let mut pw = PacketWriter::new(buf);
        data.encode_packet(&mut pw).expect("must encode");

        let inner = pw.into_inner();
        let cmp = T::decode_from_data_complete(inner).expect("must decode complete");
        assert_eq!(data, cmp);
    }

    #[macro_export]
    macro_rules! test_encode_decode {
        ($d:expr) => {
            let mut data = bytes::BytesMut::new();
            $crate::packet::test_util::test_encode_decode($d, &mut data);
        };
        ($($d:expr),*) => {
            $(
                $crate::test_encode_decode!($d);
            )*
        }
    }
}
