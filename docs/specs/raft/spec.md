# Raft Consensus Implementation - Feature Specification

**Feature**: `raft`
**Status**: Planning
**Phase**: 1 (MVP)
**Aligns With**: Single shard cluster with Raft consensus, in-memory storage, 3-node fault tolerance

## Overview

Implement Raft consensus for Seshat using the raft-rs library with in-memory storage (MemStorage) and gRPC transport. This provides fault-tolerant strong consistency across a 3-node cluster without the complexity of persistent storage in Phase 1.

**Focus**: This specification covers implementing the Storage trait, state machine, Protobuf message definitions, RaftNode wrapper, configuration validation, and full gRPC networking layer (client and server) with connection pooling and retry logic. It includes a 2-node integration test to verify message exchange works correctly.

## User Story

> As a distributed systems engineer building Seshat, I want to implement Raft consensus with in-memory storage and gRPC transport so that I can achieve fault-tolerant strong consistency across a 3-node cluster without the complexity of persistent storage.

## Acceptance Criteria

1. **GIVEN** a MemStorage implementation **WHEN** storing log entries **THEN** entries can be retrieved by index range
2. **GIVEN** a MemStorage implementation **WHEN** querying first_index/last_index **THEN** correct indices are returned
3. **GIVEN** Protobuf message definitions **WHEN** serializing Raft messages (AppendEntries, RequestVote) **THEN** they can be deserialized correctly
4. **GIVEN** a state machine **WHEN** applying a committed SET operation **THEN** the key-value is stored in the HashMap
5. **GIVEN** a state machine **WHEN** applying a committed DEL operation **THEN** the key is removed from the HashMap
6. **GIVEN** a state machine **WHEN** applying multiple operations **THEN** they are applied in order
7. **GIVEN** a configuration with bootstrap=true **WHEN** loading **THEN** it validates all required fields and initial_members
8. **GIVEN** a RaftNode wrapper **WHEN** created **THEN** it correctly initializes raft-rs RawNode with the provided configuration
9. **GIVEN** a gRPC server **WHEN** it receives a Raft RPC **THEN** it routes the message to the local RaftNode
10. **GIVEN** two RaftNodes connected via gRPC **WHEN** one sends a RequestVote message **THEN** the other receives and processes it
11. **GIVEN** two RaftNodes connected via gRPC **WHEN** one sends an AppendEntries message **THEN** the other receives and processes it

## Business Rules

1. Must use raft-rs library version 0.7+ (not implementing Raft from scratch)
2. Phase 1 uses MemStorage only - in-memory Vec/HashMap for Raft log and state (no RocksDB persistence)
3. Majority quorum (2 out of 3 nodes) is required for all write operations to commit
4. Must use gRPC (tonic 0.11+) for all inter-node Raft message exchange on port 7379
5. All 3 nodes participate in the single Raft group (no sharding in Phase 1)
6. Nodes identify each other using DNS names (e.g., 'kvstore-1:7379') or IP:port addresses
7. Must support three Raft RPC types: RequestVote, AppendEntries, and basic InstallSnapshot
8. Timing constraints: heartbeat_interval=100ms, election_timeout=500-1000ms, rpc_timeout=5s
9. Bootstrap mode: all 3 nodes start with same initial_members configuration list
10. Configuration validation: node_id > 0, initial_members includes this node, at least 3 members, no duplicate IDs
11. State machine operations limited to SET/GET/DEL on in-memory HashMap for Phase 1
12. Log compaction and snapshots are simplified for Phase 1 (full implementation in Phase 4)
13. Every node is identical - no special roles or separate gateway/metadata/data tiers
14. Must use tokio 1.x async runtime for all I/O operations
15. Structured logging with tracing for all Raft events (term changes, elections, replication)

## Scope

### Included

- Wrap raft-rs RawNode with application-specific RaftNode struct
- Implement raft-rs Storage trait using MemStorage (in-memory HashMap/Vec for log entries)
- Define gRPC service with Protobuf message types (AppendEntries, RequestVote, InstallSnapshot)
- Define Protobuf schemas for Raft messages with proper field types
- gRPC service definition (.proto file) for Raft RPC methods
- Basic state machine that applies SET/GET/DEL operations to in-memory HashMap<Vec<u8>, Vec<u8>>
- Cluster bootstrap logic: 3 nodes start with same initial membership configuration
- Configuration structs for node ID, bind addresses, initial members, Raft timing parameters
- Leader transfer on graceful shutdown
- Structured logging for all Raft events using tracing + tracing-subscriber
- **gRPC server implementation** (receives Raft RPCs on port 7379, routes to local RaftNode)
- **gRPC client implementation** (RaftNode sends Raft messages to peer nodes)
- **Connection pooling** using tonic's built-in features
- **Retry logic** with exponential backoff for failed RPCs
- Unit tests for Storage trait, state machine, message serialization, config validation
- **Integration test**: 2-node message exchange (verify RequestVote/AppendEntries work via gRPC)

