use bytes::{Buf, BufMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::{
    net::crypto::{PacketHeader, ShroomCrypto, ShroomVersion, PACKET_HEADER_LEN, ShroomCryptoKeys},
    NetError, NetResult, ShroomPacket,
};

use super::{handshake::Handshake, MAX_PACKET_LEN};

// Struct for the encoder which ensures only this crate can actually use it
// Because the encoding has to be done prior with the raw encode buffer
// Check `ShroomSession::send_packet` for this
pub struct EncodeItem(pub(crate) usize);

pub struct PacketCodec {
    pub(crate) decode: PacketDecodeCodec,
    pub(crate) encode: PacketEncodeCodec,
}

impl PacketCodec {
    pub fn from_client_handshake(keys: &ShroomCryptoKeys, handshake: Handshake) -> Self {
        let v = ShroomVersion(handshake.version);
        Self {
            decode: PacketDecodeCodec(ShroomCrypto::new(keys, handshake.iv_dec, v.invert())),
            encode: PacketEncodeCodec(ShroomCrypto::new(keys, handshake.iv_enc, v))
        }
    }

    pub fn from_server_handshake(keys: &ShroomCryptoKeys, handshake: Handshake) -> Self {
        let v = ShroomVersion(handshake.version);
        Self {
            decode: PacketDecodeCodec(ShroomCrypto::new(keys, handshake.iv_enc, v)),
            encode: PacketEncodeCodec(ShroomCrypto::new(keys, handshake.iv_dec, v.invert()))
        }
    }
}

fn check_packet_len(len: usize) -> NetResult<()> {
    if len > MAX_PACKET_LEN {
        return Err(NetError::FrameSize(len));
    }

    Ok(())
}

pub struct PacketDecodeCodec(pub ShroomCrypto);

impl Decoder for PacketCodec {
    type Item = ShroomPacket;
    type Error = NetError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode.decode(src)
    }
}

impl Decoder for PacketDecodeCodec {
    type Item = ShroomPacket;
    type Error = NetError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < PACKET_HEADER_LEN {
            return Ok(None);
        }
        let hdr: PacketHeader = src[..PACKET_HEADER_LEN].try_into().expect("Packet header");
        let length = self.0.decode_header(hdr)? as usize;

        // Verify the packet is not great than the maximum limit
        check_packet_len(length)?;

        // Try to read the actual payload
        let total_len = PACKET_HEADER_LEN + length;

        //Read data
        if src.len() < total_len {
            src.reserve(total_len - src.len());
            return Ok(None);
        }

        src.advance(PACKET_HEADER_LEN);
        let mut packet_data = src.split_to(length);
        self.0.decrypt(packet_data.as_mut());
        let pkt = ShroomPacket::from_data(packet_data.freeze());

        Ok(Some(pkt))
    }
}

pub struct PacketEncodeCodec(pub ShroomCrypto);

impl<'a> Encoder<&'a [u8]> for PacketCodec {
    type Error = NetError;

    fn encode(&mut self, item: &'a [u8], dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        self.encode.encode(item, dst)
    }
}

impl<'a> Encoder<&'a [u8]> for PacketEncodeCodec {
    type Error = NetError;

    fn encode(&mut self, item: &'a [u8], dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let len = item.len();
        check_packet_len(len)?;
        dst.reserve(PACKET_HEADER_LEN + len);

        dst.put_slice(&self.0.encode_header(len as u16));
        dst.put_slice(item);
        self.0
            .encrypt(&mut dst[PACKET_HEADER_LEN..PACKET_HEADER_LEN + len]);
        Ok(())
    }
}
