# Raft Implementation Tasks

## Phase 1: Common Foundation (✅ Complete)
- [x] **common_types** - Common Type Aliases (30 min)
- [x] **common_errors** - Common Error Types (30 min)

## Phase 2: Configuration (✅ Complete)
- [x] **config_types** - Configuration Data Types (1 hour)
- [x] **config_validation** - Configuration Validation (1 hour)
- [x] **config_defaults** - Configuration Default Values (30 min)

## Phase 3: Protocol Definitions (🚧 50% Complete)
- [x] **protobuf_messages** - Protobuf Message Definitions (1.5 hours)
  - **Test**: Message serialization/deserialization roundtrips
  - **Implement**: Create raft.proto with RequestVote, AppendEntries, InstallSnapshot messages
  - **Refactor**: Organize messages and add comprehensive comments
  - **Files**: `crates/protocol/proto/raft.proto`, `crates/protocol/build.rs`, `crates/protocol/src/lib.rs`, `crates/protocol/Cargo.toml`
  - **Acceptance**: RaftService with 3 RPCs, 9 message types, EntryType enum, build.rs compiles proto, roundtrip tests pass
  - **Status**: ✅ Completed 2025-10-15

- [ ] **operation_types** - Operation Types (30 min)
  - **Test**: Write tests for Operation::apply() and serialization
  - **Implement**: Define Operation enum with Set and Del variants
  - **Refactor**: Extract apply logic into trait methods
  - **Files**: `crates/protocol/src/operations.rs`
  - **Acceptance**: Operation::Set and Operation::Del variants, apply() method, serialize/deserialize with bincode

## Phase 4: Storage Layer (✅ Complete)
- [x] **mem_storage_skeleton** - MemStorage Structure (30 min)
- [x] **mem_storage_initial_state** - Storage: initial_state() (30 min)
- [x] **mem_storage_entries** - Storage: entries() (1 hour)
- [x] **mem_storage_term** - Storage: term() (30 min)
- [x] **mem_storage_first_last_index** - Storage: first_index() and last_index() (30 min)
- [x] **mem_storage_snapshot** - Storage: snapshot() (30 min)
- [x] **mem_storage_mutations** - Storage Mutation Methods (1 hour)

## Phase 5: State Machine (Not Started)
- [ ] **state_machine_core** - StateMachine Core Structure (1 hour)
  - **Test**: Write tests for new(), get(), exists()
  - **Implement**: Define StateMachine with data HashMap and last_applied
  - **Refactor**: Add internal helpers
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: StateMachine struct with HashMap and last_applied, new(), get(), exists() methods

- [ ] **state_machine_operations** - StateMachine Apply Operations (1.5 hours)
  - **Test**: Write tests: apply Set, apply Del, operation ordering, idempotency
  - **Implement**: Implement apply(entry) with Operation deserialization
  - **Refactor**: Extract operation execution logic
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: apply() deserializes Operation, checks idempotency, updates last_applied, returns result

- [ ] **state_machine_snapshot** - StateMachine Snapshot and Restore (30 min)
  - **Test**: Write tests: snapshot with data, restore from snapshot, roundtrip
  - **Implement**: Implement snapshot() using bincode, restore() to deserialize
  - **Refactor**: Add version field to snapshot format
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: snapshot() and restore() methods, roundtrip test passes

## Phase 6: Raft Node (Not Started)
- [ ] **raft_node_initialization** - RaftNode Initialization (2 hours)
  - **Test**: Create RaftNode with valid config, verify fields are set
  - **Implement**: Define RaftNode struct, implement new() with raft::Config conversion
  - **Refactor**: Extract config conversion to helper
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: RaftNode struct, new() creates MemStorage, RawNode, StateMachine

- [ ] **raft_node_tick** - RaftNode Tick Processing (30 min)
  - **Test**: Call tick() multiple times, verify no panics
  - **Implement**: Implement tick() calling raw_node.tick()
  - **Refactor**: Add instrumentation logging
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: tick() calls raw_node.tick(), returns Result<()>

- [ ] **raft_node_propose** - RaftNode Propose Client Commands (1 hour)
  - **Test**: Propose as follower returns NotLeader error
  - **Implement**: Implement propose() calling raw_node.propose()
  - **Refactor**: Add leader check and error handling
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: propose() checks is_leader(), returns NotLeader if follower

- [ ] **raft_node_ready_handler** - RaftNode Ready Processing (1.5 hours)
  - **Test**: handle_ready with no ready state returns empty
  - **Implement**: Implement full Ready processing: persist → send → apply → advance
  - **Refactor**: Extract apply logic, add comprehensive logging
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: handle_ready() persists, sends, applies, advances in correct order

- [ ] **raft_node_leader_queries** - RaftNode Leader Queries (30 min)
  - **Test**: New node is not leader, leader_id returns None initially
  - **Implement**: Implement queries using raw_node.raft.state
  - **Refactor**: Add caching if needed
  - **Files**: `crates/raft/src/node.rs`
  - **Acceptance**: is_leader() and leader_id() return correct values

## Phase 7: Integration (Not Started)
- [ ] **single_node_bootstrap** - Single Node Bootstrap Test (1 hour)
  - **Test**: Create RaftNode, tick until becomes leader
  - **Implement**: Use test utilities to create node and run event loop
  - **Refactor**: Extract test helpers for reuse
  - **Files**: `crates/raft/tests/integration_tests.rs`, `crates/raft/tests/common/mod.rs`
  - **Acceptance**: Node becomes leader after election timeout, test passes within 5s

- [ ] **single_node_propose_apply** - Single Node Propose and Apply Test (1 hour)
  - **Test**: Become leader, propose SET, handle ready, verify get() works
  - **Implement**: Propose operation, process ready in loop, check state machine
  - **Refactor**: Add async test utilities
  - **Files**: `crates/raft/tests/integration_tests.rs`
  - **Acceptance**: Can propose and apply operation, state machine reflects changes

## Progress Summary
- **Total Tasks**: 24
- **Completed**: 13 (54.2%)
- **In Progress**: 0
- **Not Started**: 11

## Next Recommended Task
`operation_types` - Complete Phase 3 (Protocol Definitions)
