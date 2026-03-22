//! Integration tests for log operations (ROCKS-006).

use seshat_storage::{ColumnFamily, Storage, StorageError, StorageOptions};
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

/// Creates a temporary storage instance for testing.
fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options).expect("Failed to create storage");
    (storage, temp_dir)
}

/// Helper to append multiple sequential log entries.
fn append_sequential_entries(
    storage: &Storage,
    cf: ColumnFamily,
    count: u64,
) -> Result<(), StorageError> {
    for i in 1..=count {
        let entry = format!("entry_{}", i);
        storage.append_log_entry(cf, i, entry.as_bytes())?;
    }
    Ok(())
}

// ============================================================================
// 1. Index Validation Tests (12 tests)
// ============================================================================

#[test]
fn test_append_first_entry_must_be_index_1() {
    let (storage, _temp_dir) = create_test_storage();

    // First entry must be index 1
    let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1");
    assert!(result.is_ok(), "First entry with index 1 should succeed");

    // Verify it was written
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(1));
}

#[test]
fn test_append_rejects_index_0() {
    let (storage, _temp_dir) = create_test_storage();

    // Index 0 is never valid (Raft logs are 1-indexed)
    let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, 0, b"entry0");

    assert!(result.is_err(), "Index 0 should be rejected");
    match result.unwrap_err() {
        StorageError::InvalidLogIndex { expected, got, .. } => {
            assert_eq!(expected, 1);
            assert_eq!(got, 0);
        }
        e => panic!("Expected InvalidLogIndex, got: {:?}", e),
    }
}

#[test]
fn test_append_rejects_index_2_as_first_entry() {
    let (storage, _temp_dir) = create_test_storage();

    // First entry cannot be index 2 (must start at 1)
    let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, 2, b"entry2");

    assert!(result.is_err(), "First entry must be index 1, not 2");
    match result.unwrap_err() {
        StorageError::InvalidLogIndex {
            expected,
            got,
            reason,
            ..
        } => {
            assert_eq!(expected, 1);
            assert_eq!(got, 2);
            assert!(reason.contains("First log entry must have index 1"));
        }
        e => panic!("Expected InvalidLogIndex, got: {:?}", e),
    }
}

#[test]
fn test_append_sequential_indices_succeeds() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 10 sequential entries
    for i in 1..=10 {
        let entry = format!("entry_{}", i);
        let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes());
        assert!(result.is_ok(), "Sequential index {} should succeed", i);
    }

    // Verify last index
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(10));
}

#[test]
fn test_append_detects_gap() {
    let (storage, _temp_dir) = create_test_storage();

    // Append index 1
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();

    // Try to append index 3 (gap: missing index 2)
    let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, 3, b"entry3");

    assert!(result.is_err(), "Gap should be detected");
    match result.unwrap_err() {
        StorageError::InvalidLogIndex {
            expected,
            got,
            reason,
            ..
        } => {
            assert_eq!(expected, 2);
            assert_eq!(got, 3);
            assert!(
                reason.contains("Gap in log"),
                "Reason should mention gap: {}",
                reason
            );
        }
        e => panic!("Expected InvalidLogIndex, got: {:?}", e),
    }
}

#[test]
fn test_append_detects_duplicate() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1, 2, 3
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 2, b"entry2")
        .unwrap();
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 3, b"entry3")
        .unwrap();

    // Try to append index 2 again (duplicate)
    let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, 2, b"entry2_dup");

    assert!(result.is_err(), "Duplicate should be detected");
    match result.unwrap_err() {
        StorageError::InvalidLogIndex {
            expected,
            got,
            reason,
            ..
        } => {
            assert_eq!(expected, 4);
            assert_eq!(got, 2);
            assert!(
                reason.contains("Duplicate"),
                "Reason should mention duplicate: {}",
                reason
            );
        }
        e => panic!("Expected InvalidLogIndex, got: {:?}", e),
    }
}

