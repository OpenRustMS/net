pub mod crypto;
pub mod error;

pub mod codec;
pub mod server;
//pub mod service;

pub use error::NetError;
pub type NetResult<T> = Result<T, error::NetError>;
