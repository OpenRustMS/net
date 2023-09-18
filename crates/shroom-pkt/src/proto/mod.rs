pub mod bits;
pub mod conditional;
pub mod list;
pub mod option;
pub mod padding;
pub mod partial;
pub mod primitive;
pub mod shroom_enum;
pub mod string;
pub mod time;
pub mod twod;
pub mod wrapped;

use bytes::BufMut;

pub use bits::{ShroomBitFlags, ShroomPackedStruct};

pub use conditional::{CondEither, CondOption, PacketConditional};
pub use list::{
    ShroomIndexList, ShroomIndexList16, ShroomIndexList32, ShroomIndexList64, ShroomIndexList8,
    ShroomIndexListZ, ShroomIndexListZ16, ShroomIndexListZ32, ShroomIndexListZ64,
    ShroomIndexListZ8, ShroomList, ShroomList16, ShroomList32, ShroomList64, ShroomList8,
};
pub use option::{
    ShroomOption, ShroomOption8, ShroomOptionBool, ShroomOptionR8, ShroomOptionRBool,
};
pub use padding::Padding;
pub use time::{ShroomDurationMs16, ShroomDurationMs32, ShroomExpirationTime, ShroomTime};
pub use wrapped::{PacketTryWrapped, PacketWrapped};

use crate::{PacketReader, PacketResult, PacketWriter, ShroomPacketData, SizeHint};

/// Decodes this type from a packet reader
pub trait DecodePacket<'de>: Sized {
    /// Decodes the packet
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self>;

    /// Decodes the packet n times
    fn decode_packet_n(pr: &mut PacketReader<'de>, n: usize) -> PacketResult<Vec<Self>> {
        (0..n)
            .map(|_| Self::decode_packet(pr))
            .collect::<PacketResult<_>>()
    }

    /// Attempts to decode the packet
    /// If EOF is reached None is returned elsewise the Error is returned
    /// This is useful for reading an optional tail
    fn try_decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Option<Self>> {
        let mut sub_reader = pr.sub_reader();
        Ok(match Self::decode_packet(&mut sub_reader) {
            Ok(item) => {
                pr.commit_sub_reader(sub_reader)?;
                Some(item)
            }
            Err(crate::Error::EOF { .. }) => None,
            Err(err) => return Err(err),
        })
    }

    /// Decodes from the given byte slice and ensures
    /// every byte was read
    fn decode_complete(pr: &mut PacketReader<'de>) -> anyhow::Result<Self> {
        let res = Self::decode_packet(pr)?;
        if !pr.remaining_slice().is_empty() {
            anyhow::bail!("Still remaining data: {:?}", pr.remaining_slice());
        }
        Ok(res)
    }
}

/// Encodes this type on a packet writer
pub trait EncodePacket: Sized {
    /// Size Hint for types with a known type at compile time
    const SIZE_HINT: SizeHint;

    /// Get the encoded length of this type
    fn packet_len(&self) -> usize;

    /// Encodes this packet
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()>;

    /// Encodes n packets
    fn encode_packet_n<T: BufMut>(items: &[Self], pw: &mut PacketWriter<T>) -> PacketResult<()> {
        for item in items.iter() {
            item.encode_packet(pw)?;
        }

        Ok(())
    }

    /// Encodes the type on a writer and returns the data
    fn to_data(&self) -> PacketResult<bytes::Bytes> {
        let mut pw = PacketWriter::default();
        self.encode_packet(&mut pw)?;
        Ok(pw.into_inner().freeze())
    }

    /// Encodes this type as a packet
    fn to_packet(&self) -> PacketResult<ShroomPacketData> {
        Ok(ShroomPacketData::from_data(self.to_data()?))
    }
}

/// Decodes a container with the given size
pub trait DecodePacketSized<'de, T>: Sized {
    fn decode_packet_sized(pr: &mut PacketReader<'de>, size: usize) -> PacketResult<Self>;
}

impl<'de, T> DecodePacketSized<'de, T> for Vec<T>
where
    T: DecodePacket<'de>,
{
    fn decode_packet_sized(pr: &mut PacketReader<'de>, size: usize) -> PacketResult<Self> {
        T::decode_packet_n(pr, size)
    }
}

/// Helper trait to remove the lifetime from types without one
pub trait DecodePacketOwned: for<'de> DecodePacket<'de> {}
impl<T> DecodePacketOwned for T where T: for<'de> DecodePacket<'de> {}

/// Tuple support helper
macro_rules! impl_packet {
    // List of idents splitted by names or well tuple types here
    ( $($name:ident)* ) => {
        // Expand tuples and add a generic bound
        impl<$($name,)*> $crate::EncodePacket for ($($name,)*)
            where $($name: $crate::EncodePacket,)* {
                fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()> {
                    #[allow(non_snake_case)]
                    let ($($name,)*) = self;
                    $($name.encode_packet(pw)?;)*
                    Ok(())
                }

                const SIZE_HINT: $crate::SizeHint = $crate::util::SizeHint::ZERO
                        $(.add($name::SIZE_HINT))*;

                fn packet_len(&self) -> usize {
                    #[allow(non_snake_case)]
                    let ($($name,)*) = self;

                    $($name.packet_len() +)*0
                }
            }


            impl<'de, $($name,)*> $crate::DecodePacket<'de> for ($($name,)*)
            where $($name: $crate::DecodePacket<'de>,)* {
                fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
                    Ok((
                        ($($name::decode_packet(pr)?,)*)
                    ))
                }
            }
    }
}

// Implement the tuples here
macro_rules! impl_for_tuples {
    ($apply_macro:ident) => {
        $apply_macro! { A }
        $apply_macro! { A B }
        $apply_macro! { A B C }
        $apply_macro! { A B C D }
        $apply_macro! { A B C D E }
        $apply_macro! { A B C D E F }
        $apply_macro! { A B C D E F G }
        $apply_macro! { A B C D E F G H }
        $apply_macro! { A B C D E F G H I }
        $apply_macro! { A B C D E F G H I J }
        $apply_macro! { A B C D E F G H I J K }
        $apply_macro! { A B C D E F G H I J K L }
    };
}

impl_for_tuples!(impl_packet);

#[cfg(test)]
mod tests {
    use crate::EncodePacket;

    #[test]
    fn tuple_size() {
        assert_eq!(<((), (),)>::SIZE_HINT.0, Some(0));
        assert_eq!(<((), u32,)>::SIZE_HINT.0, Some(4));
        assert_eq!(<((), u32, String)>::SIZE_HINT.0, None);
    }
}