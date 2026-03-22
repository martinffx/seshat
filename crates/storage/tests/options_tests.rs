//! Tests for StorageOptions and CFOptions configuration structures.

use rocksdb::DBCompactionStyle;
use seshat_storage::{CFOptions, ColumnFamily, StorageError, StorageOptions};
use std::path::PathBuf;

// ============================================================================
// Default Value Tests
// ============================================================================

#[test]
fn test_default_storage_options() {
    let opts = StorageOptions::default();

    // Verify default values per design.md lines 201-214
    assert_eq!(opts.data_dir, PathBuf::from("./data/rocksdb"));
    assert!(opts.create_if_missing);
    assert_eq!(opts.compression, rocksdb::DBCompressionType::Lz4);
    assert_eq!(opts.write_buffer_size_mb, 64);
    assert_eq!(opts.max_write_buffer_number, 3);
    assert_eq!(opts.target_file_size_mb, 64);
    assert_eq!(opts.max_open_files, -1);
    assert!(!opts.enable_statistics);

    // Verify all 6 column families have defaults
    assert_eq!(opts.cf_options.len(), 6);
    assert!(opts.cf_options.contains_key(&ColumnFamily::SystemRaftLog));
    assert!(opts.cf_options.contains_key(&ColumnFamily::SystemRaftState));
    assert!(opts.cf_options.contains_key(&ColumnFamily::SystemData));
    assert!(opts.cf_options.contains_key(&ColumnFamily::DataRaftLog));
    assert!(opts.cf_options.contains_key(&ColumnFamily::DataRaftState));
    assert!(opts.cf_options.contains_key(&ColumnFamily::DataKv));
}

// ============================================================================
// Constructor Tests
// ============================================================================

#[test]
fn test_with_data_dir() {
    let custom_path = PathBuf::from("/custom/path");
    let opts = StorageOptions::with_data_dir(custom_path.clone());

    // Custom data_dir should be set
    assert_eq!(opts.data_dir, custom_path);

    // All other fields should have default values
    assert!(opts.create_if_missing);
    assert_eq!(opts.compression, rocksdb::DBCompressionType::Lz4);
    assert_eq!(opts.write_buffer_size_mb, 64);
    assert_eq!(opts.max_write_buffer_number, 3);
    assert_eq!(opts.target_file_size_mb, 64);
    assert_eq!(opts.max_open_files, -1);
    assert!(!opts.enable_statistics);
    assert_eq!(opts.cf_options.len(), 6);
}

// ============================================================================
// Validation Tests - Valid Cases
// ============================================================================

#[test]
fn test_validate_accepts_valid_config() {
    let opts = StorageOptions::default();
    assert!(opts.validate().is_ok());
}

#[test]
fn test_validate_accepts_minimum_valid_values() {
    let opts = StorageOptions {
        write_buffer_size_mb: 1,
        max_write_buffer_number: 2,
        target_file_size_mb: 1,
        ..Default::default()
    };

    assert!(opts.validate().is_ok());
}

#[test]
fn test_validate_accepts_maximum_valid_values() {
    let opts = StorageOptions {
        write_buffer_size_mb: 1024,
        max_write_buffer_number: 10,
        target_file_size_mb: 1024,
        ..Default::default()
    };

    assert!(opts.validate().is_ok());
}

// ============================================================================
// Validation Tests - Invalid write_buffer_size_mb
// ============================================================================

