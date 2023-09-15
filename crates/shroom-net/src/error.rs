use std::{io, str::Utf8Error};

use shroom_pkt::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("IO")]
    IO(#[from] io::Error),
    #[error("Packet")]
    Packet(#[from] Error),
    #[error("string utf8 error")]
    StringUtf8(#[from] Utf8Error),
    #[error("String limit {0} exceeed")]
    StringLimit(usize),
    #[error("Invalid header with key: {key:X}, expected: {expected_key:X}, len: {len}")]
    InvalidHeader {
        len: u16,
        key: u16,
        expected_key: u16,
    },
    #[error("Invalid enum discriminant {0}")]
    InvalidEnumDiscriminant(usize),
    #[error("Invalid enum primitive {0}")]
    InvalidEnumPrimitive(u32),
    #[error("Frame of length {0} is too large.")]
    FrameSize(usize),
    #[error("Handshake of length {0} is too large.")]
    HandshakeSize(usize),
    #[error("Unable to read handshake")]
    InvalidHandshake,
    #[error("Invalid AES key")]
    InvalidAESKey,
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(i64),
    #[error("Invalid opcode: {0:X}")]
    InvalidOpcode(u16),
    #[error("Migrated")]
    Migrated,
    #[error("Ping timeout")]
    PingTimeout,
}
