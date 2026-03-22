//! Configuration options for RocksDB storage.
//!
//! This module provides:
//! - `StorageOptions` - Database-level configuration (paths, compression, memory limits)
//! - `CFOptions` - Per-column-family tuning (compaction, write buffers, prefix extractors)
//!
//! # Examples
//!
//! ```
//! use seshat_storage::{StorageOptions, CFOptions};
//! use std::path::PathBuf;
//!
//! // Use defaults
//! let opts = StorageOptions::default();
//!
//! // Or customize
//! let opts = StorageOptions::with_data_dir(PathBuf::from("/var/lib/seshat"));
//! ```

use crate::{ColumnFamily, Result, StorageError};
use rocksdb::{DBCompactionStyle, SliceTransform};
use std::collections::HashMap;
use std::path::PathBuf;

/// Database-level configuration options.
///
/// Controls RocksDB initialization, global resource limits, and default settings
/// that apply across all column families unless overridden.
///
/// # Fields
///
/// - `data_dir`: Path to RocksDB data directory
/// - `create_if_missing`: True for bootstrap mode, false for join mode
/// - `compression`: Compression algorithm (Lz4 recommended for Phase 1)
/// - `write_buffer_size_mb`: Size of memtable per CF before flush (64MB default)
/// - `max_write_buffer_number`: Number of memtables per CF (3 default)
/// - `target_file_size_mb`: Target SST file size (64MB default)
/// - `max_open_files`: Max open file handles (-1 = unlimited)
/// - `enable_statistics`: Enable RocksDB statistics (for Phase 4 observability)
/// - `cf_options`: Per-column-family option overrides
///
/// # Examples
///
/// ```
/// use seshat_storage::StorageOptions;
///
/// let opts = StorageOptions::default();
/// assert_eq!(opts.write_buffer_size_mb, 64);
/// assert_eq!(opts.max_write_buffer_number, 3);
/// ```
#[derive(Debug, Clone)]
pub struct StorageOptions {
    /// Path to RocksDB data directory (default: "./data/rocksdb")
    pub data_dir: PathBuf,

    /// Create database if it doesn't exist (true for bootstrap, false for join)
    pub create_if_missing: bool,

    /// Compression algorithm (Lz4 for Phase 1)
    pub compression: rocksdb::DBCompressionType,

    /// Write buffer (memtable) size in MB per CF (default: 64)
    pub write_buffer_size_mb: usize,

    /// Number of write buffers (memtables) per CF (default: 3)
    pub max_write_buffer_number: usize,

    /// Target SST file size in MB (default: 64)
    pub target_file_size_mb: usize,

    /// Maximum open file handles (-1 = unlimited, 0 = use OS default, N = limit to N files)
    /// Default: 1024 (reasonable limit to prevent file descriptor exhaustion)
    pub max_open_files: i32,

    /// Enable RocksDB statistics for observability (default: false)
    pub enable_statistics: bool,

    /// Per-column-family configuration overrides
    pub cf_options: HashMap<ColumnFamily, CFOptions>,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data/rocksdb"),
            create_if_missing: true,
            compression: rocksdb::DBCompressionType::Lz4,
            write_buffer_size_mb: 64,
            max_write_buffer_number: 3,
            target_file_size_mb: 64,
            max_open_files: 1024, // Reasonable limit to prevent file descriptor exhaustion
            enable_statistics: false,
            cf_options: Self::default_cf_options(),
        }
    }
}

impl StorageOptions {
    /// Creates options with a custom data directory.
    ///
    /// Uses default values for all other settings.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the RocksDB data directory
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::StorageOptions;
    /// use std::path::PathBuf;
    ///
    /// let opts = StorageOptions::with_data_dir(PathBuf::from("/var/lib/seshat"));
    /// assert_eq!(opts.data_dir, PathBuf::from("/var/lib/seshat"));
    /// ```
    pub fn with_data_dir(path: PathBuf) -> Self {
        Self {
            data_dir: path,
            ..Default::default()
        }
    }

