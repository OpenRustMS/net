use array_init::try_array_init;
use bytes::BufMut;
use either::Either;

use crate::{PacketReader, PacketResult, PacketWriter, SizeHint};

use super::{DecodePacket, EncodePacket};

impl<'de> DecodePacket<'de> for () {
    fn decode_packet(_pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        Ok(())
    }
}

impl EncodePacket for () {
    const SIZE_HINT: SizeHint = SizeHint::ZERO;

    fn encode_packet<B: BufMut>(&self, _pw: &mut PacketWriter<B>) -> PacketResult<()> {
        Ok(())
    }

    fn packet_len(&self) -> usize {
        0
    }
}

impl<A, B> EncodePacket for Either<A, B>
where
    A: EncodePacket,
    B: EncodePacket,
{
    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()> {
        match self {
            Either::Left(a) => a.encode_packet(pw),
            Either::Right(b) => b.encode_packet(pw),
        }
    }

    fn packet_len(&self) -> usize {
        match self {
            Either::Left(l) => l.packet_len(),
            Either::Right(r) => r.packet_len(),
        }
    }
}

/// An optional tail, only read If there's enough data at the end available
pub struct OptionTail<T>(pub Option<T>);

impl<T> EncodePacket for OptionTail<T>
where
    T: EncodePacket,
{
    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        if let Some(val) = self.0.as_ref() {
            val.encode_packet(pw)?;
        }
        Ok(())
    }

    fn packet_len(&self) -> usize {
        self.0.as_ref().map(|v| v.packet_len()).unwrap_or(0)
    }
}

impl<'de, T> DecodePacket<'de> for OptionTail<T>
where
    T: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        let mut sub_reader = pr.sub_reader();
        Ok(Self(T::decode_packet(&mut sub_reader).ok()))
    }
}
macro_rules! impl_dec_enc {
    ($ty:ty, $dec:path, $enc:path) => {
        impl EncodePacket for $ty {
            const SIZE_HINT: SizeHint = $crate::SizeHint::new(std::mem::size_of::<$ty>());

            fn encode_packet<B: bytes::BufMut>(
                &self,
                pw: &mut PacketWriter<B>,
            ) -> PacketResult<()> {
                $enc(pw, *self)
            }

            fn packet_len(&self) -> usize {
                std::mem::size_of::<$ty>()
            }
        }

        impl<'de> DecodePacket<'de> for $ty {
            fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
                $dec(pr)
            }
        }
    };
}

impl_dec_enc!(bool, PacketReader::read_bool, PacketWriter::write_bool);
impl_dec_enc!(u8, PacketReader::read_u8, PacketWriter::write_u8);
impl_dec_enc!(i8, PacketReader::read_i8, PacketWriter::write_i8);
impl_dec_enc!(u16, PacketReader::read_u16, PacketWriter::write_u16);
impl_dec_enc!(u32, PacketReader::read_u32, PacketWriter::write_u32);
impl_dec_enc!(u64, PacketReader::read_u64, PacketWriter::write_u64);
impl_dec_enc!(u128, PacketReader::read_u128, PacketWriter::write_u128);
impl_dec_enc!(i16, PacketReader::read_i16, PacketWriter::write_i16);
impl_dec_enc!(i32, PacketReader::read_i32, PacketWriter::write_i32);
impl_dec_enc!(i64, PacketReader::read_i64, PacketWriter::write_i64);
impl_dec_enc!(i128, PacketReader::read_i128, PacketWriter::write_i128);
impl_dec_enc!(f32, PacketReader::read_f32, PacketWriter::write_f32);
impl_dec_enc!(f64, PacketReader::read_f64, PacketWriter::write_f64);

// Arrays

impl<'de, const N: usize, T: DecodePacket<'de>> DecodePacket<'de> for [T; N] {
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        try_array_init(|_| T::decode_packet(pr))
    }
}

impl<const N: usize, T: EncodePacket> EncodePacket for [T; N] {
    const SIZE_HINT: SizeHint = T::SIZE_HINT.mul_n(N);

    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        for v in self.iter() {
            v.encode_packet(pw)?;
        }
        Ok(())
    }

    fn packet_len(&self) -> usize {
        self.iter().map(|v| v.packet_len()).sum()
    }
}

impl<D: EncodePacket> EncodePacket for Vec<D> {
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()> {
        for v in self.iter() {
            v.encode_packet(pw)?;
        }

        Ok(())
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        self.iter().map(|v| v.packet_len()).sum()
    }
}

impl<D: EncodePacket> EncodePacket for Option<D> {
    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()> {
        if let Some(ref v) = self {
            v.encode_packet(pw)?;
        }

        Ok(())
    }

    fn packet_len(&self) -> usize {
        self.as_ref().map(|v| v.packet_len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::test_enc_dec_all;

    #[test]
    fn prim_num() {
        macro_rules! test_num {
            ($ty:ty) => {
                let min = <$ty>::MIN;
                let max = <$ty>::MAX;
                let half = (min + max) / (2 as $ty);
                test_enc_dec_all([min, max, half])
            };
            ($($ty:ty,)*) => {
                $(test_num!($ty);)*
            };
        }

        test_num!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64,);
    }

    #[test]
    fn bool() {
        test_enc_dec_all([false, true]);
    }
}