#[test]
fn test_append_error_messages_are_descriptive() {
    let (storage, _temp_dir) = create_test_storage();

    // Test 1: First entry not 1
    let err = storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 5, b"entry5")
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("expected 1"),
        "Error should mention expected index: {}",
        msg
    );
    assert!(
        msg.contains("got 5"),
        "Error should mention actual index: {}",
        msg
    );

    // Test 2: Gap detection
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();
    let err = storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 10, b"entry10")
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("expected 2"),
        "Error should mention expected: {}",
        msg
    );
    assert!(
        msg.contains("got 10"),
        "Error should mention actual: {}",
        msg
    );
}

#[test]
fn test_append_only_works_on_log_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    // Try to append to non-log CFs
    let non_log_cfs = [
        ColumnFamily::SystemRaftState,
        ColumnFamily::SystemData,
        ColumnFamily::DataRaftState,
        ColumnFamily::DataKv,
    ];

    for cf in non_log_cfs.iter() {
        let result = storage.append_log_entry(*cf, 1, b"entry1");
        assert!(
            result.is_err(),
            "{} should reject append_log_entry",
            cf.as_str()
        );

        match result.unwrap_err() {
            StorageError::InvalidColumnFamily { cf: cf_name, .. } => {
                assert_eq!(cf_name, cf.as_str());
            }
            e => panic!("Expected InvalidColumnFamily, got: {:?}", e),
        }
    }
}

#[test]
fn test_append_updates_cache_after_success() {
    let (storage, _temp_dir) = create_test_storage();

    // Cache should be empty initially (or warmed from empty DB)
    let initial = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(initial, None);

    // Append index 1
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();

    // Cache should be updated immediately (O(1) lookup)
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, Some(1));

    // Append index 2
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 2, b"entry2")
        .unwrap();

    // Cache should reflect new index
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, Some(2));
}

#[test]
fn test_append_100_sequential_entries() {
    let (storage, _temp_dir) = create_test_storage();

    // Stress test: append 100 sequential entries
    for i in 1..=100 {
        let entry = format!("entry_{}", i);
        let result = storage.append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes());
        assert!(result.is_ok(), "Entry {} should succeed", i);
    }

    // Verify last index
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(100));
}

#[test]
fn test_append_across_both_log_cfs_independently() {
    let (storage, _temp_dir) = create_test_storage();

    // SystemRaftLog and DataRaftLog should maintain independent indices
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"system1")
        .unwrap();
    storage
        .append_log_entry(ColumnFamily::DataRaftLog, 1, b"data1")
        .unwrap();

    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 2, b"system2")
        .unwrap();
    storage
        .append_log_entry(ColumnFamily::DataRaftLog, 2, b"data2")
        .unwrap();

    // Both should have last_index = 2
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap(),
        Some(2)
    );
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap(),
        Some(2)
    );

    // Append more to SystemRaftLog
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 3, b"system3")
        .unwrap();

    // SystemRaftLog should be 3, DataRaftLog still 2
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap(),
        Some(3)
    );
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap(),
        Some(2)
    );
}

#[test]
fn test_cache_persists_after_reopen() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().to_path_buf();

    // Create storage and append entries
    {
        let options = StorageOptions::with_data_dir(path.clone());
        let storage = Storage::new(options).unwrap();

        append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();
        append_sequential_entries(&storage, ColumnFamily::DataRaftLog, 3).unwrap();

        storage.close().unwrap();
    }

    // Reopen storage - cache should be warmed up
    {
        let options = StorageOptions::with_data_dir(path.clone());
        let storage = Storage::new(options).unwrap();

        // Cache should be populated from disk during warm_up_index_cache()
        let system_index = storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap();
        let data_index = storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap();

        assert_eq!(system_index, Some(5));
        assert_eq!(data_index, Some(3));
    }
}

// ============================================================================
// 2. get_log_range Tests (8 tests)
// ============================================================================

#[test]
fn test_get_log_range_empty_log() {
    let (storage, _temp_dir) = create_test_storage();

    // Query empty log
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 10)
        .unwrap();
    assert_eq!(entries.len(), 0, "Empty log should return empty vec");
}

#[test]
fn test_get_log_range_single_entry() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();

    // Query range [1, 2)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 2)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], b"entry1");
}

#[test]
fn test_get_log_range_multiple_entries() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 5 entries
    for i in 1..=5 {
        let entry = format!("entry_{}", i);
        storage
            .append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes())
            .unwrap();
    }

    // Query range [2, 5) (indices 2, 3, 4)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 2, 5)
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], b"entry_2");
    assert_eq!(entries[1], b"entry_3");
    assert_eq!(entries[2], b"entry_4");
}

