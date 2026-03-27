//! Property-based tests for storage layer using proptest.
//!
//! These tests verify invariants hold across randomly generated inputs,
//! catching edge cases that traditional tests might miss.
//!
//! Test Categories:
//! 1. Serialization & Encoding (3 properties)
//! 2. Log Index Properties (5 properties)
//! 3. Batch Atomicity (3 properties)
//! 4. Iterator Consistency (4 properties)
//! 5. Column Family Isolation (2 properties)
//! 6. Snapshot Consistency (3 properties)

use proptest::prelude::*;
use seshat_storage::{
    iterator::{Direction, IteratorMode},
    ColumnFamily, Storage, StorageOptions, WriteBatch,
};
use std::collections::HashSet;
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create temporary storage for property tests.
fn test_storage() -> (Storage, TempDir) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let opts = StorageOptions::with_data_dir(dir.path().to_path_buf());
    let storage = Storage::new(opts).expect("Failed to create storage");
    (storage, dir)
}

/// Create temporary storage at specific path.
fn test_storage_at_path(path: std::path::PathBuf) -> Storage {
    let opts = StorageOptions::with_data_dir(path);
    Storage::new(opts).expect("Failed to create storage")
}

// ============================================================================
// Strategy Definitions
// ============================================================================

#[allow(dead_code)]
fn log_index_strategy() -> impl Strategy<Value = u64> {
    1u64..=1000
}

#[allow(dead_code)]
fn sequential_indices(count: usize) -> impl Strategy<Value = Vec<u64>> {
    Just((1..=count as u64).collect())
}

/// Generate random keys (0..100 bytes).
fn key_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..100)
}

/// Generate random values (0..1000 bytes).
fn value_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..1000)
}

/// Generate small keys (1..20 bytes, useful for batch tests).
fn small_key_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..20)
}

/// Generate small values (1..100 bytes).
fn small_value_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..100)
}

#[allow(dead_code)]
fn kv_pair_strategy() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    (key_strategy(), value_strategy())
}

/// Generate small key-value pairs for batch tests.
fn small_kv_pair_strategy() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    (small_key_strategy(), small_value_strategy())
}

/// Generate a vector of unique key-value pairs.
fn unique_kv_pairs(count: usize) -> impl Strategy<Value = Vec<(Vec<u8>, Vec<u8>)>> {
    prop::collection::vec(small_kv_pair_strategy(), count..=count).prop_filter(
        "keys must be unique",
        |pairs| {
            let keys: HashSet<_> = pairs.iter().map(|(k, _)| k).collect();
            keys.len() == pairs.len()
        },
    )
}

#[allow(dead_code)]
fn log_entry_strategy() -> impl Strategy<Value = (u64, Vec<u8>)> {
    (log_index_strategy(), value_strategy())
}

// ============================================================================
// 1. Serialization & Encoding (3 properties)
// ============================================================================

proptest! {
    /// Property: Any key survives put → get roundtrip.
    #[test]
    fn prop_key_roundtrip(key in key_strategy(), value in value_strategy()) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Skip empty keys (RocksDB doesn't support them well)
        prop_assume!(!key.is_empty());

        storage.put(cf, &key, &value).unwrap();
        let retrieved = storage.get(cf, &key).unwrap();

        prop_assert_eq!(retrieved, Some(value));
    }

    /// Property: Any value survives put → get roundtrip.
    #[test]
    fn prop_value_roundtrip(key in key_strategy(), value in value_strategy()) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        prop_assume!(!key.is_empty());

        storage.put(cf, &key, &value).unwrap();
        let retrieved = storage.get(cf, &key).unwrap();

        prop_assert_eq!(retrieved, Some(value));
    }

    /// Property: Non-UTF8 binary data survives roundtrip.
    #[test]
    fn prop_binary_data_roundtrip(key in key_strategy(), value in value_strategy()) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        prop_assume!(!key.is_empty());

        storage.put(cf, &key, &value).unwrap();
        let retrieved = storage.get(cf, &key).unwrap().unwrap();

        // Binary comparison (not UTF-8)
        prop_assert_eq!(retrieved, value);
    }
}

// ============================================================================
// 2. Log Index Properties (5 properties)
// ============================================================================

