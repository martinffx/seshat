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