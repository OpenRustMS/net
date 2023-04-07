use super::{AES_KEY_LEN, RoundKey};

pub const DEFAULT_SHUFFLE_KEY: &[u8; 256] = include_bytes!("default_keys/shuffle_key.bin");
pub const DEFAULT_AES_KEY: &[u8; AES_KEY_LEN] = include_bytes!("default_keys/aes_key.bin");
pub const DEFAULT_INIT_IG_SEED: RoundKey = RoundKey(*include_bytes!("default_keys/initial_round_key.bin"));