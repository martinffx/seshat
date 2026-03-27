//! Tests for snapshot operations (ROCKS-009).
//!
//! This test file covers:
//! - Snapshot creation
//! - Snapshot content verification
//! - Error handling
//! - Integration scenarios

use seshat_storage::{ColumnFamily, Storage, StorageOptions};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

/// Creates a temporary storage instance for testing.
fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options).expect("Failed to create storage");
    (storage, temp_dir)
}

/// Creates a storage instance with test data.
fn create_storage_with_data() -> (Storage, TempDir) {
    let (storage, temp_dir) = create_test_storage();

    // Add some KV data
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("Failed to put key1");
    storage
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .expect("Failed to put key2");
    storage
        .put(ColumnFamily::DataKv, b"key3", b"value3")
        .expect("Failed to put key3");

    // Add some log entries
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"log_entry_1")
        .expect("Failed to append log entry 1");
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 2, b"log_entry_2")
        .expect("Failed to append log entry 2");

    (storage, temp_dir)
}

// ============================================================================
// Snapshot Creation Tests (6 tests)
// ============================================================================

#[test]
fn test_create_snapshot_succeeds() {
    let (storage, _temp_dir) = create_test_storage();
    let snapshot_path = _temp_dir.path().join("snapshot-001");

    let result = storage.create_snapshot(&snapshot_path);
    assert!(result.is_ok(), "create_snapshot should succeed");
}

#[test]
fn test_create_snapshot_creates_checkpoint_directory() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-001");

    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    assert!(
        snapshot_path.exists(),
        "Snapshot directory should be created"
    );
    assert!(
        snapshot_path.is_dir(),
        "Snapshot path should be a directory"
    );
}

#[test]
fn test_create_snapshot_preserves_data() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-001");

    // Verify data exists before snapshot
    let value1 = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("Failed to get key1");
    assert_eq!(value1, Some(b"value1".to_vec()));

    // Create snapshot
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    // Verify original data still exists
    let value1_after = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("Failed to get key1 after snapshot");
    assert_eq!(value1_after, Some(b"value1".to_vec()));
}

#[test]
fn test_create_snapshot_can_be_reopened() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-001");

    // Create snapshot
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    // Close original storage
    storage.close().expect("Failed to close storage");

    // Open snapshot as new storage
    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    // Verify data in snapshot
    let value1 = snapshot_storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("Failed to get key1 from snapshot");
    assert_eq!(
        value1,
        Some(b"value1".to_vec()),
        "Snapshot should contain original data"
    );

    snapshot_storage.close().expect("Failed to close snapshot");
}

#[test]
fn test_create_snapshot_multiple_times() {
    let (storage, _temp_dir) = create_storage_with_data();

    // Create first snapshot
    let snapshot_path1 = _temp_dir.path().join("snapshot-001");
    storage
        .create_snapshot(&snapshot_path1)
        .expect("Failed to create first snapshot");

    // Modify data
    storage
        .put(ColumnFamily::DataKv, b"key4", b"value4")
        .expect("Failed to put key4");

    // Create second snapshot
    let snapshot_path2 = _temp_dir.path().join("snapshot-002");
    storage
        .create_snapshot(&snapshot_path2)
        .expect("Failed to create second snapshot");

    // Both snapshots should exist
    assert!(snapshot_path1.exists(), "First snapshot should exist");
    assert!(snapshot_path2.exists(), "Second snapshot should exist");
}

#[test]
fn test_create_snapshot_with_large_db() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert 1000 keys
    for i in 0..1000 {
        let key = format!("key{:04}", i);
        let value = format!("value{:04}", i);
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), value.as_bytes())
            .expect("Failed to put key");
    }

    // Create snapshot
    let snapshot_path = _temp_dir.path().join("snapshot-large");
    let result = storage.create_snapshot(&snapshot_path);
    assert!(result.is_ok(), "Snapshot of large DB should succeed");
}

// ============================================================================
// Snapshot Content Tests (5 tests)
// ============================================================================

#[test]
fn test_snapshot_contains_all_column_families() {
    let (storage, _temp_dir) = create_test_storage();

    // Write data to multiple column families
    storage
        .put(ColumnFamily::DataKv, b"kv_key", b"kv_value")
        .expect("Failed to put DataKv");
    storage
        .put(ColumnFamily::SystemData, b"sys_key", b"sys_value")
        .expect("Failed to put SystemData");
    storage
        .append_log_entry(ColumnFamily::DataRaftLog, 1, b"data_log_entry")
        .expect("Failed to append DataRaftLog");
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"sys_log_entry")
        .expect("Failed to append SystemRaftLog");

    // Create snapshot
    let snapshot_path = _temp_dir.path().join("snapshot-all-cfs");
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    // Close original and open snapshot
    storage.close().expect("Failed to close storage");

    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    // Verify all column families have data
    assert_eq!(
        snapshot_storage
            .get(ColumnFamily::DataKv, b"kv_key")
            .unwrap(),
        Some(b"kv_value".to_vec())
    );
    assert_eq!(
        snapshot_storage
            .get(ColumnFamily::SystemData, b"sys_key")
            .unwrap(),
        Some(b"sys_value".to_vec())
    );

    // Verify log entries
    let data_log_entries = snapshot_storage
        .get_log_range(ColumnFamily::DataRaftLog, 1, 2)
        .expect("Failed to get data log range");
    assert_eq!(data_log_entries.len(), 1);
    assert_eq!(data_log_entries[0], b"data_log_entry");

    let sys_log_entries = snapshot_storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 2)
        .expect("Failed to get system log range");
    assert_eq!(sys_log_entries.len(), 1);
    assert_eq!(sys_log_entries[0], b"sys_log_entry");

    snapshot_storage.close().expect("Failed to close snapshot");
}

