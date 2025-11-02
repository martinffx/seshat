# RocksDB Storage Layer Technical Design

## Architecture Pattern

### Storage Abstraction Layer

This is a **pure persistence layer** - NOT the standard Router → Service → Repository pattern used for web APIs. The storage crate provides a low-level abstraction over RocksDB with no business logic.

```
┌──────────────────────────────────────────┐
│   seshat-storage Crate                   │
│   - OpenRaftRocksDBLog (RaftLogStorage)  │
│   - OpenRaftRocksDBStateMachine          │
│   - OpenRaftRocksDBSnapshotBuilder       │
└──────────────────────────────────────────┘
                    │
                    │ Uses Column Families
                    ▼
┌──────────────────────────────────────────┐
│   RocksDB Storage Layer                  │
│   - Column family management             │
│   - Atomic batch writes                  │
│   - Snapshot creation/restoration        │
│   - Thread-safe RocksDB access (Arc<DB>) │
└──────────────────────────────────────────┘
                    │
                    │ Maps traits to CFs:
                    │ - RaftLogStorage → data_raft_log + data_raft_state
                    │ - RaftStateMachine → data_kv
                    ▼
┌──────────────────────────────────────────┐
│   RocksDB (Arc<DB>)                      │
│   - 6 column families                    │
│   - WAL with fsync control               │
│   - LSM tree compaction                  │
└──────────────────────────────────────────┘
```

**Key Principle**: Storage layer stores bytes as directed - it has NO understanding of Raft semantics, business logic, or protocol parsing.

## Component Architecture

### Core Components

#### Storage (lib.rs)
**Main struct managing RocksDB instance and column families**

```rust
pub struct Storage {
    db: Arc<DB>,
    cf_handles: HashMap<ColumnFamily, Arc<BoundColumnFamily>>,
    metrics: Arc<RwLock<StorageMetrics>>,
    config: StorageOptions,
}

impl Storage {
    // Initialization
    pub fn new(options: StorageOptions) -> Result<Self>;

    // CRUD operations
    pub fn get(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>>;
    pub fn put(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()>;
    pub fn delete(&self, cf: ColumnFamily, key: &[u8]) -> Result<()>;
    pub fn exists(&self, cf: ColumnFamily, key: &[u8]) -> Result<bool>;

    // Batch operations
    pub fn batch_write(&self, batch: WriteBatch) -> Result<()>;

    // Raft log operations
    pub fn append_log_entry(&self, cf: ColumnFamily, index: u64, entry: &[u8]) -> Result<()>;
    pub fn get_log_range(&self, cf: ColumnFamily, start: u64, end: u64) -> Result<Vec<Vec<u8>>>;
    pub fn truncate_log_before(&self, cf: ColumnFamily, index: u64) -> Result<()>;
    pub fn get_last_log_index(&self, cf: ColumnFamily) -> Result<Option<u64>>;

    // Snapshot operations
    pub fn create_snapshot(&self, path: &Path) -> Result<SnapshotMetadata>;
    pub fn restore_snapshot(&self, path: &Path) -> Result<()>;
    pub fn validate_snapshot(&self, path: &Path) -> Result<SnapshotMetadata>;

    // Utilities
    pub fn iterator(&self, cf: ColumnFamily, mode: IteratorMode) -> Result<StorageIterator>;
    pub fn metrics(&self) -> StorageMetrics;
    pub fn sync(&self) -> Result<()>;
    pub fn compact_range(&self, cf: ColumnFamily, start: Option<&[u8]>, end: Option<&[u8]>) -> Result<()>;
    pub fn close(self) -> Result<()>;
}
```

**Thread Safety**: `Arc<DB>` enables safe multi-threaded access. RocksDB handles internal synchronization, so Storage can be safely cloned and shared across threads.

**Lifecycle**: Created once at node startup, shared across Raft groups (system and data shards), closed at graceful shutdown.

#### ColumnFamily (column_family.rs)
**Type-safe enum for 6 column families**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnFamily {
    SystemRaftLog,    // System group Raft log entries
    SystemRaftState,  // System group hard state (requires fsync)
    SystemData,       // Cluster metadata (membership, shardmap)
    DataRaftLog,      // Data shard Raft log entries
    DataRaftState,    // Data shard hard state (requires fsync)
    DataKv,           // User key-value data
}

