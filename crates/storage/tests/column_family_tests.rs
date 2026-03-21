//! Tests for ColumnFamily enum and helper methods.

use seshat_storage::ColumnFamily;
use std::collections::HashSet;

// ============================================================================
// String Conversion Tests
// ============================================================================

#[test]
fn test_as_str_returns_snake_case() {
    // Verify each variant returns correct snake_case string
    assert_eq!(ColumnFamily::SystemRaftLog.as_str(), "system_raft_log");
    assert_eq!(ColumnFamily::SystemRaftState.as_str(), "system_raft_state");
    assert_eq!(ColumnFamily::SystemData.as_str(), "system_data");
    assert_eq!(ColumnFamily::DataRaftLog.as_str(), "data_raft_log");
    assert_eq!(ColumnFamily::DataRaftState.as_str(), "data_raft_state");
    assert_eq!(ColumnFamily::DataKv.as_str(), "data_kv");
}

#[test]
fn test_as_str_strings_are_valid_rocksdb_names() {
    // RocksDB column family names should:
    // 1. Be non-empty
    // 2. Not contain null bytes
    // 3. Use lowercase and underscores (convention)

    for cf in ColumnFamily::all() {
        let name = cf.as_str();

        // Non-empty
        assert!(!name.is_empty(), "CF name should not be empty");

        // No null bytes
        assert!(!name.contains('\0'), "CF name should not contain null bytes");

        // Lowercase and underscores only (snake_case convention)
        assert!(
            name.chars().all(|c| c.is_lowercase() || c == '_'),
            "CF name '{}' should be snake_case (lowercase + underscores only)",
            name
        );
    }
}

// ============================================================================
// all() Method Tests
// ============================================================================

#[test]
fn test_all_returns_six_variants() {
    let all_cfs = ColumnFamily::all();
    assert_eq!(all_cfs.len(), 6, "Should have exactly 6 column families");
}

#[test]
fn test_all_contains_all_variants() {
    let all_cfs = ColumnFamily::all();

    // Verify each variant exists in the array
    assert!(all_cfs.contains(&ColumnFamily::SystemRaftLog));
    assert!(all_cfs.contains(&ColumnFamily::SystemRaftState));
    assert!(all_cfs.contains(&ColumnFamily::SystemData));
    assert!(all_cfs.contains(&ColumnFamily::DataRaftLog));
    assert!(all_cfs.contains(&ColumnFamily::DataRaftState));
    assert!(all_cfs.contains(&ColumnFamily::DataKv));
}

#[test]
fn test_all_variants_are_unique() {
    let all_cfs = ColumnFamily::all();
    let unique_cfs: HashSet<_> = all_cfs.iter().collect();

    assert_eq!(
        unique_cfs.len(),
        all_cfs.len(),
        "All column families should be unique (no duplicates)"
    );
}

#[test]
fn test_all_returns_consistent_order() {
    // Verify that calling all() multiple times returns the same order
    let first = ColumnFamily::all();
    let second = ColumnFamily::all();

    for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        assert_eq!(
            a, b,
            "all() should return consistent order at index {}",
            i
        );
    }
}

// ============================================================================
// requires_fsync() Tests
// ============================================================================

#[test]
fn test_requires_fsync_only_for_state_cfs() {
    // Only *_raft_state CFs should require fsync
    assert!(
        ColumnFamily::SystemRaftState.requires_fsync(),
        "SystemRaftState should require fsync"
    );
    assert!(
        ColumnFamily::DataRaftState.requires_fsync(),
        "DataRaftState should require fsync"
    );

    // All other CFs should NOT require fsync
    assert!(
        !ColumnFamily::SystemRaftLog.requires_fsync(),
        "SystemRaftLog should NOT require fsync"
    );
    assert!(
        !ColumnFamily::SystemData.requires_fsync(),
        "SystemData should NOT require fsync"
    );
    assert!(
        !ColumnFamily::DataRaftLog.requires_fsync(),
        "DataRaftLog should NOT require fsync"
    );
    assert!(
        !ColumnFamily::DataKv.requires_fsync(),
        "DataKv should NOT require fsync"
    );
}

