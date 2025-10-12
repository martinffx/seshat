# Implementation Tasks: Raft Consensus

**Status**: In Progress
**Total Tasks**: 24
**Completed**: 3/24 (12.5%)
**Estimated Time**: 19 hours
**Time Spent**: 1.5 hours

## Overview

Distributed consensus implementation using raft-rs with in-memory storage for Phase 1. This feature enables leader election, log replication, and state machine consensus across the cluster.

**Architecture Pattern**: Protocol → Raft Layer → Storage Layer (NOT Router → Service → Repository)
**TDD Approach**: Write Test → Implement Minimal → Refactor → Repeat

---

## Executive Summary

### Progress Overview
- **Overall Completion**: 2/24 tasks (8%) - 1 hour completed of 19 hours estimated
- **Active Phase**: Phase 1 (Common Types Foundation) - ✅ 100% complete
- **Next Phase**: Phase 2 (Configuration) - Ready to start
- **Velocity**: 2 tasks/hour based on Phase 1 completion

### Critical Path Analysis
The implementation follows a strict dependency chain:
1. **Phase 1** (Common Foundation) → Enables all subsequent work ✅
2. **Phases 2-4** run in parallel → Critical for Phase 6
   - Phase 2 (Configuration) → Phase 6
   - Phase 3 (Protocol) → Phase 5 → Phase 6
   - Phase 4 (Storage) → Phase 6
3. **Phase 6** (Raft Node) → Integration point, blocks Phase 7
4. **Phase 7** (Integration) → Final validation

**Bottleneck**: Phase 6 requires completion of Phases 2, 4, and 5

### Parallel Execution Opportunities
After Phase 1 completion, three tracks can execute simultaneously:
- **Track A**: Configuration (3 tasks, 2.5 hours)
- **Track B**: Protocol + State Machine (5 tasks, 5 hours)
- **Track C**: Storage Layer (7 tasks, 4.5 hours)

Maximum parallelism achievable: 3 developers could reduce timeline from 18 hours to ~7 hours

### Risk Assessment
- **No blockers**: Phase 1 complete, all paths unblocked ✅
- **Highest risk**: Phase 6 (Raft Node) - 5.5 hours, complex integration
- **Critical dependencies**: Storage Layer (7 tasks) is longest sequential path
- **Timeline status**: On track if maintaining 2 tasks/hour velocity

### Completion Estimates
At current velocity (2 tasks/hour):
- **Remaining effort**: 22 tasks, ~11 hours of work
- **Best case** (3 parallel developers): ~11 hours total (3.5 hours to Phase 6, +5.5 hours Phase 6, +2 hours Phase 7)
- **Realistic case** (1 developer): 11 hours focused development time
- **Conservative case**: 18 hours (original estimate for remaining work)

### Recommended Next Steps
```bash
# RECOMMENDED: Start Storage Layer (critical path, most tasks)
/spec:implement raft mem_storage_skeleton

# Alternative: Quick win with Configuration
/spec:implement raft config_types

# Alternative: Enable State Machine track
/spec:implement raft protobuf_messages
```

---

## Phase 1: Common Types Foundation (2 tasks - 1 hour)

**Dependencies**: None
**Can run in parallel**: Yes (with Configuration and Protocol phases)

- [x] **common_types** - Common Type Aliases (30 min)
  - **Test**: Unit tests for type definitions and conversions
  - **Implement**: Define NodeId, Term, LogIndex as u64 type aliases
  - **Refactor**: Add doc comments and usage examples
  - **Files**: `crates/common/src/types.rs`, `crates/common/src/lib.rs`
  - **Acceptance**: NodeId, Term, LogIndex defined as u64; doc comments; no warnings

- [x] **common_errors** - Common Error Types (30 min)
  - **Test**: Error creation, formatting, and raft::Error conversion
  - **Implement**: Define Error enum with thiserror; From<raft::Error>
  - **Refactor**: Add context to error messages
  - **Files**: `crates/common/src/errors.rs`, `crates/common/src/lib.rs`, `crates/common/Cargo.toml`
  - **Deps**: thiserror = "1.0"
  - **Acceptance**: Error enum (NotLeader, NoQuorum, Raft, Storage, ConfigError, Serialization); descriptive messages

