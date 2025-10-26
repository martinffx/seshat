# Feature Specification: OpenRaft Migration

## Overview

Migrate Seshat's consensus layer from `raft-rs 0.7` to `openraft` to eliminate transitive prost dependency conflicts (0.11 vs 0.14) and gain a better-maintained Raft implementation with cleaner trait APIs.

**Current State:** Phase 1 has MemStorage implementation complete (storage layer) using raft-rs with in-memory storage. RaftNode wrapper and StateMachine exist but use synchronous raft-rs APIs. RESP protocol (100% complete) not yet connected to Raft.

**Target State:** Fully functional openraft-based consensus with MemStorage (in-memory), stub inter-node communication, and no integration with KV service layer.

## User Story

As a **Seshat developer**, I want to **migrate from raft-rs to openraft** so that **we eliminate outdated transitive prost dependencies and gain a better-maintained Raft implementation with cleaner APIs**.

## Acceptance Criteria

- [ ] **AC1: Dependency Resolution** - GIVEN existing raft-rs 0.7 dependency with prost-codec feature WHEN replaced with openraft THEN transitive prost 0.11 dependency is eliminated and unified prost 0.14 is used throughout
- [ ] **AC2: Storage Integration** - GIVEN openraft storage trait implementation WHEN integrated with MemStorage backend THEN storage operations (log entries, hard state, snapshots) work correctly in-memory
- [ ] **AC3: State Machine Operations** - GIVEN openraft state machine implementation WHEN operations are proposed THEN operations are applied in correct order with strong consistency guarantees
- [ ] **AC4: Test Migration** - GIVEN existing unit tests WHEN migrated to openraft THEN all tests pass with equivalent or better coverage

## Business Rules

1. **Primary Migration Motivation** - Must eliminate prost version conflicts between raft library (0.11) and transport layer (0.14)
2. **Storage Design** - Must maintain existing MemStorage in-memory design (no persistent storage implementation)
3. **Serialization Strategy** - Use protobuf (prost) for all serialization (storage + network) - single format throughout, same as network layer
4. **Observability Continuity** - Must maintain existing logging and observability patterns using tracing crate with structured logging
5. **Transport Stub** - Transport layer should have stub/placeholder implementation for future gRPC integration
6. **No KV Integration** - No integration with KV service layer in this phase - focus on core migration only

## Scope

### Included

1. Replace raft-rs 0.7 dependency with openraft in Cargo.toml (workspace-level change)
2. Implement openraft storage traits using existing MemStorage (in-memory)
3. Migrate state machine from raft-rs RawNode API to openraft API
4. Update RaftNode wrapper to use `openraft::Raft` instead of `raft::RawNode`
5. Remove prost 0.11 dependency and standardize on prost 0.14 throughout codebase
6. **Define protobuf schemas** for storage types (LogEntry, HardState, Snapshot metadata) - use prost for encoding/decoding
7. **Remove bincode dependency** entirely - replaced by protobuf for all serialization
8. Update all unit tests in raft crate to work with openraft APIs
9. Add stub/placeholder for network transport (RaftNetwork trait)
10. Add tracing instrumentation for openraft operations (leader election, log replication)

### Excluded

1. RocksDB persistent storage implementation (future phase)
2. Full gRPC transport layer implementation (future phase)
3. Connection pooling and retry logic for network transport (future phase)
4. Integration with KV service layer (future phase)
5. RESP protocol integration (future phase)
6. Snapshot creation and restoration with RocksDB checkpoints (future phase)
7. Integration tests for 2-node and 3-node clusters (future phase)
8. Chaos testing implementation (future phase)
9. Performance benchmarking and optimization (future phase)
10. Changes to seshat main binary orchestration (separate task)
11. Bootstrap/join cluster formation modes (future phase)
12. Multi-shard cluster support (Phase 2 feature)
13. Dynamic cluster membership changes (Phase 3 feature)
14. Advanced observability features like OpenTelemetry (Phase 4 feature)
15. SQL interface support (Phase 5 feature)

## Technical Details

### Interfaces Affected

1. **openraft storage traits** - Must be implemented for MemStorage backend (in-memory)
2. **`openraft::RaftStateMachine` trait** - Applies committed operations to in-memory state
3. **`openraft::RaftNetwork` trait** - Stub implementation for future gRPC transport
4. **`RaftNode` wrapper struct** - Changes from `raft::RawNode` to `openraft::Raft`
5. **Storage trait methods** - Must map to openraft storage requirements

