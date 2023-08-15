pub mod handshake;
pub mod packet_codec;

pub use handshake::Handshake;
pub use packet_codec::PacketCodec;

use crate::{NetError, ShroomPacket};

use tokio_util::codec::{Decoder, Encoder};

/// Codec trait
pub trait ShroomCodec {
    type Encoder: for<'a> Encoder<&'a [u8], Error = NetError>;
    type Decoder: Decoder<Item = ShroomPacket, Error = NetError>;
}


pub const MAX_HANDSHAKE_LEN: usize = 24;
pub const MAX_PACKET_LEN: usize = i16::MAX as usize;
