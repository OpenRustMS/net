use bytes::BufMut;
use derive_more::{Deref, DerefMut, From, Into};
use either::Either;

use crate::{NetResult, PacketReader, PacketWriter};

use super::{DecodePacket, EncodePacket};

/// Helper trait for dealing with conditional En/decoding
pub trait PacketConditional<'de>: Sized {
    /// Encode if if the cond evaluates to true
    fn encode_packet_cond<B: BufMut>(&self, cond: bool, pw: &mut PacketWriter<B>) -> NetResult<()>;
    /// Decode if the cond evaluates to true
    fn decode_packet_cond(cond: bool, pr: &mut PacketReader<'de>) -> NetResult<Self>;
    /// Length based on cond
    fn packet_len_cond(&self, cond: bool) -> usize;
}

/// Conditional Option
#[derive(Debug, PartialEq, Eq, Clone, Copy, From, Into, Deref, DerefMut)]
pub struct CondOption<T>(pub Option<T>);

impl<T> Default for CondOption<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: EncodePacket> EncodePacket for CondOption<T> {
    fn encode_packet<B: BufMut>(&self, pw: &mut PacketWriter<B>) -> NetResult<()> {
        self.0
            .as_ref()
            .map(|p| p.encode_packet(pw))
            .unwrap_or(Ok(()))
    }

    const SIZE_HINT: Option<usize> = None;

    fn packet_len(&self) -> usize {
        self.0.as_ref().map(|v| v.packet_len()).unwrap_or(0)
    }
}

impl<'de, T> PacketConditional<'de> for CondOption<T>
where
    T: EncodePacket + DecodePacket<'de>,
{
    fn encode_packet_cond<B: BufMut>(&self, cond: bool, pw: &mut PacketWriter<B>) -> NetResult<()> {
        if cond {
            self.as_ref().expect("Must have value").encode_packet(pw)?;
        }
        Ok(())
    }

    fn decode_packet_cond(cond: bool, pr: &mut PacketReader<'de>) -> NetResult<Self> {
        Ok(Self(if cond {
            Some(T::decode_packet(pr)?)
        } else {
            None
        }))
    }

    fn packet_len_cond(&self, cond: bool) -> usize {
        cond.then(|| self.as_ref().expect("Must have value").packet_len())
            .unwrap_or(0)
    }
}

/// Conditional either type, cond false => Left, true => Right
#[derive(Debug, PartialEq, Eq, Clone, Copy, From, Into, Deref, DerefMut)]
pub struct CondEither<L, R>(pub Either<L, R>);

impl<'de, L, R> PacketConditional<'de> for CondEither<L, R>
where
    L: EncodePacket + DecodePacket<'de>,
    R: EncodePacket + DecodePacket<'de>,
{
    fn encode_packet_cond<B: BufMut>(&self, cond: bool, pw: &mut PacketWriter<B>) -> NetResult<()> {
        if cond {
            self
                .as_ref()
                .left()
                .expect("must have value")
                .encode_packet(pw)
        } else {
            self
                .as_ref()
                .right()
                .expect("must have value")
                .encode_packet(pw)
        }
    }

    fn decode_packet_cond(cond: bool, pr: &mut PacketReader<'de>) -> NetResult<Self> {
        Ok(Self(if cond {
            Either::Left(L::decode_packet(pr)?)
        } else {
            Either::Right(R::decode_packet(pr)?)
        }))
    }

    fn packet_len_cond(&self, _cond: bool) -> usize {
        //TODO use cond?
        match &self.0 {
            Either::Left(v) => v.packet_len(),
            Either::Right(v) => v.packet_len(),
        }
    }
}