use super::AES_KEY_LEN;

pub const DEFAULT_SHUFFLE_KEY: &[u8; 256] = include_bytes!("shuffle_key.bin");
pub const DEFAULT_AES_KEY: &[u8; AES_KEY_LEN] = include_bytes!("aes_key.bin");
pub const DEFAULT_INIT_IG_SEED: &[u8; 4] = include_bytes!("initial_round_key.bin");
