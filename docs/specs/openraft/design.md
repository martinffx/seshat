# Technical Design: OpenRaft Migration

## Overview

This migration replaces the existing `raft-rs` implementation with `openraft`, focusing on a simplified, in-memory storage approach for Seshat's distributed consensus layer. The primary goals are:

- Eliminate prost version conflicts
- Maintain existing in-memory storage semantics
- Provide a clean, async-first implementation
- Preserve existing state machine behavior
- Create a stub network transport for future gRPC integration

## Architecture

### System Overview

The migration shifts from `raft-rs`'s `RawNode<MemStorage>` to `openraft::Raft<RaftTypeConfig>`, introducing:

- Async-first design
- Cleaner trait-based storage interface
- Built-in leader election and log replication
- Simplified configuration and runtime management

### Component Architecture

```
+-------------------+
| Client Operations |
+--------+----------+
         |
         v
+--------+----------+
| RaftNode Wrapper  |
| (openraft::Raft)  |
+--------+----------+
         |
         v
+--------+----------+
| OpenRaftMemStorage|
| (Storage Traits)  |
+--------+----------+
         |
         v
+--------+----------+
| StateMachine      |
| (Idempotent Apply)|
+-------------------+
```

### Crate Structure

- `crates/raft/`: Core Raft node and configuration
- `crates/storage/`: In-memory storage implementation
- `crates/common/`: Shared types and utilities

## Type System Design

### OpenRaft Type Configuration

```rust
pub struct RaftTypeConfig;

impl openraft::RaftTypeConfig for RaftTypeConfig {
    type NodeId = u64;
    type Node = BasicNode;
    type Entry = LogEntry<Request>;
    type SnapshotData = Vec<u8>;
    type AsyncRuntime = TokioRuntime;
}
```

### Request Type Definition

The `Request` type wraps serialized operations from the KV/SQL service layers:

```rust
/// Request wrapper for operations submitted to Raft.
///
/// This type bridges the service layer (KV/SQL) and the Raft layer by
/// wrapping serialized Operation bytes in a protobuf-compatible format.
#[derive(Debug, Clone, prost::Message)]
pub struct Request {
    /// Serialized Operation from KV or SQL service.
    ///
    /// Format: protobuf-encoded Operation (e.g., Operation::Set, Operation::Del)
    /// The Raft layer treats this as opaque bytes - deserialization happens
    /// in the StateMachine during apply().
    #[prost(bytes = "vec", tag = "1")]
    pub operation_bytes: Vec<u8>,
}

impl Request {
    /// Create a new Request from serialized operation bytes.
    pub fn new(operation_bytes: Vec<u8>) -> Self {
        Self { operation_bytes }
    }
}

// Conversion from KV Service Operation to Request
impl From<Operation> for Request {
    fn from(op: Operation) -> Self {
        Request {
            operation_bytes: op.encode_to_vec(),
        }
    }
}
```

**Type Hierarchy:**

```
KV Service Operation (e.g., Operation::Set { key, value })
    ↓ (serialize with prost)
Request { operation_bytes: Vec<u8> }
    ↓ (wrap in LogEntry)
LogEntry<Request> { log_id, data: Request }
    ↓ (serialize with prost)
Vec<u8> (stored in RocksDB via storage crate)
```

### Type Conversions

| raft-rs Type | openraft Type | Conversion Strategy |
|--------------|--------------|---------------------|
| `eraftpb::Entry` | `LogEntry<Request>` | Create with `log_id` and `data` |
| `eraftpb::HardState` | `Vote` + `LogId` | Extract term, node_id, commit index |
| `eraftpb::ConfState` | `Membership` | Convert voters/learners to `BTreeSet` |

## Component Specifications

### Storage Layer (OpenRaftMemStorage)

Implements three critical openraft storage traits:
- `RaftLogReader`: Read log entries and vote state
- `RaftSnapshotBuilder`: Create snapshots
- `RaftStorage`: Mutation and state tracking

**Key Traits Implementation**:

```rust
struct OpenRaftMemStorage {
    vote: RwLock<Option<Vote<u64>>>,
    log: RwLock<BTreeMap<u64, LogEntry<Request>>>,
    snapshot: RwLock<Option<Snapshot<RaftTypeConfig>>>,
    state_machine: RwLock<StateMachine>,
    membership: RwLock<StoredMembership<u64, BasicNode>>
}
```

### State Machine Wrapper

```rust
struct OpenRaftStateMachine {
    inner: Arc<RwLock<StateMachine>>
}

impl RaftStateMachine for OpenRaftStateMachine {
    async fn apply(&mut self, entries: &[LogEntry<Request>]) -> Result<Vec<Response>> {
        let mut responses = Vec::new();
        for entry in entries {
            let data = &entry.payload.data;
            let result = self.inner.write().unwrap()
                .apply(entry.log_id.index, data)?;
            responses.push(Response { result });
        }
        Ok(responses)
    }
}
```