impl ColumnFamily {
    pub fn as_str(&self) -> &'static str;
    pub fn all() -> [ColumnFamily; 6];
    pub fn requires_fsync(&self) -> bool;  // True for *_raft_state CFs
    pub fn default_options(&self) -> CFOptions;
}
```

**Design Rationale**: Enum prevents CF name typos at compile time and enables CF-specific behavior (fsync requirements, optimization profiles).

#### WriteBatch (batch.rs)
**Builder pattern for atomic multi-CF write operations**

```rust
pub struct WriteBatch {
    inner: rocksdb::WriteBatch,
    cfs: Vec<ColumnFamily>,
}

impl WriteBatch {
    pub fn new() -> Self;
    pub fn put(&mut self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> &mut Self;
    pub fn delete(&mut self, cf: ColumnFamily, key: &[u8]) -> &mut Self;
    pub fn clear(&mut self);
    pub fn is_empty(&self) -> bool;
    pub fn requires_fsync(&self) -> bool;  // True if any CF requires fsync
}
```

**Atomicity Guarantee**: All operations succeed or all fail - no partial writes visible to readers.

**Usage Example**:
```rust
let mut batch = WriteBatch::new();
batch
    .put(ColumnFamily::DataKv, b"key1", b"value1")
    .put(ColumnFamily::DataKv, b"key2", b"value2")
    .put(ColumnFamily::DataRaftState, b"state", serialized_state);

storage.batch_write(batch)?;  // Atomic commit with fsync
```

#### StorageIterator (iterator.rs)
**Iterator with snapshot isolation for range queries**

```rust
pub struct StorageIterator<'a> {
    inner: DBIterator<'a>,
    cf: ColumnFamily,
}

pub enum IteratorMode {
    Start,                              // From first key in CF
    End,                                // From last key in CF
    From(Vec<u8>, Direction),          // From specific key
}

impl<'a> StorageIterator<'a> {
    pub fn seek(&mut self, key: &[u8]);
    pub fn seek_to_first(&mut self);
    pub fn seek_to_last(&mut self);
    pub fn next(&mut self) -> Option<Result<(Box<[u8]>, Box<[u8]>)>>;
    pub fn prev(&mut self) -> Option<Result<(Box<[u8]>, Box<[u8]>)>>;
    pub fn valid(&self) -> bool;
}
```

**Snapshot Isolation**: Iterator captures DB snapshot at creation time - sees consistent view even if writes happen during iteration.

#### StorageOptions (options.rs)
**Configuration for RocksDB initialization**

```rust
pub struct StorageOptions {
    pub data_dir: PathBuf,
    pub create_if_missing: bool,        // True for bootstrap, false for join
    pub compression: CompressionType,   // Lz4 for Phase 1
    pub write_buffer_size_mb: usize,    // 64MB default per CF
    pub max_write_buffer_number: usize, // 3 memtables default
    pub target_file_size_mb: usize,     // 64MB SST files
    pub max_open_files: i32,            // -1 for unlimited
    pub enable_statistics: bool,        // True for Phase 4 observability
    pub cf_options: HashMap<ColumnFamily, CFOptions>,
}

pub struct CFOptions {
    pub compaction_style: DBCompactionStyle,
    pub disable_auto_compactions: bool,
    pub level0_file_num_compaction_trigger: i32,
    pub write_buffer_size: Option<usize>,
    pub prefix_extractor: Option<SliceTransform>,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data/rocksdb"),
            create_if_missing: true,
            compression: CompressionType::Lz4,
            write_buffer_size_mb: 64,
            max_write_buffer_number: 3,
            target_file_size_mb: 64,
            max_open_files: -1,  // Unlimited
            enable_statistics: false,
            cf_options: Self::default_cf_options(),
        }
    }
}

impl StorageOptions {
    /// Create options with custom data directory, using defaults for all other settings.
    pub fn with_data_dir(path: PathBuf) -> Self {
        Self {
            data_dir: path,
            ..Default::default()
        }
    }

