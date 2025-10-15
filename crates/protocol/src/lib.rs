//! RESP (REdis Serialization Protocol) implementation
//!
//! This crate provides parsing, encoding, and command handling for the RESP protocol.

pub mod error;
pub mod parser;
pub mod types;

pub use error::{ProtocolError, Result};
pub use parser::RespParser;
pub use types::RespValue;
