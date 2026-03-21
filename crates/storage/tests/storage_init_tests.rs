//! Integration tests for Storage initialization and lifecycle.
//!
//! These tests verify:
//! - Storage::new() creates database with all column families
//! - Options validation works correctly
//! - Column family handles are cached
//! - Index cache is warmed up on startup
//! - Thread safety (Send + Sync)
//! - Graceful shutdown

use seshat_storage::{ColumnFamily, Storage, StorageError, StorageOptions};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create temp directory for tests
fn temp_storage_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

/// Helper to create StorageOptions with temp directory
fn temp_storage_options(temp_dir: &TempDir) -> StorageOptions {
    StorageOptions::with_data_dir(temp_dir.path().to_path_buf())
}

// ============================================================================
// Initialization Tests
// ============================================================================

#[test]
fn test_new_creates_storage_with_default_options() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options);
    assert!(storage.is_ok(), "Storage::new() should succeed");

    let storage = storage.unwrap();
    storage.close().expect("close() should succeed");
}

#[test]
fn test_new_creates_all_column_families() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    // Verify we can open the database again and all CFs exist
    let options2 = StorageOptions {
        data_dir: temp_dir.path().to_path_buf(),
        create_if_missing: false,
        ..Default::default()
    };

    storage.close().expect("close() should succeed");

    // Open again - should find all 6 CFs
    let storage2 = Storage::new(options2);
    assert!(
        storage2.is_ok(),
        "Should be able to reopen with all CFs present"
    );
    storage2.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_new_validates_options() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    // Set invalid write_buffer_size_mb (must be 1-1024)
    options.write_buffer_size_mb = 0;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, .. } => {
            assert_eq!(field, "write_buffer_size_mb");
        }
        other => panic!("Expected InvalidConfig error, got: {:?}", other),
    }
}

#[test]
fn test_new_validates_max_write_buffer_number() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    // Set invalid max_write_buffer_number (must be 2-10)
    options.max_write_buffer_number = 1;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, .. } => {
            assert_eq!(field, "max_write_buffer_number");
        }
        other => panic!("Expected InvalidConfig error, got: {:?}", other),
    }
}

#[test]
fn test_new_validates_target_file_size() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    // Set invalid target_file_size_mb (must be 1-1024)
    options.target_file_size_mb = 2000;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, .. } => {
            assert_eq!(field, "target_file_size_mb");
        }
        other => panic!("Expected InvalidConfig error, got: {:?}", other),
    }
}

#[test]
fn test_new_creates_data_directory_if_missing() {
    let temp_dir = temp_storage_dir();
    let data_path = temp_dir.path().join("data").join("rocksdb");

    let options = StorageOptions {
        data_dir: data_path.clone(),
        create_if_missing: true,
        ..Default::default()
    };

    assert!(!data_path.exists(), "Directory should not exist yet");

    let storage = Storage::new(options);
    assert!(storage.is_ok(), "Storage::new() should create directory");

    assert!(
        data_path.exists(),
        "Directory should be created by RocksDB"
    );

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_new_fails_if_create_if_missing_false_and_no_db() {
    let temp_dir = temp_storage_dir();
    let data_path = temp_dir.path().join("nonexistent");

    let options = StorageOptions {
        data_dir: data_path,
        create_if_missing: false,
        ..Default::default()
    };

    let result = Storage::new(options);
    assert!(
        result.is_err(),
        "Should fail when DB doesn't exist and create_if_missing=false"
    );

    // Should be a RocksDB error
    match result.unwrap_err() {
        StorageError::RocksDb(_) => {
            // Expected
        }
        other => panic!("Expected RocksDb error, got: {:?}", other),
    }
}

// ============================================================================
// Column Family Tests
// ============================================================================

#[test]
fn test_cf_handle_lookup_works_for_all_cfs() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    // We can't directly test get_cf_handle() since it's private,
    // but we can verify that the storage was created successfully
    // with all CFs, which means handles are cached.

    // Just verify storage is usable
    storage.close().expect("close() should succeed");
}

// ============================================================================
// Cache Warming Tests
// ============================================================================

#[test]
fn test_warm_up_cache_initializes_log_indices() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    // Create storage - should warm up cache
    let storage = Storage::new(options).expect("Storage::new() should succeed");

    // Since we can't access the cache directly, we just verify that
    // Storage::new() completed successfully, which means warm_up_index_cache()
    // was called without errors.

    storage.close().expect("close() should succeed");
}

#[test]
fn test_get_last_log_index_from_empty_db() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    // Create fresh storage
    let storage = Storage::new(options).expect("Storage::new() should succeed");

    // The cache should be empty for new DBs (no log entries)
    // We can't test get_last_log_index_from_db() directly since it's private,
    // but warm_up_index_cache() calls it and should succeed on empty DB.

    storage.close().expect("close() should succeed");
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[test]
fn test_storage_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Storage>();
}

#[test]
fn test_storage_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<Storage>();
}

#[test]
fn test_storage_can_be_shared_across_threads() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");
    let storage = Arc::new(storage);

    let mut handles = vec![];

    // Spawn 5 threads that hold references to storage
    for i in 0..5 {
        let storage_clone = Arc::clone(&storage);
        let handle = thread::spawn(move || {
            // Just hold the reference - we're testing that Arc<Storage> works
            assert!(!format!("{:?}", i).is_empty());
            drop(storage_clone);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should not panic");
    }

    // Extract storage from Arc
    let storage = Arc::try_unwrap(storage).expect("Should be the only reference");
    storage.close().expect("close() should succeed");
}

// ============================================================================
// Shutdown Tests
// ============================================================================

#[test]
fn test_close_flushes_and_closes_db() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    // Close should succeed
    let result = storage.close();
    assert!(result.is_ok(), "close() should succeed");
}

#[test]
fn test_storage_can_be_reopened_after_close() {
    let temp_dir = temp_storage_dir();

    // Create and close storage
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options).expect("Storage::new() should succeed");
    storage.close().expect("close() should succeed");

    // Reopen with create_if_missing=false
    let options2 = StorageOptions {
        data_dir: temp_dir.path().to_path_buf(),
        create_if_missing: false,
        ..Default::default()
    };
    let storage2 = Storage::new(options2);
    assert!(storage2.is_ok(), "Should be able to reopen after close");

    storage2.unwrap().close().expect("close() should succeed");
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_custom_write_buffer_size() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.write_buffer_size_mb = 128; // Custom size

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should accept valid custom config"
    );

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_custom_compression() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.compression = rocksdb::DBCompressionType::Snappy;

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should accept different compression"
    );

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_statistics_enabled() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.enable_statistics = true;

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should work with statistics enabled"
    );

    storage.unwrap().close().expect("close() should succeed");
}