### Excluded

- RocksDB persistent storage (deferred to rocksdb-storage spec - Phase 1 uses in-memory only)
- RESP protocol parser/server (deferred to resp-protocol-mvp spec)
- **Multi-node cluster tests with 3+ nodes** (leader election, failover) (deferred to chaos-testing spec)
- Chaos tests: leader failure, network partitions, split-brain scenarios (separate chaos-testing spec)
- Full log compaction and snapshot optimization (simplified for Phase 1, full in Phase 4)
- Multiple Raft groups / sharding (Phase 2)
- Dynamic membership changes (add/remove nodes) (Phase 3)
- Prometheus metrics and monitoring (Phase 4)
- Follower reads optimization (Phase 4)
- Advanced snapshot transfer optimization with chunking (Phase 4)
- Performance tuning and benchmarking (Phase 4)

## Dependencies

### Requires

- common crate: NodeId, Term, LogIndex, Key, Value types
- common crate: RaftHardState, VersionedLogEntry, SnapshotMetadata types
- common crate: NodeConfig, ClusterConfig, RaftConfig structs
- common crate: ClusterMembership, NodeInfo, NodeState types
- common crate: Error enum with NotLeader, NoQuorum variants
- tokio 1.x runtime with full features
- raft-rs 0.7+ library
- tonic 0.11+ and prost 0.12+ for gRPC
- tracing + tracing-subscriber for structured logging
- thiserror for error definitions

### Used By

- seshat binary (main orchestration crate)
- protocol-resp crate (gRPC service definitions)
- storage crate (future RocksDB integration in rocksdb-storage spec)

## Technical Details

### Raft Implementation

**Library**: raft-rs 0.7+

**Approach**: Wrap RawNode (not Raft) for message-level control, implement Storage trait

**Storage Trait Methods**:
- `initial_state() -> Result<(HardState, ConfState)>` - Return initial Raft state
- `entries(low: u64, high: u64, max_size: Option<u64>) -> Result<Vec<Entry>>` - Retrieve log entries
- `term(index: u64) -> Result<u64>` - Get term for specific log index
- `first_index() -> Result<u64>` - Get first available log index
- `last_index() -> Result<u64>` - Get last available log index
- `snapshot() -> Result<Snapshot>` - Get current snapshot

**In-Memory Storage**:
- **log_entries**: `Vec<Entry>` for Raft log storage
- **hard_state**: `RaftHardState { version: u8, term: u64, vote: Option<u64>, commit: u64 }`
- **conf_state**: `ConfState` with voters and learners lists
- **implementation**: Use raft-rs built-in MemStorage or implement simple wrapper

**Ready Processing**: Call tick() periodically, process Ready: persist → send messages → apply → advance

**Bootstrap**: Create Raft group with all 3 members in initial configuration

### gRPC Transport

**Framework**: tonic 0.11+ (gRPC framework) + prost 0.12+ (Protobuf serialization)

**Port**: 7379

**Messages**:
- **AppendEntries**: `{ term, leader_id, prev_log_index, prev_log_term, entries[], leader_commit }`
- **RequestVote**: `{ term, candidate_id, last_log_index, last_log_term }`
- **InstallSnapshot**: `{ term, leader_id, last_included_index, last_included_term, data, done }`

**Client** (INCLUDED in this spec):
- Connection pooling: One connection pool per peer node
- Retry logic: Exponential backoff for failed RPCs
- Timeout: 5 seconds RPC timeout

**Server** (INCLUDED in this spec):
- Bind address: 0.0.0.0:7379
- Async handlers: All RPC handlers are async with tokio
- Routes received RPCs to local RaftNode instance

**Features**:
- HTTP/2 connection multiplexing
- Streaming support for snapshot transfers
- Connection reuse across multiple RPCs

### State Machine

**Storage**: `HashMap<Vec<u8>, Vec<u8>>` for key-value pairs

**Operations**:
- **SET key value** - Insert or update key-value pair
- **DEL key** - Remove key-value pair

**Reads**: GET key - Read from HashMap (leader-only reads in Phase 1)