### Integration Points

1. **raft crate → storage crate** - MemStorage for in-memory log and state storage
2. **raft crate → common crate** - Use shared types (NodeId, Error) throughout

### Testing Requirements

1. Unit tests for openraft storage trait implementation with MemStorage
2. Unit tests for state machine applying operations correctly
3. Unit tests for RaftNode wrapper with openraft::Raft
4. Property tests for entry serialization/deserialization round-trips

### Implementation Phases

| Phase | Description | Estimated Time |
|-------|-------------|----------------|
| 1 | Replace raft-rs dependency, update Cargo.toml, resolve prost conflicts | 1-2 hours |
| 2 | Adapt MemStorage to implement openraft storage traits | 2-3 hours |
| 3 | Implement `openraft::RaftStateMachine` trait for in-memory operations | 2-3 hours |
| 4 | Create stub RaftNetwork implementation | 1 hour |
| 5 | Update RaftNode wrapper to use `openraft::Raft` API | 2-3 hours |
| 6 | Migrate unit tests to openraft equivalents, ensure all pass | 2-3 hours |
| **Total** | | **10-15 hours** |

### Risk Mitigation

| Risk | Mitigation |
|------|------------|
| openraft API significantly different from raft-rs | Review openraft examples and docs before implementation, create prototype wrapper |
| MemStorage incompatible with openraft traits | Study openraft storage trait requirements, adapt incrementally with tests |
| Prost version conflicts persist | Verify openraft uses prost 0.12+ and is compatible with tonic 0.14 |
| Tests fail after migration | Migrate tests incrementally, maintain test coverage throughout |

### Observability Requirements

- Add tracing spans for leader election with node_id and term fields
- Add tracing spans for log replication with entry count and commit index
- Log state machine operations at DEBUG level with operation type
- Use `tracing::instrument` macro on key RaftNode methods
- Ensure all errors include context for debugging (use `thiserror` with context)

## Dependencies

1. **seshat-storage** - MemStorage in-memory implementation
2. **seshat-common** - Shared types (NodeId, Error)
3. **openraft** (external) - Raft consensus library to replace raft-rs
4. **prost 0.14** - Protobuf serialization for storage and network (unified format)
5. **tokio 1.x** - Async runtime
6. **tracing** - Structured logging for observability

**Dependencies to REMOVE:**
- **bincode** - Replaced by protobuf for all serialization
- **raft-rs 0.7** - Replaced by openraft

## Conflicts & Resolution

| Conflict | Resolution |
|----------|-----------|
| raft-rs RawNode API differs from openraft::Raft API | Update RaftNode wrapper to adapt between openraft and existing interfaces |
| Prost 0.11 (raft-rs) conflicts with prost 0.14 (tonic) | Migration to openraft eliminates this conflict - openraft uses compatible prost version |
| Test mocks using raft-rs types | Migrate to openraft equivalents, may require new test fixtures |
| MemStorage needs adaptation | Implement openraft storage traits for MemStorage |

## Alignment

This feature aligns with **Phase 1 MVP preparation** by eliminating technical debt (prost version conflicts) and modernizing to a better-maintained Raft library. This establishes the foundation for future persistent storage and network transport implementation.

**Addresses immediate technical debt:** Prost version conflicts blocking modern dependency usage

**Establishes foundation for:** RocksDB persistence (future), gRPC transport (future), KV integration (future), Phase 2+ features

## Success Metrics

- [ ] Zero prost dependency conflicts in `cargo tree`
- [ ] All existing raft unit tests pass with openraft
- [ ] MemStorage works correctly with openraft storage traits
- [ ] RaftNode wrapper functions with openraft::Raft
- [ ] Code compiles without raft-rs dependency

## Next Steps

1. **Review this specification** - Ensure simplified scope is aligned with goals
2. **Create technical design** - Run `/spec:design openraft` to generate detailed architecture
3. **Generate implementation tasks** - Run `/spec:plan openraft` to break down work into dependency-ordered tasks
4. **Begin implementation** - Run `/spec:implement openraft` to start TDD-based development
5. **Track progress** - Use `/spec:progress openraft` to monitor task completion

---

**Created:** 2025-10-25
**Feature:** openraft
**Phase:** 1 (MVP Preparation)
**Priority:** HIGH (eliminates technical debt)
**Estimated Effort:** 10-15 hours
