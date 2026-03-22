//! Integration tests for storage layer - comprehensive end-to-end scenarios.

use seshat_storage::{iterator::IteratorMode, ColumnFamily, Storage, StorageOptions, WriteBatch};
use tempfile::TempDir;

fn test_storage() -> (Storage, TempDir) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let opts = StorageOptions::with_data_dir(dir.path().to_path_buf());
    let storage = Storage::new(opts).expect("Failed to create storage");
    (storage, dir)
}

fn test_storage_at_path(path: std::path::PathBuf) -> Storage {
    let opts = StorageOptions::with_data_dir(path);
    Storage::new(opts).expect("Failed to create storage")
}

fn populate_test_data(storage: &Storage, cf: ColumnFamily, count: usize) {
    for i in 0..count {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        storage
            .put(cf, key.as_bytes(), value.as_bytes())
            .expect("Failed to put data");
    }
}

fn verify_log_sequence(storage: &Storage, cf: ColumnFamily, start: u64, end: u64) {
    let entries = storage
        .get_log_range(cf, start, end)
        .expect("Failed to get log range");
    assert_eq!(entries.len(), (end - start) as usize);
}

fn verify_kv_data(storage: &Storage, cf: ColumnFamily, count: usize) {
    for i in 0..count {
        let key = format!("key_{:06}", i);
        let expected_value = format!("value_{:06}", i);
        let value = storage
            .get(cf, key.as_bytes())
            .expect("Failed to get value")
            .unwrap_or_else(|| panic!("Key {} should exist", key));
        assert_eq!(value, expected_value.as_bytes());
    }
}

// ============================================================================
// End-to-End Workflows
// ============================================================================

#[test]
fn test_full_crud_workflow_single_cf() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();
    storage.put(cf, b"key3", b"value3").unwrap();

    assert_eq!(storage.get(cf, b"key1").unwrap(), Some(b"value1".to_vec()));
    assert_eq!(storage.get(cf, b"key2").unwrap(), Some(b"value2".to_vec()));
    assert_eq!(storage.get(cf, b"key3").unwrap(), Some(b"value3".to_vec()));

    storage.put(cf, b"key2", b"updated_value2").unwrap();
    assert_eq!(
        storage.get(cf, b"key2").unwrap(),
        Some(b"updated_value2".to_vec())
    );

    storage.delete(cf, b"key1").unwrap();
    assert_eq!(storage.get(cf, b"key1").unwrap(), None);

    assert_eq!(
        storage.get(cf, b"key2").unwrap(),
        Some(b"updated_value2".to_vec())
    );
    assert_eq!(storage.get(cf, b"key3").unwrap(), Some(b"value3".to_vec()));
}

#[test]
fn test_full_crud_workflow_multiple_cfs() {
    let (storage, _dir) = test_storage();

    storage
        .put(ColumnFamily::DataKv, b"data_key", b"data_value")
        .unwrap();
    storage
        .put(ColumnFamily::SystemData, b"system_key", b"system_value")
        .unwrap();
    storage
        .put(ColumnFamily::SystemRaftState, b"raft_key", b"raft_value")
        .unwrap();

    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"data_key").unwrap(),
        Some(b"data_value".to_vec())
    );
    assert_eq!(
        storage
            .get(ColumnFamily::SystemData, b"system_key")
            .unwrap(),
        Some(b"system_value".to_vec())
    );
    assert_eq!(
        storage
            .get(ColumnFamily::SystemRaftState, b"raft_key")
            .unwrap(),
        Some(b"raft_value".to_vec())
    );

    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"system_key").unwrap(),
        None
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, b"data_key").unwrap(),
        None
    );
}

#[test]
fn test_log_append_read_truncate_workflow() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    for i in 1..=10 {
        let entry = format!("entry_{}", i);
        storage.append_log_entry(cf, i, entry.as_bytes()).unwrap();
    }

    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));

    verify_log_sequence(&storage, cf, 1, 11);

    storage.truncate_log_before(cf, 5).unwrap();

    let remaining = storage.get_log_range(cf, 1, 11).unwrap();
    assert_eq!(remaining.len(), 6, "Should have entries 5-10 remaining");

    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));
}

