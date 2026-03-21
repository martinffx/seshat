//! Integration tests for StorageIterator.
//!
//! Tests snapshot-isolated iteration, seek operations, forward/reverse iteration,
//! and range queries.

use seshat_storage::{
    iterator::{Direction, IteratorMode},
    ColumnFamily, Storage, StorageOptions,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test storage with temp directory
fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options).unwrap();
    (storage, temp_dir)
}

/// Helper to populate storage with test data
fn populate_test_data(storage: &Storage, cf: ColumnFamily, keys: &[&str], value_prefix: &str) {
    for key in keys {
        let value = format!("{}{}", value_prefix, key);
        storage.put(cf, key.as_bytes(), value.as_bytes()).unwrap();
    }
}

// ============================================================================
// Iterator Creation Tests (6 tests)
// ============================================================================

#[test]
fn test_iterator_start_mode() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert!(iter.valid());
    let (key, value) = iter.next().unwrap();
    assert_eq!(&*key, b"a");
    assert_eq!(&*value, b"val_a");
}

#[test]
fn test_iterator_end_mode() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    assert!(iter.valid());
    let (key, value) = iter.prev().unwrap();
    assert_eq!(&*key, b"c");
    assert_eq!(&*value, b"val_c");
}

#[test]
fn test_iterator_from_mode_forward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c", "d"], "val_");

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"b".to_vec(), Direction::Forward),
        )
        .unwrap();

    assert!(iter.valid());
    let (key, _) = iter.next().unwrap();
    assert_eq!(&*key, b"b");
}

#[test]
fn test_iterator_from_mode_reverse() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c", "d"], "val_");

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"c".to_vec(), Direction::Reverse),
        )
        .unwrap();

    assert!(iter.valid());
    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"c");
}

#[test]
fn test_iterator_on_empty_cf() {
    let (storage, _temp_dir) = create_test_storage();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert!(!iter.valid());
    assert!(iter.next().is_none());
    assert!(iter.key().is_none());
    assert!(iter.value().is_none());
}

#[test]
fn test_iterator_works_on_all_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    // Test that iterator can be created for all column families
    for cf in ColumnFamily::all() {
        storage.put(*cf, b"test", b"value").unwrap();

        let mut iter = storage.iterator(*cf, IteratorMode::Start).unwrap();
        assert!(iter.valid());
        assert!(iter.next().is_some());
    }
}

// ============================================================================
// Seek Operations Tests (6 tests)
// ============================================================================

#[test]
fn test_seek_positions_at_key() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(
        &storage,
        ColumnFamily::DataKv,
        &["apple", "banana", "cherry", "date"],
        "val_",
    );

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    iter.seek(b"cherry");
    assert!(iter.valid());
    assert_eq!(&*iter.key().unwrap(), b"cherry");
}

#[test]
fn test_seek_to_first() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    // Start at end, then seek to first
    iter.seek_to_first();
    assert!(iter.valid());
    assert_eq!(&*iter.key().unwrap(), b"a");
}

#[test]
fn test_seek_to_last() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Start at beginning, then seek to last
    iter.seek_to_last();
    assert!(iter.valid());
    assert_eq!(&*iter.key().unwrap(), b"c");
}

#[test]
fn test_seek_to_nonexistent_key_finds_next() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "c", "e"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Seek to "b" (doesn't exist), should position at "c" (first key >= "b")
    iter.seek(b"b");
    assert!(iter.valid());
    assert_eq!(&*iter.key().unwrap(), b"c");
}

#[test]
fn test_seek_before_first_key() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["b", "c", "d"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Seek before first key, should position at first key
    iter.seek(b"a");
    assert!(iter.valid());
    assert_eq!(&*iter.key().unwrap(), b"b");
}

#[test]
fn test_seek_after_last_key() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Seek after last key, iterator should become invalid
    iter.seek(b"z");
    assert!(!iter.valid());
    assert!(iter.key().is_none());
}

// ============================================================================
// Forward Iteration Tests (8 tests)
// ============================================================================

#[test]
fn test_next_iterates_forward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let (key, _) = iter.next().unwrap();
    assert_eq!(&*key, b"a");

    let (key, _) = iter.next().unwrap();
    assert_eq!(&*key, b"b");

    let (key, _) = iter.next().unwrap();
    assert_eq!(&*key, b"c");

    assert!(iter.next().is_none());
}

#[test]
fn test_next_returns_none_at_end() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    iter.next().unwrap(); // Consume the only element
    assert!(iter.next().is_none());
    assert!(!iter.valid());
}

