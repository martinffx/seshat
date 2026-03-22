//! Storage backend trait for dependency inversion.
//!
//! This module defines the `StorageBackend` trait that abstracts storage operations,
//! enabling testing with mock implementations and supporting the Dependency Inversion Principle.
//!
//! # Motivation
//!
//! The concrete `Storage` struct depends on `rocksdb::DB`, which makes unit testing
//! difficult without an actual database. By introducing this trait, we can:
//!
//! - Test code that uses storage with in-memory mocks
//! - Swap storage implementations (e.g., for different databases)
//! - Isolate business logic from storage concerns
//!
//! # Examples
//!
//! ```ignore
//! use seshat_storage::{StorageBackend, ColumnFamily, WriteBatch};
//!
//! struct MockStorage { ... }
//!
//! impl StorageBackend for MockStorage {
//!     fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
//!         // Return mock data
//!     }
//!     // ... implement other methods
//! }
//! ```

use crate::{ColumnFamily, Result, StorageIterator, WriteBatch};

/// Trait for storage backends, enabling testing with mocks.
///
/// This trait abstracts the core storage operations that the application needs,
/// regardless of the underlying storage implementation (RocksDB, memory, etc.).
///
/// # Design Rationale
///
/// The trait is designed to be minimal yet complete for the application's needs:
/// - `get`, `put`, `delete`: Basic CRUD operations
/// - `batch_write`: Atomic multi-operation writes
/// - `get_iter`: Range queries and prefix scanning
///
/// All methods return `Result` to handle errors uniformly.
/// The trait requires `Send + Sync` to ensure thread-safe usage.
///
/// # Implementing the Trait
///
/// ```ignore
/// use seshat_storage::{StorageBackend, ColumnFamily, Result, WriteBatch, StorageIterator};
/// use std::collections::HashMap;
/// use std::sync::{Arc, Mutex};
///
/// struct InMemoryStorage {
///     data: Arc<Mutex<HashMap<(ColumnFamily, Vec<u8>), Vec<u8>>>>,
/// }
///
/// impl InMemoryStorage {
///     pub fn new() -> Self {
///         Self { data: Arc::new(Mutex::new(HashMap::new())) }
///     }
/// }
///
/// impl StorageBackend for InMemoryStorage {
///     fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
///         Ok(self.data.lock().unwrap().get(&(cf, key.to_vec())).cloned())
///     }
///
///     fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
///         self.data.lock().unwrap().insert((cf, key.to_vec()), value.to_vec());
///         Ok(())
///     }
///
///     fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()> {
///         self.data.lock().unwrap().remove(&(cf, key.to_vec()));
///         Ok(())
///     }
///
///     fn batch_write(&self, _batch: WriteBatch) -> Result<()> {
///         Ok(())
///     }
///
///     fn get_iter(&self, _cf: ColumnFamily) -> Result<StorageIterator<'_>> {
///         unimplemented!("In-memory iterator not implemented")
///     }
/// }
/// ```
/// ```
pub trait StorageBackend: Send + Sync {
    /// Get a value by key.
    ///
    /// Returns `Ok(Some(value))` if the key exists, `Ok(None)` if not found.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to read from
    /// * `key` - Key to look up (binary data)
    ///
    /// # Errors
    ///
    /// Returns error if the database read fails.
    fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Put a key-value pair.
    ///
    /// Stores a key-value pair in the specified column family.
    /// For column families that require durability, this should flush to disk.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to write to
    /// * `key` - Key to store (binary data)
    /// * `value` - Value to store (binary data)
    ///
    /// # Errors
    ///
    /// Returns error if the database write fails.
    fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete a key.
    ///
    /// Removes a key from the specified column family.
    /// This operation is idempotent - deleting a non-existent key returns `Ok(())`.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to delete from
    /// * `key` - Key to delete (binary data)
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails.
    fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()>;

    /// Execute a batch of operations atomically.
    ///
    /// All operations in the batch succeed or all fail - no partial writes.
    ///
    /// # Arguments
    ///
    /// * `batch` - WriteBatch containing operations to execute
    ///
    /// # Errors
    ///
    /// Returns error if any operation fails. No operations are applied on failure.
    fn batch_write(&self, batch: WriteBatch) -> Result<()>;

    /// Create an iterator over a column family.
    ///
    /// Returns a snapshot-isolated iterator that provides a consistent view
    /// of the database at the time of creation.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to iterate over
    ///
    /// # Errors
    ///
    /// Returns error if the column family cannot be found or iterator creation fails.
    fn get_iter(&self, cf: ColumnFamily) -> Result<StorageIterator<'_>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageError;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockStorage {
        data: Arc<Mutex<HashMap<(ColumnFamily, Vec<u8>), Vec<u8>>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    impl StorageBackend for MockStorage {
        fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
            let data = self.data.lock().unwrap();
            Ok(data.get(&(cf, key.to_vec())).cloned())
        }

        fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.insert((cf, key.to_vec()), value.to_vec());
            Ok(())
        }

        fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.remove(&(cf, key.to_vec()));
            Ok(())
        }

        fn batch_write(&self, _batch: WriteBatch) -> Result<()> {
            Ok(())
        }

        fn get_iter(&self, _cf: ColumnFamily) -> Result<StorageIterator<'_>> {
            Err(StorageError::SnapshotFailed {
                path: "mock".to_string(),
                reason: "Mock storage does not support iterators".to_string(),
            })
        }
    }

    #[test]
    fn test_mock_storage_implements_trait() {
        let storage = MockStorage::new();

        storage
            .put(ColumnFamily::DataKv, b"key1", b"value1")
            .unwrap();

        let result = storage.get(ColumnFamily::DataKv, b"key1").unwrap();
        assert_eq!(result, Some(b"value1".to_vec()));

        let missing = storage.get(ColumnFamily::DataKv, b"nonexistent").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn test_mock_storage_delete() {
        let storage = MockStorage::new();

        storage
            .put(ColumnFamily::DataKv, b"key1", b"value1")
            .unwrap();
        assert!(storage
            .get(ColumnFamily::DataKv, b"key1")
            .unwrap()
            .is_some());

        storage.delete(ColumnFamily::DataKv, b"key1").unwrap();
        assert!(storage
            .get(ColumnFamily::DataKv, b"key1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_mock_storage_delete_nonexistent() {
        let storage = MockStorage::new();

        let result = storage.delete(ColumnFamily::DataKv, b"nonexistent");
        assert!(result.is_ok());
    }
}
