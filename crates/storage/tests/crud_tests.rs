//! Integration tests for basic CRUD operations (ROCKS-005).
//!
//! Tests all four basic operations (get, put, delete, exists) across all
//! column families with various edge cases.

use seshat_storage::{ColumnFamily, Storage, StorageOptions};
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

// ============================================================================
// GET Operation Tests
// ============================================================================

#[test]
fn test_get_returns_none_for_missing_key() {
    let (storage, _temp_dir) = create_test_storage();

    // Test across all column families
    for cf in ColumnFamily::all() {
        let result = storage.get(cf, b"nonexistent");
        assert!(result.is_ok(), "get() should not error on missing key");
        assert_eq!(
            result.unwrap(),
            None,
            "get() should return None for missing key in {}",
            cf.as_str()
        );
    }
}

#[test]
fn test_get_returns_value_for_existing_key() {
    let (storage, _temp_dir) = create_test_storage();

    // Test across all column families
    for cf in ColumnFamily::all() {
        // First put a value
        storage
            .put(cf, b"testkey", b"testvalue")
            .expect("put() should succeed");

        // Then get it back
        let result = storage.get(cf, b"testkey").expect("get() should succeed");
        assert_eq!(
            result,
            Some(b"testvalue".to_vec()),
            "get() should return the stored value in {}",
            cf.as_str()
        );
    }
}

#[test]
fn test_get_works_across_all_column_families() {
    let (storage, _temp_dir) = create_test_storage();

    // Put different values in each CF
    for (i, cf) in ColumnFamily::all().iter().enumerate() {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        storage
            .put(*cf, key.as_bytes(), value.as_bytes())
            .expect("put() should succeed");
    }

    // Verify each CF has only its own data
    for (i, cf) in ColumnFamily::all().iter().enumerate() {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);

        let result = storage
            .get(*cf, key.as_bytes())
            .expect("get() should succeed");
        assert_eq!(
            result,
            Some(value.as_bytes().to_vec()),
            "CF {} should have its own value",
            cf.as_str()
        );

        // Check that other CFs' keys don't exist here
        for (j, other_cf) in ColumnFamily::all().iter().enumerate() {
            if i != j {
                let other_key = format!("key_{}", j);
                let result = storage
                    .get(*cf, other_key.as_bytes())
                    .expect("get() should succeed");
                assert_eq!(
                    result,
                    None,
                    "CF {} should not have key from CF {}",
                    cf.as_str(),
                    other_cf.as_str()
                );
            }
        }
    }
}

#[test]
fn test_get_handles_empty_value() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .put(ColumnFamily::DataKv, b"emptykey", b"")
        .expect("put() with empty value should succeed");

    let result = storage
        .get(ColumnFamily::DataKv, b"emptykey")
        .expect("get() should succeed");
    assert_eq!(
        result,
        Some(vec![]),
        "get() should return empty vec for empty value"
    );
}

#[test]
fn test_get_handles_large_value() {
    let (storage, _temp_dir) = create_test_storage();

    // Create a 10MB value
    let large_value = vec![0xAB; 10 * 1024 * 1024];

    storage
        .put(ColumnFamily::DataKv, b"largekey", &large_value)
        .expect("put() with large value should succeed");

    let result = storage
        .get(ColumnFamily::DataKv, b"largekey")
        .expect("get() should succeed");
    assert_eq!(
        result,
        Some(large_value),
        "get() should return full large value"
    );
}

// ============================================================================
// PUT Operation Tests
// ============================================================================

#[test]
fn test_put_stores_value_successfully() {
    let (storage, _temp_dir) = create_test_storage();

    let result = storage.put(ColumnFamily::DataKv, b"key1", b"value1");
    assert!(result.is_ok(), "put() should succeed");

    // Verify the value was stored
    let get_result = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(get_result, Some(b"value1".to_vec()));
}

#[test]
fn test_put_overwrites_existing_value() {
    let (storage, _temp_dir) = create_test_storage();

    // Put initial value
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("first put() should succeed");

    // Overwrite with new value
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value2")
        .expect("second put() should succeed");

    // Verify new value
    let result = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(
        result,
        Some(b"value2".to_vec()),
        "put() should overwrite existing value"
    );
}

#[test]
fn test_put_works_across_all_column_families() {
    let (storage, _temp_dir) = create_test_storage();

    // Put to each CF and verify
    for cf in ColumnFamily::all() {
        storage
            .put(cf, b"testkey", b"testvalue")
            .expect("put() should succeed");

        let result = storage.get(cf, b"testkey").expect("get() should succeed");
        assert_eq!(
            result,
            Some(b"testvalue".to_vec()),
            "put() should work in {}",
            cf.as_str()
        );
    }
}

