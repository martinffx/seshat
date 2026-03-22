//! RocksDB storage implementation for Seshat.
//!
//! This module provides the core `Storage` struct that wraps RocksDB and provides
//! the main API for persistent storage operations.

use crate::iterator::{Direction, IteratorMode, StorageIterator};
use crate::{ColumnFamily, Result, StorageError, StorageOptions, WriteBatch};
use parking_lot::RwLock;
use rocksdb::checkpoint::Checkpoint;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, Options as DBOptions,
    WriteBatch as RocksDBWriteBatch, WriteOptions, DB,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Main storage struct that wraps RocksDB.
///
/// `Storage` provides a thread-safe API for all storage operations:
/// - Log operations (append, read, truncate)
/// - State operations (read, write)
/// - KV operations (get, put, delete)
/// - Batch operations (atomic multi-operation writes)
/// - Snapshot operations (create, restore)
///
/// # Thread Safety
///
/// All fields use `Arc` for safe sharing across threads. The DB handle and
/// column family handles are wrapped in `Arc` to enable concurrent access.
///
/// # Examples
///
/// ```no_run
/// use seshat_storage::{Storage, StorageOptions};
///
/// let options = StorageOptions::default();
/// let storage = Storage::new(options).expect("Failed to open storage");
/// ```
pub struct Storage {
    /// RocksDB database handle (thread-safe via Arc)
    db: Arc<DB>,

    /// Cache of last log index per log CF (for performance)
    last_log_index_cache: Arc<RwLock<HashMap<ColumnFamily, u64>>>,

    /// Path to the data directory (used for snapshot path validation)
    path: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance with the given options.
    ///
    /// Opens existing RocksDB database or creates a new one based on
    /// `options.create_if_missing`.
    ///
    /// # Process
    ///
    /// 1. Validates options
    /// 2. Creates RocksDB options and column family descriptors
    /// 3. Opens or creates database
    /// 4. Caches column family handles
    /// 5. Initializes empty log index cache
    /// 6. Warms up cache by reading last log indices from disk
    ///
    /// # Arguments
    ///
    /// * `options` - Storage configuration options
    ///
    /// # Returns
    ///
    /// New Storage instance ready for use
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Options validation fails
    /// - Database cannot be opened/created
    /// - Column families cannot be created
    /// - Cache warm-up fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions};
    /// use std::path::PathBuf;
    ///
    /// let options = StorageOptions::with_data_dir(PathBuf::from("/var/lib/seshat"));
    /// let storage = Storage::new(options)?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn new(mut options: StorageOptions) -> Result<Self> {
        // Step 1: Validate options
        options.validate()?;

        // Step 2: Create DB options
        let mut db_opts = DBOptions::default();
        db_opts.create_if_missing(options.create_if_missing);
        db_opts.create_missing_column_families(true);

        // Set global options from StorageOptions
        db_opts.set_compression_type(options.compression);
        db_opts.set_max_open_files(options.max_open_files);

        if options.enable_statistics {
            db_opts.enable_statistics();
        }

        // Step 3: Create column family descriptors
        let cf_descriptors: Vec<ColumnFamilyDescriptor> = ColumnFamily::all()
            .iter()
            .map(|cf| {
                let mut cf_opts = DBOptions::default();

                // Get per-CF options from config
                if let Some(cf_config) = options.cf_options.get(cf) {
                    cf_opts.set_compaction_style(cf_config.compaction_style);
                    cf_opts.set_disable_auto_compactions(cf_config.disable_auto_compactions);
                    cf_opts.set_level_zero_file_num_compaction_trigger(
                        cf_config.level0_file_num_compaction_trigger,
                    );

                    // Set write buffer size (per-CF override or global)
                    let write_buffer_size = cf_config
                        .write_buffer_size
                        .unwrap_or(options.write_buffer_size_mb * 1024 * 1024);
                    cf_opts.set_write_buffer_size(write_buffer_size);

                    // Set max write buffers
                    cf_opts.set_max_write_buffer_number(options.max_write_buffer_number as i32);

                    // Set target file size
                    cf_opts.set_target_file_size_base(
                        (options.target_file_size_mb * 1024 * 1024) as u64,
                    );

                    // Set prefix extractor if provided (need mutable access)
                    if let Some(cf_config) = options.cf_options.get_mut(cf) {
                        if let Some(prefix_extractor) = cf_config.prefix_extractor.take() {
                            cf_opts.set_prefix_extractor(prefix_extractor);
                        }
                    }
                } else {
                    // Use global defaults
                    cf_opts.set_write_buffer_size(options.write_buffer_size_mb * 1024 * 1024);
                    cf_opts.set_max_write_buffer_number(options.max_write_buffer_number as i32);
                    cf_opts.set_target_file_size_base(
                        (options.target_file_size_mb * 1024 * 1024) as u64,
                    );
                }

                ColumnFamilyDescriptor::new(cf.as_str(), cf_opts)
            })
            .collect();

