pub mod handler;
pub mod packet_buffer;
pub mod resp;
pub mod server_sess;
pub mod session_set;

use arrayvec::ArrayString;

use super::{codec::handshake::Handshake, crypto::RoundKey};

/// Handshake generator, to generate a handshake
pub trait HandshakeGenerator {
    /// Generate a new handshake
    fn generate_handshake(&self) -> Handshake;
}

/// Implementation of a very basic Handshake generator
#[derive(Debug, Clone)]
pub struct BasicHandshakeGenerator {
    version: u16,
    sub_version: ArrayString<2>,
    locale: u8,
}

impl BasicHandshakeGenerator {
    pub fn new(version: u16, sub_version: &str, locale: u8) -> Self {
        Self {
            version,
            sub_version: sub_version.try_into().expect("Subversion"),
            locale,
        }
    }

    pub fn v95() -> Self {
        Self::new(95, "1", 8)
    }

    pub fn v83() -> Self {
        Self::new(83, "1", 8)
    }
}

impl HandshakeGenerator for BasicHandshakeGenerator {
    fn generate_handshake(&self) -> Handshake {
        // Using thread_rng to generate the round keys
        let mut rng = rand::thread_rng();
        Handshake {
            version: self.version,
            subversion: self.sub_version,
            iv_enc: RoundKey::get_random(&mut rng),
            iv_dec: RoundKey::get_random(&mut rng),
            locale: self.locale,
        }
    }
}