---

## Phase 2: Configuration (3 tasks - 2.5 hours)

**Dependencies**: Phase 1 (common_foundation)
**Can run in parallel**: With Protocol phase

- [ ] **config_types** - Configuration Data Types (1 hour)
  - **Test**: Config creation and serde serialization/deserialization
  - **Implement**: Define NodeConfig, ClusterConfig, RaftConfig, InitialMember structs
  - **Refactor**: Add builder patterns if needed
  - **Files**: `crates/raft/src/config.rs`, `crates/raft/src/lib.rs`, `crates/raft/Cargo.toml`
  - **Deps**: common (path), serde = {version="1.0", features=["derive"]}, thiserror="1.0"
  - **Acceptance**: NodeConfig (id, client_addr, internal_addr, data_dir, advertise_addr); ClusterConfig (bootstrap, initial_members, replication_factor); RaftConfig (timing); InitialMember (id, addr); all derive Debug, Clone, Serialize, Deserialize

- [ ] **config_validation** - Configuration Validation (1 hour)
  - **Test**: Valid and invalid configs (node_id=0, missing members, invalid timeouts)
  - **Implement**: Add validate() methods to each config type
  - **Refactor**: Extract common validation helpers
  - **Files**: `crates/raft/src/config.rs`
  - **Acceptance**: NodeConfig::validate() checks id>0, valid addresses, writable data_dir; ClusterConfig::validate() checks >=3 members, no duplicates, node in members; RaftConfig::validate() checks election_timeout >= heartbeat*2; descriptive errors

- [ ] **config_defaults** - Configuration Default Values (30 min)
  - **Test**: Verify default values match design spec
  - **Implement**: Implement Default for RaftConfig
  - **Refactor**: Document rationale for each default value
  - **Files**: `crates/raft/src/config.rs`
  - **Acceptance**: RaftConfig::default() returns heartbeat_interval_ms=100, election_timeout_min_ms=500, election_timeout_max_ms=1000, snapshot_interval_entries=10_000, snapshot_interval_bytes=100MB, max_log_size_bytes=500MB

---

## Phase 3: Protocol Definitions (2 tasks - 2 hours)

**Dependencies**: Phase 1 (common_foundation)
**Can run in parallel**: With Configuration phase

- [ ] **protobuf_messages** - Protobuf Message Definitions (1.5 hours)
  - **Test**: Message serialization/deserialization roundtrips
  - **Implement**: Create raft.proto with RequestVote, AppendEntries, InstallSnapshot messages
  - **Refactor**: Organize messages and add comprehensive comments
  - **Files**: `crates/protocol/proto/raft.proto`, `crates/protocol/build.rs`, `crates/protocol/src/lib.rs`, `crates/protocol/Cargo.toml`
  - **Deps**: common (path), tonic="0.11", prost="0.12", serde={version="1.0", features=["derive"]}
  - **Build Deps**: tonic-build="0.11"
  - **Acceptance**: raft.proto defines RaftService with RequestVote, AppendEntries, InstallSnapshot RPCs; LogEntry and EntryType enum; build.rs compiles .proto; cargo build succeeds; roundtrip tests pass

- [ ] **operation_types** - Operation Types (30 min)
  - **Test**: Operation::apply() and serialization
  - **Implement**: Define Operation enum with Set and Del variants
  - **Refactor**: Extract apply logic into trait methods
  - **Files**: `crates/protocol/src/operations.rs`, `crates/protocol/src/lib.rs`, `crates/protocol/Cargo.toml`
  - **Deps**: bincode="1.3"
  - **Acceptance**: Operation::Set{key, value} and Operation::Del{key}; Operation::apply(&self, data: &mut HashMap); Operation::serialize() and ::deserialize() using bincode; Set returns b"OK", Del returns b"1" or b"0"

---

