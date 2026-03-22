//! Integration tests for StorageIterator.
//!
//! Tests snapshot-isolated iteration, seek operations, forward/reverse iteration,
//! and range queries.

use seshat_storage::{
    iterator::{Direction, IteratorMode},
    ColumnFamily, Storage, StorageOptions,
};
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
    let (key, value) = iter.step_forward().unwrap().unwrap();
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
    let (key, value) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"c");
    assert_eq!(&*value, b"val_c");
}

#[test]
fn test_iterator_from_mode_forward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(
        &storage,
        ColumnFamily::DataKv,
        &["a", "b", "c", "d"],
        "val_",
    );

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"b".to_vec(), Direction::Forward),
        )
        .unwrap();

    assert!(iter.valid());
    let (key, _) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"b");
}

#[test]
fn test_iterator_from_mode_reverse() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(
        &storage,
        ColumnFamily::DataKv,
        &["a", "b", "c", "d"],
        "val_",
    );

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"c".to_vec(), Direction::Reverse),
        )
        .unwrap();

    assert!(iter.valid());
    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"c");
}

#[test]
fn test_iterator_on_empty_cf() {
    let (storage, _temp_dir) = create_test_storage();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert!(!iter.valid());
    assert!(iter.step_forward().unwrap().is_none());
    assert!(iter.key().is_none());
    assert!(iter.value().is_none());
}

#[test]
fn test_iterator_works_on_all_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    for cf in ColumnFamily::all() {
        storage.put(cf, b"test", b"value").unwrap();

        let mut iter = storage.iterator(cf, IteratorMode::Start).unwrap();
        assert!(iter.valid());
        assert!(iter.step_forward().unwrap().is_some());
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

    let (key, _) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"a");

    let (key, _) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"b");

    let (key, _) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"c");

    assert!(iter.step_forward().unwrap().is_none());
}

#[test]
fn test_next_returns_none_at_end() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    iter.step_forward().unwrap();
    assert!(iter.step_forward().unwrap().is_none());
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
    while let Some((key, _value)) = iter.step_forward().unwrap() {
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
    while iter.step_forward().unwrap().is_some() {
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
    while let Some((key, value)) = iter.step_forward().unwrap() {
        assert_eq!(&*key, b"only");
        assert_eq!(&*value, b"value");
        count += 1;
    }

    assert_eq!(count, 1);
}

#[test]
fn test_iterate_1000_keys() {
    let (storage, _temp_dir) = create_test_storage();

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
    while iter.step_forward().unwrap().is_some() {
        count += 1;
    }

    assert_eq!(count, 1000);
}

#[test]
fn test_next_preserves_order() {
    let (storage, _temp_dir) = create_test_storage();

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
    while let Some((key, _)) = iter.step_forward().unwrap() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

    assert_eq!(collected_keys, vec!["apple", "banana", "mango", "zebra"]);
}

#[test]
fn test_key_value_methods_work_during_iteration() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a", "b"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    assert_eq!(&*iter.key().unwrap(), b"a");
    assert_eq!(&*iter.value().unwrap(), b"val_a");

    iter.step_forward().unwrap();

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

    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"c");

    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"b");

    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"a");

    assert!(iter.step_backward().unwrap().is_none());
}

#[test]
fn test_prev_returns_none_at_start() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(&storage, ColumnFamily::DataKv, &["a"], "val_");

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    iter.step_backward().unwrap();
    assert!(iter.step_backward().unwrap().is_none());
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
    while let Some((key, _value)) = iter.step_backward().unwrap() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

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

    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"c");

    let (key, _) = iter.step_backward().unwrap().unwrap();
    assert_eq!(&*key, b"b");
}

#[test]
fn test_from_end_mode_iterates_backward() {
    let (storage, _temp_dir) = create_test_storage();
    populate_test_data(
        &storage,
        ColumnFamily::DataKv,
        &["a", "b", "c", "d"],
        "val_",
    );

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::End)
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.step_backward().unwrap() {
        collected.push(String::from_utf8(key.to_vec()).unwrap());
    }

    assert_eq!(collected, vec!["d", "c", "b", "a"]);
}

#[test]
fn test_reverse_preserves_order() {
    let (storage, _temp_dir) = create_test_storage();

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
    while let Some((key, _)) = iter.step_backward().unwrap() {
        collected_keys.push(String::from_utf8(key.to_vec()).unwrap());
    }

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

    iter.step_forward().unwrap();
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
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .unwrap();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    storage
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .unwrap();

    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
        count += 1;
    }

    assert_eq!(count, 1);
}

#[test]
fn test_multiple_iterators_independent() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .unwrap();

    let mut iter1 = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    storage
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .unwrap();

    let mut iter2 = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count1 = 0;
    while iter1.step_forward().unwrap().is_some() {
        count1 += 1;
    }

    let mut count2 = 0;
    while iter2.step_forward().unwrap().is_some() {
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

    let (key, _) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"a");

    storage
        .put(ColumnFamily::DataKv, b"b", b"modified")
        .unwrap();
    storage.delete(ColumnFamily::DataKv, b"c").unwrap();

    let (key, value) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"b");
    assert_eq!(&*value, b"val_b");

    let (key, value) = iter.step_forward().unwrap().unwrap();
    assert_eq!(&*key, b"c");
    assert_eq!(&*value, b"val_c");

    assert!(iter.step_forward().unwrap().is_none());
}

#[test]
fn test_iterator_sees_consistent_view() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .unwrap();
    storage
        .put(ColumnFamily::DataKv, b"key2", b"value2")
        .unwrap();

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    storage.delete(ColumnFamily::DataKv, b"key1").unwrap();
    storage.delete(ColumnFamily::DataKv, b"key2").unwrap();

    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
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

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"b".to_vec(), Direction::Forward),
        )
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.step_forward().unwrap() {
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        if key_str.as_str() >= "e" {
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

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"e".to_vec(), Direction::Reverse),
        )
        .unwrap();

    let mut collected = Vec::new();
    while let Some((key, _)) = iter.step_backward().unwrap() {
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        if key_str.as_str() < "b" {
            break;
        }
        collected.push(key_str);
    }

    assert_eq!(collected, vec!["e", "d", "c", "b"]);
}

#[test]
fn test_prefix_scan() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .put(ColumnFamily::DataKv, b"user:1", b"alice")
        .unwrap();
    storage
        .put(ColumnFamily::DataKv, b"user:2", b"bob")
        .unwrap();
    storage
        .put(ColumnFamily::DataKv, b"user:3", b"charlie")
        .unwrap();
    storage
        .put(ColumnFamily::DataKv, b"post:1", b"hello")
        .unwrap();
    storage
        .put(ColumnFamily::DataKv, b"post:2", b"world")
        .unwrap();

    let mut iter = storage
        .iterator(
            ColumnFamily::DataKv,
            IteratorMode::From(b"user:".to_vec(), Direction::Forward),
        )
        .unwrap();

    let mut user_keys = Vec::new();
    while let Some((key, _)) = iter.step_forward().unwrap() {
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

    for i in 0..100 {
        let key = format!("key_{:03}", i);
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), b"value")
            .unwrap();
    }

    let mut iter = storage
        .iterator(ColumnFamily::DataKv, IteratorMode::Start)
        .unwrap();

    let mut count = 0;
    while iter.step_forward().unwrap().is_some() {
        count += 1;
        if count >= 10 {
            break;
        }
    }

    assert_eq!(count, 10);
}