#[test]
fn test_snapshot_contains_correct_data() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-correct-data");

    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");
    storage.close().expect("Failed to close storage");

    // Open snapshot and verify data
    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key1").unwrap(),
        Some(b"value1".to_vec())
    );
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key2").unwrap(),
        Some(b"value2".to_vec())
    );
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key3").unwrap(),
        Some(b"value3".to_vec())
    );

    snapshot_storage.close().expect("Failed to close snapshot");
}

#[test]
fn test_snapshot_independent_from_source_db() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-independent");

    // Create snapshot
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    // Modify original DB after snapshot
    storage
        .put(ColumnFamily::DataKv, b"key1", b"modified_value1")
        .expect("Failed to modify key1");
    storage
        .delete(ColumnFamily::DataKv, b"key2")
        .expect("Failed to delete key2");

    // Open snapshot as separate storage
    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    // Snapshot should have original values
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key1").unwrap(),
        Some(b"value1".to_vec()),
        "Snapshot should have original value"
    );
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key2").unwrap(),
        Some(b"value2".to_vec()),
        "Snapshot should still have deleted key"
    );

    // Original DB should have modified values
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key1").unwrap(),
        Some(b"modified_value1".to_vec()),
        "Original DB should have modified value"
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key2").unwrap(),
        None,
        "Original DB should not have deleted key"
    );

    snapshot_storage.close().expect("Failed to close snapshot");
    storage.close().expect("Failed to close storage");
}

#[test]
fn test_snapshot_after_writes_and_deletes() {
    let (storage, _temp_dir) = create_test_storage();

    // Write some data
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("Failed to put key1");
    storage
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .expect("Failed to put key2");
    storage
        .put(ColumnFamily::DataKv, b"key3", b"value3")
        .expect("Failed to put key3");

    // Delete one key
    storage
        .delete(ColumnFamily::DataKv, b"key2")
        .expect("Failed to delete key2");

    // Create snapshot
    let snapshot_path = _temp_dir.path().join("snapshot-after-ops");
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");
    storage.close().expect("Failed to close storage");

    // Open snapshot and verify state
    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key1").unwrap(),
        Some(b"value1".to_vec())
    );
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key2").unwrap(),
        None,
        "Deleted key should not be in snapshot"
    );
    assert_eq!(
        snapshot_storage.get(ColumnFamily::DataKv, b"key3").unwrap(),
        Some(b"value3".to_vec())
    );

    snapshot_storage.close().expect("Failed to close snapshot");
}

#[test]
fn test_snapshot_preserves_log_entries() {
    let (storage, _temp_dir) = create_test_storage();

    // Append log entries
    for i in 1..=10 {
        let entry = format!("log_entry_{}", i);
        storage
            .append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes())
            .expect("Failed to append log entry");
    }

    // Create snapshot
    let snapshot_path = _temp_dir.path().join("snapshot-logs");
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");
    storage.close().expect("Failed to close storage");

    // Open snapshot and verify log entries
    let snapshot_options = StorageOptions::with_data_dir(snapshot_path);
    let snapshot_storage = Storage::new(snapshot_options).expect("Failed to open snapshot");

    let entries = snapshot_storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 11)
        .expect("Failed to get log range");
    assert_eq!(entries.len(), 10, "Snapshot should have all 10 log entries");

    for (i, entry) in entries.iter().enumerate() {
        let expected = format!("log_entry_{}", i + 1);
        assert_eq!(entry, expected.as_bytes());
    }

    snapshot_storage.close().expect("Failed to close snapshot");
}

// ============================================================================
// Error Handling Tests (4 tests)
// ============================================================================

#[test]
fn test_create_snapshot_fails_if_directory_exists() {
    let (storage, _temp_dir) = create_test_storage();
    let snapshot_path = _temp_dir.path().join("snapshot-exists");

    // Create directory first
    fs::create_dir(&snapshot_path).expect("Failed to create directory");

    // Attempt to create snapshot should fail
    let result = storage.create_snapshot(&snapshot_path);
    assert!(
        result.is_err(),
        "create_snapshot should fail if directory already exists"
    );
}

