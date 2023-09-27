use std::str::Utf8Error;

use arrayvec::{ArrayString, CapacityError};
use bytes::BufMut;

use crate::{
    packet_str_len, DecodePacket, EncodePacket, PacketReader, PacketResult, PacketWriter, SizeHint,
};

use super::PacketTryWrapped;

// Basic support for String and str

impl EncodePacket for String {
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        self.as_str().encode_packet(pw)
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        self.as_str().packet_len()
    }
}

impl<'de> DecodePacket<'de> for String {
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        Ok(<&'de str>::decode_packet(pr)?.to_string())
    }
}

impl<'de> DecodePacket<'de> for &'de str {
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        pr.read_string()
    }
}

impl<'a> EncodePacket for &'a str {
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> PacketResult<()> {
        pw.write_str(self)
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        packet_str_len(self)
    }
}

// Basic support for ArrayString
impl<const N: usize> EncodePacket for arrayvec::ArrayString<N> {
    fn encode_packet<T>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()>
    where
        T: BufMut,
    {
        pw.write_str(self.as_str())
    }

    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        packet_str_len(self.as_str())
    }
}

impl<'de, const N: usize> DecodePacket<'de> for arrayvec::ArrayString<N> {
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        let s = pr.read_string_limited(N)?;
        Ok(arrayvec::ArrayString::from(s).unwrap())
    }
}

// Helper function which truncates after the first zero(included)
fn from_c_str<const N: usize>(b: &[u8; N]) -> Result<ArrayString<N>, Utf8Error> {
    let mut result = ArrayString::from_byte_string(b)?;
    if let Some(i) = &result.find('\0') {
        result.truncate(*i);
    }
    Ok(result)
}

/// A fixed string with the capacity of `N` bytes
/// If the len is less than `N` padding bytes 0 will be added
/// after the data
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Default)]
pub struct FixedPacketString<const N: usize>(pub arrayvec::ArrayString<N>);

impl<const N: usize> PacketTryWrapped for FixedPacketString<N> {
    type Inner = [u8; N];

    fn packet_into_inner(&self) -> Self::Inner {
        // Write the data padded by zero up to N
        let mut buf = [0; N];
        let b = &self.0;
        buf[..b.len()].copy_from_slice(b.as_bytes());
        buf
    }

    fn packet_try_from(v: Self::Inner) -> PacketResult<Self> {
        // Convert the array to the string
        Ok(Self(from_c_str(&v)?))
    }
}

impl<'a, const N: usize> TryFrom<&'a str> for FixedPacketString<N> {
    type Error = CapacityError<&'a str>;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        ArrayString::try_from(value).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use arrayvec::ArrayString;

    use crate::test_util::{test_enc_dec, test_enc_dec_all};
    use proptest::prelude::*;

    use super::FixedPacketString;

    proptest! {
        #[test]
        fn p_str(s: String) {
            test_enc_dec(s);
        }
    }

    #[test]
    fn string() {
        // String / str
        // String uses &str so no need to test that
        test_enc_dec_all(["".to_string(), "AAAAAAAAAAA".to_string(), "\0".to_string()]);
    }

    #[test]
    fn array_string() {
        test_enc_dec_all::<ArrayString<11>>([
            "".try_into().unwrap(),
            "AAAAAAAAAAA".try_into().unwrap(),
            "\0".try_into().unwrap(),
        ]);
    }

    #[test]
    fn fixed_string() {
        test_enc_dec_all::<FixedPacketString<11>>([
            "".try_into().unwrap(),
            "AAAAAAAAAAA".try_into().unwrap(),
            "a".try_into().unwrap(),
        ]);
    }
}
