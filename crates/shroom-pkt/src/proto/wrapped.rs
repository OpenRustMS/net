use bytes::BufMut;

use crate::{PacketReader, PacketResult, PacketWriter, SizeHint};

use super::{DecodePacket, EncodePacket};

/// Provide a wrapper around the `Inner` with conversion methods
/// Just implementing this wrapper Trait with an `Inner` type which already
/// implements `EncodePacket` and `DecodePacket` allows you to inherit those for the implemented type
pub trait PacketWrapped: Sized {
    type Inner;
    fn packet_into_inner(&self) -> Self::Inner;
    fn packet_from(v: Self::Inner) -> Self;
}

/// Check `PacketWrapped` but with a failable `packet_try_from` method
pub trait PacketTryWrapped: Sized {
    type Inner;
    fn packet_into_inner(&self) -> Self::Inner;
    fn packet_try_from(v: Self::Inner) -> PacketResult<Self>;
}

impl<W> EncodePacket for W
where
    W: PacketTryWrapped,
    W::Inner: EncodePacket,
{
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        self.packet_into_inner().encode_packet(pw)
    }

    const SIZE_HINT: SizeHint = W::Inner::SIZE_HINT;

    fn packet_len(&self) -> usize {
        Self::SIZE_HINT
            .0
            .unwrap_or(self.packet_into_inner().packet_len())
    }
}

impl<'de, MW> DecodePacket<'de> for MW
where
    MW: PacketTryWrapped,
    MW::Inner: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        let inner = <MW as PacketTryWrapped>::Inner::decode_packet(pr)?;
        MW::packet_try_from(inner)
    }
}

impl<W: PacketWrapped> PacketTryWrapped for W {
    type Inner = W::Inner;

    fn packet_into_inner(&self) -> Self::Inner {
        self.packet_into_inner()
    }

    fn packet_try_from(v: Self::Inner) -> PacketResult<Self> {
        Ok(<W as PacketWrapped>::packet_from(v))
    }
}
