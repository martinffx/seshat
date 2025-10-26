# Implementation Tasks: OpenRaft Migration

## Overview

This migration replaces `raft-rs` with `openraft` in the Seshat distributed key-value store. The implementation is **in-memory only** - no RocksDB integration, no full gRPC transport implementation. The network layer will be a stub for future development.

**Scope:**
- Migrate from `raft-rs` (0.7) to `openraft` (0.10)
- Resolve prost version conflict (0.12 vs 0.14)
- Preserve existing StateMachine idempotency guarantees
- Maintain 85+ test coverage
- Convert synchronous API to async

**Estimated Effort:**
- Single-agent: 15-21 hours
- Multi-agent (3 agents): 12-16 hours with parallel execution

**Total:** 6 phases, 24 tasks

---

## Execution Strategy

### Critical Path
The minimum sequential path through the migration:

1. **Phase 1: Type System** (2-3 hours) - Foundation for all other work
2. **Phase 2: Storage Layer** (4-5 hours) - Required by Node Migration
3. **Phase 5: Node Migration** (4-5 hours) - Core migration work
4. **Phase 6: Integration** (2-3 hours) - Final validation and cleanup

**Total critical path:** 12-16 hours

### Parallel Execution
After Phase 1 completes, three independent tracks can run concurrently:

**Agent 1: Storage Layer** (Phase 2)
- Task 2.1-2.4: Implement RaftLogReader, RaftSnapshotBuilder, RaftStorage
- Duration: 4-5 hours

**Agent 2: State Machine** (Phase 3)
- Task 3.1-3.4: Wrap StateMachine, implement apply(), snapshot methods
- Duration: 2-3 hours

**Agent 3: Network Stub** (Phase 4)
- Task 4.1-4.3: Create minimal RaftNetwork implementation
- Duration: 1-2 hours

After these converge, all agents work on Phase 5 (Node Migration) and Phase 6 (Integration).

### Multi-Agent Workflow
**Optimal 3-agent approach:**

```
Hour 0-3:   All → Phase 1 (Type System)
Hour 3-8:   Agent 1 → Phase 2 (Storage)
            Agent 2 → Phase 3 (State Machine)
            Agent 3 → Phase 4 (Network) → Wait for Phase 2/3
Hour 8-13:  All → Phase 5 (Node Migration)
Hour 13-16: All → Phase 6 (Integration)
```

This reduces total time from ~18 hours (sequential) to ~13-16 hours (parallel).

---

## Phases

### Phase 1: Type System & Configuration (2-3 hours)
**Dependencies:** None (start here!)
**Can run in parallel with:** Nothing (foundation for all other phases)

#### Task 1.1: Define RaftTypeConfig
**ID:** type_system_1
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Write test for NodeId type (should be u64)
- [ ] **Test**: Write test for BasicNode struct construction
- [ ] **Test**: Write test for Request/Response types with serde
- [ ] **Implement**: Create RaftTypeConfig struct with all associated types
- [ ] **Refactor**: Verify compilation and type constraints

**Files:** `crates/raft/src/types.rs`, `crates/raft/Cargo.toml`

**Acceptance:**
- RaftTypeConfig implements openraft::RaftTypeConfig
- All associated types compile correctly (NodeId=u64, Node=BasicNode, etc.)
- Type construction tests pass

**Notes:**
- NodeId = u64 (matches existing raft-rs)
- Node = BasicNode { addr: String }
- Entry = LogEntry\<Request\>
- SnapshotData = Vec\<u8\>
- AsyncRuntime = TokioRuntime

---

#### Task 1.2: Create Type Conversions
**ID:** type_system_2
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write test for eraftpb::Entry → LogEntry\<Request\> conversion
- [ ] **Test**: Write test for eraftpb::HardState → Vote + LogId conversion
- [ ] **Test**: Write test for eraftpb::ConfState → Membership conversion
- [ ] **Implement**: Implement From/Into traits for all conversions
- [ ] **Refactor**: Test edge cases (empty voters, max term values)

**Files:** `crates/raft/src/types.rs`

**Acceptance:**
- Entry conversion preserves index, term, data
- HardState splits into Vote and commit index correctly
- ConfState converts voters/learners to BTreeSet
- All conversion tests pass

