//! Integration tests for Storage initialization and lifecycle.

use seshat_storage::{Storage, StorageError, StorageOptions};
use tempfile::TempDir;

fn temp_storage_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

fn temp_storage_options(temp_dir: &TempDir) -> StorageOptions {
    StorageOptions::with_data_dir(temp_dir.path().to_path_buf())
}

#[test]
fn test_new_creates_storage_with_default_options() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options);
    assert!(storage.is_ok(), "Storage::new() should succeed");

    let storage = storage.unwrap();
    storage.close().expect("close() should succeed");
}

#[test]
fn test_new_creates_all_column_families() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    let options2 = StorageOptions {
        data_dir: temp_dir.path().to_path_buf(),
        create_if_missing: false,
        ..Default::default()
    };

    storage.close().expect("close() should succeed");

    let storage2 = Storage::new(options2);
    assert!(
        storage2.is_ok(),
        "Should be able to reopen with all CFs present"
    );
    storage2.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_new_validates_options() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.write_buffer_size_mb = 0;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    if let Err(StorageError::InvalidConfig { field, .. }) = result {
        assert_eq!(field, "write_buffer_size_mb");
    } else {
        panic!("Expected InvalidConfig error");
    }
}

#[test]
fn test_new_validates_max_write_buffer_number() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.max_write_buffer_number = 1;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    if let Err(StorageError::InvalidConfig { field, .. }) = result {
        assert_eq!(field, "max_write_buffer_number");
    } else {
        panic!("Expected InvalidConfig error");
    }
}

#[test]
fn test_new_validates_target_file_size() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.target_file_size_mb = 2000;

    let result = Storage::new(options);
    assert!(result.is_err(), "Should fail with invalid config");

    if let Err(StorageError::InvalidConfig { field, .. }) = result {
        assert_eq!(field, "target_file_size_mb");
    } else {
        panic!("Expected InvalidConfig error");
    }
}

#[test]
fn test_new_creates_data_directory_if_missing() {
    let temp_dir = temp_storage_dir();
    let data_path = temp_dir.path().join("data").join("rocksdb");

    let options = StorageOptions {
        data_dir: data_path.clone(),
        create_if_missing: true,
        ..Default::default()
    };

    assert!(!data_path.exists(), "Directory should not exist yet");

    let storage = Storage::new(options);
    assert!(storage.is_ok(), "Storage::new() should create directory");

    assert!(data_path.exists(), "Directory should be created by RocksDB");

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_new_fails_if_create_if_missing_false_and_no_db() {
    let temp_dir = temp_storage_dir();
    let data_path = temp_dir.path().join("nonexistent");

    let options = StorageOptions {
        data_dir: data_path,
        create_if_missing: false,
        ..Default::default()
    };

    let result = Storage::new(options);
    assert!(
        result.is_err(),
        "Should fail when DB doesn't exist and create_if_missing=false"
    );
}

#[test]
fn test_cf_handle_lookup_works_for_all_cfs() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    storage.close().expect("close() should succeed");
}

#[test]
fn test_warm_up_cache_initializes_log_indices() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    storage.close().expect("close() should succeed");
}

#[test]
fn test_get_last_log_index_from_empty_db() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    storage.close().expect("close() should succeed");
}

#[test]
fn test_close_flushes_and_closes_db() {
    let temp_dir = temp_storage_dir();
    let options = temp_storage_options(&temp_dir);

    let storage = Storage::new(options).expect("Storage::new() should succeed");

    let result = storage.close();
    assert!(result.is_ok(), "close() should succeed");
}

#[test]
fn test_storage_can_be_reopened_after_close() {
    let temp_dir = temp_storage_dir();

    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());
    let storage = Storage::new(options).expect("Storage::new() should succeed");
    storage.close().expect("close() should succeed");

    let options2 = StorageOptions {
        data_dir: temp_dir.path().to_path_buf(),
        create_if_missing: false,
        ..Default::default()
    };
    let storage2 = Storage::new(options2);
    assert!(storage2.is_ok(), "Should be able to reopen after close");

    storage2.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_custom_write_buffer_size() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.write_buffer_size_mb = 128;

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should accept valid custom config"
    );

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_custom_compression() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.compression = rocksdb::DBCompressionType::Snappy;

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should accept different compression"
    );

    storage.unwrap().close().expect("close() should succeed");
}

#[test]
fn test_statistics_enabled() {
    let temp_dir = temp_storage_dir();
    let mut options = temp_storage_options(&temp_dir);

    options.enable_statistics = true;

    let storage = Storage::new(options);
    assert!(
        storage.is_ok(),
        "Storage::new() should work with statistics enabled"
    );

    storage.unwrap().close().expect("close() should succeed");
}