#[test]
fn test_requires_fsync_exactly_two_cfs() {
    let fsync_count = ColumnFamily::all()
        .iter()
        .filter(|cf| cf.requires_fsync())
        .count();

    assert_eq!(
        fsync_count, 2,
        "Exactly 2 CFs should require fsync (SystemRaftState and DataRaftState)"
    );
}

// ============================================================================
// is_log_cf() Tests
// ============================================================================

#[test]
fn test_is_log_cf_only_for_log_cfs() {
    // Only *_raft_log CFs should be log CFs
    assert!(
        ColumnFamily::SystemRaftLog.is_log_cf(),
        "SystemRaftLog should be a log CF"
    );
    assert!(
        ColumnFamily::DataRaftLog.is_log_cf(),
        "DataRaftLog should be a log CF"
    );

    // All other CFs should NOT be log CFs
    assert!(
        !ColumnFamily::SystemRaftState.is_log_cf(),
        "SystemRaftState should NOT be a log CF"
    );
    assert!(
        !ColumnFamily::SystemData.is_log_cf(),
        "SystemData should NOT be a log CF"
    );
    assert!(
        !ColumnFamily::DataRaftState.is_log_cf(),
        "DataRaftState should NOT be a log CF"
    );
    assert!(
        !ColumnFamily::DataKv.is_log_cf(),
        "DataKv should NOT be a log CF"
    );
}

#[test]
fn test_is_log_cf_exactly_two_cfs() {
    let log_cf_count = ColumnFamily::all()
        .iter()
        .filter(|cf| cf.is_log_cf())
        .count();

    assert_eq!(
        log_cf_count, 2,
        "Exactly 2 CFs should be log CFs (SystemRaftLog and DataRaftLog)"
    );
}

// ============================================================================
// Derive Trait Tests
// ============================================================================

#[test]
fn test_debug_trait() {
    // Debug should produce readable output
    let cf = ColumnFamily::SystemRaftLog;
    let debug_str = format!("{:?}", cf);

    assert!(
        debug_str.contains("SystemRaftLog"),
        "Debug output should contain variant name"
    );
}

#[test]
fn test_clone_trait() {
    let original = ColumnFamily::DataKv;
    let cloned = original.clone();

    assert_eq!(original, cloned, "Cloned value should equal original");
}

#[test]
fn test_copy_trait() {
    let original = ColumnFamily::SystemData;
    let copied = original; // Copy happens here

    // Both should be usable (Copy allows this)
    assert_eq!(original.as_str(), "system_data");
    assert_eq!(copied.as_str(), "system_data");
}

#[test]
fn test_partialeq_trait() {
    // Same variants should be equal
    assert_eq!(ColumnFamily::SystemRaftLog, ColumnFamily::SystemRaftLog);

    // Different variants should not be equal
    assert_ne!(ColumnFamily::SystemRaftLog, ColumnFamily::DataRaftLog);
}

#[test]
fn test_eq_trait() {
    // Eq requires reflexivity, symmetry, and transitivity
    // These are guaranteed by derive(Eq), but let's verify reflexivity
    let cf = ColumnFamily::DataKv;
    assert_eq!(cf, cf, "A value should equal itself (reflexivity)");
}

#[test]
fn test_hash_trait() {
    use std::collections::HashMap;

    // Hash should work in HashMap
    let mut map = HashMap::new();
    map.insert(ColumnFamily::SystemRaftLog, "log_data");
    map.insert(ColumnFamily::DataKv, "kv_data");

    assert_eq!(
        map.get(&ColumnFamily::SystemRaftLog),
        Some(&"log_data")
    );
    assert_eq!(
        map.get(&ColumnFamily::DataKv),
        Some(&"kv_data")
    );
}