        // Step 4: Open or create database
        let db = DB::open_cf_descriptors(&db_opts, &options.data_dir, cf_descriptors)?;
        let db = Arc::new(db);

        // Step 5: Initialize empty log index cache
        let last_log_index_cache = Arc::new(RwLock::new(HashMap::new()));

        // Step 6: Create Storage instance
        let mut storage = Self {
            db,
            last_log_index_cache,
            path: options.data_dir.clone(),
        };

        // Step 7: Warm up cache
        storage.warm_up_index_cache()?;

        Ok(storage)
    }

    fn get_cf_handle(&self, cf: ColumnFamily) -> Result<Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(cf.as_str())
            .ok_or_else(|| StorageError::ColumnFamilyNotFound {
                name: cf.as_str().to_string(),
            })
    }

    /// Warms up index cache on startup.
    ///
    /// Reads the last log index from each log column family and populates
    /// the `last_log_index_cache`. This is called during `Storage::new()`
    /// to initialize the cache.
    ///
    /// # Process
    ///
    /// 1. Gets log CFs: SystemRaftLog, DataRaftLog
    /// 2. For each log CF:
    ///    - Calls `get_last_log_index_from_db(cf)`
    ///    - If Some(index), inserts into cache
    ///    - Logs info with CF name and index
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if database read fails or key format is corrupted.
    fn warm_up_index_cache(&mut self) -> Result<()> {
        let log_cfs = [ColumnFamily::SystemRaftLog, ColumnFamily::DataRaftLog];

        for cf in &log_cfs {
            if let Some(last_index) = self.get_last_log_index_from_db(*cf)? {
                let mut cache = self.last_log_index_cache.write();
                cache.insert(*cf, last_index);

                // Log cache warm-up (using eprintln for now, will switch to tracing later)
                eprintln!(
                    "Warmed up index cache for {}: last_index={}",
                    cf.as_str(),
                    last_index
                );
            }
        }

        Ok(())
    }

    /// Gets last log index from database (expensive, cache miss path).
    ///
    /// Reads the last log entry from the specified log column family
    /// by creating a reverse iterator and reading the first key.
    ///
    /// # Process
    ///
    /// 1. Gets CF handle
    /// 2. Creates reverse iterator: `IteratorMode::End`
    /// 3. Gets last key-value pair
    /// 4. Parses key: "log:00000000000000000042" -> 42
    /// 5. Returns Ok(Some(index)) or Ok(None) if empty
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    ///
    /// # Returns
    ///
    /// - Ok(Some(index)) - Last log index found
    /// - Ok(None) - Log is empty
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - CF is not a log CF
    /// - Database read fails
    /// - Key format is corrupted (not "log:NNNNNNNNNNNNNNNNNNNN")
    fn get_last_log_index_from_db(&self, cf: ColumnFamily) -> Result<Option<u64>> {
        // Verify this is a log CF
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "get_last_log_index_from_db only works on log CFs".to_string(),
            });
        }

        let cf_handle = self.get_cf_handle(cf)?;

        let mut iter = self.db.raw_iterator_cf(&cf_handle);
        iter.seek_to_last();

        if iter.valid() {
            let key = iter.key().unwrap();
            let key_str =
                String::from_utf8(key.to_vec()).map_err(|_| StorageError::CorruptedData {
                    cf: cf.as_str().to_string(),
                    key: key.to_vec(),
                    reason: "Key is not valid UTF-8".to_string(),
                })?;

            // Strip "log:" prefix
            if !key_str.starts_with("log:") {
                return Err(StorageError::CorruptedData {
                    cf: cf.as_str().to_string(),
                    key: key.to_vec(),
                    reason: format!("Key does not start with 'log:': {}", key_str),
                });
            }

            let index_str = &key_str[4..]; // Skip "log:"
            let index = index_str
                .parse::<u64>()
                .map_err(|_| StorageError::CorruptedData {
                    cf: cf.as_str().to_string(),
                    key: key.to_vec(),
                    reason: format!("Cannot parse log index from: {}", index_str),
                })?;

            Ok(Some(index))
        } else {
            // Empty log
            Ok(None)
        }
    }

    /// Closes the database gracefully.
    ///
    /// Flushes all pending writes and closes the database. This consumes
    /// the Storage instance, ensuring exclusive access during shutdown.
    ///
    /// # Process
    ///
    /// 1. Flushes all column families
    /// 2. Closes DB handle
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if flush or close fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// // ... use storage ...
    /// storage.close()?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn close(self) -> Result<()> {
        // Flush all column families before closing
        for cf in ColumnFamily::all().iter() {
            let cf_handle = self.get_cf_handle(*cf)?;
            self.db.flush_cf(&cf_handle)?;
        }

        // RocksDB will be closed when Arc<DB> is dropped
        // (when the last reference goes out of scope)

        Ok(())
    }

    // ========================================================================
    // CRUD Operations (ROCKS-005)
    // ========================================================================

    /// Get value by key. Returns None if key doesn't exist.
    ///
    /// This method reads a value from the specified column family.
    /// RocksDB searches: Memtable → Block cache → Bloom filter → SST files.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to read from
    /// * `key` - Key to look up (binary data)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` - Key exists, returns value
    /// - `Ok(None)` - Key does not exist
    ///
    /// # Errors
    ///
    /// Returns error if database read fails.
    ///
    /// # Performance
    ///
    /// O(log n), target <1ms p99 latency.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// let value = storage.get(ColumnFamily::DataKv, b"mykey")?;
    /// match value {
    ///     Some(v) => println!("Found: {:?}", v),
    ///     None => println!("Key not found"),
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // Step 1: Get CF handle
        let cf_handle = self.get_cf_handle(cf)?;

        // Step 2: Call RocksDB get_cf
        // RocksDB searches: Memtable → Block cache → Bloom filter → SST files
        let result = self.db.get_cf(&cf_handle, key)?;

        // Step 3: Return Ok(Some(value)) if found, Ok(None) if not found
        Ok(result)
    }

    /// Put key-value pair. Fsyncs if CF requires it.
    ///
    /// Stores a key-value pair in the specified column family.
    /// For *_raft_state CFs, this method blocks until data is fsynced to disk.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to write to
    /// * `key` - Key to store (binary data)
    /// * `value` - Value to store (binary data)
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if database write fails.
    ///
    /// # Performance
    ///
    /// O(1) amortized, may trigger background compaction.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// storage.put(ColumnFamily::DataKv, b"mykey", b"myvalue")?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
        // Step 1: Get CF handle
        let cf_handle = self.get_cf_handle(cf)?;

        // Step 2: Check if fsync is required
        let mut write_opts = WriteOptions::default();
        if cf.requires_fsync() {
            // For *_raft_state CFs, block until fsync completes
            write_opts.set_sync(true);
        }
        // Otherwise, use default (async WAL)

        // Step 3: Call RocksDB put_cf_opt
        self.db.put_cf_opt(&cf_handle, key, value, &write_opts)?;

        Ok(())
    }

    /// Delete key.
    ///
    /// Removes a key from the specified column family.
    /// This operation is idempotent - deleting a non-existent key returns Ok(()).
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to delete from
    /// * `key` - Key to delete (binary data)
    ///
    /// # Returns
    ///
    /// Ok(()) on success (even if key didn't exist)
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// storage.delete(ColumnFamily::DataKv, b"mykey")?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()> {
        // Step 1: Get CF handle
        let cf_handle = self.get_cf_handle(cf)?;

        // Step 2: Create WriteOptions (check fsync requirement)
        let mut write_opts = WriteOptions::default();
        if cf.requires_fsync() {
            // For *_raft_state CFs, block until fsync completes
            write_opts.set_sync(true);
        }

        // Step 3: Call RocksDB delete_cf_opt
        self.db.delete_cf_opt(&cf_handle, key, &write_opts)?;

        // Step 4: Return Ok(()) - RocksDB delete is idempotent
        Ok(())
    }

    /// Check if key exists (uses bloom filter optimization).
    ///
    /// Efficiently checks if a key exists without reading its value.
    /// Uses RocksDB's bloom filter to avoid disk reads when possible.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to check
    /// * `key` - Key to check (binary data)
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Key exists
    /// - `Ok(false)` - Key does not exist
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// if storage.exists(ColumnFamily::DataKv, b"mykey")? {
    ///     println!("Key exists");
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn exists(&self, cf: ColumnFamily, key: &[u8]) -> Result<bool> {
        // Step 1: Get CF handle
        let cf_handle = self.get_cf_handle(cf)?;

        // Step 2: Use get_pinned_cf which can use bloom filter
        // This is more efficient than get_cf as it may avoid reading the value
        let result = self.db.get_pinned_cf(&cf_handle, key)?;

        // Step 3: Return true if Some, false if None
        Ok(result.is_some())
    }

    // ========================================================================
    // Log Operations (ROCKS-006)
    // ========================================================================

    /// Append log entry with automatic index validation.
    ///
    /// Validates that indices are sequential (first=1, subsequent=last+1)
    /// before writing. Updates the last_log_index_cache after successful write.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    /// * `index` - Log index to append (must be sequential)
    /// * `entry_bytes` - Serialized log entry data
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - CF is not a log CF (`InvalidColumnFamily`)
    /// - Index is not sequential (`InvalidLogIndex`)
    /// - Database write fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// storage.append_log_entry(ColumnFamily::SystemRaftLog, 1, b"entry1")?;
    /// storage.append_log_entry(ColumnFamily::SystemRaftLog, 2, b"entry2")?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn append_log_entry(&self, cf: ColumnFamily, index: u64, entry_bytes: &[u8]) -> Result<()> {
        // Step 1: Validate CF is a log CF
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "append_log_entry only works on log CFs (SystemRaftLog, DataRaftLog)"
                    .to_string(),
            });
        }

        // Step 2: Get current last index
        let last_index = self.get_cached_last_log_index(cf)?;

        // Step 3: Validate sequential index
        let _expected_index = match last_index {
            None => {
                // First entry must be index 1
                if index != 1 {
                    return Err(StorageError::InvalidLogIndex {
                        cf: cf.as_str().to_string(),
                        expected: 1,
                        got: index,
                        reason: "First log entry must have index 1".to_string(),
                    });
                }
                1
            }
            Some(last) => {
                // Subsequent entries must be last + 1
                let expected = last + 1;
                if index != expected {
                    let reason = if index > expected {
                        format!("Gap in log (expected {}, got {})", expected, index)
                    } else {
                        format!("Duplicate index (last was {})", last)
                    };
                    return Err(StorageError::InvalidLogIndex {
                        cf: cf.as_str().to_string(),
                        expected,
                        got: index,
                        reason,
                    });
                }
                expected
            }
        };

        // Step 4: Format key
        let key = format_log_key(index);

        // Step 5: Write to RocksDB (no fsync for log CFs)
        let cf_handle = self.get_cf_handle(cf)?;
        let write_opts = WriteOptions::default();
        self.db
            .put_cf_opt(&cf_handle, key.as_bytes(), entry_bytes, &write_opts)?;

        // Step 6: Update cache
        self.update_cached_last_log_index(cf, index)?;

        Ok(())
    }

    /// Get range of log entries [start, end) (end exclusive).
    ///
    /// Reads log entries in the specified range. Missing indices are skipped
    /// (not an error). Returns entries in order.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    /// * `start` - Starting index (inclusive)
    /// * `end` - Ending index (exclusive)
    ///
    /// # Returns
    ///
    /// Vector of log entry bytes. May be shorter than (end - start) if entries are missing.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - CF is not a log CF (`InvalidColumnFamily`)
    /// - Database read fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// let entries = storage.get_log_range(ColumnFamily::SystemRaftLog, 1, 10)?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn get_log_range(&self, cf: ColumnFamily, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "get_log_range only works on log CFs (SystemRaftLog, DataRaftLog)"
                    .to_string(),
            });
        }

        let mut entries = Vec::new();
        let start_key = format_log_key(start);
        let mut iter = self.iterator(
            cf,
            IteratorMode::From(start_key.into_bytes(), Direction::Forward),
        )?;

        while iter.valid() {
            let key = match iter.key() {
                Some(k) => k,
                None => break,
            };

            if key.len() < 24 || &key[..4] != b"log:" {
                break;
            }

            let index_str =
                std::str::from_utf8(&key[4..24]).map_err(|_| StorageError::CorruptedData {
                    cf: cf.as_str().to_string(),
                    key: key.to_vec(),
                    reason: "Invalid log key format".to_string(),
                })?;
            let index: u64 = index_str.parse().map_err(|_| StorageError::CorruptedData {
                cf: cf.as_str().to_string(),
                key: key.to_vec(),
                reason: "Invalid log index".to_string(),
            })?;

            if index >= end {
                break;
            }

            if let Some(value) = iter.value() {
                entries.push(value.to_vec());
            }

            iter.step_forward()?;
        }

        Ok(entries)
    }

    /// Delete all log entries with index < truncate_index.
    ///
    /// Atomically removes all log entries before the specified index.
    /// If truncate_index > last_index, deletes entire log and invalidates cache.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    /// * `truncate_index` - Delete all entries with index < this value
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - CF is not a log CF (`InvalidColumnFamily`)
    /// - Database operation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// storage.truncate_log_before(ColumnFamily::SystemRaftLog, 100)?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn truncate_log_before(&self, cf: ColumnFamily, truncate_index: u64) -> Result<()> {
        // Step 1: Validate CF is a log CF
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "truncate_log_before only works on log CFs (SystemRaftLog, DataRaftLog)"
                    .to_string(),
            });
        }

        // Step 2: Get current last index
        let last_index = self.get_cached_last_log_index(cf)?;

        // Step 3: Check edge case - if truncate_index > last_index, delete entire log
        let should_invalidate_cache = match last_index {
            None => {
                // Empty log, nothing to do
                return Ok(());
            }
            Some(last) => {
                if truncate_index > last {
                    // Delete entire log
                    true
                } else {
                    // Partial truncation
                    false
                }
            }
        };

        // Step 4: Build WriteBatch with delete operations
        let mut batch = RocksDBWriteBatch::default();
        let cf_handle = self.get_cf_handle(cf)?;

        if should_invalidate_cache {
            // Delete all entries [1, last_index]
            // Safe: should_invalidate_cache is only true when last_index is Some
            let last =
                last_index.expect("BUG: should_invalidate_cache requires last_index to be Some");
            for index in 1..=last {
                let key = format_log_key(index);
                batch.delete_cf(&cf_handle, key.as_bytes());
            }
        } else {
            // Delete entries [1, truncate_index)
            for index in 1..truncate_index {
                let key = format_log_key(index);
                batch.delete_cf(&cf_handle, key.as_bytes());
            }
        }

        // Step 5: Execute batch atomically
        self.db.write(batch)?;

        // Step 6: Invalidate cache after truncation
        // After truncate, the cached last index is stale - either entries were deleted
        // (partial truncate) or all entries were deleted (full truncate).
        // Either way, we need to invalidate so get_last_log_index reads fresh from storage.
        let mut cache = self.last_log_index_cache.write();
        cache.remove(&cf);

        Ok(())
    }

    /// Get highest log index in CF.
    ///
    /// Uses cache for O(1) performance after first call.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    ///
    /// # Returns
    ///
    /// - Ok(Some(index)) - Last log index
    /// - Ok(None) - Log is empty
    ///
    /// # Errors
    ///
    /// Returns error if database read fails (on cache miss).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// if let Some(index) = storage.get_last_log_index(ColumnFamily::SystemRaftLog)? {
    ///     println!("Last index: {}", index);
    /// }
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn get_last_log_index(&self, cf: ColumnFamily) -> Result<Option<u64>> {
        // Simply call get_cached_last_log_index (public wrapper)
        self.get_cached_last_log_index(cf)
    }

    /// Get cached last log index (internal helper).
    ///
    /// Checks cache first. On cache miss, queries DB and updates cache.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family (must be SystemRaftLog or DataRaftLog)
    ///
    /// # Returns
    ///
    /// - Ok(Some(index)) - Last log index
    /// - Ok(None) - Log is empty
    ///
    /// # Errors
    ///
    /// Returns error if database read fails (on cache miss).
    fn get_cached_last_log_index(&self, cf: ColumnFamily) -> Result<Option<u64>> {
        // Step 1: Acquire read lock and check cache
        {
            let cache = self.last_log_index_cache.read();

            if let Some(&index) = cache.get(&cf) {
                // Cache hit - return immediately (O(1))
                return Ok(Some(index));
            }
        }
        // Read lock is dropped here

        // Step 2: Cache miss - query DB
        let index = self.get_last_log_index_from_db(cf)?;

        // Step 3: Update cache with result
        if let Some(idx) = index {
            let mut cache = self.last_log_index_cache.write();
            cache.insert(cf, idx);
        }

        // Step 4: Return index
        Ok(index)
    }

    /// Update cached last log index (internal helper).
    ///
    /// Updates cache with new index value.
    ///
    /// # Arguments
    ///
    /// * `cf` - Log column family
    /// * `new_index` - New last log index
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    pub fn update_cached_last_log_index(&self, cf: ColumnFamily, new_index: u64) -> Result<()> {
        let mut cache = self.last_log_index_cache.write();

        cache.insert(cf, new_index);

        // Step 3: Return Ok
        Ok(())
    }

    // ========================================================================
    // Batch Operations (ROCKS-007)
    // ========================================================================

    /// Execute atomic batch write across multiple column families.
    ///
    /// All operations in the batch succeed or all fail - no partial writes.
    /// If any column family in the batch requires fsync, the entire batch is fsynced.
    ///
    /// # Arguments
    ///
    /// * `batch` - WriteBatch containing operations to execute
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if any operation fails. No operations are applied on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, WriteBatch, ColumnFamily};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    /// let mut batch = WriteBatch::new();
    ///
    /// batch
    ///     .put(ColumnFamily::DataKv, b"key1", b"value1")
    ///     .put(ColumnFamily::DataKv, b"key2", b"value2")
    ///     .delete(ColumnFamily::DataKv, b"old_key");
    ///
    /// storage.batch_write(batch)?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn batch_write(&self, batch: WriteBatch) -> Result<()> {
        // Step 1: Handle empty batch (no-op)
        if batch.is_empty() {
            return Ok(());
        }

        // Step 1b: Check batch size limit to prevent memory exhaustion
        const MAX_BATCH_OPERATIONS: usize = 100_000;
        if batch.operations().len() > MAX_BATCH_OPERATIONS {
            return Err(StorageError::BatchTooLarge {
                operations: batch.operations().len(),
                max: MAX_BATCH_OPERATIONS,
            });
        }

        // Step 2: Create RocksDB WriteBatch
        let mut rocksdb_batch = RocksDBWriteBatch::default();

        // Step 3: Add all operations to RocksDB batch
        for op in batch.operations() {
            let cf_handle = self.get_cf_handle(op.cf())?;

            if op.is_put() {
                if let Some(value) = op.value() {
                    rocksdb_batch.put_cf(&cf_handle, op.key(), value);
                }
            } else {
                rocksdb_batch.delete_cf(&cf_handle, op.key());
            }
        }

        // Step 4: Check if any CF requires fsync
        let requires_fsync = batch.requires_fsync();

        // Step 5: Create WriteOptions with fsync flag if needed
        let mut write_opts = WriteOptions::default();
        if requires_fsync {
            write_opts.set_sync(true);
        }

        // Step 6: Execute batch atomically
        self.db.write_opt(rocksdb_batch, &write_opts)?;

        // Step 7: Return Ok(())
        Ok(())
    }

    // ========================================================================
    // Iterator Operations (ROCKS-008)
    // ========================================================================

    /// Create an iterator over a column family.
    ///
    /// Returns a snapshot-isolated iterator that provides a consistent view
    /// of the database at the time of creation. Concurrent writes do not
    /// affect the iterator's view.
    ///
    /// # Arguments
    ///
    /// * `cf` - Column family to iterate over
    /// * `mode` - Iterator mode (start position and direction)
    ///
    /// # Returns
    ///
    /// A StorageIterator instance
    ///
    /// # Errors
    ///
    /// Returns error if the column family cannot be found.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions, ColumnFamily};
    /// use seshat_storage::iterator::{IteratorMode, Direction};
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    ///
    /// // Iterate forward from start
    /// let mut iter = storage.iterator(ColumnFamily::DataKv, IteratorMode::Start)?;
    /// while let Some((key, value)) = iter.step_forward().unwrap() {
    ///     println!("Key: {:?}", key);
    /// }
    ///
    /// // Iterate from specific key
    /// let mut iter = storage.iterator(
    ///     ColumnFamily::DataKv,
    ///     IteratorMode::From(b"start_key".to_vec(), Direction::Forward)
    /// )?;
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    pub fn iterator(&self, cf: ColumnFamily, mode: IteratorMode) -> Result<StorageIterator<'_>> {
        let cf_handle = self.get_cf_handle(cf)?;
        let mut iter = self.db.raw_iterator_cf(&cf_handle);

        match mode {
            IteratorMode::Start => iter.seek_to_first(),
            IteratorMode::End => iter.seek_to_last(),
            IteratorMode::From(key, direction) => match direction {
                Direction::Forward => iter.seek(&key),
                Direction::Reverse => iter.seek_for_prev(&key),
            },
        };

        Ok(StorageIterator::new(iter, cf))
    }

    // ========================================================================
    // Snapshot Operations (ROCKS-009)
    // ========================================================================

    /// Create RocksDB checkpoint at path.
    ///
    /// Uses RocksDB's checkpoint feature to create a consistent snapshot using hard links.
    /// This is an O(1) operation that initially uses zero additional disk space.
    /// Space diverges as the original database changes (compaction, new writes).
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path where the checkpoint will be created
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Checkpoint directory already exists
    /// - Insufficient permissions to create checkpoint
    /// - Disk is full
    /// - RocksDB internal error
    ///
    /// # Performance
    ///
    /// - Time: O(1) - typically ~10ms regardless of database size
    /// - Space: Zero initially (hard links), diverges as DB changes
    /// - Does not block reads or writes during creation
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_storage::{Storage, StorageOptions};
    /// use std::path::Path;
    ///
    /// let storage = Storage::new(StorageOptions::default())?;
    ///
    /// // Create snapshot
    /// let snapshot_path = Path::new("/var/lib/seshat/snapshots/snapshot-001");
    /// storage.create_snapshot(snapshot_path)?;
    ///
    /// // Snapshot can now be opened as an independent database
    /// # Ok::<(), seshat_storage::StorageError>(())
    /// ```
    ///
    /// # Note
    ///
    /// The checkpoint includes all 6 column families automatically.
    /// The Raft crate is responsible for managing snapshot lifecycle
    /// (e.g., deleting old snapshots).
    pub fn create_snapshot(&self, path: &Path) -> Result<()> {
        // Step 0: Validate path is within data directory (security: prevent path traversal)
        let data_dir = self
            .path
            .canonicalize()
            .map_err(|e| StorageError::SnapshotFailed {
                path: self.path.display().to_string(),
                reason: format!("Cannot resolve data directory: {}", e),
            })?;

        // For the target path, we need to resolve it relative to its parent directory
        // since canonicalize() requires the path to exist
        let canonical_path = if path.exists() {
            path.canonicalize()
                .map_err(|e| StorageError::SnapshotFailed {
                    path: path.display().to_string(),
                    reason: format!("Cannot resolve path: {}", e),
                })?
        } else {
            // Path doesn't exist - try canonicalizing the full path
            // This handles paths with ".." that resolve to existing directories
            match path.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => {
                    // Fallback: resolve ".." components manually
                    let parent = path.parent().ok_or_else(|| StorageError::SnapshotFailed {
                        path: path.display().to_string(),
                        reason: "Path has no parent directory".to_string(),
                    })?;

                    // Try to canonicalize parent (handles ".." components)
                    let canonical_parent =
                        parent
                            .canonicalize()
                            .map_err(|e| StorageError::SnapshotFailed {
                                path: path.display().to_string(),
                                reason: format!("Cannot resolve parent directory: {}", e),
                            })?;

                    let filename =
                        path.file_name()
                            .ok_or_else(|| StorageError::SnapshotFailed {
                                path: path.display().to_string(),
                                reason: "Path has no filename component".to_string(),
                            })?;

                    canonical_parent.join(filename)
                }
            }
        };

        if !canonical_path.starts_with(&data_dir) {
            return Err(StorageError::SnapshotFailed {
                path: path.display().to_string(),
                reason: "Snapshot path must be within data directory".to_string(),
            });
        }

        // Note: TOCTOU mitigation
        // The canonical_path validation above ensures the target is within data_dir.
        // Even if filesystem state changes between check and create_checkpoint(),
        // an attacker cannot redirect to a path outside data_dir due to the
        // canonicalization. The use of hard links (not copies) also limits
        // the impact of any race condition.

        // Step 1: Create Checkpoint from DB
        let checkpoint = Checkpoint::new(&self.db)?;

        // Step 2: Call checkpoint.create_checkpoint(path)
        // This creates hard links to all SST files (~10ms, no data copy)
        checkpoint.create_checkpoint(path)?;

        // Step 3: Sync the parent directory to ensure checkpoint is durable
        if let Some(parent) = path.parent() {
            let dir = std::fs::OpenOptions::new().read(true).open(parent)?;
            dir.sync_all()?;
        }

        // Step 4: Return Ok(())
        Ok(())
    }
}

