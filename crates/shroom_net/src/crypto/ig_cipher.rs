use cipher::inout::InOutBuf;

use super::{ShuffleKey, DEFAULT_INIT_IG_SEED, DEFAULT_SHUFFLE_KEY};

type IgKey = [u8; 4];

/// Context for the ig crypto functions, used to create the hasher and cipher
#[derive(Debug)]
pub struct IgContext {
    shuffle_key: ShuffleKey,
    seed: IgKey,
}

/// Default
pub const DEFAULT_IG_CONTEXT: IgContext = IgContext {
    shuffle_key: *DEFAULT_SHUFFLE_KEY,
    seed: *DEFAULT_INIT_IG_SEED,
};

impl IgContext {
    /// Creates a new hasher with this context
    pub fn hasher(&self) -> IgHasher<'_> {
        IgHasher {
            state: self.seed,
            ctx: self,
        }
    }

    /// Creates a new cipher with this context
    pub fn cipher(&self) -> IgCipher<'_> {
        IgCipher {
            state: self.seed,
            ctx: self,
        }
    }

    /// Hash the data slice
    pub fn hash(&self, data: &[u8]) -> u32 {
        let mut hasher = self.hasher();
        hasher.update(data);
        hasher.finalize()
    }

    /// Get the shuffled value for the value `a`
    fn shuffle(&self, a: u8) -> u8 {
        self.shuffle_key[a as usize]
    }

    /// Updates the given key `k` with the given data
    fn update_key(&self, mut k: [u8; 4], data: u8) -> [u8; 4] {
        k[0] = k[0].wrapping_add(self.shuffle(k[1]).wrapping_sub(data));
        k[1] = k[1].wrapping_sub(k[2] ^ self.shuffle(data));
        k[2] ^= self.shuffle(k[3]).wrapping_add(data);
        k[3] = k[3].wrapping_sub(k[0].wrapping_sub(self.shuffle(data)));

        u32::from_le_bytes(k).rotate_left(3).to_le_bytes()
    }

    /// Encrypt the given data with the given key
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
}

pub struct IgHasher<'ctx> {
    state: IgKey,
    ctx: &'ctx IgContext,
}

impl<'ctx> IgHasher<'ctx> {
    pub fn update(&mut self, data: &[u8]) {
        self.state = data
            .iter()
            .fold(self.state, |key, b| self.ctx.update_key(key, *b))
    }

    pub fn finalize(self) -> u32 {
        u32::from_le_bytes(self.state)
    }
}

pub struct IgCipher<'ctx> {
    ctx: &'ctx IgContext,
    state: IgKey,
}

impl<'ctx> Default for IgCipher<'ctx> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            ctx: &DEFAULT_IG_CONTEXT,
        }
    }
}

impl<'ctx> IgCipher<'ctx> {
    pub fn encrypt(&mut self, buf: InOutBuf<u8>) {
        let buf = buf.into_out();
        for b in buf.iter_mut() {
            let plain = *b;
            *b = self.ctx.enc(plain, self.state);
            self.state = self.ctx.update_key(self.state, plain);
        }
    }

    pub fn decrypt(&mut self, buf: InOutBuf<u8>) {
        let buf = buf.into_out();
        for b in buf.iter_mut() {
            *b = self.ctx.dec(*b, self.state);
            self.state = self.ctx.update_key(self.state, *b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_IG_CONTEXT;


    #[test]
    fn ig_dec_enc() {
        let data: &[&[u8]] = &[&[1u8, 2], &[], &[1]];

        for data in data {
            let mut enc = DEFAULT_IG_CONTEXT.cipher();

            let mut buf = data.to_vec();
            enc.encrypt((buf.as_mut_slice()).into());

            let mut dec = DEFAULT_IG_CONTEXT.cipher();
            dec.decrypt((buf.as_mut_slice()).into());
            assert_eq!(buf, *data);
        }
    }
}
