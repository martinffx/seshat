//! Raft consensus wrapper for Seshat distributed key-value store.
//!
//! This crate provides a Raft consensus implementation built on top of
//! `raft-rs`, with custom storage backends and integration with Seshat's
//! architecture.

pub mod config;
pub mod storage;

// Re-export main types for convenience
pub use config::{ClusterConfig, InitialMember, NodeConfig, RaftConfig};
pub use storage::MemStorage;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
