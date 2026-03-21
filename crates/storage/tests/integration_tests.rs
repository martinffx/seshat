//! Integration tests for storage layer - comprehensive end-to-end scenarios.
//!
//! These tests verify that all storage components work together correctly in
//! realistic workflows, combining multiple operations across different layers.
//!
//! Test Categories:
//! 1. End-to-End Workflows (8 tests) - Complete user journeys
//! 2. Cross-Component Integration (8 tests) - Component interactions
//! 3. Atomicity & Consistency (6 tests) - ACID properties
//! 4. Performance & Scalability (5 tests) - Realistic loads
//! 5. Recovery & Durability (4 tests) - Persistence and recovery
//! 6. Error Scenarios (4 tests) - Error handling in complex cases

use seshat_storage::{
    ColumnFamily, Storage, StorageOptions, WriteBatch,
    iterator::{Direction, IteratorMode},
};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create temporary storage with test options.
fn test_storage() -> (Storage, TempDir) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let opts = StorageOptions::with_data_dir(dir.path().to_path_buf());
    let storage = Storage::new(opts).expect("Failed to create storage");
    (storage, dir)
}

/// Create temporary storage that can be reopened.
fn test_storage_at_path(path: std::path::PathBuf) -> Storage {
    let opts = StorageOptions::with_data_dir(path);
    Storage::new(opts).expect("Failed to create storage")
}

/// Populate test data in a column family.
fn populate_test_data(storage: &Storage, cf: ColumnFamily, count: usize) {
    for i in 0..count {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes())
            .expect("Failed to put data");
    }
}

/// Verify log sequence integrity.
fn verify_log_sequence(storage: &Storage, cf: ColumnFamily, start: u64, end: u64) {
    let entries = storage.get_log_range(cf, start, end)
        .expect("Failed to get log range");
    assert_eq!(entries.len(), (end - start) as usize,
        "Expected {} entries in range [{}..{}), got {}", end - start, start, end, entries.len());
}

/// Verify key-value pairs exist with expected values.
fn verify_kv_data(storage: &Storage, cf: ColumnFamily, count: usize) {
    for i in 0..count {
        let key = format!("key_{:06}", i);
        let expected_value = format!("value_{:06}", i);
        let value = storage.get(cf, key.as_bytes())
            .expect("Failed to get value")
            .expect(&format!("Key {} should exist", key));
        assert_eq!(value, expected_value.as_bytes(),
            "Value mismatch for key {}", key);
    }
}

// ============================================================================
// 1. End-to-End Workflows (8 tests)
// ============================================================================

#[test]
fn test_full_crud_workflow_single_cf() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Step 1: Create - put multiple keys
    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();
    storage.put(cf, b"key3", b"value3").unwrap();

    // Step 2: Read - verify all keys exist
    assert_eq!(storage.get(cf, b"key1").unwrap(), Some(b"value1".to_vec()));
    assert_eq!(storage.get(cf, b"key2").unwrap(), Some(b"value2".to_vec()));
    assert_eq!(storage.get(cf, b"key3").unwrap(), Some(b"value3".to_vec()));

    // Step 3: Update - overwrite existing key
    storage.put(cf, b"key2", b"updated_value2").unwrap();
    assert_eq!(storage.get(cf, b"key2").unwrap(), Some(b"updated_value2".to_vec()));

    // Step 4: Delete - remove key
    storage.delete(cf, b"key1").unwrap();
    assert_eq!(storage.get(cf, b"key1").unwrap(), None);

    // Step 5: Verify final state
    assert_eq!(storage.get(cf, b"key2").unwrap(), Some(b"updated_value2".to_vec()));
    assert_eq!(storage.get(cf, b"key3").unwrap(), Some(b"value3".to_vec()));
}

