# RocksDB Storage Layer Specification

## Overview

Persistent storage layer using RocksDB for durability. Enables cluster recovery after restarts and maintains consistency across nodes.

## User Story

As a Seshat node operator, I want persistent storage using RocksDB so that the cluster can recover state after restarts and maintain consistency across nodes.

## Acceptance Criteria

1. **Initialization**: All 6 column families created with correct configuration on fresh startup
2. **Latency**: Storage operations complete with <1ms p99 latency for local access
3. **Log Ordering**: Raft log operations preserve sequential ordering with no gaps
4. **Atomicity**: Batch writes across column families are all-or-nothing
5. **Snapshots**: RocksDB checkpoints succeed with metadata recording last_included_index
6. **Recovery**: Node restart preserves all persisted data and passes version checks
7. **Isolation**: Operations on different column families maintain data isolation

## Business Rules

- **system_raft_log**: System group Raft entries, compact after snapshot (~10MB)
- **system_raft_state**: System hard state (term, vote, commit), MUST fsync before RPC responses
- **system_data**: Cluster metadata (ClusterMembership, ShardMap), bounded ~100KB
- **data_raft_log**: Data shard entries, snapshot every 10,000 entries or 100MB (~100MB)
- **data_raft_state**: Data shard hard state, MUST fsync before RPC responses
- **data_kv**: User key-value data, unbounded size, optimized for point lookups
- All batch writes MUST be atomic
- All persisted structures MUST include version field for schema evolution
- Key size limit: 256 bytes (enforced by validation layer)
- Value size limit: 65,536 bytes (enforced by validation layer)
- Raft log memory limit: 512MB per Raft group before forced compaction
- Storage layer MUST NOT understand Raft semantics - only stores bytes as directed

## Scope

### Included

- RocksDB initialization with 6 column families and optimized configuration
- CRUD operations per column family (get, put, delete, exists)
- Atomic batch write operations across multiple column families
- Raft log operations: append entry, get range, truncate before index
- Snapshot creation using RocksDB checkpoint (hard links, atomic)
- Snapshot restoration from checkpoint directory
- Configuration management (NodeConfig, ClusterConfig, RaftConfig)
- Error handling with rich context propagation (thiserror)
- Iterator support for range queries within column families
- Storage metrics tracking (db_size_bytes, num_keys, snapshot_duration)

### Excluded

- TTL expiration logic (Phase 2)
- Distributed locking implementation (Phase 2)
- Metrics/observability export (Phase 4)
- Multi-shard column family management (Phase 2)
- Online schema migration tools (Phase 3)
- RocksDB tuning dashboard (Phase 4)
- Automatic compaction scheduling (Phase 1 uses RocksDB defaults)

## Architecture

### Storage Abstraction Layer

Pure persistence layer - no Raft semantics, no business logic, no protocol parsing. Stores bytes as directed by upper layers.

```
seshat-storage → OpenRaftRocksDBLog, OpenRaftRocksDBStateMachine
        │
        │ Uses Column Families
        ▼
RocksDB Storage Layer → CF management, atomic batch writes, snapshots
        │
        │ Maps traits to CFs
        ▼
RocksDB (Arc<DB>) → 6 column families, WAL with fsync control, LSM tree
```

### Crate Boundaries

**Golden Rule**: Storage crate operates exclusively on `&[u8]` and `Vec<u8>`. Zero knowledge of domain types.

| Crate | Responsibility |
|-------|---------------|
| storage | RocksDB operations on bytes, CF management, atomicity |
| raft | Owns VersionedLogEntry, RaftHardState; handles serialization with prost |
| protocol | Defines protobuf schemas shared across crates |

### Key Design Decisions

- **Serialization**: Protobuf (prost) for all storage serialization - single format for storage + network
- **Thread Safety**: Arc<DB> enables safe multi-threaded access
- **Snapshots**: RocksDB checkpoints using hard links (O(1) time, zero space initially)
- **Durability**: Fsync for raft_state CFs before returning; async WAL for others

## Components

| Component | File | Responsibility |
|-----------|------|---------------|
| Storage | lib.rs | Main struct, owns DB instance, CF handles, metrics |
| ColumnFamily | column_family.rs | Type-safe enum for 6 CFs, prevents compile-time typos |
| WriteBatch | batch.rs | Builder for atomic multi-CF writes |
| StorageIterator | iterator.rs | Iterator with snapshot isolation for range queries |
| StorageOptions | options.rs | Configuration for RocksDB initialization |
| StorageMetrics | metrics.rs | Runtime metrics (db_size, key counts, ops counters) |
| StorageError | error.rs | Rich error context with thiserror |

### Storage Struct

Manages RocksDB instance and column families. Created at node startup, shared across Raft groups, closed at shutdown.

### ColumnFamily Enum

```rust
pub enum ColumnFamily {
    SystemRaftLog,    // System group Raft log entries
    SystemRaftState,  // System hard state (requires fsync)
    SystemData,       // Cluster metadata
    DataRaftLog,      // Data shard log entries
    DataRaftState,    // Data shard hard state (requires fsync)
    DataKv,           // User key-value data
}
```

## Column Families

