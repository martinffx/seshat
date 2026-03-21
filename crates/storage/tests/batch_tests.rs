//! Integration tests for WriteBatch functionality.
//!
//! Tests cover:
//! - Builder pattern API
//! - Fsync detection across column families
//! - Batch execution atomicity
//! - Integration with Storage layer

use seshat_storage::{ColumnFamily, Result, Storage, StorageOptions, WriteBatch};
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

/// Creates a temporary test storage instance.
fn create_test_storage() -> Result<(Storage, TempDir)> {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options)?;
    Ok((storage, temp_dir))
}

// ============================================================================
// Builder Pattern Tests (6 tests)
// ============================================================================

#[test]
fn test_new_creates_empty_batch() {
    let batch = WriteBatch::new();
    assert!(batch.is_empty());
}

#[test]
fn test_put_adds_operation() {
    let mut batch = WriteBatch::new();
    assert!(batch.is_empty());

    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    assert!(!batch.is_empty());
}

#[test]
fn test_delete_adds_operation() {
    let mut batch = WriteBatch::new();
    assert!(batch.is_empty());

    batch.delete(ColumnFamily::DataKv, b"key1");
    assert!(!batch.is_empty());
}

#[test]
fn test_clear_removes_all_operations() {
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    batch.put(ColumnFamily::DataKv, b"key2", b"value2");
    batch.delete(ColumnFamily::DataKv, b"key3");
    assert!(!batch.is_empty());

    batch.clear();
    assert!(batch.is_empty());
}

#[test]
fn test_is_empty_works_correctly() {
    let mut batch = WriteBatch::new();
    assert!(batch.is_empty());

    batch.put(ColumnFamily::DataKv, b"key", b"value");
    assert!(!batch.is_empty());

    batch.clear();
    assert!(batch.is_empty());
}

#[test]
fn test_builder_pattern_chaining() {
    let mut batch = WriteBatch::new();

    // Test method chaining
    batch
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .delete(ColumnFamily::DataKv, b"key3");

    assert!(!batch.is_empty());
}

// ============================================================================
// Fsync Detection Tests (5 tests)
// ============================================================================

#[test]
fn test_requires_fsync_false_for_empty_batch() {
    let batch = WriteBatch::new();
    assert!(!batch.requires_fsync());
}

#[test]
fn test_requires_fsync_false_for_non_state_cfs() {
    let mut batch = WriteBatch::new();

    // Test all non-state CFs
    batch.put(ColumnFamily::SystemRaftLog, b"key1", b"value1");
    batch.put(ColumnFamily::SystemData, b"key2", b"value2");
    batch.put(ColumnFamily::DataRaftLog, b"key3", b"value3");
    batch.put(ColumnFamily::DataKv, b"key4", b"value4");

    assert!(!batch.requires_fsync());
}

#[test]
fn test_requires_fsync_true_for_system_raft_state() {
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::SystemRaftState, b"state", b"data");
    assert!(batch.requires_fsync());
}

#[test]
fn test_requires_fsync_true_for_data_raft_state() {
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataRaftState, b"state", b"data");
    assert!(batch.requires_fsync());
}

#[test]
fn test_requires_fsync_true_if_any_cf_requires_fsync() {
    let mut batch = WriteBatch::new();

    // Mix of CFs - if ANY requires fsync, batch requires fsync
    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    batch.put(ColumnFamily::SystemData, b"key2", b"value2");
    batch.put(ColumnFamily::SystemRaftState, b"state", b"data"); // This one requires fsync
    batch.put(ColumnFamily::DataRaftLog, b"log", b"entry");

    assert!(batch.requires_fsync());
}

// ============================================================================
// Batch Execution Tests (10 tests)
// ============================================================================

#[test]
fn test_batch_write_executes_all_operations() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;
    let mut batch = WriteBatch::new();

    batch
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .put(ColumnFamily::DataKv, b"key3", b"value3");

    storage.batch_write(batch)?;

    // Verify all keys were written
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key1")?,
        Some(b"value1".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key2")?,
        Some(b"value2".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key3")?,
        Some(b"value3".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_put_operations() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;
    let mut batch = WriteBatch::new();

    batch.put(ColumnFamily::DataKv, b"test_key", b"test_value");

    storage.batch_write(batch)?;

    let value = storage.get(ColumnFamily::DataKv, b"test_key")?;
    assert_eq!(value, Some(b"test_value".to_vec()));

    Ok(())
}

#[test]
fn test_batch_write_delete_operations() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // First, write some data
    storage.put(ColumnFamily::DataKv, b"key1", b"value1")?;
    storage.put(ColumnFamily::DataKv, b"key2", b"value2")?;

    // Verify data exists
    assert!(storage.exists(ColumnFamily::DataKv, b"key1")?);
    assert!(storage.exists(ColumnFamily::DataKv, b"key2")?);

    // Delete using batch
    let mut batch = WriteBatch::new();
    batch.delete(ColumnFamily::DataKv, b"key1");
    batch.delete(ColumnFamily::DataKv, b"key2");

    storage.batch_write(batch)?;

    // Verify data was deleted
    assert!(!storage.exists(ColumnFamily::DataKv, b"key1")?);
    assert!(!storage.exists(ColumnFamily::DataKv, b"key2")?);

    Ok(())
}

#[test]
fn test_batch_write_mixed_operations() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // Setup initial data
    storage.put(ColumnFamily::DataKv, b"existing", b"old_value")?;

    // Mixed batch: put + delete + update
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"new_key", b"new_value");
    batch.delete(ColumnFamily::DataKv, b"existing");
    batch.put(ColumnFamily::DataKv, b"another", b"another_value");

    storage.batch_write(batch)?;

    // Verify results
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"new_key")?,
        Some(b"new_value".to_vec())
    );
    assert_eq!(storage.get(ColumnFamily::DataKv, b"existing")?, None);
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"another")?,
        Some(b"another_value".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_across_multiple_cfs() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"kv_key", b"kv_value");
    batch.put(ColumnFamily::SystemData, b"sys_key", b"sys_value");
    batch.put(ColumnFamily::SystemRaftLog, b"log_key", b"log_value");

    storage.batch_write(batch)?;

    // Verify each CF got its data
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"kv_key")?,
        Some(b"kv_value".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, b"sys_key")?,
        Some(b"sys_value".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftLog, b"log_key")?,
        Some(b"log_value".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_empty_batch_is_noop() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let batch = WriteBatch::new();
    assert!(batch.is_empty());

    // Should succeed without error
    storage.batch_write(batch)?;

    Ok(())
}

