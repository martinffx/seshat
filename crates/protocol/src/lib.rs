//! RESP (REdis Serialization Protocol) implementation
//!
//! This crate provides parsing, encoding, and command handling for the RESP protocol.

pub mod encoder;
pub mod error;
pub mod inline;
pub mod parser;
pub mod types;

pub use encoder::RespEncoder;
pub use error::{ProtocolError, Result};
pub use inline::InlineCommandParser;
pub use parser::RespParser;
pub use types::RespValue;
