# Crate Architecture

Seshat uses a workspace structure with eight crates, each with clear responsibilities and boundaries.

## Dependency Graph

```
seshat (binary)
  ├─> kv
  ├─> sql
  ├─> protocol-resp
  ├─> protocol-sql
  ├─> raft
  ├─> storage
  └─> common

kv (Redis service)
  ├─> protocol-resp
  ├─> raft
  └─> common

sql (SQL service)
  ├─> protocol-sql
  ├─> raft
  └─> common

protocol-resp (RESP parser/encoder)
  └─> common

protocol-sql (PostgreSQL wire protocol)
  └─> common

raft (consensus + transport)
  ├─> storage
  └─> common

storage (RocksDB wrapper)
  └─> common

common (no dependencies)
```

## Crate Responsibilities

### `seshat/` - Main Binary

**Purpose**: Orchestration and entry point

**Responsibilities**:
- Command-line argument parsing and configuration loading
- Node lifecycle management (startup, shutdown, signal handling)
- Start Redis listener (port 6379) → routes to `kv` service
- Start PostgreSQL listener (port 5432) → routes to `sql` service (Phase 5)
- Health check endpoints and metrics exposition
- Graceful shutdown coordination

**Key Types**:
- `Node`: Main struct that owns all subsystems
- `Config`: Complete node configuration
- `Runtime`: Tokio runtime and task management

**Does NOT**:
- Implement protocol parsing (delegates to `protocol-*`)
- Contain business logic (delegates to `kv`/`sql` services)
- Implement consensus logic (delegates to `raft`)
- Directly access storage (goes through `raft`)

---

### `kv/` - Redis Service Layer

**Purpose**: Redis command execution and business logic

**Responsibilities**:
- Map RESP commands to Raft operations
- Handle Redis-specific semantics (TTL, data types, etc.)
- Command validation and authorization
- Read path optimization (local reads on leader)
- Response formatting

**Key Types**:
- `KvService`: Main service struct
- `KvCommand`: Internal command representation
- `KvResponse`: Response type
- `CommandHandler`: Trait for command execution

**Command Flow**:
```
RespCommand::Get → KvService::handle_get() → RaftNode::read_local() → RespValue
RespCommand::Set → KvService::handle_set() → RaftNode::propose() → RespValue
```

**Dependencies**:
- `protocol-resp`: Parse/encode RESP
- `raft`: Propose writes, read committed state
- `common`: Shared types

**Does NOT**:
- Parse RESP protocol (delegates to `protocol-resp`)
- Implement consensus (delegates to `raft`)
- Access storage directly (goes through `raft`)

---

### `sql/` - SQL Service Layer (Phase 5)

**Purpose**: SQL query execution and business logic

**Responsibilities**:
- Parse SQL AST to Raft operations
- Query planning and optimization
- Transaction management
- Schema validation
- Response formatting in PostgreSQL wire protocol

**Key Types**:
- `SqlService`: Main service struct
- `QueryPlan`: Execution plan
- `SqlCommand`: Internal command representation

**Dependencies**:
- `protocol-sql`: Parse/encode PostgreSQL wire protocol
- `raft`: Propose writes, read committed state
- `common`: Shared types

**Phase**: Not included in Phase 1 MVP

---

### `protocol-resp/` - RESP Protocol Parser

**Purpose**: Redis Serialization Protocol parsing and encoding

**Responsibilities**:
- Parse incoming RESP2/RESP3 messages
- Encode responses in RESP format
- Tokio codec for streaming I/O
- Handle protocol errors and edge cases
- Command parser (GET, SET, DEL, EXISTS, PING)

**Future Key Types**:
- `RespCodec`: Tokio codec for RESP framing
- `RespCommand`: Parsed command enum
- `RespValue`: RESP data types (SimpleString, BulkString, Array, etc.)
- `RespParser`: Low-level parser
- `RespEncoder`: Low-level encoder

**Dependencies**:
- `bytes`: Zero-copy byte buffer handling
- `tokio-util`: Codec framework

**Does NOT**:
- Execute commands (returns parsed commands to `kv` service)
- Access Raft or storage
- Contain business logic