**Notes:**
- Use LogEntry::new(log_id, Request { data })
- Extract Vote { term, node_id } from HardState
- Map ConfState.voters/learners to Membership

---

#### Task 1.3: Property Test Conversions
**ID:** type_system_3
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Add proptest dependency to Cargo.toml
- [ ] **Test**: Write property test for Entry round-trip (openraft → eraftpb → openraft)
- [ ] **Test**: Write property test for HardState/Vote round-trip
- [ ] **Test**: Write property test for ConfState/Membership round-trip
- [ ] **Refactor**: Verify no data loss in conversions

**Files:** `crates/raft/src/types.rs`

**Acceptance:**
- Property tests pass for 1000+ random inputs
- Round-trip conversions preserve all data
- Edge cases handled (empty sets, u64::MAX)

**Notes:**
- Use proptest for generating random valid types
- Test boundary values (0, u64::MAX)
- Verify no panics on malformed data

---

### Phase 2: Storage Layer Migration (4-5 hours)
**Dependencies:** Phase 1 (Type System)
**Can run in parallel with:** Phase 3 (State Machine), Phase 4 (Network Stub)

#### Task 2.1: Implement RaftLogReader
**ID:** storage_layer_1
**Estimated Time:** 1.5-2 hours

- [ ] **Test**: Write test for get_log_state() returning last_purged and last_log_id
- [ ] **Test**: Write test for try_get_log_entries() with range queries
- [ ] **Test**: Write test for read_vote() returning current vote state
- [ ] **Implement**: Create OpenRaftMemStorage struct with RwLock fields
- [ ] **Implement**: Implement RaftLogReader trait methods
- [ ] **Refactor**: Test concurrent read access

**Files:** `crates/storage/src/openraft_storage.rs`, `crates/storage/src/lib.rs`

**Acceptance:**
- get_log_state() returns correct LogState
- try_get_log_entries() handles ranges correctly
- read_vote() returns None initially, Some(vote) after save
- Concurrent reads don't deadlock

**Notes:**
- Use RwLock\<BTreeMap\<u64, LogEntry\<Request\>\>\> for log
- Calculate log state from BTreeMap keys/values
- Use RwLock\<Option\<Vote\<u64\>\>\> for vote storage

---

#### Task 2.2: Implement RaftSnapshotBuilder
**ID:** storage_layer_2
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write test for build_snapshot() creating valid Snapshot
- [ ] **Test**: Write test verifying snapshot includes state machine data
- [ ] **Test**: Write test for snapshot metadata (last_log_id, membership)
- [ ] **Implement**: Implement build_snapshot() delegating to StateMachine::snapshot()
- [ ] **Implement**: Wrap result in openraft Snapshot type
- [ ] **Refactor**: Test snapshot data integrity with bincode

**Files:** `crates/storage/src/openraft_storage.rs`

**Acceptance:**
- build_snapshot() creates Snapshot with correct metadata
- Snapshot data contains serialized state machine
- Snapshot can be deserialized correctly
- Multiple snapshots work correctly

**Notes:**
- Call self.state_machine.read().unwrap().snapshot()
- Create SnapshotMeta with last_log_id and membership
- Store snapshot in RwLock\<Option\<Snapshot\<RaftTypeConfig\>\>\>

---

#### Task 2.3: Implement RaftStorage Trait
**ID:** storage_layer_3
**Estimated Time:** 2-2.5 hours

- [ ] **Test**: Write test for save_vote() persisting vote
- [ ] **Test**: Write test for append() adding entries to log
- [ ] **Test**: Write test for delete_conflict_logs_since() removing entries
- [ ] **Test**: Write test for purge_logs_upto() truncating old entries
- [ ] **Test**: Write test for apply_to_state_machine() applying entries
- [ ] **Test**: Write test for install_snapshot() restoring state
- [ ] **Implement**: Implement all RaftStorage methods
- [ ] **Refactor**: Test atomicity of operations

**Files:** `crates/storage/src/openraft_storage.rs`

**Acceptance:**
- save_vote() persists vote correctly
- append() maintains log order
- delete_conflict_logs_since() removes correct range
- purge_logs_upto() keeps required entries
- apply_to_state_machine() preserves idempotency
- install_snapshot() restores state correctly