#[test]
fn test_validate_rejects_write_buffer_size_too_small() {
    let opts = StorageOptions {
        write_buffer_size_mb: 0,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "write_buffer_size_mb");
            assert!(reason.contains("1-1024"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

#[test]
fn test_validate_rejects_write_buffer_size_too_large() {
    let opts = StorageOptions {
        write_buffer_size_mb: 1025,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "write_buffer_size_mb");
            assert!(reason.contains("1-1024"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

// ============================================================================
// Validation Tests - Invalid max_write_buffer_number
// ============================================================================

#[test]
fn test_validate_rejects_max_write_buffer_number_too_small() {
    let opts = StorageOptions {
        max_write_buffer_number: 1,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "max_write_buffer_number");
            assert!(reason.contains("2-10"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

#[test]
fn test_validate_rejects_max_write_buffer_number_too_large() {
    let opts = StorageOptions {
        max_write_buffer_number: 11,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "max_write_buffer_number");
            assert!(reason.contains("2-10"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

// ============================================================================
// Validation Tests - Invalid target_file_size_mb
// ============================================================================

#[test]
fn test_validate_rejects_target_file_size_too_small() {
    let opts = StorageOptions {
        target_file_size_mb: 0,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "target_file_size_mb");
            assert!(reason.contains("1-1024"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

#[test]
fn test_validate_rejects_target_file_size_too_large() {
    let opts = StorageOptions {
        target_file_size_mb: 1025,
        ..Default::default()
    };

    let result = opts.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        StorageError::InvalidConfig { field, reason } => {
            assert_eq!(field, "target_file_size_mb");
            assert!(reason.contains("1-1024"));
        }
        _ => panic!("Expected InvalidConfig error"),
    }
}

// ============================================================================
// Column Family Default Options Tests - Raft Logs
// ============================================================================

#[test]
fn test_default_cf_options_for_system_raft_log() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::SystemRaftLog)
        .expect("SystemRaftLog CF should have options");

    // Per design.md lines 265-271
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(!cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 2);
    assert_eq!(cf_opts.write_buffer_size, None);
    assert!(cf_opts.prefix_extractor.is_none());
}

#[test]
fn test_default_cf_options_for_data_raft_log() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::DataRaftLog)
        .expect("DataRaftLog CF should have options");

    // Same as SystemRaftLog per design.md lines 265-271
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(!cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 2);
    assert_eq!(cf_opts.write_buffer_size, None);
    assert!(cf_opts.prefix_extractor.is_none());
}

// ============================================================================
// Column Family Default Options Tests - Raft State
// ============================================================================

#[test]
fn test_default_cf_options_for_system_raft_state() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::SystemRaftState)
        .expect("SystemRaftState CF should have options");

    // Per design.md lines 273-279
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 10);
    assert_eq!(cf_opts.write_buffer_size, Some(4 * 1024 * 1024)); // 4MB
    assert!(cf_opts.prefix_extractor.is_none());
}

#[test]
fn test_default_cf_options_for_data_raft_state() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::DataRaftState)
        .expect("DataRaftState CF should have options");

    // Same as SystemRaftState per design.md lines 273-279
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 10);
    assert_eq!(cf_opts.write_buffer_size, Some(4 * 1024 * 1024)); // 4MB
    assert!(cf_opts.prefix_extractor.is_none());
}

// ============================================================================
// Column Family Default Options Tests - System Data
// ============================================================================

#[test]
fn test_default_cf_options_for_system_data() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::SystemData)
        .expect("SystemData CF should have options");

    // Per design.md lines 281-287
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 10);
    assert_eq!(cf_opts.write_buffer_size, Some(8 * 1024 * 1024)); // 8MB
    assert!(cf_opts.prefix_extractor.is_none());
}

// ============================================================================
// Column Family Default Options Tests - Data KV
// ============================================================================

#[test]
fn test_default_cf_options_for_data_kv() {
    let opts = StorageOptions::default();
    let cf_opts = opts
        .cf_options
        .get(&ColumnFamily::DataKv)
        .expect("DataKv CF should have options");

    // Per design.md lines 289-304
    assert_eq!(cf_opts.compaction_style, DBCompactionStyle::Level);
    assert!(!cf_opts.disable_auto_compactions);
    assert_eq!(cf_opts.level0_file_num_compaction_trigger, 4);
    assert_eq!(cf_opts.write_buffer_size, None);

    // DataKv should have a 4-byte fixed prefix extractor
    assert!(cf_opts.prefix_extractor.is_some());
    // Note: We can't directly test SliceTransform internals, but we verify it exists
}

// ============================================================================
// Comprehensive Coverage Tests
// ============================================================================

#[test]
fn test_all_cfs_have_default_options() {
    let opts = StorageOptions::default();

    // Ensure every CF has options configured
    for cf in ColumnFamily::all() {
        assert!(
            opts.cf_options.contains_key(&cf),
            "CF {:?} missing default options",
            cf
        );
    }

    // Ensure no extra CFs are configured
    assert_eq!(opts.cf_options.len(), 6, "Should have exactly 6 CF options");
}

#[test]
fn test_cf_options_struct_construction() {
    // Test that CFOptions can be manually constructed
    let opts = CFOptions {
        compaction_style: DBCompactionStyle::Universal,
        disable_auto_compactions: true,
        level0_file_num_compaction_trigger: 5,
        write_buffer_size: Some(16 * 1024 * 1024),
        prefix_extractor: None,
    };

    assert_eq!(opts.compaction_style, DBCompactionStyle::Universal);
    assert!(opts.disable_auto_compactions);
    assert_eq!(opts.level0_file_num_compaction_trigger, 5);
    assert_eq!(opts.write_buffer_size, Some(16 * 1024 * 1024));
    assert!(opts.prefix_extractor.is_none());
}

#[test]
fn test_storage_options_manual_construction() {
    use std::collections::HashMap;

    // Test that StorageOptions can be manually constructed
    let mut cf_options = HashMap::new();
    cf_options.insert(
        ColumnFamily::DataKv,
        CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 4,
            write_buffer_size: None,
            prefix_extractor: None,
        },
    );

    let opts = StorageOptions {
        data_dir: PathBuf::from("/tmp/test"),
        create_if_missing: false,
        compression: rocksdb::DBCompressionType::Zstd,
        write_buffer_size_mb: 128,
        max_write_buffer_number: 5,
        target_file_size_mb: 128,
        max_open_files: 1000,
        enable_statistics: true,
        cf_options,
    };

    assert_eq!(opts.data_dir, PathBuf::from("/tmp/test"));
    assert!(!opts.create_if_missing);
    assert_eq!(opts.compression, rocksdb::DBCompressionType::Zstd);
    assert_eq!(opts.write_buffer_size_mb, 128);
    assert_eq!(opts.max_write_buffer_number, 5);
    assert_eq!(opts.target_file_size_mb, 128);
    assert_eq!(opts.max_open_files, 1000);
    assert!(opts.enable_statistics);
}