**Status**: ✅ 100% complete (39.9 hours, 487 tests)

---

### `protocol-sql/` - PostgreSQL Wire Protocol (Phase 5)

**Purpose**: PostgreSQL wire protocol parsing and encoding

**Responsibilities**:
- Parse PostgreSQL frontend/backend protocol messages
- SQL statement parsing
- Encode responses in PostgreSQL format
- Handle authentication flow
- Support extended query protocol (prepared statements)

**Key Types**:
- `PgCodec`: Tokio codec for PostgreSQL framing
- `PgMessage`: Message types (Query, Parse, Bind, Execute, etc.)
- `SqlStatement`: Parsed SQL AST
- `PgResponse`: Response formatting

**Dependencies**:
- `bytes`: Zero-copy byte buffer handling
- `tokio-util`: Codec framework
- SQL parser library (TBD)

**Phase**: Not included in Phase 1 MVP

---

### `raft/` - Consensus + Transport Layer

**Purpose**: Raft consensus with integrated gRPC transport

**Responsibilities**:
- Wrap `raft-rs` with application-specific logic
- Implement `Storage` trait for raft-rs (backed by `storage` crate)
- **gRPC server** for Raft messages (RequestVote, AppendEntries, InstallSnapshot)
- **gRPC client** for inter-node communication
- Connection pooling and retry logic for transport
- Leader election and log replication
- Membership changes (add/remove nodes)
- Snapshot creation and restoration
- Log compaction triggers
- State machine (applies committed log entries to key-value store)

**Key Types**:
- `RaftNode`: Wrapper around `raft::RawNode`
- `RaftStorage`: Implements `raft::Storage` trait
- `RaftService`: gRPC service implementation
- `RaftClient`: gRPC client for sending messages
- `StateMachine`: Apply committed log entries (HashMap<Vec<u8>, Vec<u8>>)
- `Operation`: Command enum (Set, Del)

**Protobuf Definitions** (in `raft/proto/`):
- `RequestVoteRequest`/`RequestVoteResponse`
- `AppendEntriesRequest`/`AppendEntriesResponse`
- `InstallSnapshotRequest`/`InstallSnapshotResponse`

**Raft Groups** (Future):
- **System Raft Group**: Cluster metadata (one instance, all nodes participate)
- **Data Raft Groups**: Key-value data (multiple instances, one per shard in Phase 2+)

**Dependencies**:
- `raft-rs`: Core consensus algorithm
- `storage`: Persistent log and snapshot storage
- `tonic`: gRPC framework
- `prost`: Protobuf serialization

**Does NOT**:
- Parse client protocols (receives operations from `kv`/`sql` services)
- Expose client-facing network endpoints (delegates to `seshat`)

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

**Phase 5 (SQL Support)**:
- `sql_tables`, `sql_indexes`, `sql_raft_log`, `sql_raft_state` column families
- **Deployment options** (operator configurable):
  - Same RocksDB as KV data (simpler setup)
  - Separate RocksDB instance at different path (workload isolation)

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
- Include protocol-specific types (those go in `protocol-*`)

---

## Module Interaction Patterns

### Client Request Flow (GET command) - Future

```
1. Client sends: GET foo
2. seshat::Node (Redis listener) receives TCP connection
3. protocol_resp::RespCodec parses → RespCommand::Get("foo")
4. seshat::Node routes to kv::KvService
5. kv::KvService.handle_get("foo")
6. kv calls raft::RaftNode.read_local("foo")
7. raft::StateMachine reads from in-memory HashMap
8. Value returned to kv::KvService
9. kv formats response → RespValue::BulkString
10. protocol_resp::RespCodec encodes response
11. Send back to client
```

### Client Write Flow (SET command) - Future

```
1. Client sends: SET foo bar
2. seshat::Node (Redis listener) receives TCP connection
3. protocol_resp::RespCodec parses → RespCommand::Set("foo", "bar")
4. seshat::Node routes to kv::KvService
5. kv::KvService.handle_set("foo", "bar")
6. kv creates Operation::Set{key: "foo", value: "bar"}
7. kv calls raft::RaftNode.propose(operation)
8. raft::RaftNode checks if leader, returns NotLeader if follower
9. raft-rs replicates log entry to followers via raft::RaftClient (gRPC)
10. Followers' raft::RaftService receives AppendEntries
11. Once majority commits, raft::StateMachine.apply() called
12. storage::Storage persists to data_kv CF
13. Response "+OK\r\n" returned to client
```