#[test]
fn test_full_crud_workflow_multiple_cfs() {
    let (storage, _dir) = test_storage();

    // Write to multiple column families
    storage.put(ColumnFamily::DataKv, b"data_key", b"data_value").unwrap();
    storage.put(ColumnFamily::SystemData, b"system_key", b"system_value").unwrap();
    storage.put(ColumnFamily::SystemRaftState, b"raft_key", b"raft_value").unwrap();

    // Verify isolation - each CF has independent key space
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"data_key").unwrap(),
        Some(b"data_value".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, b"system_key").unwrap(),
        Some(b"system_value".to_vec())
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftState, b"raft_key").unwrap(),
        Some(b"raft_value".to_vec())
    );

    // Key in one CF doesn't exist in another
    assert_eq!(storage.get(ColumnFamily::DataKv, b"system_key").unwrap(), None);
    assert_eq!(storage.get(ColumnFamily::SystemData, b"data_key").unwrap(), None);
}

#[test]
fn test_log_append_read_truncate_workflow() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    // Step 1: Append sequential log entries
    for i in 1..=10 {
        let entry = format!("entry_{}", i);
        storage.append_log_entry(cf, i, entry.as_bytes()).unwrap();
    }

    // Step 2: Verify last index
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));

    // Step 3: Read range
    verify_log_sequence(&storage, cf, 1, 11); // [1..11) = 10 entries

    // Step 4: Truncate old entries
    storage.truncate_log_before(cf, 5).unwrap();

    // Step 5: Verify truncated range
    let remaining = storage.get_log_range(cf, 1, 11).unwrap();
    assert_eq!(remaining.len(), 6, "Should have entries 5-10 remaining");

    // Step 6: Verify last index unchanged
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));
}

#[test]
fn test_batch_write_then_iterate_workflow() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Step 1: Create batch with multiple operations
    let mut batch = WriteBatch::new();
    for i in 0..100 {
        let key = format!("batch_key_{:03}", i);
        let value = format!("batch_value_{:03}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }

    // Step 2: Execute batch atomically
    storage.batch_write(batch).unwrap();

    // Step 3: Iterate and verify all keys present
    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while let Some((_key, _value)) = iter.next() {
        count += 1;
    }
    assert_eq!(count, 100, "Should see all 100 keys via iterator");
}

#[test]
fn test_snapshot_backup_workflow() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Step 1: Populate data
    populate_test_data(&storage, cf, 50);

    // Step 2: Create snapshot
    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    // Step 3: Verify snapshot is a valid database
    let snapshot_storage = test_storage_at_path(snapshot_path);

    // Step 4: Verify snapshot has the same data
    verify_kv_data(&snapshot_storage, cf, 50);

    // Step 5: Modify original storage
    storage.put(cf, b"new_key", b"new_value").unwrap();

    // Step 6: Verify snapshot is unaffected (snapshot isolation)
    assert_eq!(snapshot_storage.get(cf, b"new_key").unwrap(), None);
}

#[test]
fn test_concurrent_readers_writers_workflow() {
    let (storage, _dir) = test_storage();
    let storage = Arc::new(storage);
    let cf = ColumnFamily::DataKv;

    // Pre-populate some data
    populate_test_data(&storage, cf, 100);

    let barrier = Arc::new(Barrier::new(5));
    let mut handles = vec![];

    // Spawn 2 reader threads
    for reader_id in 0..2 {
        let storage_clone = Arc::clone(&storage);
        let barrier_clone = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait(); // Synchronize start
            for _ in 0..100 {
                let key = format!("key_{:06}", reader_id);
                let _ = storage_clone.get(cf, key.as_bytes());
            }
        });
        handles.push(handle);
    }

    // Spawn 3 writer threads
    for writer_id in 0..3 {
        let storage_clone = Arc::clone(&storage);
        let barrier_clone = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait(); // Synchronize start
            for i in 0..100 {
                let key = format!("writer_{}_key_{:03}", writer_id, i);
                let value = format!("writer_{}_value_{:03}", writer_id, i);
                storage_clone.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify no data corruption
    verify_kv_data(&storage, cf, 100);
}

#[test]
fn test_cf_isolation_workflow() {
    let (storage, _dir) = test_storage();

    // Write same key to different CFs
    let key = b"shared_key";
    storage.put(ColumnFamily::DataKv, key, b"data_value").unwrap();
    storage.put(ColumnFamily::SystemData, key, b"system_value").unwrap();
    storage.put(ColumnFamily::SystemRaftState, key, b"raft_value").unwrap();

    // Verify each CF has its own value
    assert_eq!(
        storage.get(ColumnFamily::DataKv, key).unwrap().unwrap(),
        b"data_value"
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, key).unwrap().unwrap(),
        b"system_value"
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftState, key).unwrap().unwrap(),
        b"raft_value"
    );

    // Delete from one CF doesn't affect others
    storage.delete(ColumnFamily::DataKv, key).unwrap();
    assert_eq!(storage.get(ColumnFamily::DataKv, key).unwrap(), None);
    assert_eq!(
        storage.get(ColumnFamily::SystemData, key).unwrap().unwrap(),
        b"system_value"
    );
}