#[test]
fn test_iterate_all_keys_forward() {
    let (storage, _temp_dir) = create_test_storage();
    let keys = vec!["apple", "banana", "cherry", "date", "elderberry"];
    populate_test_data(&storage, ColumnFamily::DataKv, &keys, "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut collected_keys = Vec::new();
    while let Some((key, _value)) = iter.next() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

    assert_eq!(collected_keys, keys);
}

#[test]
fn test_iterate_empty_cf() {
    let (storage, _temp_dir) = create_test_storage();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }

    assert_eq!(count, 0);
}

#[test]
fn test_iterate_single_key() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .put(ColumnFamily::DataKv, b"only", b"value")
        .unwrap();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count = 0;
    while let Some((key, value)) = iter.next() {
        assert_eq!(&*key, b"only");
        assert_eq!(&*value, b"value");
        count += 1;
    }

    assert_eq!(count, 1);
}

#[test]
fn test_iterate_1000_keys() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert 1000 keys
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let value = format!("value_{}", i);
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), value.as_bytes())
            .unwrap();
    }

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }

    assert_eq!(count, 1000);
}

#[test]
fn test_next_preserves_order() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert keys in random order
    let keys = vec!["zebra", "apple", "mango", "banana"];
    for key in &keys {
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), b"value")
            .unwrap();
    }

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut collected_keys = Vec::new();
    while let Some((key, _)) = iter.next() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

    // Should be in lexicographic order
    assert_eq!(collected_keys, vec!["apple", "banana", "mango", "zebra"]);
}

#[test]
fn test_key_value_methods_work_during_iteration() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Check initial position
    assert_eq!(&*iter.key().unwrap(), b"a");
    assert_eq!(&*iter.value().unwrap(), b"val_a");

    // Advance
    iter.next();

    // Check new position
    assert_eq!(&*iter.key().unwrap(), b"b");
    assert_eq!(&*iter.value().unwrap(), b"val_b");
}

// ============================================================================
// Reverse Iteration Tests (6 tests)
// ============================================================================

#[test]
fn test_prev_iterates_backward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"c");

    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"b");

    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"a");

    assert!(iter.prev().is_none());
}

#[test]
fn test_prev_returns_none_at_start() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    iter.prev().unwrap(); // Consume the only element
    assert!(iter.prev().is_none());
    assert!(!iter.valid());
}

#[test]
fn test_iterate_all_keys_reverse() {
    let (storage, _temp_dir) = create_test_storage();
    let keys = vec!["apple", "banana", "cherry", "date"];
    populate_test_data(&storage, ColumnFamily::DataKv, &keys, "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    let mut collected_keys = Vec::new();
    while let Some((key, _value)) = iter.prev() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

    // Should be in reverse order
    let mut expected = keys.clone();
    expected.reverse();
    assert_eq!(collected_keys, expected);
}

#[test]
fn test_seek_to_last_then_prev() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    iter.seek_to_last();
    assert_eq!(&*iter.key().unwrap(), b"c");

    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"c");

    let (key, _) = iter.prev().unwrap();
    assert_eq!(&*key, b"b");
}

#[test]
fn test_from_end_mode_iterates_backward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c", "d"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.prev() {
        collected.push(String::from_utf8(key.to_vec()).unwrap());
    }

    assert_eq!(collected, vec!["d", "c", "b", "a"]);
}

#[test]
fn test_reverse_preserves_order() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert keys
    let keys = vec!["zebra", "apple", "mango"];
    for key in &keys {
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), b"value")
            .unwrap();
    }

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    let mut collected_keys = Vec::new();
    while let Some((key, _)) = iter.prev() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

    // Should be in reverse lexicographic order
    assert_eq!(collected_keys, vec!["zebra", "mango", "apple"]);
}

// ============================================================================
// State Methods Tests (4 tests)
// ============================================================================

#[test]
fn test_valid_returns_true_for_valid_iterator() {
    let (storage, _temp_dir) = create_test_storage();
    storage.put(ColumnFamily::DataKv, b"key", b"value").unwrap();

    let iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert!(iter.valid());
}

#[test]
fn test_valid_returns_false_after_exhaustion() {
    let (storage, _temp_dir) = create_test_storage();
    storage.put(ColumnFamily::DataKv, b"key", b"value").unwrap();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    iter.next(); // Consume the only key
    assert!(!iter.valid());
}

#[test]
fn test_key_returns_current_key() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert_eq!(&*iter.key().unwrap(), b"a");
}

#[test]
fn test_value_returns_current_value() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert_eq!(&*iter.value().unwrap(), b"val_a");
}

// ============================================================================
// Snapshot Isolation Tests (4 tests)
// ============================================================================

#[test]
fn test_iterator_snapshot_isolation() {
    let (storage, _temp_dir) = create_test_storage();
    storage.put(ColumnFamily::DataKv, b"key1", b"value1").unwrap();

    // Create iterator (captures snapshot)
    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Write more data after iterator creation
    storage.put(ColumnFamily::DataKv, b"key2", b"value2").unwrap();

    // Iterator should only see data from its snapshot
    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }

    // Should only see key1, not key2
    assert_eq!(count, 1);
}