proptest! {
    /// Property: Sequential appends [1..n] always succeed.
    #[test]
    fn prop_sequential_appends_always_succeed(count in 1usize..=50) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::SystemRaftLog;

        for i in 1..=count as u64 {
            let entry = format!("entry_{}", i);
            let result = storage.append_log_entry(cf, i, entry.as_bytes());
            prop_assert!(result.is_ok(), "Sequential append {} should succeed", i);
        }

        // Verify last index
        prop_assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(count as u64));
    }

    /// Property: Gaps in log indices always fail.
    #[test]
    fn prop_gap_detection(count in 1u64..=20, gap_size in 1u64..=5) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataRaftLog;

        // Append sequential entries 1..count
        for i in 1..=count {
            storage.append_log_entry(cf, i, b"entry").unwrap();
        }

        // Try to append with gap after count
        let gap_idx = count + 1 + gap_size;
        let result = storage.append_log_entry(cf, gap_idx, b"gap_entry");

        prop_assert!(result.is_err(), "Gap at index {} should fail", gap_idx);

        // Verify last index unchanged
        prop_assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(count));
    }

    /// Property: Duplicate indices always fail.
    #[test]
    fn prop_duplicate_detection(count in 1u64..=50) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::SystemRaftLog;

        // Append sequential entries 1..count
        for i in 1..=count {
            storage.append_log_entry(cf, i, b"entry").unwrap();
        }

        // Try to append the last index again (duplicate)
        let result = storage.append_log_entry(cf, count, b"duplicate");

        prop_assert!(result.is_err(), "Duplicate index {} should fail", count);
    }

    /// Property: Cached index always equals DB index.
    #[test]
    fn prop_cache_matches_db(count in 1usize..=30) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataRaftLog;

        // Append entries
        for i in 1..=count as u64 {
            storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
        }

        // Cache should match DB
        let cached_index = storage.get_last_log_index(cf).unwrap();
        prop_assert_eq!(cached_index, Some(count as u64));

        // Read all entries to verify DB state
        let entries = storage.get_log_range(cf, 1, count as u64 + 1).unwrap();
        prop_assert_eq!(entries.len(), count);
    }

    /// Property: Truncate keeps indices ≥ truncate_index.
    #[test]
    fn prop_truncate_preserves_tail(total in 10u64..=50, truncate_before in 1u64..=25) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::SystemRaftLog;

        // Append entries
        for i in 1..=total {
            storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
        }

        // Truncate
        storage.truncate_log_before(cf, truncate_before).unwrap();

        // Verify remaining entries
        let remaining = storage.get_log_range(cf, 1, total + 1).unwrap();
        let expected_count = if truncate_before <= total {
            (total - truncate_before + 1) as usize
        } else {
            0
        };

        prop_assert_eq!(remaining.len(), expected_count,
            "After truncating before {}, expected {} entries, got {}",
            truncate_before, expected_count, remaining.len());
    }
}

// ============================================================================
// 3. Batch Atomicity (3 properties)
// ============================================================================

proptest! {
    /// Property: After batch write, all keys present or none.
    #[test]
    fn prop_batch_all_visible_or_none(pairs in unique_kv_pairs(10)) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        let mut batch = WriteBatch::new();
        for (key, value) in &pairs {
            batch.put(cf, key, value);
        }

        storage.batch_write(batch).unwrap();

        // All keys must be visible
        for (key, expected_value) in &pairs {
            let retrieved = storage.get(cf, key).unwrap();
            prop_assert_eq!(retrieved, Some(expected_value.clone()),
                "Key {:?} should be visible after batch", key);
        }
    }

    /// Property: Batch writes appear in order.
    #[test]
    fn prop_batch_order_preserved(pairs in unique_kv_pairs(5)) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Use same key, different values
        let key = b"test_key";
        let mut batch = WriteBatch::new();

        for (_, value) in &pairs {
            batch.put(cf, key, value);
        }

        storage.batch_write(batch).unwrap();

        // Last write should win
        let retrieved = storage.get(cf, key).unwrap();
        prop_assert_eq!(retrieved, Some(pairs.last().unwrap().1.clone()),
            "Last write in batch should win");
    }

    /// Property: Empty batch is a no-op.
    #[test]
    fn prop_empty_batch_is_noop(key in key_strategy(), value in value_strategy()) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        prop_assume!(!key.is_empty());

        // Put initial value
        storage.put(cf, &key, &value).unwrap();

        // Execute empty batch
        let batch = WriteBatch::new();
        storage.batch_write(batch).unwrap();

        // Value should be unchanged
        let retrieved = storage.get(cf, &key).unwrap();
        prop_assert_eq!(retrieved, Some(value));
    }
}