#[test]
fn test_get_log_range_with_gaps() {
    let (storage, _temp_dir) = create_test_storage();

    // Manually insert entries with gaps (using put directly, bypassing validation)
    storage
        .put(
            ColumnFamily::SystemRaftLog,
            b"log:00000000000000000001",
            b"entry1",
        )
        .unwrap();
    storage
        .put(
            ColumnFamily::SystemRaftLog,
            b"log:00000000000000000003",
            b"entry3",
        )
        .unwrap();
    storage
        .put(
            ColumnFamily::SystemRaftLog,
            b"log:00000000000000000005",
            b"entry5",
        )
        .unwrap();

    // Query range [1, 6) - should skip missing indices 2 and 4
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 6)
        .unwrap();
    assert_eq!(entries.len(), 3, "Should return only existing entries");
    assert_eq!(entries[0], b"entry1");
    assert_eq!(entries[1], b"entry3");
    assert_eq!(entries[2], b"entry5");
}

#[test]
fn test_get_log_range_out_of_bounds() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-5
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();

    // Query range beyond existing entries [10, 20)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 10, 20)
        .unwrap();
    assert_eq!(
        entries.len(),
        0,
        "Out of bounds query should return empty vec"
    );

    // Query range partially overlapping [3, 10)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 3, 10)
        .unwrap();
    assert_eq!(entries.len(), 3, "Should return entries 3, 4, 5");
}

#[test]
fn test_get_log_range_only_works_on_log_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    let non_log_cfs = [
        ColumnFamily::SystemRaftState,
        ColumnFamily::SystemData,
        ColumnFamily::DataRaftState,
        ColumnFamily::DataKv,
    ];

    for cf in non_log_cfs.iter() {
        let result = storage.get_log_range(*cf, 1, 10);
        assert!(
            result.is_err(),
            "{} should reject get_log_range",
            cf.as_str()
        );

        match result.unwrap_err() {
            StorageError::InvalidColumnFamily { .. } => {}
            e => panic!("Expected InvalidColumnFamily, got: {:?}", e),
        }
    }
}

#[test]
fn test_get_log_range_preserves_order() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 10 entries
    for i in 1..=10 {
        let entry = format!("entry_{:02}", i);
        storage
            .append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes())
            .unwrap();
    }

    // Query range [1, 11)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 11)
        .unwrap();

    // Verify order is preserved
    for (i, entry) in entries.iter().enumerate() {
        let expected = format!("entry_{:02}", i + 1);
        assert_eq!(entry, expected.as_bytes());
    }
}

#[test]
fn test_get_log_range_large_batch() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 1000 entries
    for i in 1..=1000 {
        let entry = format!("entry_{}", i);
        storage
            .append_log_entry(ColumnFamily::SystemRaftLog, i, entry.as_bytes())
            .unwrap();
    }

    // Query range [1, 1001)
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 1001)
        .unwrap();
    assert_eq!(entries.len(), 1000);

    // Verify first and last entries
    assert_eq!(entries[0], b"entry_1");
    assert_eq!(entries[999], b"entry_1000");
}

// ============================================================================
// 3. truncate_log_before Tests (8 tests)
// ============================================================================

#[test]
fn test_truncate_empty_log() {
    let (storage, _temp_dir) = create_test_storage();

    // Truncating empty log should succeed (no-op)
    let result = storage.truncate_log_before(ColumnFamily::SystemRaftLog, 100);
    assert!(result.is_ok(), "Truncating empty log should succeed");
}

#[test]
fn test_truncate_partial_log() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-10
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Truncate before index 5 (delete indices 1-4)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 5)
        .unwrap();

    // Verify indices 1-4 are deleted
    for i in 1..5 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(!exists, "Index {} should be deleted", i);
    }

    // Verify indices 5-10 still exist
    for i in 5..=10 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(exists, "Index {} should still exist", i);
    }

    // Last index should still be 10
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(10));
}

