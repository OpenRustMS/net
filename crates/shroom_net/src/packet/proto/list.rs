// TODO handle overflow for length ?

use std::fmt::Debug;
use std::marker::PhantomData;

use bytes::BufMut;
use derive_more::{Deref, DerefMut, From, Into};

use crate::{NetResult, PacketReader, PacketWriter, SizeHint};

use super::{DecodePacket, DecodePacketOwned, EncodePacket};

/// List length type
pub trait ShroomListLen: EncodePacket + DecodePacketOwned {
    fn to_len(&self) -> usize;
    fn from_len(ix: usize) -> Self;
}

/// List index type
pub trait ShroomListIndex: ShroomListLen + PartialEq {
    /// Terminator for ShroomIndexList
    const TERMINATOR: Self;
    /// Terminator for ShroomIndexListZ
    const ZERO_TERMINATOR: Self;
}

/// Macro to implement the index trait for a numeric type
macro_rules! impl_list_index {
    ($ty:ty) => {
        impl ShroomListLen for $ty {
            fn to_len(&self) -> usize {
                *self as usize
            }

            fn from_len(ix: usize) -> Self {
                ix as $ty
            }
        }

        impl ShroomListIndex for $ty {
            const TERMINATOR: Self = <$ty>::MAX;
            const ZERO_TERMINATOR: Self = <$ty>::MIN;
        }
    };
}

// Only unsigned are supported by default
impl_list_index!(u8);
impl_list_index!(u16);
impl_list_index!(u32);
impl_list_index!(u64);

#[derive(Debug, Clone, PartialEq, From, Into, Deref, DerefMut)]
pub struct ShroomBaseIndexList<const Z: bool, I, T>(Vec<(I, T)>);

impl<const Z: bool, I, T> Default for ShroomBaseIndexList<Z, I, T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<const Z: bool, I, T> FromIterator<(I, T)> for ShroomBaseIndexList<Z, I, T> {
    fn from_iter<ITER: IntoIterator<Item = (I, T)>>(iter: ITER) -> Self {
        Self(FromIterator::from_iter(iter))
    }
}

/// Get the terminator based on the Z bool
const fn get_term<I: ShroomListIndex>(z: bool) -> I {
    if z {
        I::ZERO_TERMINATOR
    } else {
        I::TERMINATOR
    }
}

impl<'de, const Z: bool, I, T> DecodePacket<'de> for ShroomBaseIndexList<Z, I, T>
where
    T: DecodePacket<'de>,
    I: ShroomListIndex,
{
    #[inline]
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        // Decodes until the terminator the terminator is read
        // TODO: cap size
        let mut items = Vec::new();

        loop {
            let ix = I::decode_packet(pr)?;

            // Check for terminator
            if ix == get_term(Z) {
                break Ok(items.into());
            }

            let item = T::decode_packet(&mut *pr)?;
            items.push((ix, item));
        }
    }
}

impl<const Z: bool, I, T> EncodePacket for ShroomBaseIndexList<Z, I, T>
where
    T: EncodePacket,
    I: ShroomListIndex,
{
    #[inline]
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        for (ix, item) in self.iter() {
            ix.encode_packet(pw)?;
            item.encode_packet(pw)?;
        }
        get_term::<I>(Z).encode_packet(pw)?;

        Ok(())
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    #[inline]
    fn packet_len(&self) -> usize {
        I::SIZE_HINT.0.expect("Index size") + self.iter().map(|v| v.packet_len()).sum::<usize>()
    }
}

/// A list with tuple elements of (index, value), terminated at the terminator
pub type ShroomIndexList<I, T> = ShroomBaseIndexList<false, I, T>;
/// A list with tuple elements of (index, value), terminated at the zero-terminator
pub type ShroomIndexListZ<I, T> = ShroomBaseIndexList<true, I, T>;

/// A list which uses the given type `L` length, refer to the type-alias lists
/// such as: `ShroomList32`
#[derive(Clone, PartialEq, Into, Deref, DerefMut)]
pub struct ShroomList<L, T> {
    #[deref]
    #[deref_mut]
    #[into]
    pub items: Vec<T>,
    pub _index: PhantomData<L>,
}

impl<L, T> FromIterator<T> for ShroomList<L, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            items: FromIterator::from_iter(iter),
            _index: PhantomData,
        }
    }
}

impl<L, T> Default for ShroomList<L, T> {
    fn default() -> Self {
        Self {
            items: Vec::default(),
            _index: PhantomData,
        }
    }
}

impl<L, T> From<Vec<T>> for ShroomList<L, T> {
    fn from(items: Vec<T>) -> Self {
        Self {
            items,
            _index: PhantomData,
        }
    }
}