## Phase 4: Storage Layer (7 tasks - 4.5 hours)

**Dependencies**: Phase 1 (common_foundation)
**Critical path**: Required before Raft Node

- [x] **mem_storage_skeleton** - MemStorage Structure (30 min)
  - **Test**: MemStorage::new() creation
  - **Implement**: Define MemStorage struct with RwLock fields
  - **Refactor**: Add internal helper methods
  - **Files**: `crates/raft/src/storage.rs`, `crates/raft/src/lib.rs`, `crates/raft/Cargo.toml`
  - **Deps**: raft="0.7" (with prost-codec), tokio={version="1", features=["full"]}
  - **Acceptance**: MemStorage struct with hard_state: RwLock<HardState>, conf_state: RwLock<ConfState>, entries: RwLock<Vec<Entry>>, snapshot: RwLock<Snapshot>; MemStorage::new() creates defaults; compiles with raft-rs imports

- [ ] **mem_storage_initial_state** - Storage: initial_state() (30 min)
  - **Test**: New storage returns default HardState and ConfState
  - **Implement**: Implement initial_state() reading from RwLocks
  - **Refactor**: Handle edge cases and add logging
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: initial_state() returns RaftState with HardState and ConfState; new storage returns defaults (term=0, vote=None, commit=0); after set_hard_state(), initial_state() reflects changes

- [ ] **mem_storage_entries** - Storage: entries() (1 hour)
  - **Test**: Empty range, normal range, max_size limit, compacted range, unavailable range
  - **Implement**: Implement entries() with bounds checking
  - **Refactor**: Optimize slice operations
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: entries(low, high, None) returns [low, high) range; entries(low, high, Some(max_size)) respects size limit; StorageError::Compacted if low < first_index(); StorageError::Unavailable if high > last_index()+1

- [ ] **mem_storage_term** - Storage: term() (30 min)
  - **Test**: Term for valid index, index=0, compacted index, unavailable index
  - **Implement**: Implement term() with snapshot fallback
  - **Refactor**: Add bounds checking
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: term(0) returns 0; term(index) returns entry.term for valid index; returns snapshot.metadata.term if index == snapshot.metadata.index; error for compacted/unavailable indices

- [ ] **mem_storage_first_last_index** - Storage: first_index() and last_index() (30 min)
  - **Test**: Empty log, after append, after compaction, after snapshot
  - **Implement**: Implement both methods using entries and snapshot
  - **Refactor**: Maintain invariant: first_index <= last_index + 1
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: first_index() returns snapshot.metadata.index+1 (or 1 if no snapshot); last_index() returns last entry index (or snapshot.metadata.index if empty); invariant maintained

- [ ] **mem_storage_snapshot** - Storage: snapshot() (30 min)
  - **Test**: Empty snapshot, after create_snapshot()
  - **Implement**: Implement snapshot() reading from RwLock
  - **Refactor**: Handle snapshot not ready cases
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: snapshot(request_index) returns current snapshot; Phase 1 simplified: just return stored snapshot; SnapshotTemporarilyUnavailable if not ready (Phase 2+)

- [ ] **mem_storage_mutations** - Storage Mutation Methods (1 hour)
  - **Test**: Tests for each mutation method
  - **Implement**: Implement append(), set_hard_state(), set_conf_state(), compact(), create_snapshot()
  - **Refactor**: Ensure thread safety with RwLocks
  - **Files**: `crates/raft/src/storage.rs`
  - **Acceptance**: append(&[Entry]) extends log; set_hard_state(HardState) updates hard state; set_conf_state(ConfState) updates conf state; compact(index) removes entries before index; create_snapshot(index, data) creates snapshot

---

## Phase 5: State Machine (3 tasks - 3 hours)

**Dependencies**: Phase 1 (common_foundation), Phase 3 (protocol_definitions)
**Can run in parallel**: With Storage Layer

