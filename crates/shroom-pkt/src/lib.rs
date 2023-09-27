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