impl<L, T> Debug for ShroomList<L, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShroomList")
            .field("items", &self.items)
            .finish()
    }
}

impl<'de, L, T> DecodePacket<'de> for ShroomList<L, T>
where
    L: ShroomListLen,
    T: DecodePacket<'de>,
{
    #[inline]
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        // Read the length then decode all items
        let n = L::decode_packet(pr)?;
        let n = n.to_len();

        Ok(T::decode_packet_n(pr, n)?.into())
    }
}

impl<L, T> EncodePacket for ShroomList<L, T>
where
    L: ShroomListLen,
    T: EncodePacket,
{
    #[inline]
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        // Encode the length followed by all items
        L::from_len(self.len()).encode_packet(pw)?;
        T::encode_packet_n(self, pw)?;

        Ok(())
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    #[inline]
    fn packet_len(&self) -> usize {
        L::SIZE_HINT.0.expect("Index size")
            + self.items.iter().map(|v| v.packet_len()).sum::<usize>()
    }
}

/// ShroomList with `u8` as length
pub type ShroomList8<T> = ShroomList<u8, T>;
/// ShroomList with `u16` as length
pub type ShroomList16<T> = ShroomList<u16, T>;
/// ShroomList with `u32` as length
pub type ShroomList32<T> = ShroomList<u32, T>;
/// ShroomList with `u64` as length
pub type ShroomList64<T> = ShroomList<u64, T>;

/// Index based list with `u8` as index
pub type ShroomIndexList8<T> = ShroomIndexList<u8, T>;
/// Index based list with `u16` as index
pub type ShroomIndexList16<T> = ShroomIndexList<u16, T>;
/// Index based list with `u32` as index
pub type ShroomIndexList32<T> = ShroomIndexList<u32, T>;
/// Index based list with `u64` as index
pub type ShroomIndexList64<T> = ShroomIndexList<u64, T>;

/// Zero-Index based list with `u8` as index
pub type ShroomIndexListZ8<T> = ShroomIndexListZ<u8, T>;
/// Zero-Index based list with `u16` as index
pub type ShroomIndexListZ16<T> = ShroomIndexListZ<u16, T>;
/// Zero-Index based list with `u32` as index
pub type ShroomIndexListZ32<T> = ShroomIndexListZ<u32, T>;
/// Zero-Index based list with `u64` as index
pub type ShroomIndexListZ64<T> = ShroomIndexListZ<u64, T>;

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::{packet::test_util::test_encode_decode_owned, test_encode_decode};

    use super::*;

    fn generic_list_test<I: ShroomListIndex>() {
        test_encode_decode!(
            ShroomList::<I, u8>::from(vec![1u8, 2, 3]),
            ShroomList::<I, u8>::from(vec![1]),
            ShroomList::<I, u8>::from(vec![])
        );
    }

    #[test]
    fn list() {
        generic_list_test::<u8>();
        generic_list_test::<u16>();
        generic_list_test::<u32>();
        generic_list_test::<u64>();
    }

    fn generic_index_list_test<I: ShroomListIndex + Debug + From<u8> + Clone>() {
        let data1: Vec<(I, u8)> = vec![(I::from(1), 1u8), (I::from(2), 2), (I::from(3), 3)];

        test_encode_decode!(
            ShroomIndexList::<I, u8>::from(data1.clone()),
            ShroomIndexList::<I, u8>::from(vec![(I::from(0), 1)]),
            ShroomIndexList::<I, u8>::from(vec![]),
            ShroomIndexListZ::<I, u8>::from(data1.clone()),
            ShroomIndexListZ::<I, u8>::from(vec![(I::from(1), 1)]),
            ShroomIndexListZ::<I, u8>::from(vec![])
        );
    }

    #[test]
    fn index_list() {
        generic_index_list_test::<u8>();
        generic_index_list_test::<u16>();
        generic_index_list_test::<u32>();
        generic_index_list_test::<u64>();
    }

    // Test encoding/decoding
    proptest::proptest! {
        #[test]
        fn shroom_list(data in prop::collection::vec(1u8..=10, 0..=10)) {
            test_encode_decode_owned(ShroomList32::from(data));
        }

        // There can be up to u8::MAX-1 items, because the terminator is not allowed to exit
        // And the index can be between 1 and u8::MAX - 1
        #[test]
        fn shroom_index_list(data in prop::collection::vec((1u8..=254, any::<u8>()), 0..=254)) {
            test_encode_decode_owned(ShroomIndexList8::from(data.clone()));
            test_encode_decode_owned(ShroomIndexListZ8::from(data));
        }
    }
}
