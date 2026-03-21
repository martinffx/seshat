//! Error types for the storage layer.
//!
//! This module provides comprehensive error handling for RocksDB storage operations.
//! All errors implement `std::error::Error` and `Display` via the `thiserror` crate.

use std::io;
use thiserror::Error;

/// Comprehensive error type for storage operations.
///
/// This enum covers all possible error conditions in the storage layer:
/// - Database errors (RocksDB)
/// - I/O errors (file system operations)
/// - Serialization/deserialization errors
/// - Validation errors (column families, log indices)
/// - Data corruption
/// - Configuration errors
///
/// # Examples
///
/// ```
/// use seshat_storage::{StorageError, Result};
///
/// fn example() -> Result<()> {
///     Err(StorageError::ColumnFamilyNotFound {
///         name: "data_kv".to_string(),
///     })
/// }
/// ```
#[derive(Debug, Error)]
pub enum StorageError {
    /// RocksDB database error.
    ///
    /// Wraps underlying RocksDB errors such as:
    /// - Database corruption
    /// - Write failures
    /// - Read failures
    /// - Compaction errors
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    /// I/O error during file operations.
    ///
    /// Wraps std::io::Error for operations like:
    /// - Opening database directory
    /// - Creating snapshots
    /// - Reading/writing files
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Protobuf deserialization error.
    ///
    /// Occurs when stored data cannot be decoded, indicating:
    /// - Data corruption
    /// - Version mismatch
    /// - Invalid format
    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    /// Column family not found.
    ///
    /// Indicates an attempt to access a column family that doesn't exist.
    /// This usually means:
    /// - Column family was not created during initialization
    /// - Incorrect column family name used
    ///
    /// # Fields
    /// - `name`: The name of the missing column family
    #[error("Column family not found: {name}")]
    ColumnFamilyNotFound {
        /// Name of the missing column family
        name: String,
    },

    /// Invalid log index during append operation.
    ///
    /// Log indices must be sequential starting from 1 with no gaps.
    /// This error indicates:
    /// - Gap detected (expected N, got M where M > N+1)
    /// - Duplicate index (expected N, got M where M <= N-1)
    /// - First entry not starting at index 1
    ///
    /// # Fields
    /// - `cf`: Column family name where the violation occurred
    /// - `expected`: The expected log index
    /// - `got`: The actual log index provided
    /// - `reason`: Human-readable explanation of the violation
    #[error("Invalid log index in CF {cf}: expected {expected}, got {got} ({reason})")]
    InvalidLogIndex {
        /// Column family name
        cf: String,
        /// Expected log index
        expected: u64,
        /// Actual log index received
        got: u64,
        /// Explanation of why the index is invalid
        reason: String,
    },

    /// Snapshot operation failed.
    ///
    /// Occurs during snapshot creation or restoration when:
    /// - Disk space is insufficient
    /// - Path is not accessible
    /// - Permissions are incorrect
    ///
    /// # Fields
    /// - `path`: Path where the snapshot operation was attempted
    /// - `reason`: Explanation of the failure
    #[error("Snapshot operation failed at {path}: {reason}")]
    SnapshotFailed {
        /// Snapshot path
        path: String,
        /// Reason for failure
        reason: String,
    },

    /// Data corruption detected.
    ///
    /// Indicates stored data is corrupted or invalid:
    /// - Log key format is incorrect
    /// - Data cannot be decoded
    /// - Integrity check failed
    ///
    /// # Fields
    /// - `cf`: Column family containing corrupted data
    /// - `key`: Raw key bytes where corruption was detected
    /// - `reason`: Description of the corruption
    #[error("Corrupted data in CF {cf} at key {key:?}: {reason}")]
    CorruptedData {
        /// Column family name
        cf: String,
        /// Raw key bytes (may be binary)
        key: Vec<u8>,
        /// Description of the corruption
        reason: String,
    },

    /// Version mismatch between expected and actual.
    ///
    /// Occurs when:
    /// - Stored data has a different version than expected
    /// - Schema migration is needed
    /// - Incompatible data format
    ///
    /// # Fields
    /// - `expected`: Expected version number
    /// - `got`: Actual version found in data
    #[error("Version mismatch: expected {expected}, got {got}")]
    VersionMismatch {
        /// Expected version
        expected: u32,
        /// Actual version found
        got: u32,
    },

    /// Invalid configuration parameter.
    ///
    /// Indicates a configuration value is invalid:
    /// - Out of range
    /// - Wrong type
    /// - Conflicting with other settings
    ///
    /// # Fields
    /// - `field`: Name of the invalid configuration field
    /// - `reason`: Explanation of why it's invalid
    #[error("Invalid configuration for {field}: {reason}")]
    InvalidConfig {
        /// Configuration field name
        field: String,
        /// Reason for invalidity
        reason: String,
    },

    /// Invalid column family for the requested operation.
    ///
    /// Certain operations only work on specific column families.
    /// For example:
    /// - `append_log_entry` only works on `*_raft_log` CFs
    /// - Some operations may be restricted to specific CF types
    ///
    /// # Fields
    /// - `cf`: Column family name
    /// - `reason`: Explanation of why it's invalid for this operation
    #[error("Invalid column family for operation: {cf} ({reason})")]
    InvalidColumnFamily {
        /// Column family name
        cf: String,
        /// Reason why it's invalid
        reason: String,
    },
}

/// Result type alias for storage operations.
///
/// This is a convenience type that uses `StorageError` as the error type.
/// It allows for more concise function signatures.
///
/// # Examples
///
/// ```
/// use seshat_storage::Result;
///
/// fn get_value() -> Result<Vec<u8>> {
///     Ok(vec![1, 2, 3])
/// }
/// ```
pub type Result<T> = std::result::Result<T, StorageError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<StorageError>();
    }

    #[test]
    fn test_error_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<StorageError>();
    }

    #[test]
    fn test_result_type_works() {
        fn example() -> Result<i32> {
            Ok(42)
        }
        assert_eq!(example().unwrap(), 42);
    }
}