**Notes:**
- Maintain idempotency check: index > last_applied
- Use BTreeMap::split_off for efficient range operations
- Delegate state machine apply to StateMachine::apply()
- Handle snapshot restoration via StateMachine::restore()

---

#### Task 2.4: Migrate Storage Tests
**ID:** storage_layer_4
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Convert all sync tests to async using #[tokio::test]
- [ ] **Test**: Update MemStorage API calls to OpenRaftMemStorage
- [ ] **Test**: Replace raft::Storage trait calls with openraft traits
- [ ] **Test**: Update assertions for openraft types
- [ ] **Refactor**: Verify all 85+ tests pass

**Files:** `crates/storage/src/lib.rs`

**Acceptance:**
- All storage tests converted to async
- 85+ tests passing with openraft
- Test coverage maintained or improved
- No flaky tests due to async timing

**Notes:**
- Use tokio::test macro for async tests
- Update test helpers to be async fn
- Replace eraftpb types with openraft types
- Keep test logic/assertions identical

---

### Phase 3: State Machine Integration (2-3 hours)
**Dependencies:** Phase 1 (Type System)
**Can run in parallel with:** Phase 2 (Storage Layer), Phase 4 (Network Stub)

#### Task 3.1: Create StateMachine Wrapper
**ID:** state_machine_1
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Write test for OpenRaftStateMachine initialization
- [ ] **Test**: Write test for wrapper holding Arc\<RwLock\<StateMachine\>\>
- [ ] **Implement**: Create OpenRaftStateMachine struct
- [ ] **Implement**: Implement basic delegation methods
- [ ] **Refactor**: Test wrapper compiles and links correctly

**Files:** `crates/raft/src/state_machine_wrapper.rs`

**Acceptance:**
- OpenRaftStateMachine wraps existing StateMachine
- Wrapper uses Arc\<RwLock\<\>\> for thread safety
- Initialization test passes
- Compiles without errors

**Notes:**
- Store inner: Arc\<RwLock\<StateMachine\>\>
- Prepare for async RaftStateMachine trait impl
- Keep existing StateMachine untouched

---

#### Task 3.2: Implement apply() with Idempotency
**ID:** state_machine_2
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write test verifying apply() rejects entries with index <= last_applied
- [ ] **Test**: Write test for apply() accepting entries with index > last_applied
- [ ] **Test**: Write test for apply() processing multiple entries in order
- [ ] **Implement**: Implement apply() iterating over entries and calling StateMachine::apply()
- [ ] **Implement**: Verify idempotency check preserved (delegated to StateMachine)
- [ ] **Refactor**: Test response collection and error handling

**Files:** `crates/raft/src/state_machine_wrapper.rs`

**Acceptance:**
- apply() preserves idempotency (index > last_applied)
- Entries applied in order
- Responses collected correctly
- Out-of-order entries rejected
- Duplicate entries rejected

**Notes:**
- Iterate: for entry in entries { ... }
- Call self.inner.write().unwrap().apply(entry.log_id.index, &entry.payload.data)
- Idempotency check is inside StateMachine::apply()
- Collect Response { result } for each entry

---

#### Task 3.3: Implement Snapshot Methods
**ID:** state_machine_3
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Write test for get_current_snapshot() creating snapshot
- [ ] **Test**: Write test for install_snapshot() restoring state
- [ ] **Test**: Write test for round-trip snapshot/restore
- [ ] **Implement**: Implement snapshot creation via StateMachine::snapshot()
- [ ] **Implement**: Implement snapshot restoration via StateMachine::restore()
- [ ] **Refactor**: Test with bincode serialization

**Files:** `crates/raft/src/state_machine_wrapper.rs`

**Acceptance:**
- get_current_snapshot() creates valid snapshot
- install_snapshot() restores state correctly
- Round-trip preserves all state machine data
- Bincode serialization works correctly

**Notes:**
- snapshot() returns self.inner.read().unwrap().snapshot()
- restore() calls self.inner.write().unwrap().restore(snapshot)
- Use existing bincode serialization from StateMachine

---

