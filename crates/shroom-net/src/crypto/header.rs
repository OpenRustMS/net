use crate::{NetError, NetResult};

use super::{PacketHeader, RoundKey, PACKET_HEADER_LEN};

/// Small helper to work with high low words in a 32 bit integer
struct HiLo32 {
    high: u16,
    low: u16,
}

impl HiLo32 {
    fn from_low_high(high: u16, low: u16) -> Self {
        Self { high, low }
    }

    fn from_le_bytes(b: [u8; PACKET_HEADER_LEN]) -> Self {
        let low = u16::from_le_bytes([b[0], b[1]]);
        let high = u16::from_le_bytes([b[2], b[3]]);
        Self { high, low }
    }

    fn to_le_bytes(&self) -> [u8; PACKET_HEADER_LEN] {
        let mut result = [0; PACKET_HEADER_LEN];
        result[0..2].copy_from_slice(&self.low.to_le_bytes());
        result[2..4].copy_from_slice(&self.high.to_le_bytes());
        result
    }
}

pub fn decode_header(hdr: PacketHeader, key: RoundKey, ver: u16) -> NetResult<u16> {
    let key = key.0;
    let v = HiLo32::from_le_bytes(hdr);
    let key_high = u16::from_le_bytes([key[2], key[3]]);
    let len = v.low ^ v.high;
    let hdr_key = v.low ^ ver;

    if hdr_key != key_high {
        return Err(NetError::InvalidHeader {
            len,
            key: hdr_key,
            expected_key: key_high,
        });
    }

    Ok(len)
}

pub fn encode_header(key: RoundKey, length: u16, ver: u16) -> PacketHeader {
    let key = key.0;
    let key_high = u16::from_le_bytes([key[2], key[3]]);
    let low = key_high ^ ver;
    let hilo = HiLo32::from_low_high(low ^ length, low);
    hilo.to_le_bytes()
}

#[cfg(test)]
mod tests {

    use crate::crypto::RoundKey;

    use super::{decode_header, encode_header};

    const KEY: RoundKey = RoundKey([82, 48, 120, 232]);
    const KEY2: RoundKey = RoundKey([82, 48, 120, 89]);

    #[test]
    fn header_enc_dec() {
        let tests = [
            (44, KEY, -66i16 as u16),
            (2, RoundKey([70, 114, 122, 210]), 83),
            (24, KEY2, -84i16 as u16),
            (627, KEY, -84i16 as u16),
        ];

        for (ln, key, ver) in tests {
            let a = encode_header(key, ln, ver);
            assert_eq!(decode_header(a, key, ver).expect("valid header"), ln)
        }
    }
}
