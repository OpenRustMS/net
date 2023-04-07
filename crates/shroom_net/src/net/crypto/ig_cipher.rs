use cipher::inout::InOutBuf;

use super::{key::{DEFAULT_SHUFFLE_KEY}, ShuffleKey};


pub struct IgCipher {
    shuffle_key: ShuffleKey,
}

impl Default for IgCipher {
    fn default() -> Self {
        Self::new(DEFAULT_SHUFFLE_KEY.clone())
    }
}

impl IgCipher {
    fn shuffle(&self, a: u8) -> u8 {
        self.shuffle_key[a as usize]
    }


    fn update_key(&self, key: u32, b: u8) -> u32 {
        let mut k = key.to_le_bytes();
        k[0] = k[0].wrapping_add(self.shuffle(k[1]).wrapping_sub(b));
        k[1] = k[1].wrapping_sub(k[2] ^ self.shuffle(b));
        k[2] ^= self.shuffle(k[3]).wrapping_add(b);
        k[3] = k[3].wrapping_sub(k[0].wrapping_sub(self.shuffle(b)));
    
        u32::from_le_bytes(k).rotate_left(3)
    }
    
    fn enc(&self, data: u8, key: [u8; 4]) -> u8 {
        let v = data.rotate_right(4);
        // v(even bits) = (a << 1) & 0xAA(even bits)
        let even = (v & 0xAA) >> 1;
        // v(odd bits) = (a >> 1) & 0x55(odd bits)
        let odd = (v & 0x55) << 1;
    
        let a = even | odd;
        a ^ self.shuffle(key[0])
    }
    
    fn dec(&self, data: u8, key: [u8; 4]) -> u8 {
        let a = self.shuffle(key[0]) ^ data;
        let b = a << 1;
    
        let mut v = a;
        v >>= 1;
        v ^= b;
        v &= 0x55;
        v ^= b;
        v.rotate_left(4)
    }

    pub fn new(shuffle_key: ShuffleKey) -> Self {
        Self {
            shuffle_key
        }
    }

    pub fn inno_hash_n<const N: usize>(&self, data: &[u8; N], mut key: u32) -> u32 {
        for &b in data.iter() {
            key = self.update_key(key, b);
        }

        key
    }

    pub fn inno_hash(&self, data: &[u8], mut key: u32) -> u32 {
        for &b in data.iter() {
            key = self.update_key(key, b);
        }

        key
    }

    pub fn decrypt(&self, buf: InOutBuf<u8>, key: &mut u32) {
        let buf = buf.into_out();
        for b in buf.iter_mut() {
            *b = self.dec(*b, key.to_le_bytes());
            *key = self.update_key(*key, *b);
        }
    }

    pub fn encrypt(&self, buf: InOutBuf<u8>, key: &mut u32) {
        let buf = buf.into_out();
        for b in buf.iter_mut() {
            *b = self.enc(*b, key.to_le_bytes());
            *key = self.update_key(*key, *b);
        }
    }
}



#[cfg(test)]
mod tests {
    use crate::net::crypto::{
        ig_cipher::IgCipher,
        key,
    };

    #[test]
    fn ig_dec_enc() {
        let cipher = IgCipher::default();
        let key = key::DEFAULT_INIT_IG_SEED.0;

        let v = cipher.enc(0x31, key);
        let v = cipher.dec(v, key);
        assert_eq!(v, 0x31);

        let v = cipher.dec(0xff, key);
        assert_eq!(cipher.enc(v, key), 0xff);
    }
}
