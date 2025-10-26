# RocksDB Storage Layer: Architectural Solutions

**Date**: 2025-10-26
**Author**: Principal Engineer (Architect Agent)
**Purpose**: Address critical architectural issues in RocksDB specification

---

## Executive Summary

This document provides implementation-ready solutions for two critical architectural issues identified in the RocksDB storage layer specification review:

1. **Issue 2**: Inconsistent data structure dependencies and unclear serialization boundaries
2. **Issue 3**: Missing atomic log index management and validation logic

Both issues are resolved with clear crate boundaries, explicit API contracts, and detailed implementation strategies compatible with openraft's storage traits.

---

## Issue 2: Crate Boundary Clarification

### Problem Statement

The current design creates ambiguity by:
- Stating storage "stores bytes, no knowledge of data structures" (line 599 in design.md)
- Referencing specific types like `VersionedLogEntry`, `RaftHardState` throughout storage APIs
- Unclear WHERE serialization/deserialization happens
- Mixed responsibilities between storage and raft crates

### Root Cause Analysis

The confusion stems from documenting storage operations with high-level type names (e.g., `VersionedLogEntry`) when the storage crate should only deal with raw bytes. This makes it unclear:

1. Does storage crate depend on `common` crate for type definitions?
2. Who is responsible for serialization: storage or raft?
3. What happens if storage needs to inspect data (e.g., for validation)?

### Architectural Solution

#### 1. Strict Crate Boundary Definition

**The Golden Rule**: Storage crate operates exclusively on `&[u8]` and `Vec<u8>`. Zero knowledge of domain types.

```
┌─────────────────────────────────────────────────────────────┐
│  Raft Crate (seshat-raft)                                   │
│  - Owns: VersionedLogEntry, RaftHardState, SnapshotMetadata │
│  - Responsibility: Serialize/deserialize using bincode       │
│  - Calls: storage.put(cf, key, &serialized_bytes)           │
└─────────────────────────────────────────────────────────────┘
                             │
                             │ Pure byte interface
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  Storage Crate (seshat-storage)                             │
│  - Owns: Storage, ColumnFamily, WriteBatch, StorageIterator │
│  - Responsibility: RocksDB operations on bytes               │
│  - NO dependency on common crate types                       │
│  - API accepts only: &[u8], Vec<u8>                         │
└─────────────────────────────────────────────────────────────┘
                             │
                             │ RocksDB API
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  RocksDB (rocksdb crate)                                    │
└─────────────────────────────────────────────────────────────┘
```

#### 2. Revised Storage API (Purely Bytes)

**File**: `crates/storage/src/lib.rs`

```rust
/// Pure persistence layer - operates only on bytes
pub struct Storage {
    db: Arc<DB>,
    cf_handles: HashMap<ColumnFamily, Arc<BoundColumnFamily<'static>>>,
    config: StorageOptions,
}

impl Storage {
    // ============================================================
    // BASIC OPERATIONS - Pure byte interface
    // ============================================================

    /// Get value by key. Returns None if key doesn't exist.
    ///
    /// # Arguments
    /// * `cf` - Column family to read from
    /// * `key` - Raw key bytes
    ///
    /// # Returns
    /// * `Ok(Some(value))` - Key exists, returns value bytes
    /// * `Ok(None)` - Key does not exist
    /// * `Err(_)` - RocksDB error
    pub fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Put key-value pair.
    ///
    /// # Durability
    /// If `cf.requires_fsync()` is true (raft_state CFs), this call will
    /// not return until data is synced to disk.
    pub fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete key.
    pub fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()>;

    /// Check if key exists (uses bloom filter optimization).
    pub fn exists(&self, cf: ColumnFamily, key: &[u8]) -> Result<bool>;

    // ============================================================
    // BATCH OPERATIONS - Atomic multi-operation commits
    // ============================================================

    /// Execute atomic batch write across multiple column families.
    ///
    /// All operations succeed or all fail. No partial writes visible.
    /// If any CF in batch requires fsync, entire batch is synced.
    pub fn batch_write(&self, batch: WriteBatch) -> Result<()>;

    // ============================================================
    // LOG OPERATIONS - Sequential index management
    // ============================================================

    /// Append log entry with automatic index validation.
    ///
    /// # Index Validation
    /// - First entry: `index` must be 1
    /// - Subsequent entries: `index` must be `last_index + 1`
    /// - Gaps or duplicates return `Err(StorageError::InvalidLogIndex)`
    ///
    /// # Atomicity
    /// Validation and write are NOT atomic. Concurrent appends may cause
    /// race conditions. Caller (raft crate) must serialize append calls.
    ///
    /// # Arguments
    /// * `cf` - Log column family (system_raft_log or data_raft_log)
    /// * `index` - Log index (must be sequential, starts at 1)
    /// * `entry_bytes` - Serialized log entry (caller handles serialization)
    ///
    /// # Returns
    /// * `Ok(())` - Entry appended successfully
    /// * `Err(InvalidLogIndex)` - Index validation failed
    /// * `Err(RocksDb(_))` - Underlying storage error
    pub fn append_log_entry(
        &self,
        cf: ColumnFamily,
        index: u64,
        entry_bytes: &[u8],
    ) -> Result<()>;

    /// Get range of log entries [start, end) (end exclusive).
    ///
    /// # Returns
    /// Vec of serialized entries in index order. Empty vec if no entries.
    /// Missing indices in range are NOT included (returns only existing entries).
    pub fn get_log_range(
        &self,
        cf: ColumnFamily,
        start: u64,
        end: u64,
    ) -> Result<Vec<Vec<u8>>>;

    /// Delete all log entries with index < truncate_index.
    ///
    /// Used for log compaction after snapshot. Atomic operation.
    pub fn truncate_log_before(&self, cf: ColumnFamily, truncate_index: u64) -> Result<()>;

    /// Get highest log index in CF.
    ///
    /// # Returns
    /// * `Ok(Some(index))` - Highest index found
    /// * `Ok(None)` - No entries in log (empty log)
    pub fn get_last_log_index(&self, cf: ColumnFamily) -> Result<Option<u64>>;

    // ============================================================
    // SNAPSHOT OPERATIONS
    // ============================================================

    /// Create RocksDB checkpoint at path.
    ///
    /// Uses hard links - O(1) time, initially zero additional space.
    pub fn create_snapshot(&self, path: &Path) -> Result<()>;

    /// Restore from checkpoint directory.
    ///
    /// CAUTION: Replaces current DB state. Backup before calling.
    pub fn restore_snapshot(&self, path: &Path) -> Result<()>;

    // ============================================================
    // UTILITIES
    // ============================================================

    /// Create iterator for range scans within CF.
    pub fn iterator(&self, cf: ColumnFamily, mode: IteratorMode) -> Result<StorageIterator>;

    /// Force fsync all pending writes.
    pub fn sync(&self) -> Result<()>;

    /// Manual compaction for CF in range [start, end).
    pub fn compact_range(
        &self,
        cf: ColumnFamily,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<()>;
}
```

