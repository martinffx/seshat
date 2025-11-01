//! Key-value service for Seshat distributed store
//!
//! This crate provides the key-value service implementation, including
//! business logic for Redis-compatible commands and Raft integration.
//!
//! # Architecture
//!
//! The KV layer handles:
//! - **Service Logic**: Command routing and validation (future)
//! - **Redis Compatibility**: Implement Redis command semantics
//! - **Raft Integration**: Propose operations to Raft cluster via RaftNode
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

// Raft node integration module
pub mod raft_node;

// Re-export RaftNode for convenience
pub use raft_node::{RaftNode, RaftNodeError};
