# Specification Alignment Summary

**Date:** 2025-10-27
**Context:** After completing OpenRaft 0.9 in-memory storage implementation with consolidated architecture

## Implementation vs. Specs: Key Discrepancies

### 1. OpenRaft Version ❌ CRITICAL

**Specs Say:** OpenRaft 0.10
**Reality:** OpenRaft 0.9.21 (latest available)
**Impact:** API differences - 0.10 doesn't exist

**Files to Update:**
- `docs/specs/openraft/spec.md` - Change version throughout
- `docs/specs/openraft/design.md` - Update API examples
- `docs/specs/openraft/tasks.md` - Update dependency versions
- `docs/specs/rocksdb/spec.md` - Update OpenRaft version reference

---

### 2. Storage API Pattern ❌ CRITICAL

**Specs Say:** Unified `RaftStorage` trait
**Reality:** Split traits - `RaftLogStorage` + `RaftStateMachine` (storage-v2 API)

**OpenRaft 0.9 storage-v2 Pattern:**
```rust
// TWO separate traits, not one unified trait
impl RaftLogStorage<RaftTypeConfig> for OpenRaftMemLog {
    type LogReader = OpenRaftMemLogReader;
    // Manages: log entries, votes, log state
}

impl RaftStateMachine<RaftTypeConfig> for OpenRaftMemStateMachine {
    type SnapshotBuilder = OpenRaftMemSnapshotBuilder;
    // Manages: state machine operations, snapshots
}
```

**Files to Update:**
- `docs/specs/openraft/design.md` - Update trait examples to show split pattern
- `docs/specs/rocksdb/design.md` - Update to show OpenRaftRocksDBLog + OpenRaftRocksDBStateMachine
- `docs/specs/rocksdb/spec.md` - Update integration points section

---

### 3. Crate Structure ❌ MAJOR

**Specs Say:** 6 crates
```
crates/
├── seshat/
├── raft/
├── storage/
├── common/
├── protocol-resp/
└── kv/
```

**Reality:** 4 crates (consolidated)
```
crates/
├── seshat/         - Main binary
├── seshat-storage/ - Raft types + operations + storage (MERGED raft + storage + common)
├── seshat-resp/    - RESP protocol (RENAMED from protocol-resp)
└── seshat-kv/      - KV service
```

**Key Changes:**
- `types.rs` moved from `crates/raft/` → `crates/storage/`
- `operations.rs` moved from `crates/kv/` → `crates/storage/`
- `common/` crate eliminated - types in storage
- `protocol-resp` renamed to `seshat-resp`

**Files to Update:**
- `docs/specs/openraft/spec.md` - Update crate references
- `docs/specs/openraft/design.md` - Update file paths
- `docs/specs/openraft/tasks.md` - Update file locations in all tasks
- `docs/specs/rocksdb/spec.md` - Update "Used By" and "Depends On" sections
- `docs/specs/rocksdb/design.md` - Update crate references

---

### 4. Error Handling Pattern ⚠️ IMPORTANT

**Specs Show:** Generic error construction
**Reality:** Specific OpenRaft 0.9 pattern required

**Correct Pattern:**
```rust
StorageError::IO {
    source: StorageIOError::new(
        ErrorSubject::StateMachine,  // or Snapshot, LogStore
        ErrorVerb::Write,            // or Read
        AnyError::error(e),          // Consumes error (NOT &e)
    ),
}
```

**Files to Update:**
- `docs/specs/openraft/design.md` - Add error handling section with correct examples
- `docs/specs/rocksdb/design.md` - Show error mapping for RocksDB → OpenRaft errors

---

### 5. Entry Payload Handling ⚠️ IMPORTANT

**Missing from Specs:** How to handle different entry types
**Reality:** Must handle 3 payload variants

**Required Pattern:**
```rust
match &entry.payload {
    openraft::EntryPayload::Normal(ref req) => {
        // Apply to state machine
    }
    openraft::EntryPayload::Blank => {
        // No-op entry - return empty response
    }
    openraft::EntryPayload::Membership(_) => {
        // Membership change - return empty response
    }
}
```