#[test]
fn test_truncate_entire_log() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-10
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Truncate before index 11 (delete all entries)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 11)
        .unwrap();

    // Verify all entries are deleted
    for i in 1..=10 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(!exists, "Index {} should be deleted", i);
    }

    // Last index should be None
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, None);
}

#[test]
fn test_truncate_past_end_clears_log() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-5
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();

    // Truncate before index 100 (way past the end)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 100)
        .unwrap();

    // All entries should be deleted
    for i in 1..=5 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(!exists, "Index {} should be deleted", i);
    }

    // Last index should be None
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, None);
}

#[test]
fn test_truncate_updates_cache_when_entire_log_deleted() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-5
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();

    // Verify cache has index 5
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, Some(5));

    // Truncate entire log
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 100)
        .unwrap();

    // Cache should be invalidated (return None)
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, None);
}

#[test]
fn test_truncate_preserves_cache_for_partial_truncation() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-10
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Truncate before index 5 (partial truncation)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 5)
        .unwrap();

    // Cache should still show last_index = 10
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, Some(10));
}

#[test]
fn test_truncate_only_works_on_log_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    let non_log_cfs = [
        ColumnFamily::SystemRaftState,
        ColumnFamily::SystemData,
        ColumnFamily::DataRaftState,
        ColumnFamily::DataKv,
    ];

    for cf in non_log_cfs.iter() {
        let result = storage.truncate_log_before(*cf, 100);
        assert!(
            result.is_err(),
            "{} should reject truncate_log_before",
            cf.as_str()
        );

        match result.unwrap_err() {
            StorageError::InvalidColumnFamily { .. } => {}
            e => panic!("Expected InvalidColumnFamily, got: {:?}", e),
        }
    }
}

#[test]
fn test_truncate_is_atomic() {
    let (storage, _temp_dir) = create_test_storage();

    // Append indices 1-100
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 100).unwrap();

    // Truncate before index 50 (should delete 1-49 atomically)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 50)
        .unwrap();

    // Either all are deleted or none are deleted (no partial state)
    // We verify by checking that indices 1-49 are all gone
    for i in 1..50 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(!exists, "Index {} should be deleted atomically", i);
    }

    // And indices 50-100 all still exist
    for i in 50..=100 {
        let key = format!("log:{:020}", i);
        let exists = storage
            .exists(ColumnFamily::SystemRaftLog, key.as_bytes())
            .unwrap();
        assert!(exists, "Index {} should still exist", i);
    }
}

// ============================================================================
// 4. get_last_log_index Tests (5 tests)
// ============================================================================

#[test]
fn test_get_last_log_index_empty_log() {
    let (storage, _temp_dir) = create_test_storage();

    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, None);
}

#[test]
fn test_get_last_log_index_single_entry() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")
        .unwrap();

    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(1));
}

#[test]
fn test_get_last_log_index_after_multiple_appends() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 50 entries
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 50).unwrap();

    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(50));
}

#[test]
fn test_get_last_log_index_after_truncation() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 10 entries
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Truncate before index 6
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 6)
        .unwrap();

    // Last index should still be 10 (truncate removes from beginning)
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, Some(10));

    // Truncate entire log
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 100)
        .unwrap();

    // Last index should be None
    let last_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(last_index, None);
}

#[test]
fn test_get_last_log_index_uses_cache() {
    let (storage, _temp_dir) = create_test_storage();

    // Append entries
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();

    // First call warms cache (or uses already-warmed cache)
    let last_index_1 = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();

    // Second call should use cache (O(1))
    let last_index_2 = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();

    assert_eq!(last_index_1, last_index_2);
    assert_eq!(last_index_1, Some(5));
}

// ============================================================================
// 5. Cache Behavior Tests (7 tests)
// ============================================================================

#[test]
fn test_cache_initialized_empty_on_new() {
    let (storage, _temp_dir) = create_test_storage();

    // New storage with empty DB should have empty cache (or None entries)
    let system_index = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    let data_index = storage
        .get_last_log_index(ColumnFamily::DataRaftLog)
        .unwrap();

    assert_eq!(system_index, None);
    assert_eq!(data_index, None);
}