    /// Validate configuration values.
    ///
    /// # Errors
    /// - write_buffer_size_mb: Must be 1-1024 MB
    /// - max_write_buffer_number: Must be 2-10
    /// - target_file_size_mb: Must be 1-1024 MB
    pub fn validate(&self) -> Result<()> {
        if self.write_buffer_size_mb < 1 || self.write_buffer_size_mb > 1024 {
            return Err(StorageError::InvalidConfig {
                field: "write_buffer_size_mb".to_string(),
                reason: format!("Must be 1-1024, got {}", self.write_buffer_size_mb),
            });
        }

        if self.max_write_buffer_number < 2 || self.max_write_buffer_number > 10 {
            return Err(StorageError::InvalidConfig {
                field: "max_write_buffer_number".to_string(),
                reason: format!("Must be 2-10, got {}", self.max_write_buffer_number),
            });
        }

        if self.target_file_size_mb < 1 || self.target_file_size_mb > 1024 {
            return Err(StorageError::InvalidConfig {
                field: "target_file_size_mb".to_string(),
                reason: format!("Must be 1-1024, got {}", self.target_file_size_mb),
            });
        }

        Ok(())
    }

    /// Default column family options optimized per CF type.
    fn default_cf_options() -> HashMap<ColumnFamily, CFOptions> {
        use ColumnFamily::*;

        let mut opts = HashMap::new();

        // Raft log CFs: Sequential writes, aggressive compaction
        let raft_log_opts = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 2,  // Aggressive compaction
            write_buffer_size: None,  // Use global default
            prefix_extractor: None,  // Sequential access, no prefix needed
        };
        opts.insert(SystemRaftLog, raft_log_opts.clone());
        opts.insert(DataRaftLog, raft_log_opts);

        // Raft state CFs: Tiny, rarely compacted
        let raft_state_opts = CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: true,  // Manual compaction only
            level0_file_num_compaction_trigger: 10,  // Never triggers
            write_buffer_size: Some(4 * 1024 * 1024),  // 4MB (tiny)
            prefix_extractor: None,
        };
        opts.insert(SystemRaftState, raft_state_opts.clone());
        opts.insert(DataRaftState, raft_state_opts);

        // System data CF: Small, infrequent updates
        opts.insert(SystemData, CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: true,
            level0_file_num_compaction_trigger: 10,
            write_buffer_size: Some(8 * 1024 * 1024),  // 8MB
            prefix_extractor: None,
        });

        // Data KV CF: Random access, bloom filters, high throughput
        opts.insert(DataKv, CFOptions {
            compaction_style: DBCompactionStyle::Level,
            disable_auto_compactions: false,
            level0_file_num_compaction_trigger: 4,  // Moderate compaction
            write_buffer_size: None,  // Use global default (64MB)
            // 4-byte prefix extractor for bloom filter optimization
            prefix_extractor: Some(SliceTransform::create_fixed_prefix(4)),
        });

        opts
    }
}
```

#### StorageMetrics (metrics.rs)
**Runtime metrics for monitoring**

```rust
#[derive(Debug, Clone)]
pub struct StorageMetrics {
    pub db_size_bytes: u64,
    pub num_keys: HashMap<ColumnFamily, u64>,
    pub last_snapshot_duration_ms: u64,
    pub write_ops_total: u64,
    pub read_ops_total: u64,
    pub bytes_written: u64,
    pub bytes_read: u64,
}

impl StorageMetrics {
    pub fn new() -> Self;
    pub fn update_from_db(&mut self, db: &DB);
}
```

**Update Frequency**: On-demand via `storage.metrics()` call. No background thread needed in Phase 1.

#### StorageError (error.rs)
**Rich error context using thiserror**

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("Column family not found: {0}")]
    ColumnFamilyNotFound(String),

    #[error("Invalid log index: expected {expected}, got {got}")]
    InvalidLogIndex { expected: u64, got: u64 },

    #[error("Snapshot failed at {path:?}: {reason}")]
    SnapshotFailed { path: PathBuf, reason: String },

    #[error("Corrupted data in CF {cf}, key {key:?}: {reason}")]
    CorruptedData { cf: String, key: Vec<u8>, reason: String },

    #[error("Version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },

    #[error("Invalid configuration for {field}: {reason}")]
    InvalidConfig { field: String, reason: String },
}

pub type Result<T> = std::result::Result<T, StorageError>;
```

**Propagation Strategy**: All public methods return `Result<T>`. Caller (raft crate) decides retry/fail/panic strategy.