#[test]
fn test_put_handles_empty_value() {
    let (storage, _temp_dir) = create_test_storage();

    let result = storage.put(ColumnFamily::DataKv, b"emptykey", b"");
    assert!(result.is_ok(), "put() with empty value should succeed");

    let get_result = storage
        .get(ColumnFamily::DataKv, b"emptykey")
        .expect("get() should succeed");
    assert_eq!(get_result, Some(vec![]), "empty value should be stored");
}

#[test]
fn test_put_handles_large_value() {
    let (storage, _temp_dir) = create_test_storage();

    // Create a 10MB value
    let large_value = vec![0xCD; 10 * 1024 * 1024];

    let result = storage.put(ColumnFamily::DataKv, b"largekey", &large_value);
    assert!(result.is_ok(), "put() with large value should succeed");

    let get_result = storage
        .get(ColumnFamily::DataKv, b"largekey")
        .expect("get() should succeed");
    assert_eq!(
        get_result,
        Some(large_value),
        "large value should be stored correctly"
    );
}

#[test]
fn test_put_fsyncs_for_raft_state_cfs() {
    let (storage, _temp_dir) = create_test_storage();

    // These CFs require fsync - just verify they succeed
    // (We can't directly test fsync behavior in unit tests)
    for cf in [ColumnFamily::SystemRaftState, ColumnFamily::DataRaftState] {
        let result = storage.put(cf, b"state_key", b"state_value");
        assert!(
            result.is_ok(),
            "put() should succeed with fsync for {}",
            cf.as_str()
        );

        // Verify data was written
        let get_result = storage.get(cf, b"state_key").expect("get() should succeed");
        assert_eq!(
            get_result,
            Some(b"state_value".to_vec()),
            "fsync write should persist data in {}",
            cf.as_str()
        );
    }
}

// ============================================================================
// DELETE Operation Tests
// ============================================================================

#[test]
fn test_delete_removes_existing_key() {
    let (storage, _temp_dir) = create_test_storage();

    // Put a value
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("put() should succeed");

    // Verify it exists
    let before = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(before, Some(b"value1".to_vec()));

    // Delete it
    storage
        .delete(ColumnFamily::DataKv, b"key1")
        .expect("delete() should succeed");

    // Verify it's gone
    let after = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(after, None, "delete() should remove the key");
}

#[test]
fn test_delete_is_idempotent() {
    let (storage, _temp_dir) = create_test_storage();

    // Delete non-existent key should succeed
    let result1 = storage.delete(ColumnFamily::DataKv, b"nonexistent");
    assert!(result1.is_ok(), "delete() of missing key should succeed");

    // Delete again should also succeed
    let result2 = storage.delete(ColumnFamily::DataKv, b"nonexistent");
    assert!(
        result2.is_ok(),
        "delete() should be idempotent (second delete succeeds)"
    );
}

#[test]
fn test_delete_works_across_all_column_families() {
    let (storage, _temp_dir) = create_test_storage();

    for cf in ColumnFamily::all() {
        // Put a value
        storage
            .put(cf, b"testkey", b"testvalue")
            .expect("put() should succeed");

        // Delete it
        storage
            .delete(cf, b"testkey")
            .expect("delete() should succeed");

        // Verify it's gone
        let result = storage.get(cf, b"testkey").expect("get() should succeed");
        assert_eq!(result, None, "delete() should work in {}", cf.as_str());
    }
}

// ============================================================================
// EXISTS Operation Tests
// ============================================================================

#[test]
fn test_exists_returns_false_for_missing_key() {
    let (storage, _temp_dir) = create_test_storage();

    for cf in ColumnFamily::all() {
        let result = storage
            .exists(cf, b"nonexistent")
            .expect("exists() should succeed");
        assert!(
            !result,
            "exists() should return false for missing key in {}",
            cf.as_str()
        );
    }
}

#[test]
fn test_exists_returns_true_for_existing_key() {
    let (storage, _temp_dir) = create_test_storage();

    for cf in ColumnFamily::all() {
        // Put a value
        storage
            .put(cf, b"testkey", b"testvalue")
            .expect("put() should succeed");

        // Check existence
        let result = storage
            .exists(cf, b"testkey")
            .expect("exists() should succeed");
        assert!(
            result,
            "exists() should return true for existing key in {}",
            cf.as_str()
        );
    }
}