#[test]
fn test_hash_consistency() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Same value should hash to same result
    let cf = ColumnFamily::SystemRaftState;

    let mut hasher1 = DefaultHasher::new();
    cf.hash(&mut hasher1);
    let hash1 = hasher1.finish();

    let mut hasher2 = DefaultHasher::new();
    cf.hash(&mut hasher2);
    let hash2 = hasher2.finish();

    assert_eq!(hash1, hash2, "Same value should produce same hash");
}

// ============================================================================
// Cross-Method Consistency Tests
// ============================================================================

#[test]
fn test_log_cfs_never_require_fsync() {
    // Log CFs should never require fsync (they use async WAL)
    for cf in ColumnFamily::all() {
        if cf.is_log_cf() {
            assert!(
                !cf.requires_fsync(),
                "Log CF {:?} should not require fsync",
                cf
            );
        }
    }
}

#[test]
fn test_state_cfs_always_require_fsync() {
    // State CFs should always require fsync (for durability)
    for cf in ColumnFamily::all() {
        let is_state_cf = cf.as_str().contains("_state");
        if is_state_cf {
            assert!(
                cf.requires_fsync(),
                "State CF {:?} should require fsync",
                cf
            );
        }
    }
}

#[test]
fn test_no_cf_is_both_log_and_state() {
    // A CF cannot be both a log CF and require fsync
    // (log CFs use async WAL, state CFs use fsync)
    for cf in ColumnFamily::all() {
        if cf.is_log_cf() {
            assert!(
                !cf.requires_fsync(),
                "CF {:?} cannot be both log and require fsync",
                cf
            );
        }
    }
}

// ============================================================================
// String Roundtrip Tests
// ============================================================================

#[test]
fn test_string_conversion_is_unique() {
    // Each CF should have a unique string representation
    let strings: HashSet<_> = ColumnFamily::all()
        .iter()
        .map(|cf| cf.as_str())
        .collect();

    assert_eq!(
        strings.len(),
        6,
        "All 6 CFs should have unique string representations"
    );
}

#[test]
fn test_all_strings_are_different() {
    // Explicitly verify no two CFs have the same string
    let all_cfs = ColumnFamily::all();

    for (i, cf1) in all_cfs.iter().enumerate() {
        for (j, cf2) in all_cfs.iter().enumerate() {
            if i != j {
                assert_ne!(
                    cf1.as_str(),
                    cf2.as_str(),
                    "CFs {:?} and {:?} should have different strings",
                    cf1,
                    cf2
                );
            }
        }
    }
}

// ============================================================================
// System vs Data Group Tests
// ============================================================================

#[test]
fn test_system_group_cfs() {
    // System group CFs should all start with "system_"
    let system_cfs = vec![
        ColumnFamily::SystemRaftLog,
        ColumnFamily::SystemRaftState,
        ColumnFamily::SystemData,
    ];

    for cf in system_cfs {
        assert!(
            cf.as_str().starts_with("system_"),
            "System CF {:?} should start with 'system_'",
            cf
        );
    }
}

#[test]
fn test_data_group_cfs() {
    // Data group CFs should all start with "data_"
    let data_cfs = vec![
        ColumnFamily::DataRaftLog,
        ColumnFamily::DataRaftState,
        ColumnFamily::DataKv,
    ];

    for cf in data_cfs {
        assert!(
            cf.as_str().starts_with("data_"),
            "Data CF {:?} should start with 'data_'",
            cf
        );
    }
}

#[test]
fn test_exactly_three_system_and_three_data_cfs() {
    let all_cfs = ColumnFamily::all();

    let system_count = all_cfs
        .iter()
        .filter(|cf| cf.as_str().starts_with("system_"))
        .count();

    let data_count = all_cfs
        .iter()
        .filter(|cf| cf.as_str().starts_with("data_"))
        .count();

    assert_eq!(system_count, 3, "Should have exactly 3 system CFs");
    assert_eq!(data_count, 3, "Should have exactly 3 data CFs");
}
