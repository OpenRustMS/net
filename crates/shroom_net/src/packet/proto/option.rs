use std::marker::PhantomData;

use crate::{PacketReader, PacketWriter, NetResult};

use super::{wrapped::PacketWrapped, DecodePacket, DecodePacketOwned, EncodePacket};

pub trait ShroomOptionIndex: EncodePacket + DecodePacketOwned {
    const NONE_VALUE: Self;
    const SOME_VALUE: Self;
    fn has_value(&self) -> bool;
}

impl ShroomOptionIndex for u8 {
    const NONE_VALUE: Self = 0;
    const SOME_VALUE: Self = 1;
    fn has_value(&self) -> bool {
        *self != 0
    }
}

impl ShroomOptionIndex for bool {
    const NONE_VALUE: Self = false;
    const SOME_VALUE: Self = true;
    fn has_value(&self) -> bool {
        *self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevShroomOptionIndex<Opt>(pub Opt);

impl<Opt> PacketWrapped for RevShroomOptionIndex<Opt>
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

impl<Opt> ShroomOptionIndex for RevShroomOptionIndex<Opt>
where
    Opt: ShroomOptionIndex + Copy,
{
    const NONE_VALUE: Self = RevShroomOptionIndex(Opt::SOME_VALUE);
    const SOME_VALUE: Self = RevShroomOptionIndex(Opt::NONE_VALUE);

    fn has_value(&self) -> bool {
        !self.0.has_value()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShroomOption<T, Opt> {
    pub opt: Option<T>,
    _t: PhantomData<Opt>,
}

impl<T, Opt> ShroomOption<T, Opt> {
    pub fn from_opt(opt: Option<T>) -> Self {
        Self {
            opt,
            _t: PhantomData,
        }
    }
}

impl<T, Opt> From<ShroomOption<T, Opt>> for Option<T> {
    fn from(val: ShroomOption<T, Opt>) -> Self {
        val.opt
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
    Opt: ShroomOptionIndex,
{
    fn encode_packet<B: bytes::BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        match self.opt.as_ref() {
            Some(v) => {
                Opt::SOME_VALUE.encode_packet(pw)?;
                v.encode_packet(pw)
            }
            None => Opt::NONE_VALUE.encode_packet(pw),
        }
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        match self.opt.as_ref() {
            Some(v) => Opt::SOME_VALUE.packet_len() + v.packet_len(),
            None => Opt::NONE_VALUE.packet_len(),
        }
    }
}

impl<'de, T, Opt> DecodePacket<'de> for ShroomOption<T, Opt>
where
    T: DecodePacket<'de>,
    Opt: ShroomOptionIndex,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        let d = Opt::decode_packet(pr)?;
        let v = if d.has_value() {
            Some(T::decode_packet(pr)?)
        } else {
            None
        };

        Ok(Self::from_opt(v))
    }
}

pub type ShroomOption8<T> = ShroomOption<T, u8>;
pub type ShroomOptionR8<T> = ShroomOption<T, RevShroomOptionIndex<u8>>;
pub type ShroomOptionBool<T> = ShroomOption<T, bool>;
pub type ShroomOptionRBool<T> = ShroomOption<T, RevShroomOptionIndex<bool>>;


#[cfg(test)]
mod tests {
    use crate::packet::proto::tests::enc_dec_test_all;

    use super::*;

    #[test]
    fn option() {
        enc_dec_test_all([
            ShroomOption8::from_opt(Some("abc".to_string())),
            ShroomOption8::from_opt(None),
        ]);
        enc_dec_test_all([
            ShroomOptionR8::from_opt(Some("abc".to_string())),
            ShroomOptionR8::from_opt(None),
        ]);
        enc_dec_test_all([
            ShroomOptionBool::from_opt(Some("abc".to_string())),
            ShroomOptionBool::from_opt(None),
        ]);
        enc_dec_test_all([
            ShroomOptionRBool::from_opt(Some("abc".to_string())),
            ShroomOptionRBool::from_opt(None),
        ]);
    }
}