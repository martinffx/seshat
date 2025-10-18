# Crate Architecture

Seshat uses a workspace structure with six crates, each with clear responsibilities and boundaries.

## Dependency Graph

```
seshat (binary)
  ├─> protocol-resp
  ├─> kv
  ├─> raft
  ├─> storage
  └─> common

protocol-resp
  └─> common

kv
  └─> common

raft
  ├─> kv
  ├─> storage
  └─> common

storage
  └─> common

common (no dependencies)
```

## Crate Responsibilities

### `seshat/` - Main Binary

**Purpose**: Orchestration and entry point

**Responsibilities**:
- Command-line argument parsing and configuration loading
- Node lifecycle management (startup, shutdown, signal handling)
- Wiring together all components (protocol handlers, Raft, storage)
- Health check endpoints and metrics exposition
- Graceful shutdown coordination

**Key Types**:
- `Node`: Main struct that owns all subsystems
- `Config`: Complete node configuration
- `Runtime`: Tokio runtime and task management

**Does NOT**:
- Implement protocol parsing (delegates to `protocol-resp`)
- Implement consensus logic (delegates to `raft`)
- Directly access storage (goes through `storage`)

---

### `kv/` - Key-Value Operations

**Purpose**: Domain logic for key-value operations

**Responsibilities**:
- Define KV operation types (Set, Get, Delete)
- Operation serialization/deserialization
- Domain-specific error handling
- Operation validation logic

**Key Types**:
- `Operation`: Enum of all KV operations (Set, Del)
- `OperationResult`: Result type for KV operations
- `OperationError`: KV-specific errors

**Dependencies**:
- `serde`: Serialization framework
- `bincode`: Binary serialization for Raft proposals
- `thiserror`: Error derivation

**Does NOT**:
- Parse wire protocols (delegates to `protocol-resp`)
- Execute operations (delegates to Raft/storage)
- Manage consensus (delegates to `raft`)

---

### `protocol-resp/` - RESP2 Protocol Handler

**Purpose**: Handle Redis RESP2 wire protocol

**Current Status**: Placeholder crate for future implementation

**Future Responsibilities**:
- **RESP Protocol**: Redis Serialization Protocol parser and serializer
  - Parse incoming Redis commands (GET, SET, DEL, EXISTS, PING)
  - Serialize responses in RESP format
  - Handle protocol errors and edge cases
- **TCP Framing**: Tokio codec for RESP2 framing

**Future Key Types**:
- `RespCodec`: Tokio codec for RESP framing
- `RespCommand`: Parsed command enum
- `RespValue`: Response type

**Future Dependencies**:
- `tokio`: Async I/O and codec framework
- `bytes`: Efficient byte buffer handling

**Note**: RESP implementation will be merged from `feat/resp` branch when ready.

---

### `raft/` - Consensus Layer

**Purpose**: Implement Raft consensus using raft-rs

**Responsibilities**:
- Wrap `raft-rs` with application-specific logic
- Implement `Storage` trait for raft-rs (backed by `storage` crate)
- Handle Raft message routing and processing
- Leader election and log replication
- Membership changes (add/remove nodes)
- Snapshot creation and restoration
- Log compaction triggers
- Re-export raft-rs message types for transport layer

**Key Types**:
- `RaftNode`: Wrapper around `raft::RawNode`
- `StateMachine`: Apply committed log entries using `kv::Operation`
- `MemStorage`: In-memory implementation of raft-rs `Storage` trait
- Re-exported from raft-rs: `Message`, `Entry`, `MessageType`, `Snapshot`

**Raft Groups** (Future):
- **System Raft Group**: Cluster metadata (one instance, all nodes participate)
- **Data Raft Groups**: Key-value data (multiple instances, one per shard in Phase 2+)

**Dependencies**:
- `raft` (raft-rs): Core consensus algorithm with built-in message types
- `seshat-kv`: KV operations for state machine
- `seshat-common`: Shared types
- `serde`, `bincode`: Serialization
- `tokio`: Async runtime
- `slog`: Logging

**Transport Layer**:
- This crate uses raft-rs's built-in message types (`raft::prelude::Message`)
- Transport layer (gRPC, TCP, etc.) should be implemented separately
- Network layer serializes/deserializes `Message` for transmission

**Does NOT**:
- Parse client protocols (receives `kv::Operation` from caller)
- Implement network transport (provides message types, not transport)
- Decide when to compact (receives triggers from storage)

---

### `storage/` - Persistence Layer

**Purpose**: Abstract RocksDB storage with column families

**Responsibilities**:
- RocksDB wrapper with safe Rust API
- Column family management
- Atomic batch writes
- Snapshot creation (RocksDB checkpoints)
- Iterator support for scans
- Storage metrics and resource tracking
- Enforce data size limits

**Column Families**:

**Phase 1 (Single Shard)**:
- `system_raft_log`: System group Raft log entries
- `system_raft_state`: System group hard state (term, vote, commit index)
- `system_data`: Cluster metadata (membership table, configuration)
- `data_raft_log`: Data shard Raft log entries
- `data_raft_state`: Data shard hard state
- `data_kv`: Actual key-value pairs

**Phase 2+ (Multi-Shard)**:
- Additional `shard_N_raft_log`, `shard_N_raft_state`, `shard_N_kv` per shard

**Key Types**:
- `Storage`: Main storage interface
- `ColumnFamily`: Enum of all column families
- `WriteBatch`: Atomic multi-CF writes
- `Snapshot`: Point-in-time consistent view
- `StorageMetrics`: Size and performance metrics

**Dependencies**:
- `rocksdb`: Embedded key-value store
- `serde`: Serialization framework