**Key Changes**:
1. All comments reference "bytes" not specific types
2. `append_log_entry` takes `entry_bytes: &[u8]` not `entry: &VersionedLogEntry`
3. `get_log_range` returns `Vec<Vec<u8>>` not `Vec<VersionedLogEntry>`
4. Zero mention of domain types in API surface

#### 3. Raft Crate Integration Pattern

**File**: `crates/raft/src/storage_adapter.rs`

```rust
use seshat_common::{VersionedLogEntry, RaftHardState, SnapshotMetadata};
use seshat_storage::{Storage, ColumnFamily, WriteBatch, StorageError};

/// Adapter that implements openraft storage traits using seshat-storage.
///
/// This struct OWNS serialization/deserialization logic.
pub struct RaftStorageAdapter {
    storage: Storage,
    shard_id: u64,
}

impl RaftStorageAdapter {
    pub fn new(storage: Storage, shard_id: u64) -> Self {
        Self { storage, shard_id }
    }

    /// Select appropriate log CF based on shard_id.
    fn log_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftLog
        } else {
            ColumnFamily::DataRaftLog
        }
    }

    /// Select appropriate state CF based on shard_id.
    fn state_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftState
        } else {
            ColumnFamily::DataRaftState
        }
    }

    /// Append log entries with serialization.
    ///
    /// Serializes entries and delegates to storage layer.
    pub async fn append_entries(&self, entries: Vec<VersionedLogEntry>) -> Result<()> {
        let cf = self.log_cf();

        for entry in entries {
            // SERIALIZATION HAPPENS HERE in raft crate using protobuf
            let entry_bytes = entry.encode_to_vec();

            // Storage receives pure bytes
            self.storage.append_log_entry(cf, entry.index, &entry_bytes)?;
        }

        Ok(())
    }

    /// Get log entries with deserialization.
    pub async fn get_entries(&self, start: u64, end: u64) -> Result<Vec<VersionedLogEntry>> {
        let cf = self.log_cf();

        // Storage returns pure bytes
        let entry_bytes_vec = self.storage.get_log_range(cf, start, end)?;

        // DESERIALIZATION HAPPENS HERE in raft crate using protobuf
        let entries = entry_bytes_vec
            .into_iter()
            .map(|bytes| {
                VersionedLogEntry::decode(&bytes[..])
                    .map_err(|e| RaftError::Deserialization(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Save hard state with serialization.
    pub async fn save_hard_state(&self, hard_state: RaftHardState) -> Result<()> {
        let cf = self.state_cf();

        // SERIALIZATION HAPPENS HERE using protobuf
        let bytes = hard_state.encode_to_vec();

        // Storage receives pure bytes, handles fsync automatically
        self.storage.put(cf, b"state", &bytes)?;

        Ok(())
    }

    /// Load hard state with deserialization.
    pub async fn load_hard_state(&self) -> Result<Option<RaftHardState>> {
        let cf = self.state_cf();

        // Storage returns pure bytes
        let bytes = self.storage.get(cf, b"state")?;

        match bytes {
            Some(b) => {
                // DESERIALIZATION HAPPENS HERE using protobuf
                let state = RaftHardState::decode(&b[..])
                .map_err(|e| RaftError::Deserialization(e.to_string()))?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }
}
```

**Call Flow Example - Log Append**:

```
┌────────────────────────────────────────────────────────────────────┐
│ 1. openraft::Raft                                                  │
│    raft.append_entries(entries: Vec<Entry>)                        │
└────────────────────────────────────────────────────────────────────┘
                              │
                              │ Calls storage trait method
                              ▼
┌────────────────────────────────────────────────────────────────────┐
│ 2. RaftStorageAdapter (raft crate)                                │
│    - OWNS: VersionedLogEntry struct definition                     │
│    - CONVERTS: Entry → VersionedLogEntry                           │
│    - SERIALIZES: bincode::serialize(&entry) → Vec<u8>              │
└────────────────────────────────────────────────────────────────────┘
                              │
                              │ Calls storage API with bytes
                              ▼
┌────────────────────────────────────────────────────────────────────┐
│ 3. Storage (storage crate)                                         │
│    storage.append_log_entry(cf, index, &[u8])                     │
│    - VALIDATES: Sequential index                                   │
│    - WRITES: RocksDB put with key "log:{index}"                    │
│    - NO KNOWLEDGE: of VersionedLogEntry structure                  │
└────────────────────────────────────────────────────────────────────┘
                              │
                              │ RocksDB API
                              ▼
┌────────────────────────────────────────────────────────────────────┐
│ 4. RocksDB                                                         │
│    db.put_cf(cf_handle, b"log:142", serialized_bytes)             │
└────────────────────────────────────────────────────────────────────┘
```