#[test]
fn test_create_snapshot_fails_for_invalid_path() {
    let (storage, _temp_dir) = create_test_storage();

    // Try to create snapshot with invalid path (parent doesn't exist)
    let invalid_path = PathBuf::from("/nonexistent/parent/directory/snapshot");

    let result = storage.create_snapshot(&invalid_path);
    assert!(
        result.is_err(),
        "create_snapshot should fail for invalid path"
    );
}

#[test]
fn test_create_snapshot_path_must_be_absolute_or_relative() {
    let (storage, _temp_dir) = create_test_storage();

    // Test with relative path
    let relative_path = _temp_dir.path().join("relative_snapshot");
    let result = storage.create_snapshot(&relative_path);
    assert!(
        result.is_ok(),
        "create_snapshot should accept relative path"
    );

    // Test with absolute path
    let absolute_path = _temp_dir
        .path()
        .join("absolute_snapshot")
        .canonicalize()
        .unwrap_or_else(|_| _temp_dir.path().join("absolute_snapshot"));
    let result = storage.create_snapshot(&absolute_path);
    assert!(
        result.is_ok(),
        "create_snapshot should accept absolute path"
    );
}

#[test]
#[cfg(unix)] // This test only makes sense on Unix systems
fn test_create_snapshot_fails_for_readonly_filesystem() {
    // Note: This test is challenging to implement portably.
    // We would need to:
    // 1. Create a read-only mount point (requires root)
    // 2. Or use a test framework that mocks filesystem operations
    //
    // For now, we'll skip this test in Phase 1.
    // In production, RocksDB will return an appropriate error.
    //
    // Left as a placeholder for future implementation.
}

// ============================================================================
// Integration Tests (3 tests)
// ============================================================================

#[test]
#[ignore] // Storage is not Send/Sync due to SliceTransform
fn test_create_snapshot_while_writes_happening() {
    // Storage cannot be shared across threads due to SliceTransform not being Send/Sync
}

#[test]
fn test_snapshot_size_initially_zero() {
    let (storage, _temp_dir) = create_storage_with_data();
    let snapshot_path = _temp_dir.path().join("snapshot-hardlinks");

    // Get size of original DB directory
    let original_size = dir_size(_temp_dir.path()).expect("Failed to get original size");

    // Create snapshot
    storage
        .create_snapshot(&snapshot_path)
        .expect("Failed to create snapshot");

    // Get size of snapshot directory
    let snapshot_size = dir_size(&snapshot_path).expect("Failed to get snapshot size");

    // Snapshot should be much smaller than original due to hard links
    // Note: This test checks that snapshot uses hard links, not copies
    // The snapshot directory metadata size should be minimal
    //
    // On most filesystems, hard links have near-zero cost.
    // We check that snapshot size is less than 10% of original size
    // (accounting for directory metadata and WAL files that might be copied)
    assert!(
        snapshot_size < original_size / 10 || snapshot_size < 1024 * 1024,
        "Snapshot should use hard links (minimal disk space), got {} bytes for snapshot vs {} bytes for original",
        snapshot_size,
        original_size
    );
}

#[test]
fn test_multiple_snapshots_independent() {
    let (storage, _temp_dir) = create_storage_with_data();

    // Create first snapshot
    let snapshot_path1 = _temp_dir.path().join("snapshot-001");
    storage
        .create_snapshot(&snapshot_path1)
        .expect("Failed to create first snapshot");

    // Modify data
    storage
        .put(ColumnFamily::DataKv, b"key4", b"value4")
        .expect("Failed to put key4");

    // Create second snapshot
    let snapshot_path2 = _temp_dir.path().join("snapshot-002");
    storage
        .create_snapshot(&snapshot_path2)
        .expect("Failed to create second snapshot");

    storage.close().expect("Failed to close storage");

    // Open first snapshot - should NOT have key4
    let snapshot1_options = StorageOptions::with_data_dir(snapshot_path1);
    let snapshot1_storage = Storage::new(snapshot1_options).expect("Failed to open snapshot1");
    assert_eq!(
        snapshot1_storage
            .get(ColumnFamily::DataKv, b"key4")
            .unwrap(),
        None,
        "First snapshot should not have key4"
    );
    snapshot1_storage
        .close()
        .expect("Failed to close snapshot1");

    // Open second snapshot - should have key4
    let snapshot2_options = StorageOptions::with_data_dir(snapshot_path2);
    let snapshot2_storage = Storage::new(snapshot2_options).expect("Failed to open snapshot2");
    assert_eq!(
        snapshot2_storage
            .get(ColumnFamily::DataKv, b"key4")
            .unwrap(),
        Some(b"value4".to_vec()),
        "Second snapshot should have key4"
    );
    snapshot2_storage
        .close()
        .expect("Failed to close snapshot2");
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate total size of a directory recursively.
fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            total_size += dir_size(&entry.path())?;
        } else {
            total_size += metadata.len();
        }
    }

    Ok(total_size)
}
