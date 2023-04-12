pub mod error;
pub mod net;
pub mod opcode;
pub mod packet;
pub mod util;
pub mod crypto;

pub use error::NetError;
pub use opcode::{HasOpcode, NetOpcode};
pub use packet::{DecodePacket, EncodePacket, PacketReader, PacketWriter, ShroomPacket};
pub use util::SizeHint;
pub use util::filetime::FileTime;
pub use util::packet_buffer::PacketBuffer;
pub type NetResult<T> = Result<T, error::NetError>;