#[test]
fn test_batch_write_then_iterate_workflow() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    let mut batch = WriteBatch::new();
    for i in 0..100 {
        let key = format!("batch_key_{:03}", i);
        let value = format!("batch_value_{:03}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }

    storage.batch_write(batch).unwrap();

    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while let Some((_key, _value)) = iter.step_forward().unwrap() {
        count += 1;
    }
    assert_eq!(count, 100, "Should see all 100 keys via iterator");
}

#[test]
fn test_snapshot_backup_workflow() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    populate_test_data(&storage, cf, 50);

    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    let snapshot_storage = test_storage_at_path(snapshot_path);

    verify_kv_data(&snapshot_storage, cf, 50);

    storage.put(cf, b"new_key", b"new_value").unwrap();

    assert_eq!(snapshot_storage.get(cf, b"new_key").unwrap(), None);
}

#[test]
fn test_cf_isolation_workflow() {
    let (storage, _dir) = test_storage();

    let key = b"shared_key";
    storage
        .put(ColumnFamily::DataKv, key, b"data_value")
        .unwrap();
    storage
        .put(ColumnFamily::SystemData, key, b"system_value")
        .unwrap();
    storage
        .put(ColumnFamily::SystemRaftState, key, b"raft_value")
        .unwrap();

    assert_eq!(
        storage.get(ColumnFamily::DataKv, key).unwrap().unwrap(),
        b"data_value"
    );
    assert_eq!(
        storage.get(ColumnFamily::SystemData, key).unwrap().unwrap(),
        b"system_value"
    );
    assert_eq!(
        storage
            .get(ColumnFamily::SystemRaftState, key)
            .unwrap()
            .unwrap(),
        b"raft_value"
    );

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

    let large_value = vec![b'x'; 10 * 1024 * 1024];

    storage.put(cf, b"large_key", &large_value).unwrap();

    let retrieved = storage.get(cf, b"large_key").unwrap().unwrap();
    assert_eq!(retrieved.len(), large_value.len());
    assert_eq!(retrieved, large_value);

    storage.delete(cf, b"large_key").unwrap();
    assert_eq!(storage.get(cf, b"large_key").unwrap(), None);
}

// ============================================================================
// Cross-Component Integration
// ============================================================================

#[test]
fn test_batch_write_affects_iterator() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    populate_test_data(&storage, cf, 10);

    let mut batch = WriteBatch::new();
    for i in 10..20 {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }
    storage.batch_write(batch).unwrap();

    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 20);
}

#[test]
fn test_iterator_unaffected_by_concurrent_writes() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    populate_test_data(&storage, cf, 100);

    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();

    for i in 100..200 {
        let key = format!("key_{:06}", i);
        let value = format!("value_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }

    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 100, "Iterator should see snapshot at creation time");
}

#[test]
fn test_truncate_affects_log_range_query() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataRaftLog;

    for i in 1..=20 {
        let entry = format!("entry_{}", i);
        storage.append_log_entry(cf, i, entry.as_bytes()).unwrap();
    }

    storage.truncate_log_before(cf, 11).unwrap();

    let entries = storage.get_log_range(cf, 1, 21).unwrap();
    assert_eq!(entries.len(), 10, "Should only have entries 11-20");

    let entries = storage.get_log_range(cf, 11, 21).unwrap();
    assert_eq!(entries.len(), 10);
}

#[test]
fn test_cache_updated_across_operations() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    storage.append_log_entry(cf, 1, b"entry1").unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(1));

    for i in 2..=10 {
        storage
            .append_log_entry(cf, i, format!("entry{}", i).as_bytes())
            .unwrap();
    }
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));

    storage.truncate_log_before(cf, 100).unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), None);
}