#### 4. Responsibility Matrix

| Responsibility | Storage Crate | Raft Crate | Common Crate |
|----------------|---------------|------------|--------------|
| **Data Types** | ColumnFamily, WriteBatch, StorageIterator, StorageOptions | VersionedLogEntry, RaftHardState, SnapshotMetadata | Type definitions shared across crates |
| **Serialization** | ❌ No | ✅ Yes (bincode) | N/A |
| **Deserialization** | ❌ No | ✅ Yes (bincode) | N/A |
| **Raft Semantics** | ❌ No | ✅ Yes | N/A |
| **Index Validation** | ✅ Yes (sequential) | ❌ No | N/A |
| **Fsync Logic** | ✅ Yes (per CF) | ❌ No | N/A |
| **Batch Atomicity** | ✅ Yes | ❌ No | N/A |
| **Column Family Selection** | ❌ No | ✅ Yes | N/A |
| **Snapshot Metadata** | ❌ No | ✅ Yes | N/A |
| **Error Handling** | RocksDB errors | Raft errors | Common error types |

#### 5. Dependency Graph

```
seshat-storage (crates/storage)
├── Dependencies:
│   ├── rocksdb = "0.22"
│   ├── thiserror = "1.0"
│   └── tracing = "0.1"
└── NO dependency on seshat-common

seshat-raft (crates/raft)
├── Dependencies:
│   ├── seshat-storage (local)
│   ├── seshat-common (local)
│   ├── openraft = "0.10"
│   ├── prost = "0.12"
│   ├── serde = { version = "1.0", features = ["derive"] }
│   └── tokio = { version = "1", features = ["full"] }

seshat-common (crates/common)
├── Dependencies:
│   ├── prost = "0.12"
│   ├── serde = { version = "1.0", features = ["derive"] }
│   └── thiserror = "1.0"
```

#### 6. Testing Strategy for Boundaries

**Unit Test - Storage Crate** (`crates/storage/tests/byte_interface_test.rs`):

```rust
#[test]
fn test_storage_accepts_arbitrary_bytes() {
    let storage = Storage::new(test_options()).unwrap();

    // Storage should accept ANY bytes without caring about structure
    let random_bytes: Vec<u8> = vec![0xFF, 0xAB, 0xCD, 0x12, 0x34];

    storage.put(ColumnFamily::DataKv, b"test_key", &random_bytes).unwrap();

    let retrieved = storage.get(ColumnFamily::DataKv, b"test_key").unwrap();
    assert_eq!(retrieved, Some(random_bytes));

    // Storage has NO IDEA if this is valid VersionedLogEntry or garbage
}

#[test]
fn test_storage_does_not_validate_structure() {
    let storage = Storage::new(test_options()).unwrap();

    // This is NOT a valid VersionedLogEntry, but storage doesn't care
    let invalid_bytes = vec![0x00];

    // Should succeed - storage doesn't validate
    storage.append_log_entry(ColumnFamily::DataRaftLog, 1, &invalid_bytes).unwrap();

    let retrieved = storage.get_log_range(ColumnFamily::DataRaftLog, 1, 2).unwrap();
    assert_eq!(retrieved, vec![invalid_bytes]);
}
```

**Integration Test - Raft Crate** (`crates/raft/tests/serialization_boundary_test.rs`):

```rust
#[tokio::test]
async fn test_raft_adapter_handles_serialization() {
    let storage = Storage::new(test_options()).unwrap();
    let adapter = RaftStorageAdapter::new(storage, 0);

    // Create properly typed entry
    let entry = VersionedLogEntry {
        version: 1,
        term: 5,
        index: 1,
        entry_type: EntryType::Normal,
        data: vec![1, 2, 3],
    };

    // Adapter serializes before passing to storage
    adapter.append_entries(vec![entry.clone()]).await.unwrap();

    // Adapter deserializes when retrieving
    let retrieved = adapter.get_entries(1, 2).await.unwrap();

    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].index, 1);
    assert_eq!(retrieved[0].term, 5);
}

#[tokio::test]
async fn test_storage_deserialization_error_bubbles_up() {
    let storage = Storage::new(test_options()).unwrap();

    // Corrupt storage by writing invalid bytes directly
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF];
    storage.append_log_entry(ColumnFamily::SystemRaftLog, 1, &invalid_bytes).unwrap();

    // Adapter should fail with deserialization error
    let adapter = RaftStorageAdapter::new(storage, 0);
    let result = adapter.get_entries(1, 2).await;

    assert!(matches!(result, Err(RaftError::Deserialization(_))));
}
```

---

## Issue 3: Atomic Log Index Management

### Problem Statement

The `append_log_entry` method defines `InvalidLogIndex` error but doesn't specify:
1. Exact validation logic (what makes an index invalid?)
2. How to handle concurrent appends
3. How to track last_log_index efficiently
4. Edge cases: first entry, gaps, duplicates
5. Recovery strategies for validation failures

### Root Cause Analysis

Log index validation is critical for Raft safety:
- **Gaps** in log create inconsistencies (log[5] exists but log[4] missing)
- **Duplicates** indicate potential data corruption or race conditions
- **Non-sequential** writes violate Raft's sequential log guarantee

Current design mentions validation but doesn't define the "how".

### Architectural Solution

#### 1. Index Validation Algorithm

**File**: `crates/storage/src/lib.rs` (append_log_entry implementation)

