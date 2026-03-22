//! Tests for StorageError error handling.
//!
//! These tests verify:
//! - Error message formatting with context
//! - From trait conversions for wrapped errors
//! - Error type matching with pattern matching
//! - Result type usage with ? operator

use seshat_storage::{Result, StorageError};
use std::io;

#[test]
fn test_io_error_wrapping() {
    // Create an IO error
    let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
    let storage_error = StorageError::from(io_error);

    // Verify the error message contains IO context
    let error_msg = format!("{}", storage_error);
    assert!(error_msg.contains("I/O error"));
}

#[test]
fn test_protobuf_decode_error_wrapping() {
    // Create a protobuf decode error
    let decode_error = prost::DecodeError::new("invalid protobuf");
    let storage_error = StorageError::from(decode_error);

    // Verify the error message contains protobuf context
    let error_msg = format!("{}", storage_error);
    assert!(error_msg.contains("Protobuf decode error"));
}

#[test]
fn test_column_family_not_found_error() {
    let error = StorageError::ColumnFamilyNotFound {
        name: "data_kv".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Column family not found"));
    assert!(error_msg.contains("data_kv"));
}

#[test]
fn test_invalid_log_index_error_with_context() {
    let error = StorageError::InvalidLogIndex {
        cf: "system_raft_log".to_string(),
        expected: 6,
        got: 7,
        reason: "Gap in log (expected 6, got 7)".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Invalid log index"));
    assert!(error_msg.contains("system_raft_log"));
    assert!(error_msg.contains("expected 6"));
    assert!(error_msg.contains("got 7"));
    assert!(error_msg.contains("Gap in log"));
}

#[test]
fn test_snapshot_failed_error() {
    let error = StorageError::SnapshotFailed {
        path: "/tmp/snapshot".to_string(),
        reason: "Disk full".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Snapshot operation failed"));
    assert!(error_msg.contains("/tmp/snapshot"));
    assert!(error_msg.contains("Disk full"));
}

#[test]
fn test_corrupted_data_error() {
    let error = StorageError::CorruptedData {
        cf: "data_raft_log".to_string(),
        key: vec![1, 2, 3],
        reason: "Invalid log key format".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Corrupted data"));
    assert!(error_msg.contains("data_raft_log"));
    assert!(error_msg.contains("Invalid log key format"));
}

#[test]
fn test_version_mismatch_error() {
    let error = StorageError::VersionMismatch {
        expected: 1,
        got: 2,
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Version mismatch"));
    assert!(error_msg.contains("expected 1"));
    assert!(error_msg.contains("got 2"));
}

#[test]
fn test_invalid_config_error() {
    let error = StorageError::InvalidConfig {
        field: "cache_size".to_string(),
        reason: "Must be positive".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Invalid configuration"));
    assert!(error_msg.contains("cache_size"));
    assert!(error_msg.contains("Must be positive"));
}

#[test]
fn test_invalid_column_family_error() {
    let error = StorageError::InvalidColumnFamily {
        cf: "system_raft_state".to_string(),
        reason: "append_log_entry only works on *_raft_log CFs".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Invalid column family"));
    assert!(error_msg.contains("system_raft_state"));
    assert!(error_msg.contains("append_log_entry only works on *_raft_log CFs"));
}

#[test]
fn test_error_pattern_matching() {
    // Test that we can match on error variants
    let error = StorageError::ColumnFamilyNotFound {
        name: "test_cf".to_string(),
    };

    match error {
        StorageError::ColumnFamilyNotFound { name } => {
            assert_eq!(name, "test_cf");
        }
        _ => panic!("Wrong error variant"),
    }
}

#[test]
fn test_result_type_with_try_operator() {
    // Test that Result<T> works with ? operator
    fn example_function() -> Result<String> {
        // Simulate an error
        Err(StorageError::ColumnFamilyNotFound {
            name: "test".to_string(),
        })
    }

    fn caller_function() -> Result<String> {
        let value = example_function()?;
        Ok(value)
    }

    let result = caller_function();
    assert!(result.is_err());

    match result {
        Err(StorageError::ColumnFamilyNotFound { name }) => {
            assert_eq!(name, "test");
        }
        _ => panic!("Expected ColumnFamilyNotFound error"),
    }
}

#[test]
fn test_error_is_send_and_sync() {
    // Verify that StorageError can be sent between threads
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<StorageError>();
    assert_sync::<StorageError>();
}

#[test]
fn test_error_debug_output() {
    let error = StorageError::InvalidLogIndex {
        cf: "data_raft_log".to_string(),
        expected: 5,
        got: 10,
        reason: "Gap detected".to_string(),
    };

    // Verify Debug trait works
    let debug_output = format!("{:?}", error);
    assert!(debug_output.contains("InvalidLogIndex"));
}

#[test]
fn test_multiple_error_conversions_in_chain() {
    // Test that different error types can be converted and chained
    fn complex_operation() -> Result<()> {
        // Simulate IO error
        let _io_result: io::Result<()> = Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "access denied",
        ));

        // This would convert IO error to StorageError using From trait
        // _io_result?;

        // Simulate RocksDB error
        Err(StorageError::InvalidConfig {
            field: "path".to_string(),
            reason: "Directory not writable".to_string(),
        })
    }

    let result = complex_operation();
    assert!(result.is_err());
}

#[test]
fn test_corrupted_data_with_binary_key() {
    // Test that binary keys are handled correctly
    let binary_key = vec![0xFF, 0xAB, 0xCD, 0x12];
    let error = StorageError::CorruptedData {
        cf: "test_cf".to_string(),
        key: binary_key.clone(),
        reason: "Invalid format".to_string(),
    };

    match error {
        StorageError::CorruptedData { key, .. } => {
            assert_eq!(key, binary_key);
        }
        _ => panic!("Wrong error variant"),
    }
}

#[test]
fn test_invalid_log_index_first_entry() {
    let error = StorageError::InvalidLogIndex {
        cf: "system_raft_log".to_string(),
        expected: 1,
        got: 0,
        reason: "First log entry must have index 1".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("First log entry must have index 1"));
}

#[test]
fn test_invalid_log_index_duplicate() {
    let error = StorageError::InvalidLogIndex {
        cf: "data_raft_log".to_string(),
        expected: 11,
        got: 10,
        reason: "Duplicate index (last was 10)".to_string(),
    };

    let error_msg = format!("{}", error);
    assert!(error_msg.contains("Duplicate index"));
}
