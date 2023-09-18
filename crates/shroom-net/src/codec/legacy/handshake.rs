use std::{io::Read, iter};

use arrayvec::{ArrayString, ArrayVec};
use shroom_pkt::{DecodePacket, EncodePacket, PacketReader, PacketWrapped, PacketWriter};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{
    crypto::{RoundKey, ROUND_KEY_LEN},
    NetError, NetResult,
};

use super::{LocaleCode, MAX_HANDSHAKE_LEN};

const HS_BUF_LEN: usize = MAX_HANDSHAKE_LEN + 2;

/// Handshake buffer
pub type HandshakeBuf = ArrayVec<u8, HS_BUF_LEN>;

/// Codec Handshake
#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
pub struct Handshake {
    /// Version
    pub version: u16,
    // Subversion up to a length of 2
    pub subversion: ArrayString<2>,
    /// Encrypt IV
    pub iv_enc: RoundKey,
    /// Decrypt IV
    pub iv_dec: RoundKey,
    /// Locale
    pub locale: LocaleCode,
}

impl Handshake {
    /// Decode the handshake length
    fn decode_handshake_len(data: [u8; 2]) -> NetResult<usize> {
        let ln = u16::from_le_bytes(data) as usize;
        if ln <= MAX_HANDSHAKE_LEN {
            Ok(ln)
        } else {
            Err(NetError::HandshakeSize(ln))
        }
    }

    /// Read a handshake from the underlying reader async
    pub async fn read_handshake_async<R: AsyncRead + Unpin>(mut r: R) -> NetResult<Self> {
        let mut ln_data = [0u8; 2];
        r.read_exact(&mut ln_data).await?;
        let ln = Self::decode_handshake_len(ln_data)?;

        let mut handshake_data = [0u8; MAX_HANDSHAKE_LEN];
        r.read_exact(&mut handshake_data[..ln]).await?;
        Self::decode_complete(&mut PacketReader::new(&handshake_data[..ln]))
            .map_err(|_| NetError::InvalidHandshake)
    }

    /// Read a shandshake from the underlying reader
    pub fn read_handshake<R: Read>(mut r: R) -> NetResult<Self> {
        let mut ln_data = [0u8; 2];
        r.read_exact(&mut ln_data)?;
        let ln = Self::decode_handshake_len(ln_data)?;

        let mut handshake_data = [0u8; MAX_HANDSHAKE_LEN];
        r.read_exact(&mut handshake_data[..ln])?;

        Self::decode_complete(&mut PacketReader::new(&handshake_data[..ln]))
            .map_err(|_| NetError::InvalidHandshake)
    }

    /// Encode the handshake onto the buffer
    pub fn to_buf(&self) -> HandshakeBuf {
        let mut buf = HandshakeBuf::default();
        let n = self.packet_len();
        buf.extend(iter::repeat(0).take(n + 2));
        let mut pw = PacketWriter::new(buf.as_mut());
        pw.write_u16(n as u16).expect("Handshake len");
        self.encode_packet(&mut pw).unwrap();

        buf
    }
}

// Wrapper to implement encode/decode
impl PacketWrapped for Handshake {
    type Inner = (
        u16,
        ArrayString<2>,
        [u8; ROUND_KEY_LEN],
        [u8; ROUND_KEY_LEN],
        LocaleCode,
    );

    fn packet_into_inner(&self) -> Self::Inner {
        (
            self.version,
            self.subversion,
            self.iv_enc.0,
            self.iv_dec.0,
            self.locale,
        )
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self {
            version: v.0,
            subversion: v.1,
            iv_enc: RoundKey(v.2),
            iv_dec: RoundKey(v.3),
            locale: v.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{codec::legacy::LocaleCode, crypto::RoundKey};

    use super::Handshake;
    use arrayvec::ArrayString;
    use shroom_pkt::test_util::test_enc_dec;

    #[test]
    fn test_handshake_encode_decode() {
        let handshake = Handshake {
            version: 1,
            subversion: ArrayString::try_from("2").unwrap(),
            iv_enc: RoundKey([1u8; 4]),
            iv_dec: RoundKey([2u8; 4]),
            locale: LocaleCode::Global,
        };

        test_enc_dec(handshake);
    }
}