```rust
impl Storage {
    /// Append log entry with sequential index validation.
    ///
    /// # Index Rules
    /// 1. First entry: index MUST be 1 (not 0, Raft logs are 1-indexed)
    /// 2. Subsequent entries: index MUST equal last_index + 1
    /// 3. Gaps or duplicates return InvalidLogIndex error
    ///
    /// # Concurrency
    /// This method is NOT thread-safe for concurrent appends to same CF.
    /// Caller must serialize append calls (single writer pattern).
    ///
    /// # Performance
    /// - Caches last_log_index in memory (updated on each append)
    /// - On first call or cache miss, queries RocksDB for last key
    /// - Validation is O(1) using cached index
    ///
    /// # Error Recovery
    /// - InvalidLogIndex: Caller should re-sync log from leader
    /// - RocksDB errors: Retry with exponential backoff
    pub fn append_log_entry(
        &self,
        cf: ColumnFamily,
        index: u64,
        entry_bytes: &[u8],
    ) -> Result<()> {
        // CRITICAL: Validate CF is a log column family
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "append_log_entry only works on *_raft_log CFs".to_string(),
            });
        }

        // Get or initialize last log index for this CF
        let current_last_index = self.get_cached_last_log_index(cf)?;

        // Validate sequential index
        let expected_index = match current_last_index {
            None => {
                // Empty log - first entry must be index 1
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
                let expected = last + 1;
                if index != expected {
                    // Gap or duplicate detected
                    let reason = if index <= last {
                        format!("Duplicate index (last was {})", last)
                    } else {
                        format!("Gap in log (expected {}, got {})", expected, index)
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

        // Build key: "log:{index}"
        let key = format_log_key(index);

        // Write to RocksDB
        let cf_handle = self.get_cf_handle(cf)?;

        // Apply fsync if this CF requires it (it doesn't for log CFs, only state CFs)
        let write_opts = WriteOptions::default();

        self.db.put_cf_opt(cf_handle, key.as_bytes(), entry_bytes, &write_opts)
            .map_err(|e| StorageError::RocksDb(e))?;

        // Update cached last index AFTER successful write
        self.update_cached_last_log_index(cf, index)?;

        tracing::debug!(
            cf = cf.as_str(),
            index = index,
            size_bytes = entry_bytes.len(),
            "Appended log entry"
        );

        Ok(())
    }

    /// Get cached last log index, initializing from RocksDB if needed.
    ///
    /// # Returns
    /// - `Ok(Some(index))` - Last index found (from cache or DB)
    /// - `Ok(None)` - Empty log
    /// - `Err(_)` - RocksDB error
    fn get_cached_last_log_index(&self, cf: ColumnFamily) -> Result<Option<u64>> {
        // Check in-memory cache first
        let cache_guard = self.last_log_index_cache.read().unwrap();

        if let Some(&cached_index) = cache_guard.get(&cf) {
            return Ok(Some(cached_index));
        }

        drop(cache_guard); // Release read lock before expensive DB operation

        // Cache miss - query RocksDB for last key
        let last_index = self.get_last_log_index_from_db(cf)?;

        // Update cache
        if let Some(index) = last_index {
            let mut cache_guard = self.last_log_index_cache.write().unwrap();
            cache_guard.insert(cf, index);
        }

        Ok(last_index)
    }

    /// Query RocksDB for last log index (expensive operation).
    fn get_last_log_index_from_db(&self, cf: ColumnFamily) -> Result<Option<u64>> {
        let cf_handle = self.get_cf_handle(cf)?;

        // Create reverse iterator (starts at end of CF)
        let mut iter = self.db.iterator_cf(cf_handle, IteratorMode::End);

        // Get last key-value pair
        match iter.next() {
            Some(Ok((key_bytes, _))) => {
                // Parse key: "log:12345" -> 12345
                let key_str = String::from_utf8_lossy(&key_bytes);

                if let Some(index_str) = key_str.strip_prefix("log:") {
                    let index = index_str.parse::<u64>()
                        .map_err(|e| StorageError::CorruptedData {
                            cf: cf.as_str().to_string(),
                            key: key_bytes.to_vec(),
                            reason: format!("Invalid log key format: {}", e),
                        })?;

                    Ok(Some(index))
                } else {
                    // Key doesn't match expected format
                    Err(StorageError::CorruptedData {
                        cf: cf.as_str().to_string(),
                        key: key_bytes.to_vec(),
                        reason: "Log key missing 'log:' prefix".to_string(),
                    })
                }
            }
            Some(Err(e)) => Err(StorageError::RocksDb(e)),
            None => Ok(None), // Empty CF
        }
    }

    /// Update cached last log index after successful append.
    fn update_cached_last_log_index(&self, cf: ColumnFamily, new_index: u64) -> Result<()> {
        let mut cache_guard = self.last_log_index_cache.write().unwrap();
        cache_guard.insert(cf, new_index);
        Ok(())
    }
}

/// Format log key from index with zero-padding for correct lexicographic ordering.
///
/// # Format
/// Returns a 24-byte key: "log:" (4 bytes) + zero-padded index (20 digits)
/// Example: index 1 -> "log:00000000000000000001"
///
/// # Rationale
/// RocksDB uses lexicographic byte ordering. Without zero-padding:
/// - Wrong order: "log:1", "log:10", "log:100", "log:2" (incorrect!)
/// - Correct order: "log:00000000000000000001", "log:00000000000000000002", ... "log:00000000000000000010"
///
/// 20 digits supports indices up to 10^20 - 1 (far beyond practical limits).
fn format_log_key(index: u64) -> String {
    format!("log:{:020}", index)
}

/// Parse log index from key bytes.
///
/// # Format
/// Expects "log:{20-digit-zero-padded-index}"
/// Example: "log:00000000000000000042" -> 42
fn parse_log_index(key: &[u8]) -> Result<u64> {
    let key_str = String::from_utf8_lossy(key);

    key_str
        .strip_prefix("log:")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| StorageError::CorruptedData {
            cf: "unknown".to_string(),
            key: key.to_vec(),
            reason: "Invalid log key format (expected 'log:{20-digit-index}')".to_string(),
        })
}
```