#### Task 3.4: Comprehensive Idempotency Tests
**ID:** state_machine_4
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Write test applying same entry twice (should reject second)
- [ ] **Test**: Write test applying entries out of order (should reject)
- [ ] **Test**: Write test for gap in indices (should accept after gap)
- [ ] **Test**: Write test verifying last_applied tracking
- [ ] **Test**: Test idempotency after snapshot restoration

**Files:** `crates/raft/src/state_machine_wrapper.rs`

**Acceptance:**
- Duplicate entries rejected
- Out-of-order entries rejected
- last_applied tracked correctly
- Idempotency preserved after snapshot restore
- All idempotency guarantees verified

**Notes:**
- Test with sequential indices: 1, 2, 3
- Test duplicate: 1, 2, 2 (reject third)
- Test out-of-order: 1, 3, 2 (reject third)
- Verify StateMachine::apply() logic enforces this

---

### Phase 4: Network Stub Implementation (1-2 hours)
**Dependencies:** Phase 1 (Type System)
**Can run in parallel with:** Phase 2 (Storage Layer), Phase 3 (State Machine)

#### Task 4.1: Define StubNetwork Struct
**ID:** network_stub_1
**Estimated Time:** 0.25-0.5 hour

- [ ] **Test**: Write test for StubNetwork creation
- [ ] **Implement**: Create StubNetwork struct with node_id field
- [ ] **Implement**: Add new() constructor
- [ ] **Refactor**: Add basic tracing instrumentation

**Files:** `crates/raft/src/network_stub.rs`

**Acceptance:**
- StubNetwork compiles
- new() constructor works
- Basic logging in place

**Notes:**
- Simple struct: { node_id: u64 }
- Add tracing::info in new()
- Prepare for async RaftNetwork trait

---

#### Task 4.2: Implement RaftNetwork Trait
**ID:** network_stub_2
**Estimated Time:** 0.5-1 hour

- [ ] **Test**: Write test for send_append_entries() returning Ok
- [ ] **Test**: Write test for send_vote() returning Ok
- [ ] **Test**: Write test for send_install_snapshot() returning Ok
- [ ] **Implement**: Implement RaftNetwork trait with #[async_trait]
- [ ] **Implement**: Add tracing to each method showing it's a stub
- [ ] **Refactor**: Return Ok(Default::default()) for all methods

**Files:** `crates/raft/src/network_stub.rs`

**Acceptance:**
- RaftNetwork trait implemented
- All methods return Ok without panic
- Tracing shows stub calls
- Tests verify no-op behavior

**Notes:**
- Use #[async_trait] for trait implementation
- Log at debug level: tracing::debug!("StubNetwork: ...")
- Return Ok(AppendEntriesResponse::default()), etc.
- Add TODO comments for future gRPC integration

---

#### Task 4.3: Test Stub Network
**ID:** network_stub_3
**Estimated Time:** 0.25-0.5 hour

- [ ] **Test**: Write test verifying no panics on send calls
- [ ] **Test**: Write test checking tracing output (using tracing-subscriber-test)
- [ ] **Test**: Write test for concurrent send calls
- [ ] **Refactor**: Verify all network methods callable

**Files:** `crates/raft/src/network_stub.rs`

**Acceptance:**
- No panics on any send method
- Tracing output verified
- Concurrent calls work
- All tests pass

**Notes:**
- Use tokio::test for async tests
- Verify Ok() responses
- Check tracing with subscriber test utilities

---

### Phase 5: RaftNode Migration (4-5 hours)
**Dependencies:** Phase 2 (Storage Layer), Phase 3 (State Machine), Phase 4 (Network Stub)
**Can run in parallel with:** Nothing (requires all previous phases)

#### Task 5.1: Update Cargo Dependencies
**ID:** node_migration_1
**Estimated Time:** 0.5-1 hour

- [ ] **Implement**: Remove raft = "0.7" dependency
- [ ] **Implement**: Remove prost-old dependency
- [ ] **Implement**: Remove slog dependency
- [ ] **Implement**: Add openraft = { version = "0.10", features = ["tokio"] }
- [ ] **Implement**: Add async-trait = "0.1"
- [ ] **Implement**: Add tracing = "0.1"
- [ ] **Test**: Run cargo tree | grep prost to verify conflict resolved
- [ ] **Refactor**: Run cargo build to verify compilation

**Files:** `crates/raft/Cargo.toml`, `crates/storage/Cargo.toml`