**Files to Update:**
- `docs/specs/openraft/design.md` - Add entry payload handling section
- `docs/specs/rocksdb/design.md` - Show how RocksDB implementation handles these

---

### 6. Implementation Status ✅ UPDATE NEEDED

**Current Status:**
- ✅ Phase 2 Complete: In-memory storage (OpenRaftMemLog + OpenRaftMemStateMachine)
- ✅ 143 tests passing
- ✅ Clean build with zero warnings

**Files to Update:**
- `docs/specs/openraft/tasks.md` - Mark Phase 2 tasks complete
- `docs/specs/openraft/spec.md` - Update progress section

---

## RocksDB Spec Alignment

### 7. OpenRaft Integration Points ⚠️ NEEDS UPDATE

**Current RocksDB Spec Says:**
```
Integration Points:
- openraft storage traits: RaftLogReader, RaftSnapshotBuilder, RaftStorage (openraft version)
- raft crate provides OpenRaftMemStorage wrapper
```

**Should Say:**
```
Integration Points:
- openraft storage-v2 traits: RaftLogStorage + RaftStateMachine (split pattern)
- seshat-storage crate provides:
  - OpenRaftRocksDBLog (implements RaftLogStorage)
  - OpenRaftRocksDBStateMachine (implements RaftStateMachine)
```

**Files to Update:**
- `docs/specs/rocksdb/spec.md` - Line 79 (Integration Points)
- `docs/specs/rocksdb/design.md` - Update architecture diagram to show split traits

---

### 8. Column Family Design ✅ MOSTLY ALIGNED

**Current CF Design:** 6 column families
**Alignment Check:** Need to verify CF design supports split storage model

**For RaftLogStorage (OpenRaftRocksDBLog):**
- Uses: `data_raft_log` (log entries)
- Uses: `data_raft_state` (vote storage)
- Methods: append, truncate, purge, get_log_state, save_vote, read_vote

**For RaftStateMachine (OpenRaftRocksDBStateMachine):**
- Uses: `data_kv` (state machine data)
- Uses: Snapshot metadata (needs clarification - where stored?)
- Methods: apply, get_current_snapshot, install_snapshot, build_snapshot

**Files to Update:**
- `docs/specs/rocksdb/design.md` - Add section on CF usage by each trait
- `docs/specs/rocksdb/spec.md` - Clarify which CFs serve which storage traits

---

## Summary of Required Updates

### High Priority (Blocking Correctness)

1. **OpenRaft version** 0.10 → 0.9.21 everywhere
2. **Storage API** unified RaftStorage → split RaftLogStorage + RaftStateMachine
3. **Crate structure** 6 crates → 4 crates, update all file paths
4. **Error patterns** Add OpenRaft 0.9 specific error construction examples

### Medium Priority (Important for Clarity)

5. **Entry payload handling** Add examples of 3-variant match
6. **Implementation status** Mark Phase 2 complete in tasks.md
7. **RocksDB integration** Update to reference split storage traits
8. **CF design clarification** Document which CFs serve which traits

### Files Requiring Updates

**OpenRaft Specs:**
- ✅ `docs/specs/openraft/status.md` - ALREADY UPDATED
- ❌ `docs/specs/openraft/spec.md` - Version, crates, progress
- ❌ `docs/specs/openraft/design.md` - API patterns, errors, file paths
- ❌ `docs/specs/openraft/tasks.md` - Mark Phase 2 complete, update file paths

**RocksDB Specs:**
- ❌ `docs/specs/rocksdb/spec.md` - Integration points, dependencies, crate refs
- ❌ `docs/specs/rocksdb/design.md` - Architecture, split traits, CF usage

---

## Next Steps

1. Update OpenRaft spec files (spec.md, design.md, tasks.md)
2. Update RocksDB spec files to align with split storage pattern
3. Add implementation examples to design docs
4. Verify all file paths reference `seshat-storage` (not `seshat-raft`)

---

**Created:** 2025-10-27
**Purpose:** Track alignment between specs and actual implementation
**Status:** Ready for spec updates