#### 2. Storage Struct with Index Cache

**File**: `crates/storage/src/lib.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Storage instance with cached last log indices.
pub struct Storage {
    db: Arc<DB>,
    cf_handles: HashMap<ColumnFamily, Arc<BoundColumnFamily<'static>>>,
    config: StorageOptions,

    /// Cache of last log index per CF.
    /// Key: ColumnFamily (only log CFs)
    /// Value: Last appended log index
    ///
    /// Synchronized with RwLock:
    /// - Read lock for cached lookups (hot path)
    /// - Write lock for cache updates (after successful append)
    ///
    /// Cache is invalidated on:
    /// - truncate_log_before (resets to new last index)
    /// - restore_snapshot (clears cache, forces re-query)
    last_log_index_cache: Arc<RwLock<HashMap<ColumnFamily, u64>>>,
}

impl Storage {
    pub fn new(options: StorageOptions) -> Result<Self> {
        // ... existing initialization code ...

        let storage = Self {
            db: Arc::new(db),
            cf_handles,
            config: options,
            last_log_index_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Warm up cache by querying last index for each log CF
        storage.warm_up_index_cache()?;

        Ok(storage)
    }

    /// Pre-populate index cache on startup.
    fn warm_up_index_cache(&self) -> Result<()> {
        let log_cfs = vec![
            ColumnFamily::SystemRaftLog,
            ColumnFamily::DataRaftLog,
        ];

        for cf in log_cfs {
            if let Some(last_index) = self.get_last_log_index_from_db(cf)? {
                let mut cache = self.last_log_index_cache.write().unwrap();
                cache.insert(cf, last_index);

                tracing::info!(
                    cf = cf.as_str(),
                    last_index = last_index,
                    "Warmed up log index cache"
                );
            }
        }

        Ok(())
    }

    /// Clear index cache (called after snapshot restoration).
    pub(crate) fn invalidate_index_cache(&self) {
        let mut cache = self.last_log_index_cache.write().unwrap();
        cache.clear();
        tracing::debug!("Invalidated log index cache");
    }
}
```

#### 3. Edge Case Handling

**Scenario 1: First Entry in Empty Log**

```rust
// Initial state: empty log
assert_eq!(storage.get_last_log_index(ColumnFamily::DataRaftLog)?, None);

// First append MUST use index 1
storage.append_log_entry(ColumnFamily::DataRaftLog, 1, b"entry_1")?; // ✅ OK

// This would fail (index 0 not allowed)
storage.append_log_entry(ColumnFamily::DataRaftLog, 0, b"entry_0")?; // ❌ Error
```

**Scenario 2: Detecting Gap**

```rust
// State: last_index = 5
storage.append_log_entry(ColumnFamily::DataRaftLog, 5, b"entry_5")?;

// Next append MUST be index 6
storage.append_log_entry(ColumnFamily::DataRaftLog, 7, b"entry_7")?; // ❌ Error

// Error returned:
// InvalidLogIndex { expected: 6, got: 7, reason: "Gap in log (expected 6, got 7)" }
```

**Scenario 3: Detecting Duplicate**

```rust
// State: last_index = 10
storage.append_log_entry(ColumnFamily::DataRaftLog, 10, b"entry_10")?;

// Duplicate append
storage.append_log_entry(ColumnFamily::DataRaftLog, 10, b"entry_10_dup")?; // ❌ Error

// Error returned:
// InvalidLogIndex { expected: 11, got: 10, reason: "Duplicate index (last was 10)" }
```

**Scenario 4: Log Truncation (Compaction)**

```rust
// State: log indices [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
// Last index = 10

// Compact: delete entries before index 8 (keep 8, 9, 10)
storage.truncate_log_before(ColumnFamily::DataRaftLog, 8)?;

// After truncation: log indices [8, 9, 10]
// Last index still = 10

// Next append continues from 10
storage.append_log_entry(ColumnFamily::DataRaftLog, 11, b"entry_11")?; // ✅ OK
```

**Scenario 5: Concurrent Append Attempts (Race Condition)**

```rust
// Thread 1 and Thread 2 both try to append index 6
// State: last_index = 5

// Thread 1: Gets cached last_index = 5, expects index 6
// Thread 2: Gets cached last_index = 5, expects index 6 (same!)

// Thread 1 writes first:
storage.append_log_entry(ColumnFamily::DataRaftLog, 6, b"thread1_data")?; // ✅ OK
// Cache updated: last_index = 6

// Thread 2 writes next:
storage.append_log_entry(ColumnFamily::DataRaftLog, 6, b"thread2_data")?; // ❌ Error
// Error: InvalidLogIndex { expected: 7, got: 6, reason: "Duplicate index (last was 6)" }

// SOLUTION: Caller (raft crate) MUST serialize append calls
```

#### 4. Truncate Implementation with Cache Update

**File**: `crates/storage/src/lib.rs`