**Acceptance:**
- cargo tree shows single prost version (0.14)
- No prost version conflicts
- cargo build succeeds
- All dependencies compatible

**Notes:**
- Keep tokio, serde, bincode, tonic (0.14), prost (0.14)
- Remove all raft-rs related dependencies
- Verify openraft uses prost 0.14 (matching tonic 0.14)

---

#### Task 5.2: Migrate RaftNode Initialization
**ID:** node_migration_2
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write async test for RaftNode::new() initialization
- [ ] **Implement**: Update RaftNode struct to hold openraft::Raft\<RaftTypeConfig\>
- [ ] **Implement**: Create openraft Config with election/heartbeat timeouts
- [ ] **Implement**: Implement async new() creating Raft instance
- [ ] **Test**: Test initialization with single node
- [ ] **Refactor**: Test initialization with multiple peers

**Files:** `crates/raft/src/node.rs`

**Acceptance:**
- RaftNode::new() is async
- Creates openraft::Raft instance successfully
- Config parameters match existing values
- Initialization tests pass

**Notes:**
- Config { election_timeout_min: 150, election_timeout_max: 300, heartbeat_interval: 50 }
- Use Raft::new(id, Arc::new(config), network, storage).await?
- Store raft: Raft\<RaftTypeConfig\> in struct

---

#### Task 5.3: Migrate propose() to client_write()
**ID:** node_migration_3
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write async test for propose() submitting request
- [ ] **Test**: Write async test for propose() handling response
- [ ] **Implement**: Update propose() to be async fn
- [ ] **Implement**: Implement using raft.client_write(ClientWriteRequest::new(Request { data }))
- [ ] **Test**: Test successful proposal
- [ ] **Refactor**: Test proposal on non-leader (should fail or forward)

**Files:** `crates/raft/src/node.rs`

**Acceptance:**
- propose() is async
- Uses client_write() correctly
- Returns result properly
- Tests verify leader handling

**Notes:**
- Signature: async fn propose(&self, data: Vec\<u8\>) -> Result\<()\>
- Create request: ClientWriteRequest::new(Request { data })
- Call: self.raft.client_write(request).await?
- Handle ClientWriteResponse

---

#### Task 5.4: Migrate Remaining API Methods
**ID:** node_migration_4
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write async test for is_leader() using metrics()
- [ ] **Test**: Write async test for leader_id() using current_leader()
- [ ] **Test**: Write test for get() direct state machine access
- [ ] **Implement**: Implement is_leader() via self.raft.is_leader().await
- [ ] **Implement**: Implement leader_id() via self.raft.current_leader().await
- [ ] **Implement**: Update get() to access storage.state_machine directly
- [ ] **Refactor**: Remove tick() and handle_ready() methods (no longer needed)

**Files:** `crates/raft/src/node.rs`

**Acceptance:**
- is_leader() works correctly
- leader_id() returns correct node ID or None
- get() reads from state machine
- tick() and handle_ready() removed
- All API tests pass

**Notes:**
- is_leader(): self.raft.is_leader().await
- leader_id(): self.raft.current_leader().await
- get(): self.storage.state_machine.read().unwrap().get(key)
- Remove tick/handle_ready logic - openraft handles internally

---

#### Task 5.5: Migrate RaftNode Tests
**ID:** node_migration_5
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Convert all node tests to async using #[tokio::test]
- [ ] **Test**: Remove tests for tick() and handle_ready()
- [ ] **Test**: Update propose tests to use client_write
- [ ] **Test**: Update leader election tests for openraft behavior
- [ ] **Test**: Fix any timing-related test issues
- [ ] **Refactor**: Verify all remaining tests pass

**Files:** `crates/raft/src/node.rs`

**Acceptance:**
- All node tests converted to async
- Obsolete tests removed (tick, handle_ready)
- propose → client_write tests working
- All tests pass consistently

**Notes:**
- Use #[tokio::test] macro
- Add .await to all async calls
- Update test helpers to async fn
- Remove synchronous tick/ready loop tests

---

### Phase 6: Integration & Cleanup (2-3 hours)
**Dependencies:** Phase 5 (Node Migration)
**Can run in parallel with:** Nothing (final validation phase)