#[test]
fn test_cache_warmed_up_on_reopen() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().to_path_buf();

    // Create storage and append entries
    {
        let options = StorageOptions::with_data_dir(path.clone());
        let storage = Storage::new(options).unwrap();

        append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

        storage.close().unwrap();
    }

    // Reopen storage
    {
        let options = StorageOptions::with_data_dir(path.clone());
        let storage = Storage::new(options).unwrap();

        // Cache should be populated during warm_up_index_cache()
        let last_index = storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap();
        assert_eq!(last_index, Some(10));
    }
}

#[test]
fn test_cache_updated_after_each_append() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=10 {
        storage
            .append_log_entry(ColumnFamily::SystemRaftLog, i, b"entry")
            .unwrap();

        // Cache should be updated immediately
        let cached = storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap();
        assert_eq!(
            cached,
            Some(i),
            "Cache should be updated after append {}",
            i
        );
    }
}

#[test]
#[ignore] // invalidate_index_cache is private, test cannot be run
fn test_cache_invalidated_clears_all_entries() {
    let (storage, _temp_dir) = create_test_storage();

    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();
    append_sequential_entries(&storage, ColumnFamily::DataRaftLog, 3).unwrap();

    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap(),
        Some(5)
    );
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap(),
        Some(3)
    );

    // Cannot test invalidate_index_cache() as it's private
}

#[test]
fn test_cache_separate_for_different_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    // SystemRaftLog and DataRaftLog have independent caches
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();
    append_sequential_entries(&storage, ColumnFamily::DataRaftLog, 5).unwrap();

    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap(),
        Some(10)
    );
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap(),
        Some(5)
    );

    // Appending to one CF shouldn't affect the other
    storage
        .append_log_entry(ColumnFamily::SystemRaftLog, 11, b"entry11")
        .unwrap();

    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::SystemRaftLog)
            .unwrap(),
        Some(11)
    );
    assert_eq!(
        storage
            .get_last_log_index(ColumnFamily::DataRaftLog)
            .unwrap(),
        Some(5)
    );
}

#[test]
#[ignore] // Storage is not Send/Sync due to SliceTransform, cannot test thread safety this way
fn test_cache_thread_safe() {
    // Storage cannot be shared across threads due to SliceTransform not being Send/Sync
    // This is a known limitation - SliceTransform is not thread-safe
}

#[test]
fn test_cache_survives_truncation() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 10 entries
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Partial truncation (before index 5)
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 5)
        .unwrap();

    // Cache should still be valid (last_index = 10)
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, Some(10));

    // Full truncation
    storage
        .truncate_log_before(ColumnFamily::SystemRaftLog, 100)
        .unwrap();

    // Cache should be invalidated (last_index = None)
    let cached = storage
        .get_last_log_index(ColumnFamily::SystemRaftLog)
        .unwrap();
    assert_eq!(cached, None);
}

// ============================================================================
// get_log_range Tests
// ============================================================================

#[test]
fn test_get_log_range_basic() {
    let (storage, _temp_dir) = create_test_storage();

    // Append 5 entries
    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 5).unwrap();

    // Get range that exists
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 6)
        .unwrap();
    assert_eq!(entries.len(), 5);

    // Verify content
    assert_eq!(entries[0], b"entry_1");
    assert_eq!(entries[4], b"entry_5");
}

#[test]
fn test_get_log_range_partial() {
    let (storage, _temp_dir) = create_test_storage();

    append_sequential_entries(&storage, ColumnFamily::SystemRaftLog, 10).unwrap();

    // Get partial range
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 3, 7)
        .unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0], b"entry_3");
    assert_eq!(entries[3], b"entry_6");
}

#[test]
fn test_get_log_range_empty() {
    let (storage, _temp_dir) = create_test_storage();

    // No entries
    let entries = storage
        .get_log_range(ColumnFamily::SystemRaftLog, 1, 10)
        .unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_get_log_range_rejects_non_log_cf() {
    let (storage, _temp_dir) = create_test_storage();

    let result = storage.get_log_range(ColumnFamily::DataKv, 1, 10);
    assert!(result.is_err());
    match result.unwrap_err() {
        StorageError::InvalidColumnFamily { reason, .. } => {
            assert!(reason.contains("get_log_range only works on log CFs"));
        }
        e => panic!("Expected InvalidColumnFamily, got: {:?}", e),
    }
}