```rust
impl Storage {
    /// Delete all log entries with index < truncate_index.
    ///
    /// # Atomicity
    /// Uses WriteBatch for atomic deletion of all matching keys.
    ///
    /// # Cache Behavior
    /// Does NOT modify cache unless entire log is deleted.
    /// Cache remains valid because last_index doesn't change
    /// (we only delete prefix of log).
    pub fn truncate_log_before(&self, cf: ColumnFamily, truncate_index: u64) -> Result<()> {
        if !cf.is_log_cf() {
            return Err(StorageError::InvalidColumnFamily {
                cf: cf.as_str().to_string(),
                reason: "truncate_log_before only works on *_raft_log CFs".to_string(),
            });
        }

        // Get current last index
        let last_index = self.get_cached_last_log_index(cf)?;

        // Edge case: truncate entire log
        if let Some(last) = last_index {
            if truncate_index > last {
                // Truncating past end of log - delete everything
                tracing::warn!(
                    cf = cf.as_str(),
                    truncate_index = truncate_index,
                    last_index = last,
                    "Truncating entire log (truncate_index > last_index)"
                );

                // Build batch to delete all entries
                let mut batch = WriteBatch::default();
                let cf_handle = self.get_cf_handle(cf)?;

                for index in 1..=last {
                    let key = format_log_key(index);
                    batch.delete_cf(cf_handle, key.as_bytes());
                }

                self.db.write(batch)?;

                // Invalidate cache - log is now empty
                let mut cache = self.last_log_index_cache.write().unwrap();
                cache.remove(&cf);

                tracing::info!(cf = cf.as_str(), "Truncated entire log, cache invalidated");
                return Ok(());
            }
        }

        // Normal case: truncate prefix [1, truncate_index)
        let mut batch = WriteBatch::default();
        let cf_handle = self.get_cf_handle(cf)?;

        let mut deleted_count = 0;
        for index in 1..truncate_index {
            let key = format_log_key(index);
            batch.delete_cf(cf_handle, key.as_bytes());
            deleted_count += 1;
        }

        // Atomic commit
        self.db.write(batch)?;

        tracing::info!(
            cf = cf.as_str(),
            truncate_index = truncate_index,
            deleted_count = deleted_count,
            "Truncated log prefix"
        );

        // Cache remains valid (last_index unchanged)
        Ok(())
    }
}
```

#### 5. Concurrency Model

**Single Writer Pattern (Enforced by Raft)**:

```rust
// In raft crate - RaftStorageAdapter
impl RaftStorageAdapter {
    /// Append entries with internal mutex to prevent concurrent appends.
    ///
    /// openraft guarantees sequential calls, but we add defensive mutex.
    pub async fn append_entries(&self, entries: Vec<VersionedLogEntry>) -> Result<()> {
        // Acquire lock to ensure sequential appends
        let _guard = self.append_lock.lock().await;

        for entry in entries {
            let entry_bytes = bincode::serialize(&entry)?;
            self.storage.append_log_entry(self.log_cf(), entry.index, &entry_bytes)?;
        }

        Ok(())
    }
}
```

**Why Single Writer is Sufficient**:
1. **openraft** guarantees: Only leader appends to its own log
2. Followers receive entries via RPC and append sequentially
3. No concurrent writers to same log from different threads
4. Cache is thread-safe (RwLock) but appends are serialized by design

**Performance Impact**: None - Raft's sequential log guarantee means no parallelism opportunity anyway.

#### 6. Error Recovery Strategies

**Error**: `InvalidLogIndex { expected: 6, got: 7 }`

**Cause**: Gap in log (entry 6 missing)

**Recovery**:
```rust
// In raft crate - when append fails
match storage.append_log_entry(cf, index, bytes) {
    Err(StorageError::InvalidLogIndex { expected, got, .. }) => {
        tracing::error!(
            expected = expected,
            got = got,
            "Log index validation failed - initiating log sync"
        );

        // Request missing entries from leader
        self.request_log_sync_from_leader(expected, got).await?;

        // Retry append after sync
        storage.append_log_entry(cf, index, bytes)?;
    }
    Err(e) => return Err(e),
    Ok(()) => {}
}
```

**Error**: `InvalidLogIndex { expected: 11, got: 10 }` (Duplicate)

**Cause**: Entry 10 already exists (duplicate append)

**Recovery**:
```rust
// This indicates a logic bug in raft layer
// Duplicates should not happen under normal operation

tracing::error!(
    expected = expected,
    got = got,
    "Duplicate log index detected - this indicates a bug"
);

// Option 1: Overwrite (dangerous, data loss risk)
// storage.put(cf, format_log_key(got), bytes)?; // NOT RECOMMENDED

// Option 2: Ignore (safe, assume idempotent retry)
tracing::warn!("Ignoring duplicate append (assuming idempotent retry)");
return Ok(());

// Option 3: Panic (fail-fast for debugging)
panic!("Duplicate log index {} - raft layer bug", got);
```

#### 7. Performance Optimizations

**Optimization 1: In-Memory Cache**
- Cached `last_log_index` per CF avoids expensive RocksDB query on every append
- Cache hit: O(1) HashMap lookup
- Cache miss: O(log n) RocksDB reverse iterator (only on startup or after cache invalidation)

**Optimization 2: Batch Validation**

```rust
/// Append multiple entries with single validation check.
pub fn append_log_entries_batch(
    &self,
    cf: ColumnFamily,
    entries: Vec<(u64, Vec<u8>)>, // (index, bytes) pairs
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    // Validate all indices are sequential
    let current_last = self.get_cached_last_log_index(cf)?;
    let expected_start = current_last.map(|i| i + 1).unwrap_or(1);

    for (i, (index, _)) in entries.iter().enumerate() {
        let expected = expected_start + i as u64;
        if *index != expected {
            return Err(StorageError::InvalidLogIndex {
                cf: cf.as_str().to_string(),
                expected,
                got: *index,
                reason: format!("Batch validation failed at position {}", i),
            });
        }
    }

    // All indices valid - batch write
    let mut batch = WriteBatch::default();
    let cf_handle = self.get_cf_handle(cf)?;

    for (index, bytes) in &entries {
        let key = format_log_key(*index);
        batch.put_cf(cf_handle, key.as_bytes(), bytes);
    }

    self.db.write(batch)?;

    // Update cache to last entry
    let last_index = entries.last().unwrap().0;
    self.update_cached_last_log_index(cf, last_index)?;

    tracing::debug!(
        cf = cf.as_str(),
        count = entries.len(),
        start_index = entries[0].0,
        end_index = last_index,
        "Appended batch of log entries"
    );

    Ok(())
}
```

**Optimization 3: Avoid Fsync on Log Appends**

Log entries use async WAL (no fsync), only hard state requires fsync:

