//! Iterator support for range queries and prefix scanning.
//!
//! This module provides snapshot-isolated iteration over key-value pairs
//! within a column family using RocksDB's DBIterator.

use crate::{ColumnFamily, Result, StorageError};
use rocksdb::DBIterator;

/// Iterator mode for specifying iteration start position and direction.
#[derive(Debug, Clone)]
pub enum IteratorMode {
    /// Start from the first key in the column family (forward iteration)
    Start,
    /// Start from the last key in the column family (reverse iteration)
    End,
    /// Start from a specific key with the given direction
    From(Vec<u8>, Direction),
}

/// Direction for iterator traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Iterate forward (ascending key order)
    Forward,
    /// Iterate backward (descending key order)
    Reverse,
}

/// Iterator over key-value pairs in a column family.
///
/// Provides snapshot-isolated iteration - the iterator sees a consistent
/// view of the database at the time it was created, even if concurrent
/// writes occur.
///
/// # Lifetime
///
/// The iterator borrows from the Storage instance and must not outlive it.
///
/// # Examples
///
/// ```no_run
/// use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::{IteratorMode, Direction}};
///
/// let storage = Storage::new(StorageOptions::default())?;
/// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
///
/// while let Some((key, value)) = iter.next() {
///     println!("Key: {:?}, Value: {:?}", key, value);
/// }
/// # Ok::<(), seshat_storage::StorageError>(())
/// ```
pub struct StorageIterator<'a> {
    /// The underlying RocksDB iterator
    inner: DBIterator<'a>,
    /// Column family being iterated
    cf: ColumnFamily,
}

impl<'a> StorageIterator<'a> {
    /// Creates a new StorageIterator (internal constructor).
    ///
    /// # Arguments
    ///
    /// * `inner` - RocksDB DBIterator
    /// * `cf` - Column family being iterated
    pub(crate) fn new(inner: DBIterator<'a>, cf: ColumnFamily) -> Self {
        Self { inner, cf }
    }

    /// Seek to a specific key.
    ///
    /// Positions the iterator at the first key >= the given key (forward)
    /// or the first key <= the given key (reverse).
    ///
    /// # Arguments
    ///
    /// * `key` - Key to seek to
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// iter.seek(b"mykey");
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn seek(&mut self, key: &[u8]) {
        self.inner.seek(key);
    }

    /// Seek to the first key in the column family.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// iter.seek_to_first();
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn seek_to_first(&mut self) {
        self.inner.seek_to_first();
    }

    /// Seek to the last key in the column family.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// iter.seek_to_last();
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn seek_to_last(&mut self) {
        self.inner.seek_to_last();
    }

    /// Advance to the next key and return (key, value).
    ///
    /// Returns None if the iterator is exhausted.
    ///
    /// # Returns
    ///
    /// - Some((key, value)) - Next key-value pair
    /// - None - Iterator is exhausted
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// while let Some((key, value)) = iter.next() {
    ///     println!("Key: {:?}", key);
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn next(&mut self) -> Option<(Box<[u8]>, Box<[u8]>)> {
        // RocksDB's DBIterator::next() returns Option<Result<(Box<[u8]>, Box<[u8]>), Error>>
        // We need to convert this to Option<(Box<[u8]>, Box<[u8]>)>
        match self.inner.next() {
            Some(Ok((key, value))) => Some((key, value)),
            Some(Err(_)) => None, // On error, treat as end of iteration
            None => None,
        }
    }

    /// Move to the previous key and return (key, value).
    ///
    /// Returns None if the iterator is at the beginning.
    ///
    /// # Returns
    ///
    /// - Some((key, value)) - Previous key-value pair
    /// - None - Iterator is at the beginning
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::End)?;
    /// while let Some((key, value)) = iter.prev() {
    ///     println!("Key: {:?}", key);
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn prev(&mut self) -> Option<(Box<[u8]>, Box<[u8]>)> {
        // Check if iterator is valid before calling prev
        if !self.inner.valid() {
            return None;
        }

        // Get current key-value pair
        let result = self.key().and_then(|key| {
            self.value().map(|value| (key, value))
        });

        // Move iterator backwards
        self.inner.prev();

        result
    }

    /// Check if the iterator is currently positioned at a valid key.
    ///
    /// # Returns
    ///
    /// - true - Iterator points to a valid key-value pair
    /// - false - Iterator is exhausted or invalid
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// if iter.valid() {
    ///     println!("Iterator is at a valid position");
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn valid(&self) -> bool {
        self.inner.valid()
    }

    /// Get the current key without advancing the iterator.
    ///
    /// Returns None if the iterator is not positioned at a valid key.
    ///
    /// # Returns
    ///
    /// - Some(key) - Current key
    /// - None - Iterator is not valid
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// if let Some(key) = iter.key() {
    ///     println!("Current key: {:?}", key);
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn key(&self) -> Option<Box<[u8]>> {
        if !self.inner.valid() {
            return None;
        }

        self.inner.key().map(|k| k.to_vec().into_boxed_slice())
    }

    /// Get the current value without advancing the iterator.
    ///
    /// Returns None if the iterator is not positioned at a valid key.
    ///
    /// # Returns
    ///
    /// - Some(value) - Current value
    /// - None - Iterator is not valid
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use seshat_storage::{Storage, StorageOptions, ColumnFamily, iterator::IteratorMode};
    /// # let storage = Storage::new(StorageOptions::default())?;
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// if let Some(value) = iter.value() {
    ///     println!("Current value: {:?}", value);
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn value(&self) -> Option<Box<[u8]>> {
        if !self.inner.valid() {
            return None;
        }

        self.inner.value().map(|v| v.to_vec().into_boxed_slice())
    }

    /// Get the column family this iterator is iterating over.
    pub fn cf(&self) -> ColumnFamily {
        self.cf
    }
}