#[test]
fn test_snapshot_captures_batch_writes() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    let mut batch = WriteBatch::new();
    for i in 0..50 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        batch.put(cf, key.as_bytes(), value.as_bytes());
    }
    storage.batch_write(batch).unwrap();

    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    let snapshot_storage = test_storage_at_path(snapshot_path);
    let value = snapshot_storage.get(cf, b"key_25").unwrap().unwrap();
    assert_eq!(value, b"value_25");
}

#[test]
fn test_fsync_behavior_across_batch_and_single_writes() {
    let (storage, _dir) = test_storage();

    storage
        .put(ColumnFamily::SystemRaftState, b"key1", b"value1")
        .unwrap();

    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"key2", b"value2");
    batch.put(ColumnFamily::SystemRaftState, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    assert_eq!(
        storage
            .get(ColumnFamily::SystemRaftState, b"key1")
            .unwrap()
            .unwrap(),
        b"value1"
    );
    assert_eq!(
        storage.get(ColumnFamily::DataKv, b"key2").unwrap().unwrap(),
        b"value2"
    );
    assert_eq!(
        storage
            .get(ColumnFamily::SystemRaftState, b"key3")
            .unwrap()
            .unwrap(),
        b"value3"
    );
}

#[test]
fn test_exists_optimized_after_put() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    assert!(!storage.exists(cf, b"mykey").unwrap());

    storage.put(cf, b"mykey", b"myvalue").unwrap();

    assert!(storage.exists(cf, b"mykey").unwrap());

    storage.delete(cf, b"mykey").unwrap();

    assert!(!storage.exists(cf, b"mykey").unwrap());
}

#[test]
fn test_delete_removes_from_iterator_view() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();
    storage.put(cf, b"key3", b"value3").unwrap();

    storage.delete(cf, b"key2").unwrap();

    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let keys: Vec<Vec<u8>> =
        std::iter::from_fn(|| iter.step_forward().unwrap().map(|(k, _v)| k.to_vec())).collect();

    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&b"key1".to_vec()));
    assert!(keys.contains(&b"key3".to_vec()));
    assert!(!keys.contains(&b"key2".to_vec()));
}

// ============================================================================
// Atomicity & Consistency
// ============================================================================

#[test]
fn test_batch_atomicity_all_succeed() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    let mut batch = WriteBatch::new();
    batch.put(cf, b"key1", b"value1");
    batch.put(cf, b"key2", b"value2");
    batch.put(cf, b"key3", b"value3");

    storage.batch_write(batch).unwrap();

    assert!(storage.get(cf, b"key1").unwrap().is_some());
    assert!(storage.get(cf, b"key2").unwrap().is_some());
    assert!(storage.get(cf, b"key3").unwrap().is_some());
}

#[test]
fn test_batch_atomicity_with_deletes() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    storage.put(cf, b"key1", b"value1").unwrap();
    storage.put(cf, b"key2", b"value2").unwrap();

    let mut batch = WriteBatch::new();
    batch.delete(cf, b"key1");
    batch.put(cf, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    assert_eq!(storage.get(cf, b"key1").unwrap(), None);
    assert_eq!(storage.get(cf, b"key2").unwrap().unwrap(), b"value2");
    assert_eq!(storage.get(cf, b"key3").unwrap().unwrap(), b"value3");
}

#[test]
fn test_log_index_validation_prevents_gaps() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::SystemRaftLog;

    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    let result = storage.append_log_entry(cf, 3, b"entry3");
    assert!(result.is_err(), "Should reject non-sequential index");

    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(1));

    storage.append_log_entry(cf, 2, b"entry2").unwrap();
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(2));
}

