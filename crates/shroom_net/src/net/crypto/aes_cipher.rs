use std::ops::Add;

use aes::Aes256;
use cipher::{
    block_padding::NoPadding,
    generic_array::GenericArray,
    inout::InOutBuf,
    typenum::{U1000, U16, U460},
    BlockEncrypt, KeyInit,
};

use crate::{NetError, NetResult};

use super::{key::DEFAULT_AES_KEY, RoundKey, AES_BLOCK_LEN};

const BLOCK_LEN: usize = 1460;
const FIRST_BLOCK_LEN: usize = BLOCK_LEN - 4;
type U1460 = <U1000 as Add<U460>>::Output;

pub struct ShroomAESCipher {
    cipher: Aes256,
}

impl Default for ShroomAESCipher {
    fn default() -> Self {
        Self::new(DEFAULT_AES_KEY.as_slice()).unwrap()
    }
}

impl ShroomAESCipher {
    pub fn new(key: &[u8]) -> NetResult<Self> {
        Ok(Self {
            cipher: Aes256::new_from_slice(key).map_err(|_| NetError::InvalidAESKey)?,
        })
    }

    fn get_next_key(&self, key: &mut GenericArray<u8, U16>) {
        self.cipher
            .encrypt_padded::<NoPadding>(key, AES_BLOCK_LEN)
            .unwrap();
    }

    fn crypt_block(&self, mut key: GenericArray<u8, U16>, buf: InOutBuf<'_, '_, u8>) {
        let (aes_blocks, mut aes_tail) = buf.into_chunks::<U16>();
        for mut aes_block in aes_blocks {
            self.get_next_key(&mut key);
            aes_block.xor_in2out(&key);
        }

        self.get_next_key(&mut key);
        aes_tail.xor_in2out(&key[..aes_tail.len()]);
    }

    pub fn crypt(&self, key: RoundKey, buf: InOutBuf<'_, '_, u8>) {
        //Expands the 4 byte round key to a 16 byte IV, this IV is re-used for every chunk
        let iv = key.expand();
        let n = buf.len();

        // Crypt first block
        // TODO: hot path should be optimized as first buffer has no tail and crypt_blocks should get the size
        // Need proper benchmarking for that
        let (first_chunk, buf) = buf.split_at(FIRST_BLOCK_LEN.min(n));
        self.crypt_block(iv, first_chunk);

        // Crypt all middle blocks
        let (blocks, tail_block) = buf.into_chunks::<U1460>();
        for mut block in blocks {
            self.crypt_block(iv, block.get_out().as_mut().into());
        }

        // Crypt tail block
        self.crypt_block(iv, tail_block);
    }
}

#[cfg(test)]
mod tests {
    use crate::net::crypto::RoundKey;

    use super::ShroomAESCipher;


    fn enc_dec(cipher: &ShroomAESCipher, key: RoundKey, data: &mut [u8]) {
        cipher.crypt(key, data.into());
        cipher.crypt(key, data.into());
    }

    const KEY: RoundKey = RoundKey([1, 2, 3, 4]);

    #[test]
    fn en_dec_aes() {
        let aes = ShroomAESCipher::default();
        let data = b"abcdef";

        let mut data_enc = *data;
        enc_dec(&aes, KEY, data_enc.as_mut());
        assert_eq!(*data, data_enc);
    }

    #[test]
    fn en_dec_aes_large() {
        let aes = ShroomAESCipher::default();
        let data = (0..=255).cycle().take(4096).collect::<Vec<u8>>();

        let mut data_enc: Vec<u8> = data.clone();
        enc_dec(&aes, KEY, data_enc.as_mut_slice());
        assert_eq!(data, data_enc);
    }
}
