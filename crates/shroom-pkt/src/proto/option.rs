use std::marker::PhantomData;

use derive_more::{Deref, DerefMut, Into};

use crate::{PacketReader, PacketResult, PacketWriter, SizeHint};

use super::{wrapped::PacketWrapped, DecodePacket, DecodePacketOwned, EncodePacket};

/// Discriminant for Option
pub trait ShroomOptionDiscriminant: EncodePacket + DecodePacketOwned {
    const NONE_VALUE: Self;
    const SOME_VALUE: Self;
    fn has_value(&self) -> bool;
}

impl ShroomOptionDiscriminant for u8 {
    const NONE_VALUE: Self = 0;
    const SOME_VALUE: Self = 1;
    fn has_value(&self) -> bool {
        *self != 0
    }
}

impl ShroomOptionDiscriminant for bool {
    const NONE_VALUE: Self = false;
    const SOME_VALUE: Self = true;
    fn has_value(&self) -> bool {
        *self
    }
}

/// Reversed Option Discriminant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevShroomOptionDiscriminant<Opt>(pub Opt);

impl<Opt> PacketWrapped for RevShroomOptionDiscriminant<Opt>
where
    Opt: Copy,
{
    type Inner = Opt;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(v)
    }
}

impl<Opt> ShroomOptionDiscriminant for RevShroomOptionDiscriminant<Opt>
where
    Opt: ShroomOptionDiscriminant + Copy,
{
    const NONE_VALUE: Self = RevShroomOptionDiscriminant(Opt::SOME_VALUE);
    const SOME_VALUE: Self = RevShroomOptionDiscriminant(Opt::NONE_VALUE);

    fn has_value(&self) -> bool {
        !self.0.has_value()
    }
}

/// Optional type, first read the discriminant `D`
/// and then reads the value If D is some
#[derive(Debug, Clone, Copy, PartialEq, Into, Deref, DerefMut)]
pub struct ShroomOption<T, D> {
    #[into]
    #[deref]
    #[deref_mut]
    pub opt: Option<T>,
    _t: PhantomData<D>,
}

impl<T, D> ShroomOption<T, D> {
    pub fn from_opt(opt: Option<T>) -> Self {
        Self {
            opt,
            _t: PhantomData,
        }
    }
}

impl<T, Opt> From<Option<T>> for ShroomOption<T, Opt> {
    fn from(value: Option<T>) -> Self {
        Self::from_opt(value)
    }
}

impl<T, Opt> EncodePacket for ShroomOption<T, Opt>
where
    T: EncodePacket,
    Opt: ShroomOptionDiscriminant,
{
    fn encode_packet<B: bytes::BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        match self.as_ref() {
            Some(v) => {
                Opt::SOME_VALUE.encode_packet(pw)?;
                v.encode_packet(pw)
            }
            None => Opt::NONE_VALUE.encode_packet(pw),
        }
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        match self.as_ref() {
            Some(v) => Opt::SOME_VALUE.packet_len() + v.packet_len(),
            None => Opt::NONE_VALUE.packet_len(),
        }
    }
}

impl<'de, T, Opt> DecodePacket<'de> for ShroomOption<T, Opt>
where
    T: DecodePacket<'de>,
    Opt: ShroomOptionDiscriminant,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        let d = Opt::decode_packet(pr)?;
        Ok(if d.has_value() {
            Some(T::decode_packet(pr)?)
        } else {
            None
        }
        .into())
    }
}

/// Optional with u8 as discriminator, 0 signaling None, otherwise en/decode `T`
pub type ShroomOption8<T> = ShroomOption<T, u8>;
/// Optional with reversed u8 as discriminator, 0 signaling en/decode `T`
pub type ShroomOptionR8<T> = ShroomOption<T, RevShroomOptionDiscriminant<u8>>;
/// Optional with `bool` as discriminator, false signaling None, otherwise en/decode `T`
pub type ShroomOptionBool<T> = ShroomOption<T, bool>;
/// Optional with reversed `bool` as discriminator, false signaling en/decode `T`
pub type ShroomOptionRBool<T> = ShroomOption<T, RevShroomOptionDiscriminant<bool>>;

#[cfg(test)]
mod tests {
    use crate::test_util::test_enc_dec_all;

    use super::*;

    #[test]
    fn option() {
        test_enc_dec_all([
            ShroomOption8::from_opt(Some("abc".to_string())),
            ShroomOption8::from_opt(None),
        ]);
        test_enc_dec_all([
            ShroomOptionR8::from_opt(Some("abc".to_string())),
            ShroomOptionR8::from_opt(None),
        ]);
        test_enc_dec_all([
            ShroomOptionBool::from_opt(Some("abc".to_string())),
            ShroomOptionBool::from_opt(None),
        ]);
        test_enc_dec_all([
            ShroomOptionRBool::from_opt(Some("abc".to_string())),
            ShroomOptionRBool::from_opt(None),
        ]);
    }
}