### Raft Message Flow (Heartbeats/Replication)

```
1. raft::RaftNode (leader) ticks every 100ms
2. raft-rs generates AppendEntries messages
3. raft::RaftNode sends via raft::RaftClient (gRPC)
4. Target node's raft::RaftService receives
5. Passes to target's raft::RaftNode
6. raft-rs processes, generates response
7. Response sent back via gRPC
8. Leader's raft::RaftNode updates follower progress
```

### Snapshot Creation Flow

```
1. storage::Storage monitors log size
2. When threshold exceeded (10,000 entries), signal raft::RaftNode
3. raft::RaftNode calls raft-rs snapshot()
4. raft::StateMachine serializes current state (HashMap)
5. storage::Storage.create_checkpoint() (RocksDB hard links)
6. raft::RaftNode records snapshot metadata (index, term)
7. raft::RaftNode truncates old log entries via storage::Storage
```

---

## Testing Strategy by Crate

### `protocol-resp/` Tests ✅
- Unit tests: RESP parser/serializer correctness
- Property tests: Round-trip parsing with `proptest`
- Integration tests: Codec with Tokio streams
- **Status**: 487 tests passing

### `kv/` Tests
- Unit tests: Command handling logic
- Integration tests: Command flow with mock Raft
- Property tests: Invariant checking

### `raft/` Tests
- Unit tests: State machine transitions
- Integration tests: Leader election scenarios, single-node bootstrap
- Chaos tests: Network partitions, node failures
- gRPC transport tests: Client-server communication

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

## Layer Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Client Layer                        │
│  (Redis clients, PostgreSQL clients)                 │
└─────────────────────────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────┐
│                 Protocol Layer                       │
│  protocol-resp (RESP2/3)    protocol-sql (PgWire)   │
│  ✅ Complete                ⏳ Phase 5               │
└─────────────────────────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────┐
│                 Service Layer                        │
│  kv (Redis commands)        sql (SQL queries)        │
│  ⏳ Phase 1                 ⏳ Phase 5               │
└─────────────────────────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────┐
│              Consensus + Transport                   │
│  raft (raft-rs + gRPC + state machine)              │
│  ⏳ Phase 1                                          │
└─────────────────────────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────┐
│                 Storage Layer                        │
│  storage (RocksDB + column families)                │
│  ⏳ Phase 1                                          │
└─────────────────────────────────────────────────────┘
```

**Key Principles**:
1. **Protocol Layer**: Parse/encode only, no business logic
2. **Service Layer**: Map protocol commands to Raft operations
3. **Consensus Layer**: Includes both raft-rs wrapper AND gRPC transport
4. **Storage Layer**: Dumb persistence, no Raft awareness

---

## Future Evolution: Adding PostgreSQL Interface

When adding PostgreSQL support (Phase 5):

1. **Implement `protocol-sql/` crate**:
   - PostgreSQL wire protocol parser
   - Message encoding/decoding
   - Authentication flow

2. **Implement `sql/` service crate**:
   - SQL statement parsing
   - Query planning
   - Transaction management
   - Map SQL operations to Raft proposals

3. **Add listener in `seshat/`**:
   - Start PostgreSQL listener on port 5432
   - Route to `sql::SqlService`

4. **Storage considerations for Phase 5**:
   - SQL data stored in RocksDB using column families (same as KV)
   - **Deployment choice** (operator-configurable):
     - **Single RocksDB**: All data in one instance (simpler, lower resources)
     - **Separate RocksDB instances**: KV and SQL isolated (better performance tuning)
   - The `storage` crate will support both configurations
   - Operators choose based on workload requirements

This demonstrates the power of the layered architecture - adding a new protocol is isolated to the protocol and service layers, with storage deployment being a runtime configuration choice.

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