#### Task 6.1: End-to-End Integration Tests
**ID:** integration_1
**Estimated Time:** 1-1.5 hours

- [ ] **Test**: Write test for full propose → apply → get flow
- [ ] **Test**: Write test for snapshot creation and restoration
- [ ] **Test**: Write test for idempotency end-to-end
- [ ] **Test**: Write test for multiple proposals in sequence
- [ ] **Test**: Test state machine consistency after operations
- [ ] **Refactor**: Test error handling paths

**Files:** `crates/raft/tests/integration_tests.rs`

**Acceptance:**
- Full flow tests pass
- Snapshot round-trip works
- Idempotency verified end-to-end
- Error cases handled correctly
- Integration tests stable and repeatable

**Notes:**
- Create test helpers: setup_test_node(), propose_and_verify()
- Test with realistic data patterns
- Verify state machine state matches expectations
- Test concurrent operations if possible

---

#### Task 6.2: Verify Prost Conflict Resolved
**ID:** integration_2
**Estimated Time:** 0.25-0.5 hour

- [ ] **Test**: Run cargo tree | grep prost
- [ ] **Test**: Verify only prost 0.14 appears in tree
- [ ] **Test**: Check tonic compatibility (should use prost 0.14)
- [ ] **Test**: Verify openraft compatibility (should use prost 0.14)
- [ ] **Test**: Run cargo build --all-features to verify
- [ ] **Refactor**: Check for any warning about multiple prost versions

**Files:** (No files - command line validation)

**Acceptance:**
- cargo tree shows single prost version (0.14)
- No version conflict warnings
- All dependencies use same prost version
- Clean build with no conflicts

**Notes:**
- Document prost version in plan
- Verify with: cargo tree | grep prost | sort | uniq
- Check openraft's prost dependency matches tonic's

---

#### Task 6.3: Remove raft-rs Code
**ID:** integration_3
**Estimated Time:** 0.5-1 hour

- [ ] **Implement**: Search codebase for 'use raft::' imports
- [ ] **Implement**: Remove old raft::Storage trait implementation
- [ ] **Implement**: Remove eraftpb imports and conversions (if any remain)
- [ ] **Implement**: Remove slog-related code
- [ ] **Implement**: Search for RawNode references
- [ ] **Implement**: Remove any dead code from migration
- [ ] **Refactor**: Run cargo clippy to find unused imports

**Files:** `crates/raft/src/`, `crates/storage/src/`

**Acceptance:**
- No raft-rs references in code
- No eraftpb imports
- No slog imports
- No unused imports or dead code
- cargo clippy passes cleanly

**Notes:**
- Search: rg 'use raft::' --type rust
- Search: rg 'eraftpb' --type rust
- Search: rg 'RawNode' --type rust
- Remove old MemStorage raft::Storage impl if separate file

---

#### Task 6.4: Update Documentation
**ID:** integration_4
**Estimated Time:** 0.5-1 hour

- [ ] **Implement**: Update crates/raft/README.md to mention openraft
- [ ] **Implement**: Update crates/storage/README.md with OpenRaftMemStorage
- [ ] **Implement**: Update module-level doc comments in lib.rs files
- [ ] **Implement**: Update examples if any exist
- [ ] **Implement**: Update docs/architecture/crates.md if needed
- [ ] **Refactor**: Remove references to raft-rs from comments

**Files:** `crates/raft/README.md`, `crates/storage/README.md`, `docs/architecture/crates.md`

**Acceptance:**
- All README files updated
- Module docs mention openraft
- No raft-rs references in docs
- Examples (if any) work with openraft
- Architecture docs reflect new structure

**Notes:**
- Update dependency list in README
- Update code examples to show async usage
- Document breaking changes (async APIs)
- Note prost conflict resolution

---

## Progress Tracking

### Completed Tasks by Phase

- [ ] **Phase 1: Type System & Configuration** (0/3 complete)
  - [ ] Task 1.1: Define RaftTypeConfig
  - [ ] Task 1.2: Create Type Conversions
  - [ ] Task 1.3: Property Test Conversions

- [ ] **Phase 2: Storage Layer Migration** (0/4 complete)
  - [ ] Task 2.1: Implement RaftLogReader
  - [ ] Task 2.2: Implement RaftSnapshotBuilder
  - [ ] Task 2.3: Implement RaftStorage Trait
  - [ ] Task 2.4: Migrate Storage Tests