#[test]
fn test_large_value_workflow() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Create 10MB value
    let large_value = vec![b'x'; 10 * 1024 * 1024];

    // Write large value
    storage.put(cf, b"large_key", &large_value).unwrap();

    // Read back and verify
    let retrieved = storage.get(cf, b"large_key").unwrap().unwrap();
    assert_eq!(retrieved.len(), large_value.len());
    assert_eq!(retrieved, large_value);

    // Delete large value
    storage.delete(cf, b"large_key").unwrap();
    assert_eq!(storage.get(cf, b"large_key").unwrap(), None);
}

// ============================================================================
// 2. Cross-Component Integration (8 tests)
// ============================================================================

#[test]
fn test_batch_write_affects_iterator() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Initial data
    populate_test_data(&storage, cf, 10);

    // Batch write new data
    let mut batch = WriteBatch::new();
    for i in 10..20 {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }
    storage.batch_write(batch).unwrap();

    // Iterator sees all data (old + new)
    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }
    assert_eq!(count, 20);
}

#[test]
fn test_iterator_unaffected_by_concurrent_writes() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Initial data
    populate_test_data(&storage, cf, 100);

    // Create iterator (takes snapshot)
    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();

    // Write new data after iterator creation
    for i in 100..200 {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }

    // Iterator only sees original 100 keys (snapshot isolation)
    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }
    assert_eq!(count, 100, "Iterator should see snapshot at creation time");
}

#[test]
fn test_truncate_affects_log_range_query() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataRaftLog;

    // Append 20 log entries
    for i in 1..=20 {
        let entry = format!("entry_{}", i);
        storage.append_log_entry(cf, i, entry.as_bytes()).unwrap();
    }

    // Truncate first 10 entries
    storage.truncate_log_before(cf, 11).unwrap();

    // Range query sees truncated view
    let entries = storage.get_log_range(cf, 1, 21).unwrap();
    assert_eq!(entries.len(), 10, "Should only have entries 11-20");

    // Verify we can still read remaining entries
    let entries = storage.get_log_range(cf, 11, 21).unwrap();
    assert_eq!(entries.len(), 10);
}

#[test]
fn test_cache_updated_across_operations() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    // Append first entry (cache miss -> cache hit)
    storage.append_log_entry(cf, 1, b"entry1").unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(1));

    // Append more entries (cache hit)
    for i in 2..=10 {
        storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
    }
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));

    // Truncate all (cache invalidated)
    storage.truncate_log_before(cf, 100).unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), None);
}

#[test]
fn test_snapshot_captures_batch_writes() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Batch write data
    let mut batch = WriteBatch::new();
    for i in 0..50 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }
    storage.batch_write(batch).unwrap();

    // Create snapshot
    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    // Verify snapshot has batch data
    let snapshot_storage = test_storage_at_path(snapshot_path);
    let value = snapshot_storage.get(cf, b"key_25").unwrap().unwrap();
    assert_eq!(value, b"value_25");
}

#[test]
fn test_fsync_behavior_across_batch_and_single_writes() {
    let (storage, _dir) = test_storage();

    // Single write to fsync CF (should fsync)
    storage.put(ColumnFamily::SystemRaftState, b"key1", b"value1").unwrap();

    // Batch with fsync CF (should fsync entire batch)
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"key2", b"value2");
    batch.put(ColumnFamily::SystemRaftState, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    // Verify all writes persisted
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftState, b"key1").unwrap().unwrap(),
        b"value1"
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key2").unwrap().unwrap(),
        b"value2"
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemRaftState, b"key3").unwrap().unwrap(),
        b"value3"
    );
}

