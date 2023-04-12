pub mod aes_cipher;
mod default_keys;
pub mod header;
pub mod ig_cipher;
mod round_key;
pub mod shanda_cipher;

// Re-exports
pub use default_keys::{DEFAULT_AES_KEY, DEFAULT_INIT_IG_SEED, DEFAULT_SHUFFLE_KEY};
pub use round_key::RoundKey;
pub use ig_cipher::IgCipher;

use std::sync::Arc;

use cipher::inout::InOutBuf;

use crate::NetResult;

use self::{
    aes_cipher::ShroomAESCipher,
    ig_cipher::{IgContext, DEFAULT_IG_CONTEXT},
    shanda_cipher::ShandaCipher,
};

pub const ROUND_KEY_LEN: usize = 4;
pub const AES_KEY_LEN: usize = 32;
pub const AES_BLOCK_LEN: usize = 16;
pub const PACKET_HEADER_LEN: usize = 4;

pub type AesKey = [u8; AES_KEY_LEN];
pub type ShuffleKey = [u8; 256];
pub type PacketHeader = [u8; PACKET_HEADER_LEN];

pub type SharedIgContext = Arc<IgContext>;

/// Represents a version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShroomVersion(pub u16);

impl ShroomVersion {
    pub fn invert(&self) -> Self {
        Self(!self.0)
    }
}

/// Crypto Context providing all keys for this crypto
/// Should be used via `SharedCryptoContext` to avoid
/// re-allocating this for every crypto
#[derive(Debug)]
pub struct CryptoContext {
    pub aes_key: AesKey,
    pub ig_ctx: IgContext,
}

impl Default for CryptoContext {
    fn default() -> Self {
        Self {
            aes_key: *DEFAULT_AES_KEY,
            ig_ctx: DEFAULT_IG_CONTEXT,
        }
    }
}

/// Alias for a shared context
pub type SharedCryptoContext = Arc<CryptoContext>;

pub struct ShroomCrypto {
    shroom_aes_cipher: ShroomAESCipher,
    ctx: SharedCryptoContext,
    round_key: RoundKey,
    version: ShroomVersion,
}

impl ShroomCrypto {
    /// Creates a new crypto used en/decoding packets
    /// with the given context, initial `RoundKey`and version
    pub fn new(ctx: SharedCryptoContext, round_key: RoundKey, version: ShroomVersion) -> Self {
        Self {
            shroom_aes_cipher: ShroomAESCipher::new(&ctx.aes_key).unwrap(),
            round_key,
            ctx,
            version,
        }
    }

    /// Updates the current round key
    fn update_round_key(&mut self) {
        self.round_key = self.round_key.update(&self.ctx.ig_ctx);
    }

    /// Decodes and verifies a header from the given bytes
    pub fn encode_header(&self, length: u16) -> PacketHeader {
        header::encode_header(self.round_key, length, self.version.0)
    }

    /// Decodes and verifies a header from the given bytes
    pub fn decode_header(&self, hdr: PacketHeader) -> NetResult<u16> {
        header::decode_header(hdr, self.round_key, self.version.0)
    }

    /// Decrypt a chunk of data
    /// IMPORTANT: only call this with a full block of data, because the internal state updates
    pub fn encrypt(&mut self, mut data: InOutBuf<u8>) {
        ShandaCipher::encrypt(data.reborrow());
        self.shroom_aes_cipher.crypt(self.round_key, data);
        self.update_round_key();
    }

    /// Encrypts a chunk of data
    /// IMPORTANT: only call this with a full block of data, because the internal state updates
    pub fn decrypt(&mut self, mut data: InOutBuf<u8>) {
        self.shroom_aes_cipher
            .crypt(self.round_key, data.reborrow());
        self.update_round_key();
        ShandaCipher::decrypt(data);
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::{RoundKey, ShroomCrypto};

    use super::{SharedCryptoContext, ShroomVersion};
    const V: ShroomVersion = ShroomVersion(95);

    #[test]
    fn version() {
        assert_eq!(ShroomVersion(95).invert().0 as i16, -96);
        assert_eq!(ShroomVersion(83).invert().0 as i16, -84);
    }

    #[test]
    fn en_dec() {
        let key = RoundKey([1, 2, 3, 4]);

        let mut enc = ShroomCrypto::new(SharedCryptoContext::default(), key, V);
        let mut dec = ShroomCrypto::new(SharedCryptoContext::default(), key, V);
        let data = b"abcdef";

        let mut data_enc = *data;
        enc.encrypt(data_enc.as_mut_slice().into());
        dec.decrypt(data_enc.as_mut_slice().into());

        assert_eq!(*data, data_enc);
        assert_eq!(enc.round_key, dec.round_key);
    }

    #[test]
    fn en_dec_100() {
        let key = RoundKey([1, 2, 3, 4]);

        let mut enc = ShroomCrypto::new(SharedCryptoContext::default(), key, V);
        let mut dec = ShroomCrypto::new(SharedCryptoContext::default(), key, V);
        let data = b"abcdef".to_vec();

        for _ in 0..100 {
            let mut data_enc = data.clone();
            enc.encrypt(data_enc.as_mut_slice().into());
            dec.decrypt(data_enc.as_mut_slice().into());

            assert_eq!(*data, data_enc);
        }
    }
}
