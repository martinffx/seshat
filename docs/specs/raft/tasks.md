# Raft Implementation Tasks

## Phase 1: Common Foundation (✅ Complete)
- [x] **common_types** - Common Type Aliases (30 min)
- [x] **common_errors** - Common Error Types (30 min)

## Phase 2: Configuration (✅ Complete)
- [x] **config_types** - Configuration Data Types (1 hour)
- [x] **config_validation** - Configuration Validation (1 hour)
- [x] **config_defaults** - Configuration Default Values (30 min)

## Phase 3: Protocol Definitions (✅ Complete)
- [x] **protobuf_messages** - Protobuf Message Definitions (1.5 hours)
  - **Test**: Message serialization/deserialization roundtrips
  - **Implement**: Create raft.proto with RequestVote, AppendEntries, InstallSnapshot messages
  - **Refactor**: Organize messages and add comprehensive comments
  - **Files**: `crates/protocol/proto/raft.proto`, `crates/protocol/build.rs`, `crates/protocol/src/lib.rs`, `crates/protocol/Cargo.toml`
  - **Acceptance**: RaftService with 3 RPCs, 9 message types, EntryType enum, build.rs compiles proto, roundtrip tests pass
  - **Status**: ✅ Completed 2025-10-15

- [x] **operation_types** - Operation Types (30 min)
  - **Test**: Write tests for Operation::apply() and serialization
  - **Implement**: Define Operation enum with Set and Del variants
  - **Refactor**: Extract apply logic into trait methods
  - **Files**: `crates/protocol/src/operations.rs`
  - **Acceptance**: Operation::Set and Operation::Del variants, apply() method, serialize/deserialize with bincode
  - **Status**: ✅ Completed 2025-10-15
  - **Implementation Details**:
    - Created Operation enum with Set {key, value} and Del {key} variants
    - Implemented apply() method that modifies HashMap and returns response bytes
    - Implemented serialize() using bincode::serialize
    - Implemented deserialize() using bincode::deserialize
    - Added OperationError with SerializationError variant
    - Comprehensive test coverage (19 tests) including roundtrips, edge cases, and error handling
    - All tests passing

## Phase 4: Storage Layer (✅ Complete)
- [x] **mem_storage_skeleton** - MemStorage Structure (30 min)
- [x] **mem_storage_initial_state** - Storage: initial_state() (30 min)
- [x] **mem_storage_entries** - Storage: entries() (1 hour)
- [x] **mem_storage_term** - Storage: term() (30 min)
- [x] **mem_storage_first_last_index** - Storage: first_index() and last_index() (30 min)
- [x] **mem_storage_snapshot** - Storage: snapshot() (30 min)
- [x] **mem_storage_mutations** - Storage Mutation Methods (1 hour)

## Phase 5: State Machine (✅ Complete)
- [x] **state_machine_core** - StateMachine Core Structure (1 hour)
  - **Test**: Write tests for new(), get(), exists()
  - **Implement**: Define StateMachine with data HashMap and last_applied
  - **Refactor**: Add internal helpers
  - **Files**: `crates/raft/src/state_machine.rs`
  - **Acceptance**: StateMachine struct with HashMap and last_applied, new(), get(), exists(), last_applied() methods
  - **Status**: ✅ Completed 2025-10-15
  - **Implementation Details**:
    - Created StateMachine struct with `data: HashMap<Vec<u8>, Vec<u8>>` and `last_applied: u64`
    - Implemented `new()` constructor initializing empty HashMap and last_applied=0
    - Implemented `get(&self, key: &[u8]) -> Option<Vec<u8>>` using HashMap::get with cloned value
    - Implemented `exists(&self, key: &[u8]) -> bool` using HashMap::contains_key
    - Implemented `last_applied(&self) -> u64` returning last_applied field
    - Added Default trait implementation
    - Comprehensive test coverage (9 tests) covering all methods and edge cases
    - All tests passing
    - Added module to lib.rs with re-export