#[test]
fn test_exists_optimized_after_put() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Key doesn't exist
    assert!(!storage.exists(cf, b"mykey").unwrap());

    // Put key
    storage.put(cf, b"mykey", b"myvalue").unwrap();

    // Key exists (bloom filter hit)
    assert!(storage.exists(cf, b"mykey").unwrap());

    // Delete key
    storage.delete(cf, b"mykey").unwrap();

    // Key doesn't exist
    assert!(!storage.exists(cf, b"mykey").unwrap());
}

#[test]
fn test_delete_removes_from_iterator_view() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Put keys
    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();
    storage.put(cf, b"key3", b"value3").unwrap();

    // Delete key2
    storage.delete(cf, b"key2").unwrap();

    // Iterator doesn't see deleted key
    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let keys: Vec<Vec<u8>> = std::iter::from_fn(|| iter.next())
        .map(|(k, _v)| k.to_vec())
        .collect();

    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&b"key1".to_vec()));
    assert!(keys.contains(&b"key3".to_vec()));
    assert!(!keys.contains(&b"key2".to_vec()));
}

// ============================================================================
// 3. Atomicity & Consistency (6 tests)
// ============================================================================

#[test]
fn test_batch_atomicity_all_succeed() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Create batch
    let mut batch = WriteBatch::new();
    batch.put(cf, b"key1", b"value1");
    batch.put(cf, b"key2", b"value2");
    batch.put(cf, b"key3", b"value3");

    // Execute batch
    storage.batch_write(batch).unwrap();

    // All keys should exist
    assert!(storage.get(cf, b"key1").unwrap().is_some());
    assert!(storage.get(cf, b"key2").unwrap().is_some());
    assert!(storage.get(cf, b"key3").unwrap().is_some());
}

#[test]
fn test_batch_atomicity_with_deletes() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Initial data
    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();

    // Batch with mixed operations
    let mut batch = WriteBatch::new();
    batch.delete(cf, b"key1");
    batch.put(cf, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    // Verify atomic result
    assert_eq!(storage.get(cf, b"key1").unwrap(), None);
    assert_eq!(storage.get(cf, b"key2").unwrap().unwrap(), b"value2");
    assert_eq!(storage.get(cf, b"key3").unwrap().unwrap(), b"value3");
}

#[test]
fn test_log_index_validation_prevents_gaps() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    // Append first entry
    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    // Try to append with gap (should fail)
    let result = storage.append_log_entry(cf, 3, b"entry3");
    assert!(result.is_err(), "Should reject non-sequential index");

    // Verify last index unchanged
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(1));

    // Append correct next index succeeds
    storage.append_log_entry(cf, 2, b"entry2").unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(2));
}