### Raft Node Migration

**Key Changes**:
- Async methods
- `client_write()` replaces `propose()`
- Removed `tick()` and `handle_ready()`
- Direct state machine access

```rust
struct RaftNode {
    raft: Raft<RaftTypeConfig>,
    storage: Arc<OpenRaftMemStorage>
}

impl RaftNode {
    async fn propose(&self, data: Vec<u8>) -> Result<()> {
        let request = ClientWriteRequest::new(Request { data });
        self.raft.client_write(request).await
    }

    async fn is_leader(&self) -> bool {
        self.raft.is_leader().await
    }
}
```

### Network Stub

A placeholder implementation for future gRPC transport:

```rust
struct StubNetwork {
    node_id: u64
}

#[async_trait]
impl RaftNetwork<RaftTypeConfig> for StubNetwork {
    async fn send_append_entries(&self, _req: AppendEntriesRequest)
        -> Result<AppendEntriesResponse> {
        Ok(Default::default())
    }
    // Similar stubs for vote and snapshot
}
```

## Error Handling

### Error Type Mapping

OpenRaft errors must be mapped to application-level errors for proper handling in the KV/SQL service layers:

```rust
use openraft::error::{ClientWriteError, RaftError, StorageError};

/// Raft layer error type
#[derive(Debug, thiserror::Error)]
pub enum RaftError {
    #[error("Not the leader (current leader: {leader_id:?})")]
    NotLeader { leader_id: Option<u64> },

    #[error("No quorum available")]
    NoQuorum,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError<RaftTypeConfig>),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("OpenRaft error: {0}")]
    OpenRaft(String),
}

/// Convert OpenRaft ClientWriteError to RaftError
impl From<ClientWriteError<RaftTypeConfig>> for RaftError {
    fn from(err: ClientWriteError<RaftTypeConfig>) -> Self {
        match err {
            ClientWriteError::ForwardToLeader(forward) => {
                RaftError::NotLeader {
                    leader_id: forward.leader_id,
                }
            }
            ClientWriteError::ChangeMembershipError(e) => {
                RaftError::OpenRaft(e.to_string())
            }
        }
    }
}

/// Convert OpenRaft RaftError to our RaftError
impl From<openraft::error::RaftError<RaftTypeConfig>> for RaftError {
    fn from(err: openraft::error::RaftError<RaftTypeConfig>) -> Self {
        RaftError::OpenRaft(err.to_string())
    }
}
```

### Error Propagation Chain

```
OpenRaft Error Types
    ↓ (map in raft crate)
RaftError (raft crate)
    ↓ (map in service layer)
KvServiceError (kv crate)
    ↓ (format)
RespValue::Error (protocol-resp crate)
    ↓ (encode)
"-(error) ERR message\r\n" (client)
```

**Example Error Flow:**

1. Client sends SET command
2. RaftNode::propose() calls raft.client_write()
3. OpenRaft returns ClientWriteError::ForwardToLeader { leader_id: Some(2) }
4. Converted to RaftError::NotLeader { leader_id: Some(2) }
5. KV Service maps to KvServiceError::NotLeader(2)
6. Formatted as RespValue::Error("-MOVED 2\r\n")
7. Client receives MOVED error with leader ID

## Dependencies

**Added**:
- `openraft = "0.10"`
- `async-trait = "0.1"`
- `tracing = "0.1"`

**Removed**:
- `raft = "0.7"`
- `prost-old = "0.11"`
- `slog`

## Implementation Phases

| Phase | Description | Effort | Risks |
|-------|-------------|--------|-------|
| 1: Type System | Define RaftTypeConfig, types | 2-3h | Low |
| 2: Storage Layer | Implement storage traits | 4-5h | Medium |
| 3: State Machine | Wrap and integrate | 2-3h | Medium |
| 4: Network Stub | Create placeholder transport | 1-2h | None |
| 5: RaftNode Migration | Update node wrapper | 4-5h | High |
| 6: Integration | Cleanup and test | 2-3h | Medium |

**Total Estimated Effort**: 15-21 hours

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Async Complexity | Medium | Use `tokio::runtime::Handle` |
| API Breaking Changes | High | Create compatibility layer |
| Test Coverage | Medium | Track and maintain 85+ tests |
| Idempotency Loss | High | Preserve existing `apply()` logic |
| Performance | Low | Profile after migration |

## Success Criteria

- [ ] No prost version conflicts
- [ ] 85+ tests passing
- [ ] Single-node cluster functional
- [ ] MemStorage remains in-memory
- [ ] Idempotent state machine behavior
- [ ] Clean, async-first implementation

## Future Work

- Full gRPC transport implementation
- RocksDB storage backend
- KV service layer integration
- Cluster membership management
- Log compaction and snapshots

---

**Created:** 2025-10-26
**Feature:** OpenRaft Migration
**Status:** Ready for Implementation
**Estimated Effort:** 15-21 hours