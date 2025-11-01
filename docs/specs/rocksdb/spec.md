# RocksDB Storage Layer Specification

## User Story

As a Seshat node operator, I want persistent storage using RocksDB so that the cluster can recover state after restarts and maintain consistency across nodes

## Acceptance Criteria

1. GIVEN a fresh node startup WHEN RocksDB initializes THEN all 6 column families (system_raft_log, system_raft_state, system_data, data_raft_log, data_raft_state, data_kv) are created with correct configuration

2. GIVEN a key-value operation WHEN storage.get/put/delete is called THEN operation completes with <1ms p99 latency for local storage access

3. GIVEN Raft log entries WHEN append/get_range/truncate operations execute THEN sequential ordering is preserved and no gaps exist in log indices

4. GIVEN an atomic batch write across multiple column families WHEN batch.commit is called THEN either all writes succeed or all fail (no partial commits)

5. GIVEN a snapshot trigger condition (10,000 entries OR 100MB) WHEN snapshot is created THEN RocksDB checkpoint succeeds and metadata records last_included_index

6. GIVEN a node restart WHEN RocksDB reopens existing database THEN all persisted data (keys, raft state, metadata) is accessible and version checks pass

7. GIVEN operations on different column families WHEN concurrent reads/writes occur THEN data isolation is maintained (no cross-contamination between CFs)

## Business Rules

- System Raft log CF: Store system group Raft entries, compact after snapshot (~10MB typical size)
- System Raft state CF: Single-key storage for hard state (term, vote, commit), MUST fsync before responding to RPCs
- System data CF: Store cluster metadata (ClusterMembership, ShardMap), bounded ~100KB size
- Data Raft log CF: Store data shard log entries, snapshot every 10,000 entries or 100MB, ~100MB typical compacted size
- Data Raft state CF: Single-key storage for data shard hard state, MUST fsync before responding to RPCs
- Data KV CF: Store user key-value data wrapped in StoredValue, unbounded size, optimize for point lookups
- All write batches across column families MUST be atomic
- All persisted structures MUST include version field for schema evolution
- Key size limit: 256 bytes maximum (enforced by validation layer above storage)
- Value size limit: 65,536 bytes maximum (enforced by validation layer above storage)
- Raft log memory limit: 512MB per Raft group before forced compaction
- Storage layer MUST NOT understand Raft semantics - only stores bytes as directed

## Scope

### Included

- RocksDB initialization with 6 column families and optimized configuration (Lz4 compression, 64MB buffers, prefix bloom filters)
- CRUD operations per column family (get, put, delete, exists)
- Atomic batch write operations across multiple column families
- Raft log operations: append entry, get range of entries, truncate before index
- Snapshot creation using RocksDB checkpoint (hard links, atomic)
- Snapshot restoration from checkpoint directory
- Configuration management: load/store NodeConfig, ClusterConfig, RaftConfig
- Error handling with rich context propagation (thiserror)
- Iterator support for range queries within column families
- Storage metrics tracking (db_size_bytes, num_keys, snapshot_duration)

### Excluded

- TTL expiration logic (Phase 2 - handled by higher layer)
- Distributed locking implementation (Phase 2 - separate feature)
- Metrics/observability export (Phase 4 - OpenTelemetry integration)
- Multi-shard column family management (Phase 2 - dynamic shard creation)
- Online schema migration tools (Phase 3 - separate migration system)
- RocksDB tuning dashboard (Phase 4 - operational tooling)
- Automatic compaction scheduling (use RocksDB defaults for Phase 1)

## Dependencies

### Depends On
- **seshat-storage crate** - RaftTypeConfig, Operation types, OpenRaft trait definitions
- **rocksdb crate** - underlying storage engine (v0.22+)
- **bincode crate** - serialization for state machine data
- **serde crate** - serialization trait implementations
- **thiserror crate** - error type definitions
- **openraft 0.9 migration** (✅ COMPLETE) - Phase 2 complete, split storage-v2 API implemented
- **async-trait crate** - for async trait implementations

### Used By
- **seshat-storage crate** - Will use OpenRaftRocksDBLog and OpenRaftRocksDBStateMachine
- **seshat-kv crate** - Indirectly via seshat-storage for persisting operations
- **seshat binary** - Orchestrates initialization and lifecycle

### Integration Points (OpenRaft 0.9 storage-v2)
- **RaftLogStorage trait** - Implemented by OpenRaftRocksDBLog (log entries, vote storage)
- **RaftStateMachine trait** - Implemented by OpenRaftRocksDBStateMachine (state operations, snapshots)
- **RaftSnapshotBuilder trait** - Implemented by OpenRaftRocksDBSnapshotBuilder
- **Column Families** - Map to split storage traits:
  - `data_raft_log` + `data_raft_state` → RaftLogStorage
  - `data_kv` → RaftStateMachine
- **Config loading** - NodeConfig specifies data_dir path for RocksDB

## Technical Details

### Column Families

1. system_raft_log
   - Purpose: System group Raft log entries
   - Key Format: log:{index}
   - Value Type: LogEntry (protobuf via prost)
   - Size: ~10MB compacted
   - Compaction: Truncate after snapshot

2. system_raft_state
   - Purpose: System group hard state
   - Key Format: state (single key)
   - Value Type: RaftHardState (protobuf via prost)
   - Size: <1KB
   - Durability: fsync required before RPC responses

3. system_data
   - Purpose: Cluster metadata
   - Key Format: membership, shardmap
   - Value Type: ClusterMembership, ShardMap (protobuf via prost)
   - Size: ~100KB bounded
   - Compaction: Automatic by RocksDB

4. data_raft_log
   - Purpose: Data shard Raft log entries
   - Key Format: log:{index}
   - Value Type: LogEntry (protobuf via prost)
   - Size: ~100MB compacted
   - Compaction: Snapshot every 10,000 entries or 100MB

5. data_raft_state
   - Purpose: Data shard hard state
   - Key Format: state (single key)
   - Value Type: RaftHardState (protobuf via prost)
   - Size: <1KB
   - Durability: fsync required before RPC responses

6. data_kv
   - Purpose: User key-value data
   - Key Format: raw user key (arbitrary bytes)
   - Value Type: StoredValue (protobuf via prost)
   - Size: Unbounded (user data)
   - Optimization: Prefix bloom filters for point lookups

### Performance Requirements

- Local Storage Ops (p99): <1ms (get, put, delete on single key)
- Batch Commit (p99): <5ms (atomic writes across CFs)
- Snapshot Creation: <10s for 100MB data
- Throughput Target: >5,000 ops/sec per node
- Concurrent Operations: Thread-safe for concurrent reads and writes

## Alignment

This feature aligns with Phase 1 MVP - Persistent storage foundation for single-shard cluster. Enables cluster recovery after restarts, provides durability for Raft consensus, and stores user key-value data. Critical blocker for 3-node cluster stability testing.

## Estimated Effort

**14-17 hours** total implementation time:
- +2 hours for openraft trait integration complexity
- Async trait implementation overhead
- Testing with openraft types