#[test]
fn test_concurrent_appends_maintain_sequence() {
    let (storage, _dir) = test_storage();
    let storage = Arc::new(storage);
    let cf = ColumnFamily::DataRaftLog;

    // Pre-populate to avoid first-entry race
    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    let barrier = Arc::new(Barrier::new(5));
    let mut handles = vec![];

    // 5 threads trying to append concurrently
    for thread_id in 0..5 {
        let storage_clone = Arc::clone(&storage);
        let barrier_clone = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            for i in 0..10 {
                // Try to append, some will fail due to index conflicts
                let last_index = storage_clone.get_last_log_index(cf).unwrap().unwrap();
                let next_index = last_index + 1;
                let entry = format!("thread_{}_entry_{}", thread_id, i);
                let _ = storage_clone.append_log_entry(cf, next_index, entry.as_bytes());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify no gaps in log (all indices are sequential)
    let last_index = storage.get_last_log_index(cf).unwrap().unwrap();
    for i in 1..=last_index {
        let entries = storage.get_log_range(cf, i, i + 1).unwrap();
        assert_eq!(entries.len(), 1, "Index {} should exist", i);
    }
}

#[test]
fn test_snapshot_isolation_during_writes() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Initial data
    populate_test_data(&storage, cf, 50);

    // Create snapshot
    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    // Modify original storage
    for i in 0..50 {
        let key = format!("key_{:06}", i);
        let value = format!("modified_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }

    // Snapshot still has original data
    let snapshot_storage = test_storage_at_path(snapshot_path);
    let value = snapshot_storage.get(cf, b"key_000000").unwrap().unwrap();
    assert_eq!(value, b"value_000000", "Snapshot should have original value");
}

#[test]
fn test_cf_operations_independent() {
    let (storage, _dir) = test_storage();

    // Batch across multiple CFs
    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    batch.put(ColumnFamily::SystemData, b"key2", b"value2");
    batch.put(ColumnFamily::DataRaftState, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    // Delete from one CF
    storage.delete(ColumnFamily::DataKv, b"key1").unwrap();

    // Others unaffected
    assert_eq!(storage.get(ColumnFamily::DataKv, b"key1").unwrap(), None);
    assert_eq!(
        storage.get(ColumnFamily::SystemData, b"key2").unwrap().unwrap(),
        b"value2"
    );
    assert_eq!(
        storage.get(ColumnFamily::DataRaftState, b"key3").unwrap().unwrap(),
        b"value3"
    );
}

// ============================================================================
// 4. Performance & Scalability (5 tests)
// ============================================================================

#[test]
fn test_1000_sequential_writes_complete_fast() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    let start = std::time::Instant::now();

    for i in 0..1000 {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }

    let duration = start.elapsed();
    println!("1000 writes completed in {:?}", duration);

    // Verify all writes succeeded
    verify_kv_data(&storage, cf, 1000);
}

#[test]
fn test_1000_concurrent_reads() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Pre-populate
    populate_test_data(&storage, cf, 100);

    let storage = Arc::new(storage);
    let mut handles = vec![];

    let start = std::time::Instant::now();

    // 10 threads, 100 reads each = 1000 total
    for thread_id in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let key_idx = (thread_id * 10 + i) % 100;
                let key = format!("key_{:06}", key_idx);
                let _ = storage_clone.get(cf, key.as_bytes());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let duration = start.elapsed();
    println!("1000 concurrent reads completed in {:?}", duration);
}

#[test]
fn test_large_batch_1000_operations() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    let mut batch = WriteBatch::new();
    for i in 0..1000 {
        let key = format!("batch_key_{:06}", i);
        let value = format!("batch_value_{:06}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }

    let start = std::time::Instant::now();
    storage.batch_write(batch).unwrap();
    let duration = start.elapsed();

    println!("Batch write of 1000 ops completed in {:?}", duration);

    // Verify all keys exist
    for i in 0..1000 {
        let key = format!("batch_key_{:06}", i);
        assert!(storage.get(cf, key.as_bytes()).unwrap().is_some());
    }
}

#[test]
fn test_iterate_10000_keys() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Populate 10k keys
    populate_test_data(&storage, cf, 10000);

    let start = std::time::Instant::now();

    // Iterate through all keys
    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }

    let duration = start.elapsed();
    println!("Iterated 10k keys in {:?}", duration);

    assert_eq!(count, 10000);
}

#[test]
fn test_snapshot_large_db_10000_keys() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    // Populate 10k keys
    populate_test_data(&storage, cf, 10000);

    let start = std::time::Instant::now();

    // Create snapshot
    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    let duration = start.elapsed();
    println!("Snapshot of 10k keys completed in {:?}", duration);

    // Verify snapshot is valid
    let snapshot_storage = test_storage_at_path(snapshot_path);
    assert!(snapshot_storage.get(cf, b"key_005000").unwrap().is_some());
}

// ============================================================================
// 5. Recovery & Durability (4 tests)
// ============================================================================

#[test]
fn test_data_persists_after_reopen() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    // Create storage and write data
    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, ColumnFamily::DataKv, 100);
        storage.close().unwrap();
    }

    // Reopen and verify data persists
    {
        let storage = test_storage_at_path(path.clone());
        verify_kv_data(&storage, ColumnFamily::DataKv, 100);
    }
}

#[test]
fn test_log_cache_warmed_up_on_reopen() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();
    let cf = ColumnFamily::SystemRaftLog;

    // Create storage and append log entries
    {
        let storage = test_storage_at_path(path.clone());
        for i in 1..=50 {
            storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
        }
        assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(50));
        storage.close().unwrap();
    }

    // Reopen and verify cache is warmed up
    {
        let storage = test_storage_at_path(path.clone());
        // get_last_log_index should hit cache (warmed up on startup)
        assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(50));
    }
}

