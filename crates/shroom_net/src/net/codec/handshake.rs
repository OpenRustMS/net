use std::{io::{Read, Write}, str::FromStr};

use anyhow::anyhow;
use rand::{RngCore, CryptoRng};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    packet::PacketWrapped,
    DecodePacket, EncodePacket, NetError, NetResult, PacketWriter, crypto::{RoundKey, ROUND_KEY_LEN}, shroom_enum_code, util::must_init_array_str,
};

use super::MAX_HANDSHAKE_LEN;

/// Handshake buffer
pub type HandshakeBuf = [u8; MAX_HANDSHAKE_LEN + 2];

//Locale code for handshake, T means test server
shroom_enum_code!(
    LocaleCode,
    u8,
    Korea = 1,
    KoreaT = 2,
    Japan = 3,
    China = 4,
    ChinaT = 5, 
    Taiwan = 6,
    TaiwanT = 7,
    Global = 8,
    Europe = 9,
    RlsPe = 10
);

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
pub struct HandshakeVersion {
    pub version: u16,
    pub sub_version_len: u16,
    pub sub_version: SubVersion,
}

pub type SubVersion = [u8; 1];


impl HandshakeVersion {
    pub const fn new(version: u16, subversion: SubVersion) -> Self {
        Self {
            version,
            sub_version_len: 1,
            sub_version: subversion
        }
    }

    pub fn major(&self) -> u16 {
        self.version
    }

    pub const fn must_parse(version: u16, subversion: &str) -> Self {
        Self {
            version,
            sub_version_len: 1,
            sub_version: must_init_array_str(subversion)
        }
    }

    pub const fn v83() -> Self {
        Self::must_parse(83, "1")
    }

    pub const fn v95() -> Self {
        Self::must_parse(95, "1")
    }
}

impl FromStr for HandshakeVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (major, minor) = s.split_once('.').ok_or_else(|| anyhow!("Invalid version: {s}"))?;
        
        Ok(Self::new(
            major.parse()?,
            minor.as_bytes().try_into()?
        ))
    }
}

/// Codec Handshake
#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
pub struct Handshake {
    /// Version
    pub version: HandshakeVersion,
    /// Encrypt IV
    pub iv_enc: RoundKey,
    /// Decrypt IV
    pub iv_dec: RoundKey,
    /// Locale
    pub locale: LocaleCode,
}

impl Handshake {
    /// Generates a handshake with random IVs
    pub fn new_random<R: RngCore + CryptoRng>(version: HandshakeVersion, locale: LocaleCode, mut rng: R) -> Self {
        Self {
            version,
            iv_dec: RoundKey::get_random(&mut rng),
            iv_enc: RoundKey::get_random(&mut rng),
            locale
        }
    }


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
        Self::decode_from_data(&handshake_data[..ln]).map_err(|_| NetError::InvalidHandshake)
    }

    /// Read a shandshake from the underlying reader
    pub fn read_handshake<R: Read>(mut r: R) -> NetResult<Self> {
        let mut ln_data = [0u8; 2];
        r.read_exact(&mut ln_data)?;
        let ln = Self::decode_handshake_len(ln_data)?;

        let mut handshake_data = [0u8; MAX_HANDSHAKE_LEN];
        r.read_exact(&mut handshake_data[..ln])?;
        Self::decode_from_data(&handshake_data[..ln]).map_err(|_| NetError::InvalidHandshake)
    }

    /// Write a handshake async
    pub async fn write_handshake_async<W: AsyncWrite + Unpin>(&self, mut w: W) -> NetResult<()> {
        let mut buf = HandshakeBuf::default();
        let n = self.encode_with_len(&mut buf);
        w.write_all(&buf[..n]).await?;

        Ok(())
    }

    /// Write handshake
    pub fn write_handshake<W: Write>(&self, mut w: W) -> NetResult<()> {
        let mut buf = HandshakeBuf::default();
        let n = self.encode_with_len(&mut buf);
        w.write_all(&buf[..n])?;

        Ok(())
    }

    /// Encode the handshake onto the buffer
    pub fn encode_with_len(&self, buf: &mut HandshakeBuf) -> usize {
        let n = self.packet_len();

        let mut pw = PacketWriter::new(buf.as_mut());
        pw.write_u16(n as u16).expect("Handshake len");
        self.encode_packet(&mut pw).unwrap();

        n + 2
    }
}

// Wrapper to implement encode/decode
impl PacketWrapped for Handshake {
    type Inner = (
        u16,
        u16,
        SubVersion,
        [u8; ROUND_KEY_LEN],
        [u8; ROUND_KEY_LEN],
        LocaleCode,
    );

    fn packet_into_inner(&self) -> Self::Inner {
        (
            self.version.version,
            self.version.sub_version_len,
            self.version.sub_version,
            self.iv_enc.0,
            self.iv_dec.0,
            self.locale,
        )
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self {
            version: HandshakeVersion::new(v.0, v.2),
            iv_enc: RoundKey(v.3),
            iv_dec: RoundKey(v.4),
            locale: v.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{DecodePacket, EncodePacket, PacketWriter, ShroomPacket, crypto::RoundKey, net::codec::handshake::{LocaleCode, HandshakeVersion}};

    use super::Handshake;

    #[test]
    fn test_handshake_encode_decode() {
        let handshake = Handshake {
            version: HandshakeVersion::must_parse(83, "2"),
            iv_enc: RoundKey([1u8; 4]),
            iv_dec: RoundKey([2u8; 4]),
            locale: LocaleCode::Global,
        };

        let mut pw = PacketWriter::default();
        handshake.encode_packet(&mut pw).unwrap();
        let pkt = ShroomPacket::from_writer(pw);
        let mut pr = pkt.into_reader();
        let dec = Handshake::decode_packet(&mut pr).unwrap();

        assert_eq!(handshake, dec);
    }
}