    /// Validates configuration parameters.
    ///
    /// Checks that all settings are within acceptable ranges:
    /// - `write_buffer_size_mb`: 1-1024 MB
    /// - `max_write_buffer_number`: 2-10
    /// - `target_file_size_mb`: 1-1024 MB
    ///
    /// # Returns
    ///
    /// `Ok(())` if all settings are valid, `Err(StorageError::InvalidConfig)` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::StorageOptions;
    ///
    /// let opts = StorageOptions::default();
    /// assert!(opts.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        // Validate write_buffer_size_mb: 1-1024 MB
        if self.write_buffer_size_mb < 1 || self.write_buffer_size_mb > 1024 {
            return Err(StorageError::InvalidConfig {
                field: "write_buffer_size_mb".to_string(),
                reason: format!(
                    "must be between 1-1024 MB, got {}",
                    self.write_buffer_size_mb
                ),
            });
        }

        // Validate max_write_buffer_number: 2-10
        if self.max_write_buffer_number < 2 || self.max_write_buffer_number > 10 {
            return Err(StorageError::InvalidConfig {
                field: "max_write_buffer_number".to_string(),
                reason: format!("must be between 2-10, got {}", self.max_write_buffer_number),
            });
        }

        // Validate target_file_size_mb: 1-1024 MB
        if self.target_file_size_mb < 1 || self.target_file_size_mb > 1024 {
            return Err(StorageError::InvalidConfig {
                field: "target_file_size_mb".to_string(),
                reason: format!(
                    "must be between 1-1024 MB, got {}",
                    self.target_file_size_mb
                ),
            });
        }

        Ok(())
    }

    /// Returns default column family options for all CFs.
    ///
    /// Creates optimized settings based on column family type:
    /// - Raft Log CFs: Aggressive compaction (level0_trigger=2)
    /// - Raft State CFs: Manual compaction, small write buffers (4MB)
    /// - System Data CF: Manual compaction, medium write buffers (8MB)
    /// - Data KV CF: Auto compaction with 4-byte prefix extractor
    ///
    /// # Returns
    ///
    /// HashMap mapping each ColumnFamily to its default CFOptions.
    fn default_cf_options() -> HashMap<ColumnFamily, CFOptions> {
        let mut options = HashMap::new();

        // Raft Log CFs (SystemRaftLog, DataRaftLog)
        // Per design.md lines 265-271
        let raft_log_opts = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 2, // Aggressive compaction
            write_buffer_size: None,               // Use global setting
            prefix_extractor: None,
            prefix_length: 0,
        };

        options.insert(ColumnFamily::SystemRaftLog, raft_log_opts.clone());
        options.insert(ColumnFamily::DataRaftLog, raft_log_opts);

        // Raft State CFs (SystemRaftState, DataRaftState)
        // Per design.md lines 273-279
        let raft_state_opts = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: true, // Manual compaction only
            level0_file_num_compaction_trigger: 10,
            write_buffer_size: Some(4 * 1024 * 1024), // 4MB
            prefix_extractor: None,
            prefix_length: 0,
        };

        options.insert(ColumnFamily::SystemRaftState, raft_state_opts.clone());
        options.insert(ColumnFamily::DataRaftState, raft_state_opts);

        // System Data CF
        // Per design.md lines 281-287
        options.insert(
            ColumnFamily::SystemData,
            CFOptions {
                compaction_style: DBCompactionStyle::Level,
                disable_auto_compactions: true, // Manual compaction only
                level0_file_num_compaction_trigger: 10,
                write_buffer_size: Some(8 * 1024 * 1024), // 8MB
                prefix_extractor: None,
                prefix_length: 0,
            },
        );

        // Data KV CF
        // Per design.md lines 289-304
        options.insert(
            ColumnFamily::DataKv,
            CFOptions {
                compaction_style: DBCompactionStyle::Level,
                disable_auto_compactions: false, // Auto compaction enabled
                level0_file_num_compaction_trigger: 4,
                write_buffer_size: None, // Use global setting
                prefix_extractor: Some(SliceTransform::create_fixed_prefix(4)), // 4-byte prefix
                prefix_length: 4,
            },
        );

        options
    }
}

