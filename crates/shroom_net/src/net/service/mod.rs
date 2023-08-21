pub mod handler;
pub mod resp;
pub mod server_sess;
pub mod session_set;
pub mod handshake_gen;

use std::time::Duration;
pub use handshake_gen::*;

pub const DEFAULT_MIGRATE_DELAY: Duration = Duration::from_millis(7500);

