use bytes::{Buf, BufMut};
use shroom_pkt::ShroomPacketData;
use tokio_util::codec::{Decoder, Encoder};

use crate::{
    crypto::{PacketHeader, ShroomCrypto, PACKET_HEADER_LEN},
    NetError, NetResult,
};

use super::MAX_PACKET_LEN;

/// Check the packet length
fn check_packet_len(len: usize) -> NetResult<()> {
    if len > MAX_PACKET_LEN {
        return Err(NetError::FrameSize(len));
    }

    Ok(())
}

pub struct LegacyDecoder(pub ShroomCrypto);

impl Decoder for LegacyDecoder {
    type Item = ShroomPacketData;
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
        self.0.decrypt(packet_data.as_mut().into());
        let pkt = ShroomPacketData::from_data(packet_data.freeze());

        Ok(Some(pkt))
    }
}

pub struct LegacyEncoder(pub ShroomCrypto);

impl<'a> Encoder<&'a [u8]> for LegacyEncoder {
    type Error = NetError;

    fn encode(&mut self, item: &'a [u8], dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let len = item.len();
        check_packet_len(len)?;
        dst.reserve(PACKET_HEADER_LEN + len);

        dst.put_slice(&self.0.encode_header(len as u16));
        dst.put_slice(item);
        self.0
            .encrypt((&mut dst[PACKET_HEADER_LEN..PACKET_HEADER_LEN + len]).into());
        Ok(())
    }
}