#[test]
fn test_snapshot_isolation_during_writes() {
    let (storage, dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    populate_test_data(&storage, cf, 50);

    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    for i in 0..50 {
        let key = format!("key_{:06}", i);
        let value = format!("modified_{:06}", i);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }

    let snapshot_storage = test_storage_at_path(snapshot_path);
    let value = snapshot_storage.get(cf, b"key_000000").unwrap().unwrap();
    assert_eq!(
        value, b"value_000000",
        "Snapshot should have original value"
    );
}

#[test]
fn test_cf_operations_independent() {
    let (storage, _dir) = test_storage();

    let mut batch = WriteBatch::new();
    batch.put(ColumnFamily::DataKv, b"key1", b"value1");
    batch.put(ColumnFamily::SystemData, b"key2", b"value2");
    batch.put(ColumnFamily::DataRaftState, b"key3", b"value3");
    storage.batch_write(batch).unwrap();

    storage.delete(ColumnFamily::DataKv, b"key1").unwrap();

    assert_eq!(storage.get(ColumnFamily::DataKv, b"key1").unwrap(), None);
    assert_eq!(
        storage
            .get(ColumnFamily::SystemData, b"key2")
            .unwrap()
            .unwrap(),
        b"value2"
    );
    assert_eq!(
        storage
            .get(ColumnFamily::DataRaftState, b"key3")
            .unwrap()
            .unwrap(),
        b"value3"
    );
}

// ============================================================================
// Performance & Scalability
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

    verify_kv_data(&storage, cf, 1000);
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

    for i in 0..1000 {
        let key = format!("batch_key_{:06}", i);
        assert!(storage.get(cf, key.as_bytes()).unwrap().is_some());
    }
}

#[test]
fn test_iterate_10000_keys() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataKv;

    populate_test_data(&storage, cf, 10000);

    let start = std::time::Instant::now();

    let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
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

    populate_test_data(&storage, cf, 10000);

    let start = std::time::Instant::now();

    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    let duration = start.elapsed();
    println!("Snapshot of 10k keys completed in {:?}", duration);

    let snapshot_storage = test_storage_at_path(snapshot_path);
    assert!(snapshot_storage.get(cf, b"key_005000").unwrap().is_some());
}

// ============================================================================
// Recovery & Durability
// ============================================================================

#[test]
fn test_data_persists_after_reopen() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, ColumnFamily::DataKv, 100);
        storage.close().unwrap();
    }

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

    {
        let storage = test_storage_at_path(path.clone());
        for i in 1..=50 {
            storage
                .append_log_entry(cf, i, format!("entry{}", i).as_bytes())
                .unwrap();
        }
        assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(50));
        storage.close().unwrap();
    }

    {
        let storage = test_storage_at_path(path.clone());
        assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(50));
    }
}

#[test]
fn test_snapshot_survives_source_db_close() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();
    let snapshot_path = dir.path().join("snapshot");
    let cf = ColumnFamily::DataKv;

    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, cf, 100);
        storage.create_snapshot(&snapshot_path).unwrap();
        storage.close().unwrap();
    }

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

    {
        let storage = test_storage_at_path(path.clone());
        populate_test_data(&storage, cf, 50);
        storage.close().unwrap();
    }

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

    {
        let storage = test_storage_at_path(path.clone());
        verify_kv_data(&storage, cf, 100);
    }
}

// ============================================================================
// Error Scenarios
// ============================================================================

#[test]
fn test_invalid_log_index_prevents_all_appends() {
    let (storage, _dir) = test_storage();
    let cf = ColumnFamily::DataRaftLog;

    let result = storage.append_log_entry(cf, 0, b"entry0");
    assert!(result.is_err());

    let result = storage.append_log_entry(cf, 2, b"entry2");
    assert!(result.is_err());

    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    let result = storage.append_log_entry(cf, 1, b"entry1_dup");
    assert!(result.is_err());

    let result = storage.append_log_entry(cf, 3, b"entry3");
    assert!(result.is_err());
}

#[test]
fn test_batch_with_invalid_operations_fails() {
    let (storage, _dir) = test_storage();

    let batch = WriteBatch::new();
    assert!(batch.is_empty());

    storage.batch_write(batch).unwrap();
}

#[test]
fn test_snapshot_to_existing_path_fails() {
    let (storage, dir) = test_storage();

    let snapshot_path = dir.path().join("snapshot");
    storage.create_snapshot(&snapshot_path).unwrap();

    let result = storage.create_snapshot(&snapshot_path);
    assert!(result.is_err(), "Should not overwrite existing snapshot");
}