#[test]
fn test_exists_works_across_all_column_families() {
    let (storage, _temp_dir) = create_test_storage();

    // Put different keys in each CF
    for (i, cf) in ColumnFamily::all().iter().enumerate() {
        let key = format!("key_{}", i);
        storage
            .put(*cf, key.as_bytes(), b"value")
            .expect("put() should succeed");
    }

    // Verify exists() works for each CF
    for (i, cf) in ColumnFamily::all().iter().enumerate() {
        let key = format!("key_{}", i);
        let result = storage
            .exists(*cf, key.as_bytes())
            .expect("exists() should succeed");
        assert!(
            result,
            "exists() should return true for key in {}",
            cf.as_str()
        );

        // Check that other CFs' keys don't exist here
        for (j, other_cf) in ColumnFamily::all().iter().enumerate() {
            if i != j {
                let other_key = format!("key_{}", j);
                let result = storage
                    .exists(*cf, other_key.as_bytes())
                    .expect("exists() should succeed");
                assert!(
                    !result,
                    "exists() should return false for key from {} in {}",
                    other_cf.as_str(),
                    cf.as_str()
                );
            }
        }
    }
}

// ============================================================================
// Cross-Operation Tests
// ============================================================================

#[test]
fn test_put_then_get_roundtrip() {
    let (storage, _temp_dir) = create_test_storage();

    let test_cases = vec![
        (b"key1" as &[u8], b"value1" as &[u8]),
        (b"key2", b"value2"),
        (b"", b"empty_key_value"),
        (b"empty_value_key", b""),
        (b"binary_key", &[0x00, 0xFF, 0xAB, 0xCD]),
    ];

    for (key, value) in test_cases {
        storage
            .put(ColumnFamily::DataKv, key, value)
            .expect("put() should succeed");

        let result = storage
            .get(ColumnFamily::DataKv, key)
            .expect("get() should succeed");
        assert_eq!(
            result,
            Some(value.to_vec()),
            "roundtrip should preserve value for key {:?}",
            key
        );
    }
}

#[test]
fn test_put_then_delete_then_get() {
    let (storage, _temp_dir) = create_test_storage();

    // Put
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("put() should succeed");

    // Verify it's there
    let before = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(before, Some(b"value1".to_vec()));

    // Delete
    storage
        .delete(ColumnFamily::DataKv, b"key1")
        .expect("delete() should succeed");

    // Verify it's gone
    let after = storage
        .get(ColumnFamily::DataKv, b"key1")
        .expect("get() should succeed");
    assert_eq!(after, None);
}

#[test]
fn test_put_then_exists() {
    let (storage, _temp_dir) = create_test_storage();

    // Key doesn't exist yet
    let before = storage
        .exists(ColumnFamily::DataKv, b"key1")
        .expect("exists() should succeed");
    assert!(!before);

    // Put value
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("put() should succeed");

    // Key now exists
    let after = storage
        .exists(ColumnFamily::DataKv, b"key1")
        .expect("exists() should succeed");
    assert!(after);
}

