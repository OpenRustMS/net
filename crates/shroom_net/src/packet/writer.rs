use bytes::{BufMut, BytesMut};

use crate::{opcode::NetOpcode, NetError, NetResult, ShroomPacket};

use super::{packet_str_len, shroom128_to_bytes};

#[derive(Debug)]
pub struct PacketWriter<T = BytesMut> {
    pub buf: T,
}

impl Default for PacketWriter<BytesMut> {
    fn default() -> Self {
        Self {
            buf: Default::default(),
        }
    }
}

impl<T> PacketWriter<T> {
    pub fn into_inner(self) -> T {
        self.buf
    }
}

impl PacketWriter<BytesMut> {
    pub fn with_capacity(cap: usize) -> Self {
        Self::new(BytesMut::with_capacity(cap))
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn into_packet(self) -> ShroomPacket {
        ShroomPacket::from_writer(self)
    }
}

impl<T> PacketWriter<T>
where
    T: BufMut,
{
    pub fn new(buf: T) -> Self {
        Self { buf }
    }

    #[inline]
    pub fn check_capacity(&self, cap: usize) -> NetResult<()> {
        if self.buf.remaining_mut() < cap {
            Err(NetError::OutOfCapacity)
        } else {
            Ok(())
        }
    }

    pub fn write_opcode(&mut self, op: impl NetOpcode) -> NetResult<()> {
        self.write_u16(op.into())
    }

    pub fn write_u8(&mut self, v: u8) -> NetResult<()> {
        self.check_capacity(1)?;
        self.buf.put_u8(v);
        Ok(())
    }

    pub fn write_i8(&mut self, v: i8) -> NetResult<()> {
        self.check_capacity(1)?;
        self.buf.put_i8(v);
        Ok(())
    }

    pub fn write_bool(&mut self, v: bool) -> NetResult<()> {
        self.check_capacity(1)?;
        self.write_u8(v.into())
    }

    pub fn write_i16(&mut self, v: i16) -> NetResult<()> {
        self.check_capacity(2)?;
        self.buf.put_i16_le(v);
        Ok(())
    }

    pub fn write_i32(&mut self, v: i32) -> NetResult<()> {
        self.check_capacity(4)?;
        self.buf.put_i32_le(v);
        Ok(())
    }

    pub fn write_i64(&mut self, v: i64) -> NetResult<()> {
        self.check_capacity(8)?;
        self.buf.put_i64_le(v);
        Ok(())
    }

    pub fn write_i128(&mut self, v: i128) -> NetResult<()> {
        self.check_capacity(16)?;
        self.write_u128(v as u128)
    }

    pub fn write_f32(&mut self, v: f32) -> NetResult<()> {
        self.check_capacity(4)?;
        self.buf.put_f32_le(v);
        Ok(())
    }

    pub fn write_f64(&mut self, v: f64) -> NetResult<()> {
        self.check_capacity(8)?;
        self.buf.put_f64_le(v);
        Ok(())
    }

    pub fn write_u16(&mut self, v: u16) -> NetResult<()> {
        self.check_capacity(2)?;
        self.buf.put_u16_le(v);
        Ok(())
    }

    pub fn write_u32(&mut self, v: u32) -> NetResult<()> {
        self.check_capacity(4)?;
        self.buf.put_u32_le(v);
        Ok(())
    }

    pub fn write_u64(&mut self, v: u64) -> NetResult<()> {
        self.check_capacity(8)?;
        self.buf.put_u64_le(v);
        Ok(())
    }

    pub fn write_u128(&mut self, v: u128) -> NetResult<()> {
        self.write_array(&shroom128_to_bytes(v))
    }

    pub fn write_bytes(&mut self, v: &[u8]) -> NetResult<()> {
        self.check_capacity(v.len())?;
        self.buf.put(v);
        Ok(())
    }

    pub fn write_array<const N: usize>(&mut self, v: &[u8; N]) -> NetResult<()> {
        self.check_capacity(N)?;
        self.buf.put(v.as_slice());
        Ok(())
    }

    pub fn write_str(&mut self, v: &str) -> NetResult<()> {
        self.check_capacity(packet_str_len(v))?;
        let b = v.as_bytes();
        self.buf.put_u16_le(b.len() as u16);
        self.buf.put_slice(b);
        Ok(())
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.buf
    }

    pub fn get_ref(&mut self) -> &T {
        &self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::PacketWriter;

    #[test]
    fn write() -> anyhow::Result<()> {
        let mut pw = PacketWriter::with_capacity(64);
        pw.write_u8(0)?;
        pw.write_bytes(&[1, 2, 3, 4])?;

        assert_eq!(pw.len(), 5);
        Ok(())
    }
}