## Implementation Design

### RocksDB Configuration

#### Column Family Setup

**6 Column Families with optimized settings**:

1. **system_raft_log** - System group Raft log
   - Optimize for: Sequential writes, range scans
   - Compaction: Level style, aggressive (keep log size small)
   - Key format: `"log:{:020}"` (e.g., `"log:00000000000000000142"`) - zero-padded for correct lexicographic ordering
   - Value: Protobuf serialized LogEntry (via prost)

2. **system_raft_state** - System group hard state
   - Optimize for: Single-key updates with fsync
   - Compaction: Disabled (always <1KB)
   - Key format: `"state"` (single entry)
   - Value: Protobuf serialized RaftHardState (via prost)

3. **system_data** - Cluster metadata
   - Optimize for: Small number of keys (<10), infrequent updates
   - Compaction: Disabled (bounded size ~100KB)
   - Keys: `"membership"`, `"shardmap"`
   - Values: Protobuf serialized ClusterMembership or ShardMap (via prost)

4. **data_raft_log** - Data shard Raft log
   - Optimize for: High-throughput sequential writes, range scans
   - Compaction: Level style, moderate
   - Key format: `"log:{:020}"` (zero-padded for correct lexicographic ordering)
   - Value: Protobuf serialized LogEntry (via prost)

5. **data_raft_state** - Data shard hard state
   - Optimize for: Single-key updates with fsync
   - Compaction: Disabled (always <1KB)
   - Key format: `"state"`
   - Value: Protobuf serialized RaftHardState (via prost)

6. **data_kv** - User key-value data
   - Optimize for: Random reads/writes, high key count
   - Bloom filters: Enabled (10 bits per key)
   - Prefix extractor: 4-byte hash prefix
   - Key format: Raw user bytes (no encoding)
   - Value: Protobuf serialized StoredValue (via prost)

#### Write-Ahead Log (WAL)

- **Enabled**: True for all writes
- **Sync Mode**:
  - Synchronous fsync for `*_raft_state` CFs (durability guarantee)
  - Async for other CFs (rely on WAL + background fsync)
- **WAL Directory**: Same as `data_dir` in Phase 1 (simplify config)
- **WAL Size Limit**: 64MB (triggers rotation)

#### Compaction Strategy

- **Style**: Level compaction (better space efficiency than universal)
- **L0 Trigger**: 4 SST files (triggers L0→L1 compaction)
- **Target File Size**: 64MB per SST file
- **Max Bytes L1**: 256MB (L1 size before L1→L2 compaction)
- **Compression**:
  - None for L0/L1 (hot data, optimize CPU)
  - Lz4 for L2+ (balance CPU vs disk space)

#### Memory Configuration

**Default Settings** (from `StorageOptions::default()`):
- **Write Buffer Size**: 64MB per CF
- **Max Write Buffers**: 3 memtables per CF
- **Target SST File Size**: 64MB
- **Max Open Files**: -1 (unlimited)
- **Compression**: Lz4 (L2+), None (L0/L1)

**Memory Usage Breakdown**:
- **Write Buffer**: 64MB per CF × 3 memtables = 192MB per CF
- **Total Memtables**: 192MB × 6 CFs = ~1.1GB max
- **Block Cache**: 256MB shared across all CFs
- **Total Memory**: ~1.5GB in steady state

**Per-CF Overrides**:
- **Raft State CFs**: 4MB write buffer (small, single-key updates)
- **System Data CF**: 8MB write buffer (bounded size ~100KB)
- **Log/Data CFs**: Use global 64MB default

#### Bloom Filters

- **Enabled**: Only for `data_kv` CF
- **Bits per Key**: 10 (1% false positive rate)
- **Purpose**: Reduce disk reads for GET operations
- **Not Needed**: Raft log access is sequential (range scans)

### Durability Strategy

**Fsync Policy**:
- `*_raft_state` CFs: **Synchronous fsync** before returning from `put()` - critical for Raft safety
- Other CFs: **Async WAL writes** - rely on WAL for durability, fsync in background
- Batch writes: If batch touches any `raft_state` CF, **fsync entire batch atomically**

**Crash Recovery**: RocksDB automatically replays WAL on startup. Storage layer just validates schema versions after opening DB.