- [x] **state_machine_operations** - StateMachine Apply Operations (1.5 hours)
  - **Test**: Write tests: apply Set, apply Del, operation ordering, idempotency
  - **Implement**: Implement apply(entry) with Operation deserialization
  - **Refactor**: Extract operation execution logic
  - **Files**: `crates/raft/src/state_machine.rs`, `crates/raft/Cargo.toml`
  - **Acceptance**: apply() deserializes Operation, checks idempotency, updates last_applied, returns result
  - **Status**: ✅ Completed 2025-10-15
  - **Implementation Details**:
    - Added `seshat-protocol` dependency to raft crate's Cargo.toml
    - Implemented `apply(&mut self, index: u64, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>`
    - Idempotency check: Rejects index <= last_applied with descriptive error
    - Deserializes Operation from bytes using `Operation::deserialize(data)`
    - Executes operation on HashMap using `operation.apply(&mut self.data)`
    - Updates last_applied after successful execution
    - Returns operation result bytes
    - Comprehensive test coverage (10 new tests):
      1. test_apply_set_operation - Apply Set, verify result and state
      2. test_apply_del_operation_exists - Apply Del on existing key
      3. test_apply_del_operation_not_exists - Apply Del on missing key
      4. test_operation_ordering - Multiple Sets to same key
      5. test_idempotency_check - Reject duplicate index
      6. test_out_of_order_rejected - Reject lower index
      7. test_apply_multiple_operations - Sequence of operations
      8. test_apply_with_invalid_data - Corrupted bytes
      9. test_apply_empty_key - Edge case: empty key
      10. test_apply_large_value - Edge case: large value (10KB)
    - All 19 tests passing (9 existing + 10 new)
    - Proper error handling with Box<dyn std::error::Error>
    - Clear error messages for idempotency violations
    - No unwrap() in production code

- [x] **state_machine_snapshot** - StateMachine Snapshot and Restore (30 min)
  - **Test**: Write tests: snapshot with data, restore from snapshot, roundtrip
  - **Implement**: Implement snapshot() using bincode, restore() to deserialize
  - **Refactor**: Add version field to snapshot format
  - **Files**: `crates/raft/src/state_machine.rs`, `crates/raft/Cargo.toml`
  - **Acceptance**: snapshot() and restore() methods, roundtrip test passes
  - **Status**: ✅ Completed 2025-10-15
  - **Implementation Details**:
    - Added `bincode` dependency to raft crate's Cargo.toml
    - Added `Serialize` and `Deserialize` derives to StateMachine struct
    - Implemented `snapshot(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>>`
      - Serializes entire state machine (data HashMap + last_applied) using bincode
      - Returns serialized bytes for log compaction or state transfer
    - Implemented `restore(&mut self, snapshot: &[u8]) -> Result<(), Box<dyn std::error::Error>>`
      - Deserializes snapshot and overwrites current state
      - Replaces data HashMap and last_applied index
    - Comprehensive test coverage (9 new tests):
      1. test_snapshot_empty - Empty state machine snapshot
      2. test_snapshot_with_data - Snapshot with existing data
      3. test_restore_from_snapshot - Basic restore functionality
      4. test_snapshot_restore_roundtrip - Full serialization roundtrip
      5. test_restore_empty_snapshot - Edge case: empty snapshot
      6. test_restore_overwrites_existing_state - Verify complete replacement
      7. test_restore_with_invalid_data - Error handling for corrupted data
      8. test_snapshot_large_state - 100 keys performance test
    - All 35 unit tests + 3 doc tests passing (38 total state machine tests)
    - No clippy warnings
    - Clean error handling with Box<dyn std::error::Error>
    - Comprehensive documentation with usage examples

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
- **Completed**: 17 (70.8%)
- **In Progress**: 0
- **Not Started**: 7

## Next Recommended Task
`raft_node_initialization` - Begin Phase 6 (Raft Node initialization)