```rust
impl ColumnFamily {
    /// Returns true if writes to this CF require immediate fsync.
    pub fn requires_fsync(&self) -> bool {
        matches!(self,
            ColumnFamily::SystemRaftState | ColumnFamily::DataRaftState
        )
    }
}

// In put() implementation:
let write_opts = if cf.requires_fsync() {
    let mut opts = WriteOptions::default();
    opts.set_sync(true); // Blocks until fsync
    opts
} else {
    WriteOptions::default() // Async WAL
};
```

#### 8. Testing Strategy

**Unit Test - Sequential Index Validation**:

```rust
#[test]
fn test_append_requires_sequential_indices() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    // First entry must be index 1
    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    // Second entry must be index 2
    storage.append_log_entry(cf, 2, b"entry2").unwrap();

    // Gap: trying to append index 4 (missing index 3)
    let result = storage.append_log_entry(cf, 4, b"entry4");
    assert!(matches!(result, Err(StorageError::InvalidLogIndex { expected: 3, got: 4, .. })));

    // Fix gap by appending index 3
    storage.append_log_entry(cf, 3, b"entry3").unwrap();

    // Now index 4 works
    storage.append_log_entry(cf, 4, b"entry4").unwrap();
}

#[test]
fn test_append_rejects_duplicates() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    storage.append_log_entry(cf, 1, b"entry1").unwrap();

    // Duplicate index 1
    let result = storage.append_log_entry(cf, 1, b"entry1_dup");
    assert!(matches!(result, Err(StorageError::InvalidLogIndex { expected: 2, got: 1, .. })));
}

#[test]
fn test_first_entry_must_be_index_one() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    // Index 0 not allowed
    let result = storage.append_log_entry(cf, 0, b"entry0");
    assert!(matches!(result, Err(StorageError::InvalidLogIndex { expected: 1, got: 0, .. })));

    // Index 2 not allowed as first entry
    let result = storage.append_log_entry(cf, 2, b"entry2");
    assert!(matches!(result, Err(StorageError::InvalidLogIndex { expected: 1, got: 2, .. })));

    // Index 1 is correct
    storage.append_log_entry(cf, 1, b"entry1").unwrap();
}
```

**Unit Test - Cache Behavior**:

```rust
#[test]
fn test_cache_updated_after_append() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    // Cache should be None initially (empty log)
    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), None);

    // Append updates cache
    storage.append_log_entry(cf, 1, b"entry1").unwrap();
    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), Some(1));

    storage.append_log_entry(cf, 2, b"entry2").unwrap();
    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), Some(2));
}

#[test]
fn test_cache_survives_truncation() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    // Append 10 entries
    for i in 1..=10 {
        storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
    }

    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), Some(10));

    // Truncate first 5 entries (log now has 6-10)
    storage.truncate_log_before(cf, 6).unwrap();

    // Cache should still show last_index = 10
    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), Some(10));

    // Next append continues from 10
    storage.append_log_entry(cf, 11, b"entry11").unwrap();
}

#[test]
fn test_cache_invalidated_when_entire_log_truncated() {
    let storage = Storage::new(test_options()).unwrap();
    let cf = ColumnFamily::DataRaftLog;

    // Append 5 entries
    for i in 1..=5 {
        storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
    }

    // Truncate past end (deletes entire log)
    storage.truncate_log_before(cf, 100).unwrap();

    // Cache should be invalidated (None)
    assert_eq!(storage.get_cached_last_log_index(cf).unwrap(), None);

    // Can start fresh from index 1
    storage.append_log_entry(cf, 1, b"new_entry1").unwrap();
}
```

