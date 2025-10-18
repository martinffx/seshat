//! Key-value service for Seshat distributed store
//!
//! This crate provides the key-value service implementation, including
//! operation definitions and business logic for Redis-compatible commands.
//!
//! # Architecture
//!
//! The KV layer handles:
//! - **Operations**: State machine commands (Set, Del)
//! - **Service Logic**: Command routing and validation (future)
//! - **Redis Compatibility**: Implement Redis command semantics
//!
//! # Example
//!
//! ```rust
//! use seshat_kv::Operation;
//! use std::collections::HashMap;
//!
//! let mut state = HashMap::new();
//! let op = Operation::Set {
//!     key: b"foo".to_vec(),
//!     value: b"bar".to_vec(),
//! };
//! let result = op.apply(&mut state).unwrap();
//! assert_eq!(result, b"OK");
//! ```

pub mod operations;

// Re-export commonly used types for convenience
pub use operations::{Operation, OperationError, OperationResult};
