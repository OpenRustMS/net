use crate::{error::NetError, DecodePacket, EncodePacket, NetResult, SizeHint};

/// Opcode trait which allows conversion from and to the opcode from an `u16`
pub trait NetOpcode: TryFrom<u16> + Into<u16> + Copy + Clone + Send + Sync {
    /// Parses the opcode from an u16
    fn get_opcode(v: u16) -> NetResult<Self> {
        Self::try_from(v).map_err(|_| NetError::InvalidOpcode(v))
    }
}

/// Blanket implementation for u16
impl NetOpcode for u16 {}

/// Adds an opcode to the type by implementing this trait
pub trait HasOpcode {
    /// Opcode type
    type Opcode: NetOpcode;

    /// Opcode value
    const OPCODE: Self::Opcode;
}

/// Helper type to augment any `EncodePacket` + `DecodePacket` with a `HasOpcode` trait
#[derive(Debug, Default)]
pub struct WithOpcode<const OP: u16, T>(pub T);
impl<const OP: u16, T> HasOpcode for WithOpcode<OP, T> {
    type Opcode = u16;

    const OPCODE: Self::Opcode = OP;
}

impl<const OP: u16, T> EncodePacket for WithOpcode<OP, T>
where
    T: EncodePacket,
{
    const SIZE_HINT: SizeHint = T::SIZE_HINT;

    fn packet_len(&self) -> usize {
        self.0.packet_len()
    }

    fn encode_packet<B: bytes::BufMut>(&self, pw: &mut crate::PacketWriter<B>) -> NetResult<()> {
        self.0.encode_packet(pw)
    }
}

impl<'de, const OP: u16, T> DecodePacket<'de> for WithOpcode<OP, T>
where
    T: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut crate::PacketReader<'de>) -> NetResult<Self> {
        Ok(Self(T::decode_packet(pr)?))
    }
}

/// Helper macro to add an Opcode to a `packet_ty` easily
#[macro_export]
macro_rules! packet_opcode {
    ($packet_ty:ty, $op:path, $ty:ty) => {
        impl $crate::HasOpcode for $packet_ty {
            type Opcode = $ty;

            const OPCODE: Self::Opcode = $op;
        }
    };
    ($packet_ty:ty, $ty:ident::$op:ident) => {
        impl $crate::HasOpcode for $packet_ty {
            type Opcode = $ty;

            const OPCODE: Self::Opcode = $ty::$op;
        }
    };
}
