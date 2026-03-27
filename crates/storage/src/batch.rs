//! WriteBatch API for atomic multi-operation writes.
//!
//! This module provides a builder-pattern API for batching multiple write operations
//! (put/delete) across different column families and executing them atomically.
//!
//! # Examples
//!
//! ```no_run
//! use seshat_storage::{WriteBatch, ColumnFamily, Storage, StorageOptions};
//!
//! let storage = Storage::new(StorageOptions::default())?;
//! let mut batch = WriteBatch::new();
//!
//! batch
//!     .put(ColumnFamily::DataKv, b"key1", b"value1")
//!     .put(ColumnFamily::DataKv, b"key2", b"value2")
//!     .delete(ColumnFamily::DataKv, b"old_key");
//!
//! storage.batch_write(batch)?;
//! # Ok::<(), seshat_storage::StorageError>(())
//! ```

use crate::ColumnFamily;

/// Atomic batch of write operations across multiple column families.
///
/// `WriteBatch` uses the builder pattern to collect multiple put/delete operations
/// and execute them atomically via `Storage::batch_write()`.
///
/// # Atomicity Guarantee
///
/// All operations in the batch succeed or all fail - no partial writes.
///
/// # Examples
///
/// ```no_run
/// use seshat_storage::{WriteBatch, ColumnFamily};
///
/// let mut batch = WriteBatch::new();
/// batch
///     .put(ColumnFamily::DataKv, b"key1", b"value1")
///     .put(ColumnFamily::SystemData, b"config", b"settings")
///     .delete(ColumnFamily::DataKv, b"old_key");
///
/// assert!(!batch.is_empty());
/// assert_eq!(batch.requires_fsync(), false); // DataKv and SystemData don't require fsync
/// ```
#[derive(Debug, Default)]
pub struct WriteBatch {
    /// Operations to be executed atomically
    operations: Vec<BatchOperation>,
}

/// A single operation in a batch (internal representation).
#[derive(Debug, Clone)]
pub(crate) enum BatchOperation {
    /// Put operation: store key-value pair
    Put {
        cf: ColumnFamily,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    /// Delete operation: remove key
    Delete { cf: ColumnFamily, key: Vec<u8> },
}

impl WriteBatch {
    /// Creates a new empty batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::WriteBatch;
    ///
    /// let batch = WriteBatch::new();
    /// assert!(batch.is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Adds a put operation to the batch.
    ///
    /// Returns `&mut self` for method chaining.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to write to
    /// * `key` - Key to store
    /// * `value` - Value to store
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::{WriteBatch, ColumnFamily};
    ///
    /// let mut batch = WriteBatch::new();
    /// batch.put(ColumnFamily::DataKv, b"key", b"value");
    /// ```
    pub fn put(&mut self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> &mut Self {
        self.operations.push(BatchOperation::Put {
            cf,
            key: key.to_vec(),
            value: value.to_vec(),
        });
        self
    }

    /// Adds a delete operation to the batch.
    ///
    /// Returns `&mut self` for method chaining.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to delete from
    /// * `key` - Key to delete
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::{WriteBatch, ColumnFamily};
    ///
    /// let mut batch = WriteBatch::new();
    /// batch.delete(ColumnFamily::DataKv, b"key");
    /// ```
    pub fn delete(&mut self, cf: ColumnFamily, key: &[u8]) -> &mut Self {
        self.operations.push(BatchOperation::Delete {
            cf,
            key: key.to_vec(),
        });
        self
    }

    /// Clears all operations from the batch.
    ///
    /// After calling `clear()`, the batch can be reused.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::{WriteBatch, ColumnFamily};
    ///
    /// let mut batch = WriteBatch::new();
    /// batch.put(ColumnFamily::DataKv, b"key", b"value");
    /// assert!(!batch.is_empty());
    ///
    /// batch.clear();
    /// assert!(batch.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.operations.clear();
    }

    /// Returns true if the batch contains no operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::{WriteBatch, ColumnFamily};
    ///
    /// let mut batch = WriteBatch::new();
    /// assert!(batch.is_empty());
    ///
    /// batch.put(ColumnFamily::DataKv, b"key", b"value");
    /// assert!(!batch.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Returns true if any operation in the batch targets a CF that requires fsync.
    ///
    /// Raft state CFs (`SystemRaftState`, `DataRaftState`) require fsync for durability.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::{WriteBatch, ColumnFamily};
    ///
    /// let mut batch = WriteBatch::new();
    /// batch.put(ColumnFamily::DataKv, b"key", b"value");
    /// assert!(!batch.requires_fsync()); // DataKv doesn't require fsync
    ///
    /// batch.put(ColumnFamily::SystemRaftState, b"state", b"data");
    /// assert!(batch.requires_fsync()); // SystemRaftState requires fsync
    /// ```
    pub fn requires_fsync(&self) -> bool {
        self.operations.iter().any(|op| op.cf().requires_fsync())
    }

    /// Returns the operations in this batch (internal API for Storage).
    ///
    /// This method is used by `Storage::batch_write()` to access the operations
    /// and apply them to RocksDB.
    pub(crate) fn operations(&self) -> &[BatchOperation] {
        &self.operations
    }
}

impl BatchOperation {
    /// Returns the column family for this operation.
    pub(crate) fn cf(&self) -> ColumnFamily {
        match self {
            BatchOperation::Put { cf, .. } => *cf,
            BatchOperation::Delete { cf, .. } => *cf,
        }
    }

    /// Returns the key for this operation.
    pub(crate) fn key(&self) -> &[u8] {
        match self {
            BatchOperation::Put { key, .. } => key,
            BatchOperation::Delete { key, .. } => key,
        }
    }

    /// Returns the value for Put operations.
    pub(crate) fn value(&self) -> Option<&[u8]> {
        match self {
            BatchOperation::Put { value, .. } => Some(value),
            BatchOperation::Delete { .. } => None,
        }
    }

    /// Returns true if this is a Put operation.
    pub(crate) fn is_put(&self) -> bool {
        matches!(self, BatchOperation::Put { .. })
    }
}