**Property Test - Append Sequence Invariant**:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_sequential_appends_always_succeed(count in 1usize..100) {
        let storage = Storage::new(test_options()).unwrap();
        let cf = ColumnFamily::DataRaftLog;

        // Sequential appends starting from 1 should always succeed
        for i in 1..=count {
            let data = format!("entry{}", i);
            storage.append_log_entry(cf, i as u64, data.as_bytes()).unwrap();
        }

        // Verify last index
        let last = storage.get_last_log_index(cf).unwrap();
        assert_eq!(last, Some(count as u64));
    }

    #[test]
    fn test_random_indices_fail_validation(indices in prop::collection::vec(1u64..1000, 10..50)) {
        let storage = Storage::new(test_options()).unwrap();
        let cf = ColumnFamily::DataRaftLog;

        // Random indices should mostly fail (unless accidentally sequential)
        let mut last_success = 0u64;

        for index in indices {
            let expected = last_success + 1;
            let result = storage.append_log_entry(cf, index, b"data");

            if index == expected {
                // Correct index - should succeed
                assert!(result.is_ok());
                last_success = index;
            } else {
                // Wrong index - should fail
                assert!(result.is_err());
            }
        }
    }
}
```

**Integration Test - Concurrent Safety**:

```rust
#[tokio::test]
async fn test_concurrent_appends_with_mutex() {
    let storage = Arc::new(Storage::new(test_options()).unwrap());
    let cf = ColumnFamily::DataRaftLog;

    // Mutex to serialize appends (mimics raft layer behavior)
    let append_lock = Arc::new(tokio::sync::Mutex::new(()));

    // Spawn 10 tasks trying to append concurrently
    let handles: Vec<_> = (1..=10)
        .map(|i| {
            let storage = storage.clone();
            let lock = append_lock.clone();

            tokio::spawn(async move {
                let _guard = lock.lock().await; // Serialize
                storage.append_log_entry(cf, i, format!("entry{}", i).as_bytes()).unwrap();
            })
        })
        .collect();

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all 10 entries appended successfully
    assert_eq!(storage.get_last_log_index(cf).unwrap(), Some(10));
}
```

---

## Summary of Changes Required

### File: `crates/storage/src/lib.rs`

1. **Remove all domain type references** from doc comments and API signatures
2. **Add index cache field** to `Storage` struct:
   ```rust
   last_log_index_cache: Arc<RwLock<HashMap<ColumnFamily, u64>>>
   ```
3. **Implement full validation logic** in `append_log_entry()`:
   - Cache-first lookup for last_index
   - Sequential index validation
   - Detailed error messages
4. **Add helper methods**:
   - `get_cached_last_log_index()`
   - `get_last_log_index_from_db()`
   - `update_cached_last_log_index()`
   - `warm_up_index_cache()`
   - `invalidate_index_cache()`
5. **Update truncate_log_before()** to handle cache invalidation
6. **Add batch append optimization** (optional): `append_log_entries_batch()`

### File: `crates/storage/src/column_family.rs`

1. **Add helper method**:
   ```rust
   pub fn is_log_cf(&self) -> bool {
       matches!(self, ColumnFamily::SystemRaftLog | ColumnFamily::DataRaftLog)
   }
   ```

### File: `crates/storage/src/error.rs`

1. **Enhance InvalidLogIndex error**:
   ```rust
   #[error("Invalid log index in CF {cf}: expected {expected}, got {got} ({reason})")]
   InvalidLogIndex {
       cf: String,
       expected: u64,
       got: u64,
       reason: String,
   }
   ```
2. **Add InvalidColumnFamily error**:
   ```rust
   #[error("Invalid column family for operation: {cf} ({reason})")]
   InvalidColumnFamily {
       cf: String,
       reason: String,
   }
   ```

### File: `crates/raft/src/storage_adapter.rs` (NEW)

1. **Create adapter struct** that bridges openraft and storage crate
2. **Implement serialization/deserialization** in adapter methods
3. **Add append mutex** for defensive concurrency control
4. **Implement error mapping** from `StorageError` to `RaftError`

### File: `crates/storage/tests/index_validation_test.rs` (NEW)

1. Add all unit tests from section 8

### File: `crates/storage/tests/property_tests.rs`

1. Add property tests for index validation

### File: `docs/specs/rocksdb/design.md`

**Lines to update**:

1. **Lines 270-308** (Column Family Setup): Remove type references, replace with "serialized bytes"
   ```markdown
   - Value: Serialized log entry bytes (format determined by caller)
   ```

2. **Lines 440-453** (Raft Integration Flow): Update to show serialization happening in raft crate:
   ```markdown
   3. RaftStorage serializes: entries -> Vec<Vec<u8>> using bincode
   4. RaftStorage calls: storage.append_log_entry(cf, index, &bytes)
   5. Storage validates: Sequential index (no gaps, no duplicates)
   6. Storage calls: db.put_cf(cf_handle, b"log:{index}", bytes)
   ```

3. **Lines 596-602** (Integration section): Add new subsection:
   ```markdown
   ### Serialization Boundary

   **Raft Crate Responsibilities**:
   - Define domain types (VersionedLogEntry, RaftHardState, etc.)
   - Serialize before calling storage
   - Deserialize after retrieving from storage
   - Handle version mismatches

   **Storage Crate Responsibilities**:
   - Accept only `&[u8]` and `Vec<u8>`
   - No knowledge of data structures
   - Pure persistence operations
   - Index validation (structural, not semantic)
   ```

4. **Lines 67-72** (append_log_entry signature): Update to show validation logic:
   ```rust
   /// Append log entry with automatic index validation.
   ///
   /// Validates that index is sequential (first = 1, subsequent = last + 1).
   /// Returns InvalidLogIndex error if gap or duplicate detected.
   /// Caller must serialize entry to bytes before calling.
   pub fn append_log_entry(&self, cf: ColumnFamily, index: u64, entry_bytes: &[u8]) -> Result<()>;
   ```

### File: `docs/specs/rocksdb/spec.md`

**Lines to update**:

1. **Line 36** (Business Rules): Add validation rule:
   ```markdown
   - Log indices MUST be sequential starting at 1 (no gaps, no duplicates)
   - Storage layer validates index sequence, rejects invalid appends
   - Raft layer responsible for serialization/deserialization
   ```

2. **Lines 78-79** (Integration Points): Clarify:
   ```markdown
   - raft crate - serializes Raft types, validates Raft semantics, calls storage with bytes
   - storage crate - validates sequential indices, provides byte-level persistence
   ```

---

## Implementation Checklist

For the coder-agent to implement:

- [ ] Update `Storage` struct with index cache field
- [ ] Implement cache initialization in `Storage::new()`
- [ ] Implement full `append_log_entry()` with validation
- [ ] Add cache helper methods (get, update, warm_up, invalidate)
- [ ] Update `truncate_log_before()` for cache handling
- [ ] Add `is_log_cf()` to `ColumnFamily`
- [ ] Enhance `StorageError` variants
- [ ] Remove all domain type references from storage API docs
- [ ] Create `RaftStorageAdapter` in raft crate
- [ ] Write unit tests for index validation
- [ ] Write property tests for sequential invariant
- [ ] Write integration tests for concurrent appends
- [ ] Update design.md with boundary clarifications
- [ ] Update spec.md with validation rules

---

## Validation Criteria

These solutions are correct if:

1. **Boundary Test**: `cargo build -p seshat-storage` succeeds WITHOUT depending on `seshat-common`
2. **API Test**: All storage public methods accept only `&[u8]` or `Vec<u8>` (no domain types)
3. **Validation Test**: Sequential appends succeed, gaps/duplicates fail with detailed errors
4. **Cache Test**: `get_cached_last_log_index()` returns O(1) after warm-up
5. **Truncate Test**: Cache correctly handles partial and full log truncation
6. **Concurrency Test**: Mutex-protected appends pass with 100% success rate
7. **Integration Test**: Raft adapter successfully serializes/deserializes all Raft types

---

**End of Document**
