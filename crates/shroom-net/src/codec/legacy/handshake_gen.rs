use arrayvec::ArrayString;

use crate::crypto::{RoundKey, ShroomVersion};

use super::{handshake::Handshake, LocaleCode};

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

    pub fn version(&self) -> ShroomVersion {
        ShroomVersion(self.version)
    }

    /// Creates a handshake generator
    pub fn global(v: u16) -> Self {
        Self::new(v, "1", LocaleCode::Global)
    }

    /// Create a handshake generator for global v95
    pub fn v95() -> Self {
        Self::global(95)
    }

    /// Create a handshake generator for global v83
    pub fn v83() -> Self {
        Self::global(83)
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
