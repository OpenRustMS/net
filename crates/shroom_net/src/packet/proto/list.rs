use std::marker::PhantomData;
use std::{fmt::Debug, slice};

use bytes::BufMut;

use crate::{NetResult, PacketReader, PacketWriter};

use super::{DecodePacket, DecodePacketOwned, EncodePacket};

pub trait ShroomListLen: EncodePacket + DecodePacketOwned {
    fn to_len(&self) -> usize;
    fn from_len(ix: usize) -> Self;
}

pub trait ShroomListIndex: ShroomListLen + PartialEq {
    const TERMINATOR: Self;
}

pub trait ShroomListIndexZ: ShroomListLen + PartialEq {
    const TERMINATOR: Self;
}

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
        }

        impl ShroomListIndexZ for $ty {
            const TERMINATOR: Self = <$ty>::MIN;
        }
    };
}

impl_list_index!(u8);
impl_list_index!(u16);
impl_list_index!(u32);
impl_list_index!(u64);

#[derive(Debug, Clone, PartialEq)]
pub struct ShroomIndexList<I, T> {
    pub items: Vec<(I, T)>,
}

impl<I, T> ShroomIndexList<I, T> {
    pub fn iter(&self) -> slice::Iter<'_, (I, T)> {
        self.items.iter()
    }
}

impl<I, T> From<Vec<(I, T)>> for ShroomIndexList<I, T> {
    fn from(items: Vec<(I, T)>) -> Self {
        Self { items }
    }
}

impl<I, T> Default for ShroomIndexList<I, T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<'de, I, T> DecodePacket<'de> for ShroomIndexList<I, T>
where
    T: DecodePacket<'de>,
    I: ShroomListIndex,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        let mut items = Vec::new();

        loop {
            let ix = I::decode_packet(pr)?;
            if ix == I::TERMINATOR {
                break;
            }
            let item = T::decode_packet(&mut *pr)?;
            items.push((ix, item));
        }

        Ok(ShroomIndexList { items })
    }
}

impl<I, T> EncodePacket for ShroomIndexList<I, T>
where
    T: EncodePacket,
    I: ShroomListIndex,
{
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        let items = &self.items;

        for (ix, item) in items.iter() {
            ix.encode_packet(pw)?;
            item.encode_packet(pw)?;
        }
        I::TERMINATOR.encode_packet(pw)?;

        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        I::SIZE_HINT.unwrap() + self.items.iter().map(|v| v.packet_len()).sum::<usize>()
    }
}

/// Like `ShroomIndexList`just using zero index as terminator
#[derive(Debug, Clone, PartialEq)]
pub struct ShroomIndexListZ<I, T> {
    pub items: Vec<(I, T)>,
}

impl<I, T> ShroomIndexListZ<I, T> {
    pub fn iter(&self) -> slice::Iter<'_, (I, T)> {
        self.items.iter()
    }
}

impl<I, E> FromIterator<(I, E)> for ShroomIndexListZ<I, E> {
    fn from_iter<T: IntoIterator<Item = (I, E)>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

impl<I, T> Default for ShroomIndexListZ<I, T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<I, T> From<Vec<(I, T)>> for ShroomIndexListZ<I, T> {
    fn from(items: Vec<(I, T)>) -> Self {
        Self { items }
    }
}

impl<'de, I, T> DecodePacket<'de> for ShroomIndexListZ<I, T>
where
    T: DecodePacket<'de>,
    I: ShroomListIndexZ,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        let mut items = Vec::new();

        loop {
            let ix = I::decode_packet(pr)?;
            if ix == I::TERMINATOR {
                break;
            }
            let item = T::decode_packet(&mut *pr)?;
            items.push((ix, item));
        }

        Ok(Self { items })
    }
}