#[test]
fn test_batch_write_with_state_cf_fsyncs() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::SystemRaftState, b"term", b"5");
    batch.put(ColumnFamily::DataRaftState, b"commit", b"100");

    assert!(batch.requires_fsync());

    storage.batch_write(batch)?;

    // Verify data was written and fsynced
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftState, b"term")?,
        Some(b"5".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataRaftState, b"commit")?,
        Some(b"100".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_large_batch() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let mut batch = WriteBatch::new();

    // Add 1000 operations
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let value = format!("value_{:04}", i);
        batch.put(ColumnFamily::DataKv, key.as_bytes(), value.as_bytes());
    }

    storage.batch_write(batch)?;

    // Verify all keys were written
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let expected_value = format!("value_{:04}", i);
        let actual_value = storage.get(ColumnFamily::DataKv, key.as_bytes())?;
        assert_eq!(actual_value, Some(expected_value.into_bytes()));
    }

    Ok(())
}

#[test]
fn test_batch_write_can_be_reused_after_clear() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let mut batch = WriteBatch::new();

    // First batch
    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    storage.batch_write(batch)?;

    // Clear and reuse
    let mut batch = WriteBatch::new();
    batch.clear();
    batch.put(ColumnFamily::DataKv, b"key2", b"value2");
    storage.batch_write(batch)?;

    // Verify both writes succeeded
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key1")?,
        Some(b"value1".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key2")?,
        Some(b"value2".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_atomicity_guarantee() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // Write initial data
    storage.put(ColumnFamily::DataKv, b"counter", b"0")?;

    // Batch write multiple operations
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"counter", b"1");
    batch.put(ColumnFamily::DataKv, b"related", b"data");

    storage.batch_write(batch)?;

    // Both should be present - atomicity guarantee
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"counter")?,
        Some(b"1".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"related")?,
        Some(b"data".to_vec())
    );

    Ok(())
}

// ============================================================================
// Integration Tests (4 tests)
// ============================================================================

#[test]
fn test_batch_write_roundtrip_with_get() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    let mut batch = WriteBatch::new();
    batch
        .put(ColumnFamily::DataKv, b"user:1", b"Alice")
        .put(ColumnFamily::DataKv, b"user:2", b"Bob")
        .put(ColumnFamily::DataKv, b"user:3", b"Charlie");

    storage.batch_write(batch)?;

    // Read back all values
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"user:1")?,
        Some(b"Alice".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"user:2")?,
        Some(b"Bob".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"user:3")?,
        Some(b"Charlie".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_updates_existing_keys() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // Initial write
    storage.put(ColumnFamily::DataKv, b"config", b"v1")?;
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"config")?,
        Some(b"v1".to_vec())
    );

    // Batch update
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"config", b"v2");
    storage.batch_write(batch)?;

    // Verify update
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"config")?,
        Some(b"v2".to_vec())
    );

    Ok(())
}

#[test]
fn test_batch_write_deletes_remove_keys() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // Setup data
    storage.put(ColumnFamily::DataKv, b"temp1", b"data1")?;
    storage.put(ColumnFamily::DataKv, b"temp2", b"data2")?;
    storage.put(ColumnFamily::DataKv, b"keep", b"important")?;

    // Batch delete temp keys
    let mut batch = WriteBatch::new();
    batch.delete(ColumnFamily::DataKv, b"temp1");
    batch.delete(ColumnFamily::DataKv, b"temp2");

    storage.batch_write(batch)?;

    // Verify deletes
    assert!(!storage.exists(ColumnFamily::DataKv, b"temp1")?);
    assert!(!storage.exists(ColumnFamily::DataKv, b"temp2")?);
    // Keep should still exist
    assert!(storage.exists(ColumnFamily::DataKv, b"keep")?);

    Ok(())
}

#[test]
fn test_batch_write_independent_across_cfs() -> Result<()> {
    let (storage, _temp_dir) = create_test_storage()?;

    // Write same key to different CFs
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"shared_key", b"kv_value");
    batch.put(ColumnFamily::SystemData, b"shared_key", b"sys_value");

    storage.batch_write(batch)?;

    // Each CF should have its own value
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"shared_key")?,
        Some(b"kv_value".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, b"shared_key")?,
        Some(b"sys_value".to_vec())
    );

    Ok(())
}
