//! Key-value service for Seshat distributed store
//!
//! This crate provides the key-value service implementation, including
//! business logic for Redis-compatible commands.
//!
//! # Architecture
//!
//! The KV layer handles:
//! - **Service Logic**: Command routing and validation (future)
//! - **Redis Compatibility**: Implement Redis command semantics
//!
//! Operations (Set, Del) are defined in seshat-storage.
//!
//! # Example
//!
//! ```rust
//! use seshat_storage::Operation;
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

// Re-export Operation types from seshat-storage
pub use seshat_storage::{Operation, OperationError, OperationResult};
