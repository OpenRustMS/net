use super::{AES_KEY_LEN, RoundKey};

pub const ROUND_SHIFTING_KEY: &[u8; 256] = include_bytes!("../../../../../default_keys/round_shifting_key.bin");
pub const MAPLE_AES_KEY: [u8; AES_KEY_LEN] = *include_bytes!("../../../../../default_keys/aes_key.bin");
pub const INIT_ROUND_KEY: RoundKey = RoundKey(*include_bytes!("../../../../../default_keys/initial_round_key.bin"));