/// Helper function to format log key with zero-padding.
///
/// Formats a log index as "log:00000000000000000042" for correct
/// lexicographic ordering in RocksDB.
///
/// # Arguments
///
/// * `index` - Log index to format
///
/// # Returns
///
/// Formatted key string with 20-digit zero-padded index
///
/// # Examples
///
/// ```
/// # fn format_log_key(index: u64) -> String {
/// #     format!("log:{:020}", index)
/// # }
/// assert_eq!(format_log_key(0), "log:00000000000000000000");
/// assert_eq!(format_log_key(42), "log:00000000000000000042");
/// assert_eq!(format_log_key(u64::MAX), format!("log:{:020}", u64::MAX));
/// ```
fn format_log_key(index: u64) -> String {
    format!("log:{:020}", index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_format_log_key_zero_pads_correctly() {
        assert_eq!(format_log_key(0), "log:00000000000000000000");
        assert_eq!(format_log_key(1), "log:00000000000000000001");
        assert_eq!(format_log_key(42), "log:00000000000000000042");
        assert_eq!(format_log_key(100), "log:00000000000000000100");
        assert_eq!(format_log_key(u64::MAX), format!("log:{:020}", u64::MAX));
    }

    #[test]
    fn test_create_snapshot_rejects_path_traversal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let options = StorageOptions::with_data_dir(data_dir.clone());
        let storage = Storage::new(options).unwrap();

        // Attempt path traversal: try to escape to parent directory
        // We use a path that starts outside the data directory
        let malicious_path = temp_dir.path().join("../outside_snapshot");
        let result = storage.create_snapshot(&malicious_path);

        // Should be rejected with SnapshotFailed error
        assert!(result.is_err());
        match result {
            Err(StorageError::SnapshotFailed { path: _, reason }) => {
                // The error should indicate the path is outside the data directory
                // or that the path couldn't be resolved (which is also a rejection)
                assert!(
                    reason.contains("must be within data directory")
                        || reason.contains("Cannot resolve")
                );
            }
            _ => panic!("Expected SnapshotFailed error, got {:?}", result),
        }
    }

    #[test]
    fn test_create_snapshot_rejects_absolute_path_outside_data_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let options = StorageOptions::with_data_dir(data_dir);
        let storage = Storage::new(options).unwrap();

        // Attempt to write to completely different directory
        let outside_path = Path::new("/tmp/seshat_snapshot_outside");
        let result = storage.create_snapshot(outside_path);

        // Should be rejected
        assert!(result.is_err());
        match result {
            Err(StorageError::SnapshotFailed { reason, .. }) => {
                assert!(reason.contains("must be within data directory"));
            }
            _ => panic!("Expected SnapshotFailed error, got {:?}", result),
        }
    }
}