**Checkpoint Atomicity**: RocksDB checkpoint uses hard links - either full checkpoint exists or none (no partial states).

### Serialization Strategy

**Format**: Protobuf (prost) for all storage serialization
- **Rationale**: Single format for storage + network, schema evolution, industry alignment
- **Schema Definition**: .proto files in `protocol/` directory
- **Code Generation**: prost-build generates Rust types at compile time
- **No bincode**: Protobuf used exclusively throughout the system (see `openraft/SERIALIZATION_DECISION.md`)

**Version Handling**: Protobuf provides built-in schema evolution
```proto
message LogEntry {
  uint64 version = 1;  // CURRENT_VERSION = 1 in Phase 1
  uint64 index = 2;
  uint64 term = 3;
  bytes data = 4;
}
```

**Migration Path**:
- Protobuf handles forward/backward compatibility through optional fields and default values
- If incompatible schema change: Increment version field, implement migration logic
- If `version > CURRENT_VERSION`: **Refuse to start** (cannot read future formats)

## Data Flows

### GET Operation Flow

```
1. Client calls: storage.get(ColumnFamily::DataKv, b"foo")
2. Storage looks up CF handle from cf_handles HashMap
3. Storage calls: db.get_cf(cf_handle, b"foo") -> Result<Option<Vec<u8>>>
4. RocksDB searches: Memtable → Block cache → Bloom filter → SST files
5. Storage updates: metrics.read_ops_total += 1
6. Storage returns: Ok(Some(value)) or Ok(None)
```

**Performance**: O(log n) with bloom filter optimization. Target <1ms p99.

### PUT Operation Flow

```
1. Client calls: storage.put(ColumnFamily::DataRaftState, b"state", bytes)
2. Storage checks: cf.requires_fsync() -> true for raft_state CFs
3. Storage creates WriteOptions with sync=true
4. Storage calls: db.put_cf_opt(cf_handle, b"state", bytes, write_opts)
5. RocksDB appends to WAL and fsyncs to disk (blocks ~1-5ms on SSD)
6. RocksDB inserts into memtable (in-memory)
7. Storage updates: metrics.write_ops_total += 1
8. Storage returns: Ok(())
```