- [ ] **state_machine_core** - StateMachine Core Structure (1 hour)
  - **Test**: Tests for new(), get(), exists()
  - **Implement**: Define StateMachine with data HashMap and last_applied
  - **Refactor**: Add internal helpers
  - **Files**: `crates/raft/src/state_machine.rs`, `crates/raft/src/lib.rs`
  - **Acceptance**: StateMachine struct with data: HashMap<Vec<u8>, Vec<u8>>, last_applied: u64; new() creates empty; get(key) returns Option<Vec<u8>>; exists(key) returns bool

- [ ] **state_machine_operations** - StateMachine Apply Operations (1.5 hours)
  - **Test**: Apply Set, apply Del, operation ordering, idempotency
  - **Implement**: Implement apply(entry) with Operation deserialization
  - **Refactor**: Extract operation execution logic
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: apply(entry) deserializes Operation from entry.data; checks entry.index > last_applied; calls Operation::apply() on HashMap; updates last_applied; returns result bytes; ordering and idempotency tests pass

- [ ] **state_machine_snapshot** - StateMachine Snapshot and Restore (30 min)
  - **Test**: Snapshot with data, restore from snapshot, snapshot roundtrip
  - **Implement**: Implement snapshot() using bincode, restore() to deserialize
  - **Refactor**: Add version field to snapshot format
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: snapshot() serializes SnapshotData{version:1, last_applied, data}; restore(bytes) deserializes and replaces HashMap and last_applied; roundtrip test passes (SET keys, snapshot, restore, verify)

---

## Phase 6: Raft Node (5 tasks - 5.5 hours)

**Dependencies**: Phase 2 (configuration), Phase 4 (storage_layer), Phase 5 (state_machine)
**Critical path**: Required before Integration

- [ ] **raft_node_initialization** - RaftNode Initialization (2 hours)
  - **Test**: Create RaftNode with valid config, verify fields are set
  - **Implement**: Define RaftNode struct, implement new() with raft::Config conversion
  - **Refactor**: Extract config conversion to helper
  - **Files**: `crates/raft/src/node.rs`, `crates/raft/src/lib.rs`
  - **Acceptance**: RaftNode struct with raw_node, storage, state_machine, config, node_id; new() creates MemStorage with voters from peers; creates raft::Config with timing params; creates RawNode; creates Arc<Mutex<StateMachine>>

- [ ] **raft_node_tick** - RaftNode Tick Processing (30 min)
  - **Test**: Call tick() multiple times, verify no panics
  - **Implement**: Implement tick() calling raw_node.tick()
  - **Refactor**: Add instrumentation logging
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: tick() calls self.raw_node.tick(); returns Result<()>; can be called repeatedly; test passes

- [ ] **raft_node_propose** - RaftNode Propose Client Commands (1 hour)
  - **Test**: Propose as follower returns NotLeader error
  - **Implement**: Implement propose() calling raw_node.propose()
  - **Refactor**: Add leader check and error handling
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: propose(data) checks is_leader(); returns NotLeader error if follower; calls raw_node.propose(context, data) if leader; returns Result<()>

- [ ] **raft_node_ready_handler** - RaftNode Ready Processing (1.5 hours)
  - **Test**: handle_ready with no ready state returns empty
  - **Implement**: Implement full Ready processing: persist → send → apply → advance
  - **Refactor**: Extract apply logic, add comprehensive logging
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: handle_ready() checks raw_node.has_ready(); persists hard_state and entries; extracts messages; applies committed_entries to state_machine; calls raw_node.advance(ready); handles light ready; calls raw_node.advance_apply(); returns Vec<Message>; correct order (persist before send)

- [ ] **raft_node_leader_queries** - RaftNode Leader Queries (30 min)
  - **Test**: New node is not leader, leader_id returns None initially
  - **Implement**: Implement queries using raw_node.raft.state
  - **Refactor**: Add caching if needed
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: is_leader() returns self.raw_node.raft.state == StateRole::Leader; leader_id() returns Some(id) if known, None otherwise; tests verify correct values

---

## Phase 7: Integration Testing (2 tasks - 2 hours)

**Dependencies**: Phase 6 (raft_node)
**Final validation**: Verify all components work together