#[test]
fn test_delete_then_exists() {
    let (storage, _temp_dir) = create_test_storage();

    // Put value
    storage
        .put(ColumnFamily::DataKv, b"key1", b"value1")
        .expect("put() should succeed");

    // Verify it exists
    let before = storage
        .exists(ColumnFamily::DataKv, b"key1")
        .expect("exists() should succeed");
    assert!(before);

    // Delete
    storage
        .delete(ColumnFamily::DataKv, b"key1")
        .expect("delete() should succeed");

    // Verify it no longer exists
    let after = storage
        .exists(ColumnFamily::DataKv, b"key1")
        .expect("exists() should succeed");
    assert!(!after);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_key_handling() {
    let (storage, _temp_dir) = create_test_storage();

    // Put with empty key
    storage
        .put(ColumnFamily::DataKv, b"", b"empty_key_value")
        .expect("put() with empty key should succeed");

    // Get with empty key
    let result = storage
        .get(ColumnFamily::DataKv, b"")
        .expect("get() with empty key should succeed");
    assert_eq!(result, Some(b"empty_key_value".to_vec()));

    // Exists with empty key
    let exists = storage
        .exists(ColumnFamily::DataKv, b"")
        .expect("exists() with empty key should succeed");
    assert!(exists);

    // Delete with empty key
    storage
        .delete(ColumnFamily::DataKv, b"")
        .expect("delete() with empty key should succeed");

    // Verify deletion
    let after = storage
        .get(ColumnFamily::DataKv, b"")
        .expect("get() should succeed");
    assert_eq!(after, None);
}

#[test]
fn test_empty_value_handling() {
    let (storage, _temp_dir) = create_test_storage();

    // Put empty value
    storage
        .put(ColumnFamily::DataKv, b"key", b"")
        .expect("put() with empty value should succeed");

    // Get empty value
    let result = storage
        .get(ColumnFamily::DataKv, b"key")
        .expect("get() should succeed");
    assert_eq!(result, Some(vec![]), "should return empty vec");

    // Exists should return true for empty value
    let exists = storage
        .exists(ColumnFamily::DataKv, b"key")
        .expect("exists() should succeed");
    assert!(exists, "key with empty value should exist");
}

#[test]
fn test_large_key_handling() {
    let (storage, _temp_dir) = create_test_storage();

    // Create a 1KB key
    let large_key = vec![0xAB; 1024];

    storage
        .put(ColumnFamily::DataKv, &large_key, b"large_key_value")
        .expect("put() with large key should succeed");

    let result = storage
        .get(ColumnFamily::DataKv, &large_key)
        .expect("get() with large key should succeed");
    assert_eq!(result, Some(b"large_key_value".to_vec()));
}

#[test]
fn test_large_value_handling() {
    let (storage, _temp_dir) = create_test_storage();

    // Create a 10MB value
    let large_value = vec![0xEF; 10 * 1024 * 1024];

    storage
        .put(ColumnFamily::DataKv, b"key", &large_value)
        .expect("put() with 10MB value should succeed");

    let result = storage
        .get(ColumnFamily::DataKv, b"key")
        .expect("get() should succeed");
    assert_eq!(
        result,
        Some(large_value.clone()),
        "10MB value should be stored and retrieved correctly"
    );

    // Verify exists
    let exists = storage
        .exists(ColumnFamily::DataKv, b"key")
        .expect("exists() should succeed");
    assert!(exists);

    // Delete and verify
    storage
        .delete(ColumnFamily::DataKv, b"key")
        .expect("delete() should succeed");

    let after = storage
        .get(ColumnFamily::DataKv, b"key")
        .expect("get() should succeed");
    assert_eq!(after, None);
}

#[test]
#[ignore] // Storage is not Send/Sync due to SliceTransform
fn test_concurrent_operations() {
    // Storage cannot be shared across threads due to SliceTransform not being Send/Sync
}

#[test]
fn test_binary_data_handling() {
    let (storage, _temp_dir) = create_test_storage();

    // Test with binary data (not UTF-8)
    let binary_key = vec![0x00, 0xFF, 0xAB, 0xCD, 0xEF];
    let binary_value = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF];

    storage
        .put(ColumnFamily::DataKv, &binary_key, &binary_value)
        .expect("put() with binary data should succeed");

    let result = storage
        .get(ColumnFamily::DataKv, &binary_key)
        .expect("get() should succeed");
    assert_eq!(
        result,
        Some(binary_value.clone()),
        "binary data should roundtrip correctly"
    );

    let exists = storage
        .exists(ColumnFamily::DataKv, &binary_key)
        .expect("exists() should succeed");
    assert!(exists);

    storage
        .delete(ColumnFamily::DataKv, &binary_key)
        .expect("delete() should succeed");

    let after = storage
        .get(ColumnFamily::DataKv, &binary_key)
        .expect("get() should succeed");
    assert_eq!(after, None);
}

#[test]
fn test_multiple_keys_in_same_cf() {
    let (storage, _temp_dir) = create_test_storage();

    // Put 1000 keys in the same CF
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let value = format!("value_{:04}", i);
        storage
            .put(ColumnFamily::DataKv, key.as_bytes(), value.as_bytes())
            .expect("put() should succeed");
    }

    // Verify all keys exist
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let value = format!("value_{:04}", i);

        let result = storage
            .get(ColumnFamily::DataKv, key.as_bytes())
            .expect("get() should succeed");
        assert_eq!(result, Some(value.as_bytes().to_vec()));

        let exists = storage
            .exists(ColumnFamily::DataKv, key.as_bytes())
            .expect("exists() should succeed");
        assert!(exists);
    }

    // Delete every other key
    for i in (0..1000).step_by(2) {
        let key = format!("key_{:04}", i);
        storage
            .delete(ColumnFamily::DataKv, key.as_bytes())
            .expect("delete() should succeed");
    }

    // Verify deletions
    for i in 0..1000 {
        let key = format!("key_{:04}", i);
        let result = storage
            .get(ColumnFamily::DataKv, key.as_bytes())
            .expect("get() should succeed");

        if i % 2 == 0 {
            assert_eq!(result, None, "even keys should be deleted");
        } else {
            let value = format!("value_{:04}", i);
            assert_eq!(
                result,
                Some(value.as_bytes().to_vec()),
                "odd keys should exist"
            );
        }
    }
}
