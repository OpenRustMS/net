pub mod handler;
pub mod resp;
pub mod server_sess;
pub mod session_set;

use std::time::Duration;

use arrayvec::ArrayString;

use crate::{crypto::RoundKey};
use super::codec::handshake::{Handshake, LocaleCode};



pub const DEFAULT_MIGRATE_DELAY: Duration = Duration::from_millis(7500);

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
    locale: LocaleCode,
}

impl BasicHandshakeGenerator {
    /// Create a new handshake generator, will panic if subversion is larger than 2
    pub fn new(version: u16, sub_version: &str, locale: LocaleCode) -> Self {
        Self {
            version,
            sub_version: sub_version.try_into().expect("Subversion"),
            locale,
        }
    }

    /// Create a handshake generator for global v95
    pub fn v95() -> Self {
        Self::new(95, "1", LocaleCode::Global)
    }

    /// Create a handshake generator for global v83
    pub fn v83() -> Self {
        Self::new(83, "1", LocaleCode::Global)
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
