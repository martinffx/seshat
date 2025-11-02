//! RESP (REdis Serialization Protocol) implementation
//!
//! This crate provides parsing, encoding, and command handling for the RESP protocol.

pub mod buffer_pool;
pub mod codec;
pub mod command;
pub mod encoder;
pub mod error;
pub mod inline;
pub mod parser;
pub mod types;

pub use buffer_pool::BufferPool;
pub use codec::RespCodec;
pub use command::RespCommand;
pub use encoder::RespEncoder;
pub use error::{ProtocolError, Result};
pub use inline::InlineCommandParser;
pub use parser::RespParser;
pub use types::RespValue;