**Apply Logic**: Process committed log entries and execute operations against HashMap

**Serialization**: Operations serialized as Protobuf messages in log entries

### Configuration

**Bootstrap Mode**:
- All 3 nodes start with identical initial_members list
- **node_config**: `{ id: u64, client_addr: '0.0.0.0:6379', internal_addr: '0.0.0.0:7379', data_dir: Path }`
- **cluster_config**: `{ bootstrap: true, initial_members: Vec<NodeInfo>, replication_factor: 3 }`
- **example_member**: `NodeInfo { id: 1, addr: 'kvstore-1:7379', state: NodeState::Active, joined_at: timestamp }`

**Raft Timing**:
- heartbeat_interval_ms: 100
- election_timeout_min_ms: 500
- election_timeout_max_ms: 1000
- raft_rpc_timeout_secs: 5

**Validation Rules**:
- node_id must be > 0
- initial_members must include this node's ID
- At least 3 members required for fault tolerance
- No duplicate node IDs allowed
- replication_factor must be odd (3 in Phase 1)

### Async Runtime

**Library**: tokio 1.x with full features

**Usage**:
- All I/O operations are async
- Timer-driven tick() calls for Raft
- Async gRPC handlers
- Background tasks for message processing

### Logging

**Library**: tracing + tracing-subscriber

**Requirements**:
- Structured logging for all Raft events
- Log term changes, elections, log replication, commits
- Use #[instrument] attribute for async functions
- JSON output format for production environments
- Include context: node_id, term, log_index in all events

### Error Handling

**Libraries**: thiserror for Error enum definition, anyhow for binary/main

**Required Errors**:
- `NotLeader { leader_id: Option<u64> }`
- `NoQuorum`
- `RaftError(raft::Error)`
- `StorageError`
- `RpcError`
- `ConfigError`

**Patterns**:
- All errors include context via #[error(...)]
- Propagate errors with ? operator
- Fail fast: validate configuration on startup

## Testing Strategy

### Unit Tests

Focus on testing our business logic:

- **Storage trait implementation**: initial_state, entries, term, first_index, last_index, snapshot
- **State machine apply logic**: SET/DEL operations, operation ordering
- **Configuration validation rules**: node_id validation, duplicate IDs, initial_members validation
- **Protobuf message serialization/deserialization**: Verify our message definitions work correctly
- **Error type conversions and context**: Ensure proper error propagation
- **RaftNode initialization**: Verify correct initialization with valid configuration

### Integration Tests

- **2-node message exchange**: Start 2 RaftNode instances, send RequestVote and AppendEntries messages via gRPC, verify both nodes receive and process messages correctly

**Not testing (deferred to chaos-testing spec)**:
- 3+ node cluster behavior (leader election, failover)
- Network partitions and split-brain scenarios
- Leader crashes and recovery

### Tools

- tokio-test for async test utilities (if needed for state machine)
- proptest for property-based testing of edge cases

### TDD Workflow

1. Write test case for specific functionality
2. Write minimal implementation to pass test
3. Refactor code while keeping tests green
4. Repeat for next functionality

## Success Metrics

1. Storage trait fully implemented: all methods (initial_state, entries, term, first_index, last_index, snapshot) working correctly
2. MemStorage can store and retrieve log entries by index range
3. MemStorage correctly tracks first_index and last_index
4. State machine correctly applies SET operations to HashMap
5. State machine correctly applies DEL operations to HashMap
6. State machine correctly applies operations in order
7. Protobuf definitions compile and messages serialize/deserialize correctly
8. RaftNode wrapper successfully initializes raft-rs RawNode with configuration
9. Configuration validation catches all invalid configurations (missing node_id, duplicate IDs, etc.)
10. **gRPC server starts on port 7379 and accepts Raft RPC connections**
11. **gRPC server receives Raft RPCs and successfully routes them to local RaftNode**
12. **gRPC client successfully sends Raft messages to peer nodes**
13. **Integration test passes: 2 nodes exchange RequestVote and AppendEntries messages via gRPC**
14. **Connection pooling works correctly with multiple peers**
15. **Retry logic handles failed RPCs with exponential backoff**
16. 100% test coverage for Storage trait and state machine implementation
17. All unit tests pass with no panics or unwraps

## References

- raft-rs documentation: https://github.com/tikv/raft-rs
- Raft paper: https://raft.github.io/raft.pdf
- tonic gRPC: https://github.com/hyperium/tonic
- Architecture: `docs/architecture/crates.md`
- Standards: `docs/standards/tech.md`, `docs/standards/practices.md`