- [ ] **Phase 3: State Machine Integration** (0/4 complete)
  - [ ] Task 3.1: Create StateMachine Wrapper
  - [ ] Task 3.2: Implement apply() with Idempotency
  - [ ] Task 3.3: Implement Snapshot Methods
  - [ ] Task 3.4: Comprehensive Idempotency Tests

- [ ] **Phase 4: Network Stub Implementation** (0/3 complete)
  - [ ] Task 4.1: Define StubNetwork Struct
  - [ ] Task 4.2: Implement RaftNetwork Trait
  - [ ] Task 4.3: Test Stub Network

- [ ] **Phase 5: RaftNode Migration** (0/5 complete)
  - [ ] Task 5.1: Update Cargo Dependencies
  - [ ] Task 5.2: Migrate RaftNode Initialization
  - [ ] Task 5.3: Migrate propose() to client_write()
  - [ ] Task 5.4: Migrate Remaining API Methods
  - [ ] Task 5.5: Migrate RaftNode Tests

- [ ] **Phase 6: Integration & Cleanup** (0/4 complete)
  - [ ] Task 6.1: End-to-End Integration Tests
  - [ ] Task 6.2: Verify Prost Conflict Resolved
  - [ ] Task 6.3: Remove raft-rs Code
  - [ ] Task 6.4: Update Documentation

**Total Progress**: 0/24 tasks (0%)

### Milestones

- [ ] **Type system complete** → Foundation for parallel work (Phases 2-4)
- [ ] **Storage layer complete** → 85+ tests passing with openraft
- [ ] **State machine complete** → Idempotency validated end-to-end
- [ ] **Network stub complete** → Ready for future gRPC transport
- [ ] **Node migration complete** → No prost conflicts, all APIs async
- [ ] **Integration complete** → End-to-end tests passing, docs updated

---

## Risk Mitigation

### High-Risk Tasks

#### 1. Task 3.2: Implement apply() - Idempotency Preservation
**Risk:** Loss of idempotency guarantees during state machine migration
**Impact:** HIGH - Could allow duplicate entries to corrupt state
**Mitigation:**
- Keep existing StateMachine::apply() logic unchanged
- Wrapper only delegates, doesn't modify behavior
- Add comprehensive idempotency tests before proceeding

**Validation Gate:**
- All idempotency tests (Task 3.4) must pass before Phase 5
- Test: duplicate entries rejected
- Test: out-of-order entries rejected
- Test: last_applied tracked correctly

---

#### 2. Task 5.1: Update Dependencies - Prost Conflict Resolution
**Risk:** Prost version conflict (0.12 vs 0.14) blocks compilation
**Impact:** HIGH - Blocks entire Node Migration phase
**Mitigation:**
- Verify openraft 0.10 uses prost 0.14
- Check tonic 0.14 compatibility
- Run `cargo tree | grep prost` immediately after dependency update

**Validation Gate:**
- GO/NO-GO decision point: Single prost version in cargo tree
- If conflict persists: investigate openraft version or tonic downgrade
- Must resolve before Task 5.2 (Node Initialization)

---

#### 3. Task 5.4: Migrate API - Public API Changes
**Risk:** Breaking changes to RaftNode API affect dependent crates
**Impact:** MEDIUM - Requires updates in kv/, sql/, seshat/ crates
**Mitigation:**
- Document all async signature changes
- Create migration guide for async API usage
- Update dependent crates in same commit

**Validation Gate:**
- Integration tests verify API contracts unchanged (semantically)
- All async conversions use proper error handling
- No blocking calls in async contexts

---

#### 4. Task 2.4: Migrate Storage Tests - Test Coverage Loss
**Risk:** Lost test coverage during async migration
**Impact:** MEDIUM - Reduced confidence in storage correctness
**Mitigation:**
- Track test count before/after: must maintain 85+ tests
- Convert tests incrementally, verify each batch passes
- Add new async-specific tests (race conditions, deadlocks)

**Validation Gate:**
- Minimum 85 tests passing
- No flaky tests due to async timing
- Coverage report shows maintained or improved coverage

---

