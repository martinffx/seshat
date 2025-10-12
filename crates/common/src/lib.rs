//! Common types and utilities shared across Seshat crates.
//!
//! This crate provides fundamental type definitions, shared utilities,
//! and common abstractions used throughout the Seshat distributed key-value store.

pub mod errors;
pub mod types;

// Re-export commonly used types for convenience
pub use errors::{Error, Result};
pub use types::{LogIndex, NodeId, Term};

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