**Performance**: O(1) amortized. May trigger background compaction (doesn't block).

### Batch Write Operation Flow

```
1. Client creates: let mut batch = WriteBatch::new()
2. Client adds ops: batch.put(ColumnFamily::DataKv, b"key1", b"val1")
3. Client adds ops: batch.put(ColumnFamily::DataRaftState, b"state", bytes)
4. Client commits: storage.batch_write(batch)
5. Storage checks: batch.requires_fsync() -> true (DataRaftState touched)
6. Storage creates WriteOptions with sync=true
7. Storage calls: db.write_opt(batch.inner, write_opts)
8. RocksDB applies all operations atomically, fsyncs WAL
9. Storage updates metrics
10. Storage returns: Ok(())
```

**Atomicity**: All operations succeed or all fail. No partial writes visible to readers.

**Performance**: Amortizes fsync cost across multiple operations. Batch 10-100 operations for best throughput.

### Snapshot Creation Flow

```
1. Client calls: storage.create_snapshot(Path::new("/data/snapshots/snap-20250125"))
2. Storage creates: Checkpoint::new(&db)?
3. Storage calls: checkpoint.create_checkpoint(path)?
4. RocksDB creates hard links to all SST files atomically (~10ms, no data copy)
5. Storage reads: db.latest_sequence_number() for metadata
6. Storage stats: checkpoint directory for size_bytes
7. Storage creates: SnapshotMetadata { last_included_index, last_included_term, created_at, size_bytes }
8. Storage updates: metrics.last_snapshot_duration_ms
9. Storage returns: Ok(metadata)
```

**Efficiency**: O(1) time - uses hard links (no data copying). Zero additional disk space initially. Space diverges as original DB changes.

### OpenRaft Integration Flow

**IMPORTANT**: After OpenRaft migration, this integration uses openraft storage traits, not raft-rs.

```
Read Path (via RaftLogReader trait):
1. openraft calls: RaftLogReader::try_get_log_entries(range)
2. OpenRaftMemStorage (in raft crate) selects CF based on shard_id
3. OpenRaftMemStorage calls: storage.get_log_range(cf, range.start, range.end)
4. Storage reads from RocksDB: db.get_cf(cf, b"log:{:020}")
5. Storage returns: Vec<Vec<u8>> (serialized log entries)
6. OpenRaftMemStorage deserializes: Vec<u8> -> LogEntry<Request>
7. OpenRaftMemStorage returns: Vec<LogEntry<Request>> to openraft

Write Path (via RaftStorage trait):
1. openraft calls: RaftStorage::append(entries)
2. OpenRaftMemStorage serializes: LogEntry<Request> -> Vec<u8> (protobuf via prost)
3. OpenRaftMemStorage selects CF based on shard_id
4. OpenRaftMemStorage calls: storage.append_log_entry(cf, index, &bytes)
5. Storage validates: No gaps in log indices (fails with InvalidLogIndex if gap detected)
6. Storage calls: db.put_cf(data_raft_log, b"log:{:020}", bytes)
7. Storage returns: Ok(())
8. OpenRaftMemStorage returns: Success to openraft
```

**Separation of Concerns**:
- **OpenRaftMemStorage (raft crate)**: Implements openraft storage traits, handles serialization, understands Raft semantics
- **Storage (storage crate)**: Pure persistence layer, stores bytes as directed, no Raft knowledge

**Key Differences from raft-rs**:
- OpenRaft uses three separate traits (RaftLogReader, RaftSnapshotBuilder, RaftStorage) instead of single Storage trait
- All trait methods are async (requires tokio runtime)
- Different entry types: `LogEntry<Request>` instead of `eraftpb::Entry`
- Storage layer remains synchronous (RocksDB operations), async wrapper in raft crate

## Module Organization

### Source Files

```
storage/
├── src/
│   ├── lib.rs              # Public API, Storage struct, re-exports
│   ├── column_family.rs    # ColumnFamily enum with CF metadata
│   ├── batch.rs            # WriteBatch builder for atomic operations
│   ├── iterator.rs         # StorageIterator with snapshot isolation
│   ├── error.rs            # StorageError with thiserror
│   ├── metrics.rs          # StorageMetrics tracking
│   ├── snapshot.rs         # Checkpoint creation/restoration helpers
│   └── options.rs          # StorageOptions and CFOptions configuration
├── tests/
│   ├── integration_tests.rs   # Full Storage workflows with temp RocksDB
│   ├── property_tests.rs      # proptest for serialization roundtrip, atomicity
│   └── common/
│       └── mod.rs             # Shared test utilities (temp dir, sample data)
└── Cargo.toml
```

### Public API Surface

**Exported from `lib.rs`**:
```rust
pub use storage::Storage;
pub use column_family::ColumnFamily;
pub use batch::WriteBatch;
pub use iterator::{StorageIterator, IteratorMode};
pub use options::{StorageOptions, CFOptions};
pub use metrics::StorageMetrics;
pub use error::{StorageError, Result};
```

### Internal Helpers (Private)

- CF handle caching in `HashMap<ColumnFamily, Arc<BoundColumnFamily>>`
- RocksDB initialization logic in `Storage::new()`
- `WriteOptions` creation based on fsync requirements
- Metrics update helper methods
- Key formatting: `format_log_key(index: u64) -> String` returns `"log:{:020}"` (zero-padded for correct lexicographic ordering)

## Integration Design

### Raft Crate Integration

**IMPORTANT**: This section describes the integration AFTER OpenRaft migration is complete.

**OpenRaft Storage Implementation**:
```rust
// In raft crate (crates/raft/src/storage.rs or crates/storage/src/openraft_storage.rs)
use openraft::storage::{RaftLogReader, RaftSnapshotBuilder, RaftStorage as OpenRaftStorageTrait};
use seshat_storage::Storage; // RocksDB storage layer

pub struct OpenRaftMemStorage {
    storage: Arc<Storage>,
    shard_id: u64,  // System (0) or data shard ID
}

// Read operations
#[async_trait]
impl RaftLogReader<RaftTypeConfig> for OpenRaftMemStorage {
    async fn try_get_log_entries(
        &mut self,
        range: std::ops::Range<u64>
    ) -> Result<Vec<LogEntry<Request>>> {
        let cf = self.select_log_cf();
        let bytes_vec = self.storage.get_log_range(cf, range.start, range.end)?;

        // Deserialize each entry using protobuf
        use prost::Message;
        bytes_vec.into_iter()
            .map(|bytes| LogEntry::decode(&bytes[..]))
            .collect()
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>> {
        let cf = self.select_state_cf();
        match self.storage.get(cf, b"vote")? {
            Some(bytes) => {
                use prost::Message;
                Ok(Some(Vote::decode(&bytes[..])?))
            },
            None => Ok(None),
        }
    }

    // ... other RaftLogReader methods
}

// Write operations
#[async_trait]
impl OpenRaftStorageTrait<RaftTypeConfig> for OpenRaftMemStorage {
    async fn append<I>(
        &mut self,
        entries: I
    ) -> Result<()>
    where
        I: IntoIterator<Item = LogEntry<Request>> + Send,
    {
        let cf = self.select_log_cf();

        for entry in entries {
            use prost::Message;
            let bytes = entry.encode_to_vec();
            self.storage.append_log_entry(cf, entry.log_id.index, &bytes)?;
        }
        Ok(())
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<()> {
        let cf = self.select_state_cf();
        use prost::Message;
        let bytes = vote.encode_to_vec();
        self.storage.put(cf, b"vote", &bytes)?;
        Ok(())
    }

    // ... other RaftStorage methods
}

impl OpenRaftMemStorage {
    fn select_log_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftLog
        } else {
            ColumnFamily::DataRaftLog
        }
    }

    fn select_state_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftState
        } else {
            ColumnFamily::DataRaftState
        }
    }
}
```

**Responsibilities**:
- **OpenRaftMemStorage**: Implements openraft storage traits, handles protobuf serialization/deserialization, CF selection
- **Storage**: Pure persistence (stores raw bytes), thread-safety, atomicity, RocksDB operations

**Serialization Strategy**:
- **Format**: Protobuf (prost) for all storage serialization
- **Rationale**: Single format for storage + network, schema evolution, industry alignment (see `openraft/SERIALIZATION_DECISION.md`)
- **No bincode**: Protobuf used exclusively throughout the system

### Common Crate Dependencies

**Data Structures Defined in Protobuf Schema**:
```proto
// protocol/raft.proto
message LogEntry {
  uint64 version = 1;
  uint64 index = 2;
  uint64 term = 3;
  bytes data = 4;
}

message RaftHardState {
  uint64 version = 1;
  uint64 term = 2;
  uint64 vote = 3;
  uint64 commit = 4;
}

// protocol/storage.proto
message StoredValue {
  uint64 version = 1;
  bytes data = 2;
  uint64 created_at = 3;
  optional uint64 ttl = 4;
}

// protocol/cluster.proto
message ClusterMembership {
  uint64 version = 1;
  repeated NodeInfo nodes = 2;
}

message ShardMap {
  uint64 version = 1;
  repeated ShardInfo shards = 2;
}

// protocol/snapshot.proto
message SnapshotMetadata {
  uint64 last_included_index = 1;
  uint64 last_included_term = 2;
  uint64 created_at = 3;
  uint64 size_bytes = 4;
}
```

**Dependency Flow**:
- `storage` crate: Stores bytes, no knowledge of data structures
- `protocol` directory: Defines protobuf schemas shared across crates
- `raft` crate: Serializes protobuf messages using prost before passing to storage
- `common` crate: Re-exports generated protobuf types for convenience

## Performance Considerations

### Latency Optimization (<1ms p99 target)

1. **Bloom Filters**: Reduce disk reads for `data_kv` GET operations
2. **Block Cache**: 256MB shared cache for frequently accessed blocks
3. **Write Buffers**: 64MB per CF batches writes before flushing to disk
4. **Async WAL**: Most writes don't block on fsync (only `raft_state` CFs)
5. **No Extra Serialization**: Direct `&[u8]` API avoids unnecessary copies
6. **Arc<DB>**: Zero-cost sharing across threads (no cloning DB instance)

### Batch Write Strategy

**Motivation**: Amortize fsync cost across multiple operations

**Use Cases**:
- Raft log replication: Batch multiple log entries in one atomic write
- State machine application: Batch user KV writes + Raft state update
- Log compaction: Batch truncation of old log entries

**Size Limits**: No hard limit in Phase 1. Consider 10MB batches in Phase 2+ to avoid memory spikes.

### Snapshot Efficiency

- **Checkpoint Creation**: O(1) time using hard links - typically <10ms for 10GB dataset
- **Zero Space Initially**: Hard links share data until original DB diverges
- **Restoration**: O(n) time - must copy all files to `data_dir` (~1-2s per GB)
- **No Write Blocking**: Checkpoint creation doesn't block reads/writes
- **Manual Cleanup**: Raft crate responsible for deleting old checkpoints

### Memory Considerations

**Total Memory Budget**: ~1.5GB
- Write buffers: 1.1GB (64MB × 3 × 6 CFs)
- Block cache: 256MB
- Iterator snapshots: Minimal overhead (bounded by number of active iterators)

**Backpressure**: RocksDB automatically stalls writes if memtables full (prevents OOM).

### Disk I/O Patterns

- **Raft Log**: Sequential writes, occasional sequential reads (for replication)
- **Raft State**: Random writes with fsync, infrequent reads (on restart)
- **Data KV**: Random reads and writes, high IOPS workload
- **Compaction**: Background sequential reads + writes, throttled to avoid impacting foreground
- **WAL**: Sequential writes, flushed per-write for state CFs

## Phase 1 Scope

### Included in Phase 1

- All 6 column families with optimized settings
- CRUD operations: `get`, `put`, `delete`, `exists`
- Atomic batch writes across CFs
- Raft log operations: `append_log_entry`, `get_log_range`, `truncate_log_before`
- Snapshot creation/restoration using RocksDB checkpoints
- Iterator support for range queries
- Basic metrics tracking
- Comprehensive error handling with `thiserror`
- Full test coverage (unit + integration + property tests)

### Deferred to Phase 2+

- **Dynamic CF Creation**: Phase 2 adds per-shard CFs (`ShardRaftLog(shard_id)`)
  - May require DB restart to add new CFs in Phase 2
  - Consider using single `data_raft_log` CF with prefix keys in Phase 2 to avoid restarts

- **Advanced Compaction Control**: Manual compaction policies, TTL-based compaction
  - Phase 1 uses RocksDB defaults (sufficient for single-shard workload)

- **Per-Shard Metrics**: Phase 2 tracks metrics per shard, not just per CF
  - Requires key prefix parsing to attribute metrics to shards

- **Background Metrics Collection**: Phase 1 updates metrics on-demand via `metrics()` call
  - Phase 4 adds background thread to push metrics to Prometheus

- **Separate WAL Directory**: Phase 1 uses same dir for WAL and SST files
  - Phase 4 may separate WAL to different disk for performance

- **Online Schema Migration**: Phase 1 validates versions, refuses to start if mismatch
  - Phase 3 adds online migration to handle version upgrades gracefully

### Testing Strategy

**Unit Tests** (in each module):
```rust
#[cfg(test)]
mod tests {
    // Test ColumnFamily enum methods
    // Test WriteBatch builder pattern
    // Test error conversions
    // Test metrics updates
}
```

**Integration Tests** (`tests/integration_tests.rs`):
```rust
// Test full CRUD workflows with temporary RocksDB
// Test batch atomicity (partial failure scenarios)
// Test snapshot creation/restoration roundtrip
// Test iterator snapshot isolation
// Test concurrent reads/writes from multiple threads
// Test crash recovery (kill process, reopen DB)
```

**Property Tests** (`tests/property_tests.rs`):
```rust
// proptest: Serialization roundtrip for all versioned structs
// proptest: Batch operations preserve atomicity
// proptest: Iterator sees consistent view during concurrent writes
```

---

**Cross-references**:
- Architecture: `/Users/martinrichards/code/seshat/docs/architecture/crates.md` - How storage fits into 8-crate structure
- Data Structures: `/Users/martinrichards/code/seshat/docs/architecture/data-structures.md` - `VersionedLogEntry`, `RaftHardState` definitions
- Tech Stack: `/Users/martinrichards/code/seshat/docs/standards/tech.md` - RocksDB choice rationale
- TDD Workflow: `/Users/martinrichards/code/seshat/docs/standards/practices.md` - Test → Code → Refactor pattern