### Validation Gates Summary

**After Phase 1:**
- [ ] All type conversions compile
- [ ] Property tests pass (1000+ random inputs)
- [ ] No panics in type conversion tests

**After Phase 2:**
- [ ] 85+ storage tests passing
- [ ] No async deadlocks or race conditions
- [ ] RaftLogReader, RaftSnapshotBuilder, RaftStorage fully implemented

**After Phase 3:**
- [ ] All idempotency tests pass
- [ ] StateMachine wrapper preserves existing behavior
- [ ] Snapshot round-trip verified

**After Phase 4:**
- [ ] Network stub compiles and links
- [ ] All send methods return Ok without panic
- [ ] Ready for future gRPC integration

**After Phase 5:**
- [ ] Single prost version (0.14) in cargo tree
- [ ] All RaftNode tests passing (async)
- [ ] No raft-rs imports remain

**After Phase 6:**
- [ ] End-to-end integration tests pass
- [ ] Zero clippy warnings
- [ ] Documentation updated
- [ ] Migration complete

---

## Fast Feedback Loops

### After Each Task
Run quick unit tests to verify immediate correctness:

```bash
# Fast feedback (<30 seconds)
cargo test --lib --package raft
cargo test --lib --package storage
```

### After Each Phase
Run full test suite to ensure integration correctness:

```bash
# Full validation (1-2 minutes)
cargo test --all

# Check for unused imports and dead code
cargo clippy --all-targets
```

### After Dependency Updates (Task 5.1)
Critical validation before proceeding:

```bash
# Verify prost conflict resolution (<5 seconds)
cargo tree | grep prost | sort | uniq

# Expected output: Single line with prost v0.14.x
# If multiple versions appear: STOP and investigate
```

### Continuous Validation
Run on every commit:

```bash
# Standard validation pipeline
cargo build --all-features
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

---

## Next Steps

### Getting Started

**1. Begin with Phase 1 (Type System)**
This is the foundation for all other work. No other phase can start until Phase 1 completes.

```bash
# Command to begin
/spec:implement openraft type_system

# Or start with first task
/spec:implement openraft 1.1
```

**2. After Phase 1: Launch Parallel Tracks**
Once type system is complete, three agents can work concurrently:

```bash
# Agent 1: Storage Layer
/spec:implement openraft storage_layer

# Agent 2: State Machine (parallel)
/spec:implement openraft state_machine

# Agent 3: Network Stub (parallel)
/spec:implement openraft network_stub
```

**3. Converge on Node Migration**
After Phases 2-4 complete, all agents work on Phase 5:

```bash
# All agents: Node Migration
/spec:implement openraft node_migration
```

**4. Final Integration**
Complete with Phase 6 validation and cleanup:

```bash
# All agents: Integration & Cleanup
/spec:implement openraft integration
```

### Tracking Progress

Update this file as you complete tasks:

```bash
# After each task, mark as complete
/spec:progress openraft

# View overall feature progress
/spec:progress openraft verbose
```

---

## Appendix: Quick Reference

### Key Files Modified
- `crates/raft/src/types.rs` - Type definitions and conversions
- `crates/storage/src/openraft_storage.rs` - Storage trait implementation
- `crates/raft/src/state_machine_wrapper.rs` - State machine wrapper
- `crates/raft/src/network_stub.rs` - Stub network implementation
- `crates/raft/src/node.rs` - RaftNode migration
- `crates/raft/Cargo.toml` - Dependency updates
- `crates/storage/Cargo.toml` - Dependency updates

### Critical Dependencies
- openraft = "0.10" (with tokio feature)
- async-trait = "0.1"
- tracing = "0.1"
- tokio (existing, for async runtime)
- prost = "0.14" (must match tonic 0.14)

### Success Criteria
- [ ] 85+ tests passing
- [ ] Single prost version (0.14)
- [ ] Zero clippy warnings
- [ ] Zero compilation errors
- [ ] All idempotency tests pass
- [ ] End-to-end integration tests pass
- [ ] Documentation updated

---

**Created:** 2025-10-26
**Feature:** openraft-migration
**Estimated Single-Agent Time:** 15-21 hours
**Estimated Multi-Agent Time:** 12-16 hours (3 agents)
**Current Status:** Ready to begin
