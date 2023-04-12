pub mod bits;
pub mod conditional;
pub mod geo;
pub mod list;
pub mod option;
pub mod padding;
pub mod partial;
pub mod primitive;
pub mod shroom_enum;
pub mod string;
pub mod time;
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

use crate::{NetResult, PacketReader, PacketWriter, ShroomPacket};

/// Decodes this type from a packet reader
pub trait DecodePacket<'de>: Sized {
    /// Decodes from the given reader
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self>;

    fn decode_packet_n(pr: &mut PacketReader<'de>, n: usize) -> NetResult<Vec<Self>> {
        (0..n)
            .map(|_| Self::decode_packet(pr))
            .collect::<NetResult<_>>()
    }

    /// Attempts to decode the packet
    /// If EOF is reached None is returned elsewise the Error is returned
    /// This is useful for reading an optional tail
    fn try_decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Option<Self>> {
        let mut sub_reader = pr.sub_reader();
        Ok(match Self::decode_packet(&mut sub_reader) {
            Ok(item) => {
                pr.commit_sub_reader(sub_reader)?;
                Some(item)
            }
            Err(crate::NetError::EOF { .. }) => None,
            Err(err) => return Err(err),
        })
    }

    /// Decodes from the given byte slice
    fn decode_from_data(data: &'de [u8]) -> NetResult<Self> {
        let mut r = PacketReader::new(data);
        Self::decode_packet(&mut r)
    }

    /// Decodes from the given byte slice and ensures
    /// every byte was read
    fn decode_from_data_complete(data: &'de [u8]) -> anyhow::Result<Self> {
        let mut r = PacketReader::new(data);
        let res = Self::decode_packet(&mut r)?;
        if !r.remaining_slice().is_empty() {
            anyhow::bail!("Still remaining data: {:?}", r.remaining_slice());
        }
        Ok(res)
    }
}

/// Encodes this type on a packet writer
pub trait EncodePacket: Sized {
    /// Size Hint for types with a known type at compile time
    const SIZE_HINT: Option<usize>;

    /// Get the encoded length of this type
    fn packet_len(&self) -> usize;

    /// Encodes the packet onto the writer
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> NetResult<()>;

    /// Encodes this data as slice
    fn encode_packet_n<T: BufMut>(items: &[Self], pw: &mut PacketWriter<T>) -> NetResult<()> {
        for item in items.iter() {
            item.encode_packet(pw)?;
        }

        Ok(())
    }

    /// Encodes the type on a writer and returns the data
    fn to_data(&self) -> NetResult<bytes::Bytes> {
        let mut pw = PacketWriter::default();
        self.encode_packet(&mut pw)?;
        Ok(pw.into_inner().freeze())
    }

    /// Encodes this type as a packet
    fn to_packet(&self) -> NetResult<ShroomPacket> {
        Ok(ShroomPacket::from_data(self.to_data()?))
    }
}

/// Decodes a container with the given size
pub trait DecodePacketSized<'de, T>: Sized {
    fn decode_packet_sized(pr: &mut PacketReader<'de>, size: usize) -> NetResult<Self>;
}

impl<'de, T> DecodePacketSized<'de, T> for Vec<T>
where
    T: DecodePacket<'de>,
{
    fn decode_packet_sized(pr: &mut PacketReader<'de>, size: usize) -> NetResult<Self> {
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
                fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> NetResult<()> {
                    #[allow(non_snake_case)]
                    let ($($name,)*) = self;
                    $($name.encode_packet(pw)?;)*
                    Ok(())
                }

                const SIZE_HINT: Option<usize> = $crate::util::SizeHint::zero()
                        $(.add($crate::util::SizeHint($name::SIZE_HINT)))*.0;

                fn packet_len(&self) -> usize {
                    #[allow(non_snake_case)]
                    let ($($name,)*) = self;

                    $($name.packet_len() +)*0
                }
            }


            impl<'de, $($name,)*> $crate::DecodePacket<'de> for ($($name,)*)
            where $($name: $crate::DecodePacket<'de>,)* {
                fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
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

    use super::DecodePacketOwned;

    /// Helper function to test If encoding matches decoding
    pub(crate) fn enc_dec_test<T>(val: T)
    where
        T: EncodePacket + DecodePacketOwned + PartialEq + std::fmt::Debug,
    {
        let data = val.to_packet().expect("encode");
        let mut pr = data.into_reader();
        let decoded = T::decode_packet(&mut pr).expect("decode");

        assert_eq!(val, decoded);
    }

    /// Helper function to test If encoding matches decoding
    pub(crate) fn enc_dec_test_all<T>(vals: impl IntoIterator<Item = T>)
    where
        T: EncodePacket + DecodePacketOwned + PartialEq + std::fmt::Debug,
    {
        for val in vals {
            enc_dec_test(val);
        }
    }

    #[test]
    fn tuple_size() {
        assert_eq!(<((), (),)>::SIZE_HINT, Some(0));
        assert_eq!(<((), u32,)>::SIZE_HINT, Some(4));
        assert_eq!(<((), u32, String)>::SIZE_HINT, None);
    }
}