impl<I, T> EncodePacket for ShroomIndexListZ<I, T>
where
    T: EncodePacket,
    I: ShroomListIndexZ,
{
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        let items = &self.items;

        for (ix, item) in items.iter() {
            ix.encode_packet(pw)?;
            item.encode_packet(pw)?;
        }
        I::TERMINATOR.encode_packet(pw)?;

        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        I::SIZE_HINT.unwrap() + self.items.iter().map(|v| v.packet_len()).sum::<usize>()
    }
}
#[derive(Clone, PartialEq)]
pub struct ShroomList<I, T> {
    pub items: Vec<T>,
    pub _index: PhantomData<I>,
}

impl<I, T> ShroomList<I, T> {
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.items.iter()
    }
}

impl<I, E> FromIterator<E> for ShroomList<I, E> {
    fn from_iter<T: IntoIterator<Item = E>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().collect(),
            _index: PhantomData,
        }
    }
}

impl<I, T> Default for ShroomList<I, T> {
    fn default() -> Self {
        Self {
            items: Vec::default(),
            _index: PhantomData,
        }
    }
}

impl<I, T> From<Vec<T>> for ShroomList<I, T> {
    fn from(items: Vec<T>) -> Self {
        Self {
            items,
            _index: PhantomData,
        }
    }
}

impl<I, T> Debug for ShroomList<I, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShroomList")
            .field("items", &self.items)
            .finish()
    }
}

impl<'de, I, T> DecodePacket<'de> for ShroomList<I, T>
where
    I: ShroomListLen,
    T: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        let n = I::decode_packet(pr)?;
        let n = n.to_len();

        Ok(Self {
            items: T::decode_packet_n(pr, n)?,
            _index: PhantomData,
        })
    }
}

impl<I, T> EncodePacket for ShroomList<I, T>
where
    I: ShroomListLen,
    T: EncodePacket,
{
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        let items = &self.items;
        I::from_len(items.len()).encode_packet(pw)?;
        T::encode_packet_n(items, pw)?;

        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        I::SIZE_HINT.unwrap() + self.items.iter().map(|v| v.packet_len()).sum::<usize>()
    }
}

pub type ShroomList8<T> = ShroomList<u8, T>;
pub type ShroomList16<T> = ShroomList<u16, T>;
pub type ShroomList32<T> = ShroomList<u32, T>;
pub type ShroomList64<T> = ShroomList<u64, T>;

pub type ShroomIndexList8<T> = ShroomIndexList<u8, T>;
pub type ShroomIndexList16<T> = ShroomIndexList<u16, T>;
pub type ShroomIndexList32<T> = ShroomIndexList<u32, T>;
pub type ShroomIndexList64<T> = ShroomIndexList<u64, T>;

pub type ShroomIndexListZ8<T> = ShroomIndexListZ<u8, T>;
pub type ShroomIndexListZ16<T> = ShroomIndexListZ<u16, T>;
pub type ShroomIndexListZ32<T> = ShroomIndexListZ<u32, T>;
pub type ShroomIndexListZ64<T> = ShroomIndexListZ<u64, T>;

#[cfg(test)]
mod tests {
    use crate::packet::proto::tests::enc_dec_test_all;

    use super::*;

    #[test]
    fn list() {
        enc_dec_test_all([
            ShroomList8::from(vec![1u8, 2, 3]),
            ShroomList8::from(vec![1]),
            ShroomList8::from(vec![]),
        ]);
    }

    #[test]
    fn index_list() {
        enc_dec_test_all([
            ShroomIndexList8::from(vec![(1, 1u8), (3, 2), (2, 3)]),
            ShroomIndexList8::from(vec![(0, 1)]),
            ShroomIndexList8::from(vec![]),
        ]);
    }

    #[test]
    fn index_list_z() {
        enc_dec_test_all([
            ShroomIndexList8::from(vec![(1, 1u8), (3, 2), (2, 3)]),
            ShroomIndexList8::from(vec![(1, 1)]),
            ShroomIndexList8::from(vec![]),
        ]);
    }
}
