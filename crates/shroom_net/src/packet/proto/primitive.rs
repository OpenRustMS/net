use array_init::try_array_init;
use bytes::BufMut;
use either::Either;

use crate::{NetResult, PacketReader, PacketWriter};

use super::{DecodePacket, EncodePacket};

impl<'de> DecodePacket<'de> for () {
    fn decode_packet(_pr: &mut PacketReader<'de>) -> NetResult<Self> {
        Ok(())
    }
}

impl EncodePacket for () {
    fn encode_packet<B: BufMut>(&self, _pw: &mut PacketWriter<B>) -> NetResult<()> {
        Ok(())
    }

    const SIZE_HINT: Option<usize> = Some(0);

    fn packet_len(&self) -> usize {
        0
    }
}

impl<A, B> EncodePacket for Either<A, B>
where
    A: EncodePacket,
    B: EncodePacket,
{
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> NetResult<()> {
        match self {
            Either::Left(a) => a.encode_packet(pw),
            Either::Right(b) => b.encode_packet(pw),
        }
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        match self {
            Either::Left(l) => l.packet_len(),
            Either::Right(r) => r.packet_len(),
        }
    }
}

pub struct OptionTail<T>(Option<T>);

impl<T> EncodePacket for OptionTail<T>
where
    T: EncodePacket,
{
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        if let Some(val) = self.0.as_ref() {
            val.encode_packet(pw)?;
        }
        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        self.0.as_ref().map(|v| v.packet_len()).unwrap_or(0)
    }
}

impl<'de, T> DecodePacket<'de> for OptionTail<T>
where
    T: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        let mut sub_reader = pr.sub_reader();
        Ok(Self(T::decode_packet(&mut sub_reader).ok()))
    }
}

macro_rules! impl_dec {
    ($ty:ty, $dec:path) => {
        impl<'de> DecodePacket<'de> for $ty {
            fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
                $dec(pr)
            }
        }
    };
}

macro_rules! impl_enc {
    ($ty:ty, $enc:path) => {
        impl EncodePacket for $ty {
            fn encode_packet<B: bytes::BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
                $enc(pw, *self)
            }

            const SIZE_HINT: Option<usize> = Some(std::mem::size_of::<$ty>());

            fn packet_len(&self) -> usize {
                std::mem::size_of::<$ty>()
            }
        }
    };
}

/*
macro_rules! impl_tracing {
    ($ty:ty) => {
        impl crate::proto::tracing::HasTraceInformation for $ty {
            fn write_trace<TW: crate::proto::tracing::TracingWriter>(
                tw: &mut TW,
                v: Option<&Self>,
            ) {
                match v {
                    Some(v) => {
                        let tracing_val: crate::proto::tracing::TracingValue = v.into();
                        crate::proto::tracing::TracingValue::write_trace(tw, Some(&tracing_val));
                    }
                    _ => {}
                }
            }
        }
    };
}*/

macro_rules! impl_dec_enc {
    ($ty:ty, $dec:path, $enc:path) => {
        impl_dec!($ty, $dec);
        impl_enc!($ty, $enc);
        //impl_tracing!($ty);
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

impl<'de, const N: usize, T: DecodePacket<'de>> DecodePacket<'de> for [T; N] {
    fn decode_packet(pr: &mut PacketReader<'de>) -> NetResult<Self> {
        try_array_init(|_| T::decode_packet(pr))
    }
}

impl<const N: usize, T: EncodePacket> EncodePacket for [T; N] {
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        for v in self.iter() {
            v.encode_packet(pw)?;
        }
        Ok(())
    }

    const SIZE_HINT: Option<usize> = mul(T::SIZE_HINT, N);

    fn packet_len(&self) -> usize {
        self.iter().map(|v| v.packet_len()).sum()
    }
}

const fn mul(sz: Option<usize>, n: usize) -> Option<usize> {
    match sz {
        Some(sz) => Some(sz * n),
        _ => None,
    }
}

impl<D: EncodePacket> EncodePacket for Vec<D> {
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> NetResult<()> {
        for v in self.iter() {
            v.encode_packet(pw)?;
        }

        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        self.iter().map(|v| v.packet_len()).sum()
    }
}

impl<D: EncodePacket> EncodePacket for Option<D> {
    fn encode_packet<T: BufMut>(&self, pw: &mut PacketWriter<T>) -> NetResult<()> {
        if let Some(ref v) = self {
            v.encode_packet(pw)?;
        }

        Ok(())
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        self.as_ref().map(|v| v.packet_len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use crate::packet::proto::tests::enc_dec_test_all;

    #[test]
    fn test_name() {
        macro_rules! test_num {
            ($ty:ty) => {
                let min = <$ty>::MIN;
                let max = <$ty>::MAX;
                let half = (min + max) / (2 as $ty);
                enc_dec_test_all([min, max, half])
            };
            ($($ty:ty,)*) => {
                $(test_num!($ty);)*
            };
        }

        test_num!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64,);
    }
}