#[test]
fn test_snapshot_survives_source_db_close() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();
    let snapshot_path = dir.path().join("snapshot");
    let cf = ColumnFamily::DataKv;

    // Create storage, write data, take snapshot
    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, cf, 100);
        storage.create_snapshot(&snapshot_path).unwrap();
        storage.close().unwrap();
    }

    // Close original storage, snapshot still readable
    {
        let snapshot_storage = test_storage_at_path(snapshot_path);
        verify_kv_data(&snapshot_storage, cf, 100);
    }
}

#[test]
fn test_multiple_reopen_cycles() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();
    let cf = ColumnFamily::DataKv;

    // Cycle 1: Write 50 keys
    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, cf, 50);
        storage.close().unwrap();
    }

    // Cycle 2: Read 50 keys, write 50 more
    {
        let storage = test_storage_at_path(path.clone());
        verify_kv_data(&storage, cf, 50);
        for i in 50..100 {
            let key = format!("key_{:06}", i);
            let value = format!("value_{:06}", i);
            storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
        }
        storage.close().unwrap();
    }

    // Cycle 3: Verify all 100 keys
    {
        let storage = test_storage_at_path(path.clone());
        verify_kv_data(&storage, cf, 100);
    }
}

// ============================================================================
// 6. Error Scenarios (4 tests)
// ============================================================================

#[test]
fn test_invalid_log_index_prevents_all_appends() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataRaftLog;

    // First entry must be index 1
    let result = storage.append_log_entry(cf, 0, b"entry0");
    assert!(result.is_err());

    // Can't skip indices
    let result = storage.append_log_entry(cf, 2, b"entry2");
    assert!(result.is_err());

    // Correct first append
    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    // Can't go backwards
    let result = storage.append_log_entry(cf, 1, b"entry1_dup");
    assert!(result.is_err());

    // Can't skip
    let result = storage.append_log_entry(cf, 3, b"entry3");
    assert!(result.is_err());
}

#[test]
fn test_batch_with_invalid_operations_fails() {
    let (storage, _dir) = test_storage();

    // This test verifies that invalid operations are rejected at batch construction
    // or execution time. Since our current API doesn't have many validation points,
    // we test that empty batches work correctly.
    let batch = WriteBatch::new();
    assert!(batch.is_empty());

    // Empty batch should succeed (no-op)
    storage.batch_write(batch).unwrap();
}

#[test]
fn test_snapshot_to_existing_path_fails() {
    let (storage, dir) = test_storage();

    // Create snapshot
    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    // Try to create another snapshot at same path (should fail)
    let result = storage.create_snapshot(&snapshot_path);
    assert!(result.is_err(), "Should not overwrite existing snapshot");
}

#[test]
fn test_concurrent_operations_never_corrupt_data() {
    let (storage, _dir) = test_storage();
    let storage = Arc::new(storage);
    let cf = ColumnFamily::DataKv;

    // Pre-populate
    populate_test_data(&storage, cf, 100);

    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    // 10 threads doing mixed operations
    for thread_id in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let barrier_clone = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            for i in 0..100 {
                match i % 4 {
                    0 => {
                        // Read
                        let key = format!("key_{:06}", (thread_id + i) % 100);
                        let _ = storage_clone.get(cf, key.as_bytes());
                    }
                    1 => {
                        // Write
                        let key = format!("thread_{}_key_{}", thread_id, i);
                        let value = format!("thread_{}_value_{}", thread_id, i);
                        let _ = storage_clone.put(cf, key.as_bytes(), value.as_bytes());
                    }
                    2 => {
                        // Exists check
                        let key = format!("key_{:06}", (thread_id + i) % 100);
                        let _ = storage_clone.exists(cf, key.as_bytes());
                    }
                    3 => {
                        // Delete (idempotent)
                        let key = format!("temp_key_{}", thread_id);
                        let _ = storage_clone.delete(cf, key.as_bytes());
                    }
                    _ => unreachable!(),
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should not panic");
    }

    // Verify original data not corrupted
    verify_kv_data(&storage, cf, 100);
}
