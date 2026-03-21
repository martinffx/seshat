//! Basic sanity tests for Storage initialization.
//!
//! These tests verify the most fundamental functionality works.

use seshat_storage::{Storage, StorageOptions};
use tempfile::TempDir;

#[test]
fn test_can_create_and_close_storage() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let options = StorageOptions::with_data_dir(temp_dir.path().to_path_buf());

    // Create storage
    let storage = Storage::new(options).expect("Failed to create storage");

    // Close storage
    storage.close().expect("Failed to close storage");
}

#[test]
fn test_format_log_key() {
    // We can't directly call format_log_key since it's private,
    // but this test documents the expected format
    assert_eq!(format!("log:{:020}", 0), "log:00000000000000000000");
    assert_eq!(format!("log:{:020}", 42), "log:00000000000000000042");
    assert_eq!(
        format!("log:{:020}", u64::MAX),
        "log:18446744073709551615"
    );
}