// ============================================================================
// 4. Iterator Consistency (4 properties)
// ============================================================================

proptest! {
    /// Property: Iterator finds all inserted keys.
    #[test]
    fn prop_iterator_sees_all_keys(pairs in unique_kv_pairs(20)) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Insert all pairs
        for (key, value) in &pairs {
            storage.put(cf, key, value).unwrap();
        }

        // Collect all keys from iterator
        let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
        let mut seen_keys = HashSet::new();

        while let Some((key, _value)) = iter.step_forward().unwrap() {
            seen_keys.insert(key.to_vec());
        }

        // Verify all keys seen
        for (key, _) in &pairs {
            prop_assert!(seen_keys.contains(key),
                "Iterator should see key {:?}", key);
        }
    }

    /// Property: Forward iteration is sorted.
    #[test]
    fn prop_iterator_order_preserved(count in 5usize..=20) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Insert keys with sortable names
        for i in 0..count {
            let key = format!("key_{:06}", i);
            storage.put(cf, key.as_bytes(), b"value").unwrap();
        }

        // Iterate and collect keys
        let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
        let mut keys = Vec::new();
        while let Some((key, _)) = iter.step_forward().unwrap() {
            keys.push(key.to_vec());
        }

        // Verify sorted order
        let mut sorted_keys = keys.clone();
        sorted_keys.sort();
        prop_assert_eq!(keys, sorted_keys, "Keys should be in sorted order");
    }

    /// Property: Reverse iteration is forward reversed.
    #[test]
    fn prop_reverse_is_forward_reversed(count in 5usize..=15) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Insert keys
        for i in 0..count {
            let key = format!("key_{:03}", i);
            storage.put(cf, key.as_bytes(), b"value").unwrap();
        }

        // Forward iteration
        let mut forward_iter = storage.iterator(cf, IteratorMode::Start).unwrap();
        let mut forward_keys = Vec::new();
        while let Some((key, _)) = forward_iter.step_forward().unwrap() {
            forward_keys.push(key.to_vec());
        }

        // Reverse iteration
        let mut reverse_iter = storage.iterator(cf, IteratorMode::End).unwrap();
        let mut reverse_keys = Vec::new();
        while let Some((key, _)) = reverse_iter.step_backward().unwrap() {
            reverse_keys.push(key.to_vec());
        }

        // Reverse should be forward reversed
        let mut expected = forward_keys.clone();
        expected.reverse();
        prop_assert_eq!(reverse_keys, expected,
            "Reverse iteration should be forward iteration reversed");
    }

    /// Property: Seek either finds key or ends iterator.
    #[test]
    fn prop_seek_always_valid_or_end(pairs in unique_kv_pairs(15), seek_key in key_strategy()) {
        let (storage, _dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        prop_assume!(!seek_key.is_empty());

        // Insert pairs
        for (key, value) in &pairs {
            storage.put(cf, key, value).unwrap();
        }

        // Seek to key using From mode
        let iter_result = storage.iterator(cf, IteratorMode::From(seek_key.clone(), Direction::Forward));

        // Should either succeed or fail gracefully
        match iter_result {
            Ok(mut iter) => {
                // Iterator should be valid (either found key or next key)
                if let Some((found_key, _)) = iter.step_forward().unwrap() {
                    // Found key should be >= seek_key
                    prop_assert!(found_key.as_ref() >= seek_key.as_slice(),
                        "Seek result should be >= seek key");
                }
                // If None, iterator is at end (valid state)
            }
            Err(_) => {
                // Error is acceptable (e.g., invalid state)
            }
        }
    }
}

// ============================================================================
// 5. Column Family Isolation (2 properties)
// ============================================================================

