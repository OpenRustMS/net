use cipher::{generic_array::GenericArray, typenum::U16};
use rand::{CryptoRng, Rng, RngCore};

use super::{ig_cipher::IgContext, ROUND_KEY_LEN};



#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq)]
pub struct RoundKey(pub [u8; ROUND_KEY_LEN]);

impl From<[u8; ROUND_KEY_LEN]> for RoundKey {
    fn from(value: [u8; ROUND_KEY_LEN]) -> Self {
        Self(value)
    }
}

impl From<RoundKey> for u32 {
    fn from(value: RoundKey) -> Self {
        u32::from_le_bytes(value.0)
    }
}

impl From<u32> for RoundKey {
    fn from(value: u32) -> Self {
        Self(value.to_le_bytes())
    }
}

impl rand::Fill for RoundKey {
    fn try_fill<R: rand::Rng + ?Sized>(&mut self, rng: &mut R) -> Result<(), rand::Error> {
        let data: [u8; ROUND_KEY_LEN] = rng.gen();
        self.0 = data;
        Ok(())
    }
}

impl RoundKey {
    /// Returns a Roundkey just containing zeros
    pub const fn zero() -> Self {
        RoundKey([0; ROUND_KEY_LEN])
    }

    /// Generate a random round key
    pub fn get_random<R>(mut rng: R) -> Self
    where
        R: CryptoRng + RngCore,
    {
        let mut zero = Self::zero();
        rng.fill(&mut zero);
        zero
    }

    /// Update the round key
    pub fn update(self, ig: &IgContext) -> RoundKey {
        ig.hash(&self.0).into()
    }

    /// Expand the round key to an IV
    pub fn expand(&self) -> GenericArray<u8, U16> {
        array_init::array_init(|i| self.0[i % ROUND_KEY_LEN]).into()
    }
}