#[test]
fn test_multiple_iterators_independent() {
    let (storage, _temp_dir) = create_test_storage();
    storage.put(ColumnFamily::DataKv, b"key1", b"value1").unwrap();

    // Create first iterator
    let mut iter1 = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Add more data
    storage.put(ColumnFamily::DataKv, b"key2", b"value2").unwrap();

    // Create second iterator
    let mut iter2 = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // iter1 should see 1 key, iter2 should see 2 keys
    let mut count1 = 0;
    while iter1.next().is_some() {
        count1 += 1;
    }

    let mut count2 = 0;
    while iter2.next().is_some() {
        count2 += 1;
    }

    assert_eq!(count1, 1);
    assert_eq!(count2, 2);
}

#[test]
fn test_iterator_unaffected_by_writes() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b", "c"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Read first key
    let (key, _) = iter.next().unwrap();
    assert_eq!(&*key, b"a");

    // Modify data during iteration
    storage
        .put(ColumnFamily::DataKv, b"b", b"modified")
        .unwrap();
    storage.delete(ColumnFamily::DataKv, b"c").unwrap();

    // Iterator should see original data
    let (key, value) = iter.next().unwrap();
    assert_eq!(&*key, b"b");
    assert_eq!(&*value, b"val_b"); // Original value, not "modified"

    let (key, value) = iter.next().unwrap();
    assert_eq!(&*key, b"c");
    assert_eq!(&*value, b"val_c"); // Still visible, not deleted

    assert!(iter.next().is_none());
}

#[test]
fn test_iterator_sees_consistent_view() {
    let (storage, _temp_dir) = create_test_storage();
    storage.put(ColumnFamily::DataKv, b"key1", b"value1").unwrap();
    storage.put(ColumnFamily::DataKv, b"key2", b"value2").unwrap();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    // Delete all keys after iterator creation
    storage.delete(ColumnFamily::DataKv, b"key1").unwrap();
    storage.delete(ColumnFamily::DataKv, b"key2").unwrap();

    // Iterator should still see both keys
    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
    }

    assert_eq!(count, 2);
}

// ============================================================================
// Range Query Tests (4 tests)
// ============================================================================

#[test]
fn test_range_query_forward() {
    let (storage, _temp_dir) = create_test_storage();
    let keys = vec!["a", "b", "c", "d", "e", "f"];
    populate_test_data(&storage, ColumnFamily::DataKv, &keys, "val_");

    // Range query from "b" to "e"
    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"b".to_vec(), Direction::Forward),
        )
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.next() {
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        if key_str.as_bytes() >= b"e" {
            break;
        }
        collected.push(key_str);
    }

    assert_eq!(collected, vec!["b", "c", "d"]);
}

#[test]
fn test_range_query_reverse() {
    let (storage, _temp_dir) = create_test_storage();
    let keys = vec!["a", "b", "c", "d", "e", "f"];
    populate_test_data(&storage, ColumnFamily::DataKv, &keys, "val_");

    // Range query from "e" down to "b"
    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"e".to_vec(), Direction::Reverse),
        )
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.prev() {
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        if key_str.as_bytes() < b"b" {
            break;
        }
        collected.push(key_str);
    }

    assert_eq!(collected, vec!["e", "d", "c", "b"]);
}

#[test]
fn test_prefix_scan() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert keys with different prefixes
    storage.put(ColumnFamily::DataKv, b"user:1", b"alice").unwrap();
    storage.put(ColumnFamily::DataKv, b"user:2", b"bob").unwrap();
    storage.put(ColumnFamily::DataKv, b"user:3", b"charlie").unwrap();
    storage.put(ColumnFamily::DataKv, b"post:1", b"hello").unwrap();
    storage.put(ColumnFamily::DataKv, b"post:2", b"world").unwrap();

    // Scan all "user:" prefixed keys
    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"user:".to_vec(), Direction::Forward),
        )
        .unwrap();

    let mut user_keys = Vec::new();
    while let Some((key, _)) = iter.next() {
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        if !key_str.starts_with("user:") {
            break;
        }
        user_keys.push(key_str);
    }

    assert_eq!(user_keys, vec!["user:1", "user:2", "user:3"]);
}

#[test]
fn test_bounded_iteration() {
    let (storage, _temp_dir) = create_test_storage();

    // Insert 100 keys
    for i in 0..100 {
        let key = format!("key_{:03}", i);
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), b"value")
            .unwrap();
    }

    // Iterate with bound: stop after 10 keys
    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count = 0;
    while iter.next().is_some() {
        count += 1;
        if count >= 10 {
            break;
        }
    }

    assert_eq!(count, 10);
}