proptest! {
    /// Property: Write to CF1 doesn't affect CF2.
    #[test]
    fn prop_cf_operations_independent(
        key1 in key_strategy(),
        value1 in value_strategy(),
        key2 in key_strategy(),
        value2 in value_strategy()
    ) {
        let (storage, _dir) = test_storage();

        prop_assume!(!key1.is_empty() && !key2.is_empty());

        let cf1 = ColumnFamily::DataKv;
        let cf2 = ColumnFamily::SystemData;

        // Write to CF1
        storage.put(cf1, &key1, &value1).unwrap();

        // Write to CF2
        storage.put(cf2, &key2, &value2).unwrap();

        // Verify isolation
        let retrieved1 = storage.get(cf1, &key1).unwrap();
        let retrieved2 = storage.get(cf2, &key2).unwrap();

        prop_assert_eq!(retrieved1, Some(value1.clone()));
        prop_assert_eq!(retrieved2, Some(value2.clone()));

        // key1 shouldn't exist in CF2
        let cross_check = storage.get(cf2, &key1).unwrap();
        prop_assert_eq!(cross_check, None, "Key from CF1 shouldn't exist in CF2");
    }

    /// Property: Iterator on CF1 doesn't see CF2 data.
    #[test]
    fn prop_cf_iterators_independent(pairs in unique_kv_pairs(10)) {
        let (storage, _dir) = test_storage();

        let cf1 = ColumnFamily::DataKv;
        let cf2 = ColumnFamily::SystemData;

        // Write same keys to both CFs
        for (key, value) in &pairs {
            storage.put(cf1, key, value).unwrap();
            storage.put(cf2, key, b"different_value").unwrap();
        }

        // Iterate CF1
        let mut iter1 = storage.iterator(cf1, IteratorMode::Start).unwrap();
        let mut cf1_values = Vec::new();
        while let Some((_, value)) = iter1.step_forward().unwrap() {
            cf1_values.push(value.to_vec());
        }

        // Verify CF1 iterator only sees CF1 values
        for value in &cf1_values {
            prop_assert_ne!(value, b"different_value",
                "CF1 iterator should not see CF2 values");
        }
    }
}

// ============================================================================
// 6. Snapshot Consistency (3 properties)
// ============================================================================

proptest! {
    /// Property: Snapshot matches source at creation time.
    #[test]
    fn prop_snapshot_matches_source_at_creation(pairs in unique_kv_pairs(10)) {
        let (storage, dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Write data
        for (key, value) in &pairs {
            storage.put(cf, key, value).unwrap();
        }

        // Create snapshot
        let snapshot_path = dir.path().join("snapshot");
        storage.create_snapshot(&snapshot_path).unwrap();

        // Verify snapshot has same data
        let snapshot_storage = test_storage_at_path(snapshot_path);

        for (key, expected_value) in &pairs {
            let retrieved = snapshot_storage.get(cf, key).unwrap();
            prop_assert_eq!(retrieved, Some(expected_value.clone()),
                "Snapshot should have same data as source");
        }
    }

    /// Property: Changes after snapshot don't affect snapshot.
    #[test]
    fn prop_snapshot_unaffected_by_source_changes(
        initial_pairs in unique_kv_pairs(10),
        key_to_modify in 0usize..10,
        new_value in value_strategy()
    ) {
        let (storage, dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Write initial data
        for (key, value) in &initial_pairs {
            storage.put(cf, key, value).unwrap();
        }

        // Create snapshot
        let snapshot_path = dir.path().join("snapshot");
        storage.create_snapshot(&snapshot_path).unwrap();

        // Modify source after snapshot
        let (modify_key, original_value) = &initial_pairs[key_to_modify];
        storage.put(cf, modify_key, &new_value).unwrap();

        // Snapshot should have original value
        let snapshot_storage = test_storage_at_path(snapshot_path);
        let snapshot_value = snapshot_storage.get(cf, modify_key).unwrap();

        prop_assert_eq!(snapshot_value, Some(original_value.clone()),
            "Snapshot should have original value, not modified value");
    }

    /// Property: Snapshot can always be reopened.
    #[test]
    fn prop_snapshot_reopenable(pairs in unique_kv_pairs(5)) {
        let (storage, dir) = test_storage();
        let cf = ColumnFamily::DataKv;

        // Write data
        for (key, value) in &pairs {
            storage.put(cf, key, value).unwrap();
        }

        // Create snapshot
        let snapshot_path = dir.path().join("snapshot");
        storage.create_snapshot(&snapshot_path).unwrap();

        // Open snapshot multiple times
        for _ in 0..3 {
            let snapshot_storage = test_storage_at_path(snapshot_path.clone());

            // Verify data still accessible
            for (key, expected_value) in &pairs {
                let retrieved = snapshot_storage.get(cf, key).unwrap();
                prop_assert_eq!(retrieved, Some(expected_value.clone()),
                    "Snapshot should be reopenable multiple times");
            }

            // Close snapshot
            snapshot_storage.close().unwrap();
        }
    }
}
