# OpenRaft Migration - Specification Summary

## Overview
Replace raft-rs 0.7 with openraft to eliminate prost dependency conflicts (0.11→0.14) and modernize to a better-maintained Raft implementation. Keep MemStorage (in-memory), stub network transport, no KV integration yet.

## User Story
As a **Seshat developer**, I want to **migrate from raft-rs to openraft** so that **we eliminate prost dependency conflicts and gain a better-maintained Raft implementation with cleaner APIs**.

## Key Acceptance Criteria
1. **Dependency Resolution** - Eliminate prost 0.11, unify on prost 0.14 throughout codebase
2. **Storage Integration** - MemStorage works with openraft storage traits (in-memory only)
3. **State Machine** - Operations applied in correct order with strong consistency
4. **Test Migration** - All existing unit tests pass with openraft

## Critical Business Rules
1. Eliminate prost version conflicts (primary motivation)
2. Keep MemStorage in-memory design (no RocksDB)
3. Stub network transport for future gRPC
4. No KV service integration in this phase
5. Maintain tracing/observability patterns

## Scope

**Included:**
- Replace raft-rs dependency with openraft
- Adapt MemStorage to implement openraft storage traits (in-memory)
- Migrate state machine to openraft API
- Update RaftNode wrapper (raft::RawNode → openraft::Raft)
- Remove prost 0.11, standardize on prost 0.14
- Migrate unit tests to openraft
- Stub RaftNetwork trait for future gRPC

**Excluded:**
- RocksDB persistent storage (future phase)
- Full gRPC transport implementation (future phase)
- KV service integration (future phase)
- RESP protocol integration (future phase)
- Integration/chaos tests (future phase)
- Performance benchmarking (future phase)
- Cluster formation modes (future phase)

## Major Technical Changes

### Interfaces
- **openraft storage traits** - Implement for MemStorage (in-memory)
- **openraft::RaftStateMachine** - Apply operations to in-memory state
- **openraft::RaftNetwork** - Stub for future gRPC transport
- **RaftNode wrapper** - Migrate from raft::RawNode to openraft::Raft

### Integration Points
- raft → storage (MemStorage for in-memory log/state)
- raft → common (shared types: NodeId, Error)

### Implementation Phases (10-15 hours)
1. Dependency replacement (1-2h)
2. MemStorage adaptation to openraft storage traits (2-3h)
3. RaftStateMachine trait implementation (2-3h)
4. Stub RaftNetwork (1h)
5. RaftNode wrapper migration (2-3h)
6. Unit test migration (2-3h)

## Dependencies & Conflicts

**Dependencies:**
- seshat-storage (MemStorage in-memory)
- seshat-common (NodeId, Error types)
- openraft (external), tokio, tracing

**Conflicts:**
- raft::RawNode API → openraft::Raft API (wrapper updates)
- Prost 0.11 vs 0.14 (resolved by migration)
- MemStorage needs adaptation to openraft traits
- Test fixtures need migration

## Success Metrics
- [ ] Zero prost conflicts in `cargo tree`
- [ ] All unit tests pass
- [ ] MemStorage works with openraft
- [ ] RaftNode wrapper functional
- [ ] Code compiles without raft-rs

## Next Action
Run `/spec:design openraft` to create detailed technical design.

---
**Feature:** openraft | **Phase:** 1 (MVP Preparation) | **Priority:** HIGH
**Effort:** 10-15 hours | **Focus:** Dependency swap + MemStorage adaptation