/// Per-column-family configuration options.
///
/// Controls compaction behavior, write buffer sizing, and optimization
/// strategies for individual column families.
///
/// # Fields
///
/// - `compaction_style`: Compaction strategy (Level, Universal, FIFO)
/// - `disable_auto_compactions`: Manual vs automatic compaction
/// - `level0_file_num_compaction_trigger`: L0 file count trigger
/// - `write_buffer_size`: Override global write buffer size (None = use global)
/// - `prefix_extractor`: Bloom filter prefix optimization (for DataKv)
///
/// # Examples
///
/// ```
/// use seshat_storage::CFOptions;
/// use rocksdb::DBCompactionStyle;
///
/// let opts = CFOptions {
///     compaction_style: DBCompactionStyle::Level,
///     disable_auto_compactions: false,
///     level0_file_num_compaction_trigger: 4,
///     write_buffer_size: None,
///     prefix_extractor: None,
///     prefix_length: 0,
/// };
/// ```
pub struct CFOptions {
    pub compaction_style: DBCompactionStyle,
    pub disable_auto_compactions: bool,
    pub level0_file_num_compaction_trigger: i32,
    pub write_buffer_size: Option<usize>,
    pub prefix_extractor: Option<SliceTransform>,
    pub prefix_length: usize,
}

impl Clone for CFOptions {
    fn clone(&self) -> Self {
        Self {
            compaction_style: self.compaction_style,
            disable_auto_compactions: self.disable_auto_compactions,
            level0_file_num_compaction_trigger: self.level0_file_num_compaction_trigger,
            write_buffer_size: self.write_buffer_size,
            prefix_extractor: self
                .prefix_extractor
                .as_ref()
                .map(|_| SliceTransform::create_fixed_prefix(self.prefix_length)),
            prefix_length: self.prefix_length,
        }
    }
}

impl std::fmt::Debug for CFOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOptions")
            .field("compaction_style", &self.compaction_style)
            .field("disable_auto_compactions", &self.disable_auto_compactions)
            .field(
                "level0_file_num_compaction_trigger",
                &self.level0_file_num_compaction_trigger,
            )
            .field("write_buffer_size", &self.write_buffer_size)
            .field("prefix_extractor", &self.prefix_extractor.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::SliceTransform;

    #[test]
    fn test_cfoptions_clone_preserves_prefix_length() {
        let original = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 4,
            write_buffer_size: None,
            prefix_extractor: Some(SliceTransform::create_fixed_prefix(4)),
            prefix_length: 4,
        };

        let cloned = original.clone();

        assert_eq!(cloned.prefix_length, 4);
        assert!(cloned.prefix_extractor.is_some());
        assert_eq!(cloned.compaction_style, original.compaction_style);
        assert_eq!(
            cloned.disable_auto_compactions,
            original.disable_auto_compactions
        );
        assert_eq!(
            cloned.level0_file_num_compaction_trigger,
            original.level0_file_num_compaction_trigger
        );
        assert_eq!(cloned.write_buffer_size, original.write_buffer_size);
    }

    #[test]
    fn test_cfoptions_clone_preserves_different_prefix_lengths() {
        for prefix_len in [1, 4, 8, 16, 32] {
            let original = CFOptions {
                compaction_style: DBCompactionStyle::Level,
                disable_auto_compactions: false,
                level0_file_num_compaction_trigger: 4,
                write_buffer_size: None,
                prefix_extractor: Some(SliceTransform::create_fixed_prefix(prefix_len)),
                prefix_length: prefix_len,
            };

            let cloned = original.clone();
            assert_eq!(
                cloned.prefix_length, prefix_len,
                "Clone should preserve prefix_length={}",
                prefix_len
            );
        }
    }

    #[test]
    fn test_cfoptions_clone_no_prefix() {
        let original = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 4,
            write_buffer_size: None,
            prefix_extractor: None,
            prefix_length: 0,
        };

        let cloned = original.clone();

        assert_eq!(cloned.prefix_length, 0);
        assert!(cloned.prefix_extractor.is_none());
    }
}