| Name | Purpose | Key Format | Size | Durability |
|------|---------|------------|------|-----------|
| system_raft_log | System Raft entries | `log:{index}` | ~10MB | WAL async |
| system_raft_state | System hard state | `state` | <1KB | WAL fsync |
| system_data | Cluster metadata | `membership`, `shardmap` | ~100KB | WAL async |
| data_raft_log | Data shard entries | `log:{index}` | ~100MB | WAL async |
| data_raft_state | Data shard hard state | `state` | <1KB | WAL fsync |
| data_kv | User key-value | raw bytes | unbounded | WAL async |

## Operations

### Basic CRUD

| Operation | Returns | Notes |
|-----------|---------|-------|
| get(cf, key) | Option<Vec<u8>> | None if missing |
| put(cf, key, value) | Result | Fsync if cf.requires_fsync() |
| delete(cf, key) | Result | No-op if missing |
| exists(cf, key) | bool | Uses bloom filter |

### Batch Operations

WriteBatch builder for atomic multi-CF writes:
- `new()` → `put()` → `put()` → `delete()` → `batch_write()`
- All-or-nothing semantics via RocksDB WriteBatch
- Fsync if any CF in batch requires it

### Log Operations

| Operation | Returns | Notes |
|-----------|---------|-------|
| append_log_entry(cf, index, bytes) | Result | Validates sequential indices |
| get_log_range(cf, start, end) | Vec<Vec<u8>> | Returns [start, end) |
| truncate_log_before(cf, index) | Result | Delete index < given |
| get_last_log_index(cf) | Option<u64> | Highest index or None |

### Snapshot Operations

| Operation | Returns | Notes |
|-----------|---------|-------|
| create_snapshot(path) | SnapshotMetadata | Uses checkpoints (hard links) |
| restore_snapshot(path) | Result | Replaces current DB state |
| validate_snapshot(path) | SnapshotMetadata | Pre-validate before restore |

### Durability Strategy

| CF Type | Sync Policy |
|---------|------------|
| *_raft_state | Synchronous fsync before returning |
| Other CFs | Async WAL writes |
| Batch with raft_state | Fsync entire batch |

## Performance Requirements

| Metric | Target |
|--------|--------|
| Single key ops (p99) | <1ms |
| Batch write (p99) | <5ms |
| Snapshot creation | <10s for 100MB |
| Throughput | >5,000 ops/sec |

### Optimizations

- Bloom filters on data_kv for point lookups
- 256MB shared block cache
- 64MB write buffers per CF
- Arc<DB> for zero-cost thread sharing
- LZ4 compression for SST files

## Error Handling

| Error | Recovery Strategy |
|-------|------------------|
| RocksDb | Propagate to caller (raft decides retry/fail) |
| Io | Propagate (may need node restart) |
| Serialization | Fail-fast (data corruption) |
| InvalidLogIndex | Return error (need snapshot from leader) |
| SnapshotFailed | Propagate (may retry or request from other node) |
| VersionMismatch | Fail-fast on startup (requires upgrade) |

## Dependencies

### Depends On

- seshat-storage crate - RaftTypeConfig, Operation types, OpenRaft trait definitions
- rocksdb crate v0.22+
- prost crate - protobuf serialization
- thiserror crate - error definitions
- openraft 0.9 migration (complete)
- async-trait crate - async trait implementations

### Used By

- seshat-raft crate - Implements OpenRaft traits
- seshat-kv crate - Via raft for persisting operations
- seshat binary - Orchestrates initialization and lifecycle

## Integration Points (OpenRaft 0.9)

| OpenRaft Trait | Implementation | Column Families |
|----------------|---------------|----------------|
| RaftLogStorage | OpenRaftRocksDBLog | data_raft_log + data_raft_state |
| RaftStateMachine | OpenRaftRocksDBStateMachine | data_kv |
| RaftSnapshotBuilder | OpenRaftRocksDBSnapshotBuilder | - |

## Data Types (Protocol Crate)

Defined in protobuf schema, shared across crates:

- **VersionedLogEntry**: Raft log entries with schema versioning
- **RaftHardState**: Current term, vote, commit index
- **ClusterMembership**: Node registry with addresses and states
- **ShardMap**: Shard assignments and replica placement
- **StoredValue**: User key-value data with metadata and optional TTL
- **SnapshotMetadata**: Snapshot tracking for log compaction

## Phase 1 Scope

### Included

- All 6 column families with optimized settings
- CRUD operations per CF
- Atomic batch writes across CFs
- Raft log operations (append, range, truncate)
- Snapshot creation/restoration via checkpoints
- Iterator support for range queries
- Comprehensive error handling
- Thread-safe concurrent operations

### Deferred to Phase 2+

- **Dynamic CF Creation**: Per-shard CFs (may require DB restart)
- **Advanced Compaction Control**: Manual compaction policies, TTL-based compaction
- **Per-Shard Metrics**: Track metrics per shard
- **Background Metrics Collection**: Push to Prometheus
- **Separate WAL Directory**: Different disk for WAL
- **Online Schema Migration**: Handle version upgrades gracefully

## Estimated Effort

**14-17 hours** total implementation time (+2 hours for openraft trait integration complexity).

| Phase | Name | Hours |
|-------|------|-------|
| T1 | Initialization | 3-4 |
| T2 | CRUD Operations | 3-4 |
| T3 | Raft Log Storage | 3-4 |
| T4 | State Machine Storage | 3-4 |
| T5 | OpenRaft Integration | 2-3 |