- [ ] **single_node_bootstrap** - Single Node Bootstrap Test (1 hour)
  - **Test**: Create RaftNode, tick until becomes leader
  - **Implement**: Use test utilities to create node and run event loop
  - **Refactor**: Extract test helpers for reuse
  - **Files**: `crates/raft/tests/integration_tests.rs`, `crates/raft/tests/common/mod.rs`
  - **Acceptance**: Test creates RaftNode with single-node cluster config; ticks repeatedly; after election timeout, node becomes leader; is_leader() returns true; test passes within 5s

- [ ] **single_node_propose_apply** - Single Node Propose and Apply Test (1 hour)
  - **Test**: Become leader, propose SET, handle ready, verify get() works
  - **Implement**: Propose operation, process ready in loop, check state machine
  - **Refactor**: Add async test utilities
  - **Files**: `crates/raft/tests/integration_tests.rs`
  - **Acceptance**: Test sets up single-node cluster; node becomes leader; proposes Operation::Set{key: b"foo", value: b"bar"}; calls handle_ready(); state_machine.get(b"foo") returns Some(b"bar"); test passes

---

## TDD Workflow

For each task, follow this strict cycle:

1. **Write Test (Red)** - Create failing test that specifies expected behavior
2. **Implement (Green)** - Write minimal code to make the test pass
3. **Refactor (Clean)** - Improve code quality while keeping tests green

**Key principles**:
- No production code without a failing test first
- One test at a time, one assertion at a time
- Refactor only when tests are green
- Commit after each completed cycle

---

## Dependency Graph

```
Phase 1: Common Foundation (parallel start)
├── Phase 2: Configuration
│   └── Phase 6: Raft Node
│       └── Phase 7: Integration
├── Phase 3: Protocol Definitions
│   └── Phase 5: State Machine (parallel)
│       └── Phase 6: Raft Node
└── Phase 4: Storage Layer
    └── Phase 6: Raft Node
        └── Phase 7: Integration
```

**Parallel opportunities**:
- Phases 2, 3, 4 can run in parallel after Phase 1
- Phase 5 can run parallel with Phase 4
- Integration tests (Phase 7) require all previous phases

---

## Success Criteria

- [ ] All unit tests pass (100% of task acceptance criteria met)
- [ ] All integration tests pass (single-node bootstrap and propose/apply)
- [ ] MemStorage implements all 6 Storage trait methods correctly
- [ ] StateMachine applies Set and Del operations correctly
- [ ] RaftNode can bootstrap and become leader
- [ ] RaftNode can propose and apply operations via Ready processing
- [ ] No unwrap() calls in production code paths
- [ ] All public APIs have doc comments
- [ ] cargo clippy passes with no warnings
- [ ] cargo test passes all tests

---

## Next Steps

To start implementation:

```bash
/spec:implement raft common_types
```

This will begin the first task in Phase 1. After completion, continue with:
- `common_errors`
- `config_types` (can start in parallel after Phase 1)
- ... (follow task order above)

---

## Notes

- **Phase 1 focus**: In-memory implementation only
- **NOT included**: gRPC client/server networking (separate feature)
- **NOT included**: RocksDB persistence (separate feature)
- **Deferred**: Multi-node cluster tests (chaos testing phase)
- **Architecture**: Follows Protocol → Raft Layer → Storage Layer (NOT Router → Service → Repository)
- **Task ordering**: By technical dependencies, not by crate
- **Time estimates**: Include test writing, implementation, and refactoring

---

## Related Documents

- [Raft Design](/Users/martinrichards/code/seshat/docs/specs/raft/design.md)
- [Raft Specification](/Users/martinrichards/code/seshat/docs/specs/raft/spec.md)
- [Development Practices](/Users/martinrichards/code/seshat/docs/standards/practices.md)
- [Technical Standards](/Users/martinrichards/code/seshat/docs/standards/tech.md)
- [Data Structures](/Users/martinrichards/code/seshat/docs/architecture/data-structures.md)
