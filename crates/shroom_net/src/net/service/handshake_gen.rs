use crate::net::codec::{
    handshake::{HandshakeVersion, LocaleCode},
    Handshake,
};

/// Handshake generator, to generate a handshake
pub trait HandshakeGenerator {
    /// Generate a new handshake
    fn generate_handshake(&self) -> Handshake;
}

/// Implementation of a very basic Handshake generator
#[derive(Debug, Clone)]
pub struct BasicHandshakeGenerator {
    version: HandshakeVersion,
    locale: LocaleCode,
}

impl BasicHandshakeGenerator {
    /// Create a new handshake generator, will panic if subversion is larger than 2
    pub fn new(version: HandshakeVersion, locale: LocaleCode) -> Self {
        Self { version, locale }
    }

    /// Create a handshake generator for global v95
    pub fn v95() -> Self {
        Self::new(HandshakeVersion::v95(), LocaleCode::Global)
    }

    /// Create a handshake generator for global v83
    pub fn v83() -> Self {
        Self::new(HandshakeVersion::v83(), LocaleCode::Global)
    }
}

impl HandshakeGenerator for BasicHandshakeGenerator {
    fn generate_handshake(&self) -> Handshake {
        // Using thread_rng to generate the round keys
        let rng = rand::thread_rng();
        Handshake::new_random(self.version.clone(), self.locale, rng)
    }
}