**Does NOT**:
- Understand Raft semantics (just stores bytes)
- Parse protocol messages
- Manage network connections

---

### `common/` - Shared Types and Utilities

**Purpose**: Types and utilities used across multiple crates

**Responsibilities**:
- Core data structures used by multiple crates
- Error types with `thiserror`
- Configuration types
- Utility functions (hashing, serialization helpers)
- Type aliases and constants

**Key Types**:
- `NodeId`: `u64` node identifier
- `Term`: `u64` Raft term
- `LogIndex`: `u64` Raft log index
- `Key`: Key type (byte array)
- `Value`: Value type (byte array)
- `Error`: Application error enum
- `Result<T>`: `std::result::Result<T, Error>`

**Configuration Types**:
- `NodeConfig`: Node-specific settings
- `ClusterConfig`: Cluster-wide settings
- `RaftConfig`: Raft timing parameters
- `StorageConfig`: Storage paths and limits

**Data Structures** (see `data-structures.md` for details):
- `ClusterMembership`: Node registry
- `ShardMap`: Shard assignments
- `VersionedLogEntry`: Raft log entry with schema version

**Dependencies**:
- `serde`: Serialization
- `thiserror`: Error derivation
- `anyhow`: Error context (only in `seshat` binary)

**Does NOT**:
- Contain business logic
- Depend on any other Seshat crate
- Include protocol-specific types (those go in `protocol`)

---

## Module Interaction Patterns

### Client Request Flow (GET command) - Future

```
1. Client sends: GET foo
2. protocol-resp::RespCodec parses → RespCommand::Get("foo")
3. seshat::Node receives command
4. seshat::Node checks: is this node leader for data shard?
5. If leader:
   - Read from storage::Storage (data_kv CF)
   - protocol-resp::RespCodec serializes response
   - Send back to client
6. If not leader:
   - Look up leader from raft::RaftNode
   - Transport layer forwards to leader
   - Receive response, forward to client
```

### Client Write Flow (SET command) - Future

```
1. Client sends: SET foo bar
2. protocol-resp::RespCodec parses → RespCommand::Set("foo", "bar")
3. seshat::Node converts to kv::Operation::Set
4. seshat::Node routes to raft::RaftNode
5. raft::RaftNode.propose(kv::Operation::Set)
6. raft-rs replicates log entry to followers via transport layer
7. Once majority commits, raft::StateMachine.apply() called
8. StateMachine executes kv::Operation
9. storage::Storage writes to data_kv CF
10. Response returned to client
```

### Raft Message Flow (Heartbeats/Replication)

```
1. raft::RaftNode (leader) ticks every 100ms
2. raft-rs generates raft::Message (AppendEntries)
3. raft::RaftNode passes Message to transport layer
4. Transport layer (gRPC/TCP) serializes and sends Message
5. Target node's transport receives and deserializes Message
6. Passes to target's raft::RaftNode
7. raft-rs processes, generates response Message
8. Response sent back via transport layer
9. Leader's raft::RaftNode updates follower progress
```

### Snapshot Creation Flow

```
1. storage::Storage monitors log size
2. When threshold exceeded (10,000 entries), signal raft::RaftNode
3. raft::RaftNode calls raft-rs snapshot()
4. raft::StateMachine serializes current state
5. storage::Storage.create_checkpoint() (RocksDB hard links)
6. raft::RaftNode records snapshot metadata (index, term)
7. raft::RaftNode truncates old log entries via storage::Storage
```

---

## Testing Strategy by Crate

### `protocol/` Tests
- Unit tests: RESP parser/serializer correctness
- Property tests: Round-trip parsing with `proptest`
- Integration tests: gRPC client-server communication

### `raft/` Tests
- Unit tests: State machine transitions
- Integration tests: Leader election scenarios
- Chaos tests: Network partitions, node failures

### `storage/` Tests
- Unit tests: Column family operations
- Integration tests: Snapshot consistency
- Performance tests: Throughput and latency

### `common/` Tests
- Unit tests: Data structure serialization
- Property tests: Invariant checking

### `seshat/` Tests
- Integration tests: Full end-to-end flows
- Chaos tests: 11 scenarios from PRD
- Performance tests: Redis benchmark compatibility

---

## Future Evolution: Adding PostgreSQL Interface

When adding PostgreSQL support (Phase 5+):

1. **New crate**: `protocol-sql/`
   - Implement PostgreSQL wire protocol parser
   - Support basic SQL commands (SELECT, INSERT, UPDATE, DELETE)
   - Minimal crate, similar to `protocol-resp/`

2. **New crate**: `sql/`
   - SQL-specific domain logic
   - Translate SQL operations to storage operations
   - Similar role to `kv/` but for SQL

3. **Binary mode selection in `seshat/`**:
   - Command-line flag: `--mode {redis|postgres}`
   - Start either `protocol-resp` OR `protocol-sql` listener
   - Use either `kv/` OR `sql/` domain logic
   - **Cannot run both modes simultaneously**
   - **Different data files for each mode**

4. **No changes needed in**:
   - `raft/`: Same consensus layer
   - `storage/`: Same RocksDB backend
   - `common/`: Shared types remain

This demonstrates the power of the layered architecture - adding a new protocol requires new protocol and domain crates, but the consensus and storage layers remain unchanged.

---

## Versioning and Compatibility

All crates use the same version number from the workspace root:

```toml
[workspace.package]
version = "0.1.0"
edition = "2021"
```

**Compatibility Rules**:
- Breaking changes to `common/` types → bump major version
- Protocol changes (RESP, gRPC) → bump minor version
- Implementation changes → bump patch version

**Schema Versioning**:
All persisted data includes version markers (see `data-structures.md`) to enable future migrations.
