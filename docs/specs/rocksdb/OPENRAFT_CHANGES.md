# RocksDB Spec Updates for OpenRaft Migration

## Summary of Changes

This document summarizes the required changes to the RocksDB storage spec to align with the OpenRaft migration.

## Key Changes

### 1. Architecture Diagram Update

**Old** (design.md:11-14):
```
│   Raft Crate (RaftStorage)         │
│   - Implements raft::Storage trait  │
```

**New**:
```
│   Raft Crate (OpenRaftMemStorage)      │
│   - Implements openraft storage traits │
│   - RaftLogReader, RaftSnapshotBuilder │
│   - RaftStorage (openraft version)    │
```

### 2. Raft Integration Flow Section (design.md:557-559)

**Replace entire section** with:

### OpenRaft Integration Flow

**IMPORTANT**: After OpenRaft migration, this integration uses openraft storage traits, not raft-rs.

```
Read Path (via RaftLogReader trait):
1. openraft calls: RaftLogReader::try_get_log_entries(range)
2. OpenRaftMemStorage (in raft crate) selects CF based on shard_id
3. OpenRaftMemStorage calls: storage.get_log_range(cf, range.start, range.end)
4. Storage reads from RocksDB: db.get_cf(cf, b"log:{:020}")
5. Storage returns: Vec<Vec<u8>> (serialized log entries)
6. OpenRaftMemStorage deserializes: Vec<u8> -> LogEntry<Request>
7. OpenRaftMemStorage returns: Vec<LogEntry<Request>> to openraft

Write Path (via RaftStorage trait):
1. openraft calls: RaftStorage::append(entries)
2. OpenRaftMemStorage serializes: LogEntry<Request> -> Vec<u8> (bincode)
3. OpenRaftMemStorage selects CF based on shard_id
4. OpenRaftMemStorage calls: storage.append_log_entry(cf, index, &bytes)
5. Storage validates: No gaps in log indices (fails with InvalidLogIndex if gap detected)
6. Storage calls: db.put_cf(data_raft_log, b"log:{:020}", bytes)
7. Storage returns: Ok(())
8. OpenRaftMemStorage returns: Success to openraft
```

**Separation of Concerns**:
- **OpenRaftMemStorage (raft crate)**: Implements openraft storage traits, handles serialization, understands Raft semantics
- **Storage (storage crate)**: Pure persistence layer, stores bytes as directed, no Raft knowledge

**Key Differences from raft-rs**:
- OpenRaft uses three separate traits (RaftLogReader, RaftSnapshotBuilder, RaftStorage) instead of single Storage trait
- All trait methods are async (requires tokio runtime)
- Different entry types: `LogEntry<Request>` instead of `eraftpb::Entry`
- Storage layer remains synchronous (RocksDB operations), async wrapper in raft crate

### 3. Raft Crate Integration Section (design.md:68-108)

**Replace entire "Raft Crate Integration"** section with:

### Raft Crate Integration

**IMPORTANT**: This section describes the integration AFTER OpenRaft migration is complete.

**OpenRaft Storage Implementation**:
```rust
// In raft crate (crates/raft/src/storage.rs or crates/storage/src/openraft_storage.rs)
use openraft::storage::{RaftLogReader, RaftSnapshotBuilder, RaftStorage as OpenRaftStorageTrait};
use seshat_storage::Storage; // RocksDB storage layer

pub struct OpenRaftMemStorage {
    storage: Arc<Storage>,
    shard_id: u64,  // System (0) or data shard ID
}

// Read operations
#[async_trait]
impl RaftLogReader<RaftTypeConfig> for OpenRaftMemStorage {
    async fn try_get_log_entries(
        &mut self,
        range: std::ops::Range<u64>
    ) -> Result<Vec<LogEntry<Request>>> {
        let cf = self.select_log_cf();
        let bytes_vec = self.storage.get_log_range(cf, range.start, range.end)?;

        // Deserialize each entry
        bytes_vec.into_iter()
            .map(|bytes| bincode::deserialize(&bytes))
            .collect()
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>> {
        let cf = self.select_state_cf();
        match self.storage.get(cf, b"vote")? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    // ... other RaftLogReader methods
}

// Write operations
#[async_trait]
impl OpenRaftStorageTrait<RaftTypeConfig> for OpenRaftMemStorage {
    async fn append<I>(
        &mut self,
        entries: I
    ) -> Result<()>
    where
        I: IntoIterator<Item = LogEntry<Request>> + Send,
    {
        let cf = self.select_log_cf();

        for entry in entries {
            let bytes = bincode::serialize(&entry)?;
            self.storage.append_log_entry(cf, entry.log_id.index, &bytes)?;
        }
        Ok(())
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<()> {
        let cf = self.select_state_cf();
        let bytes = bincode::serialize(vote)?;
        self.storage.put(cf, b"vote", &bytes)?;
        Ok(())
    }

    // ... other RaftStorage methods
}

impl OpenRaftMemStorage {
    fn select_log_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftLog
        } else {
            ColumnFamily::DataRaftLog
        }
    }

    fn select_state_cf(&self) -> ColumnFamily {
        if self.shard_id == 0 {
            ColumnFamily::SystemRaftState
        } else {
            ColumnFamily::DataRaftState
        }
    }
}
```

**Responsibilities**:
- **OpenRaftMemStorage**: Implements openraft storage traits, handles serialization, CF selection
- **Storage**: Pure persistence, thread-safety, atomicity, RocksDB operations

### 4. Dependencies Update (spec.md)

**Add to "Depends On" section**:
- openraft migration (BLOCKING) - must complete OpenRaft Phase 1-2 before RocksDB storage implementation
- async-trait crate - for async trait implementations

**Update "Used By" section**:
- raft crate - provides OpenRaftMemStorage wrapper that implements openraft storage traits

### 5. Estimated Effort Update

**Old**: 12-15 hours

**New**: 14-17 hours
- +2 hours for openraft trait integration complexity
- Async trait implementation overhead
- Testing with openraft types

## Implementation Notes

1. **Storage Crate Remains Synchronous**: The RocksDB Storage struct keeps its synchronous API. The async wrapper lives in the raft crate's OpenRaftMemStorage.

2. **No StateMachine in Storage**: Remove all references to StateMachine from storage crate. StateMachine lives in raft crate only.

3. **Serialization Boundary**: Storage stores raw bytes. OpenRaftMemStorage handles all serialization/deserialization.

4. **Testing Strategy**: After OpenRaft migration, update storage tests to use openraft types instead of raft-rs types.

## Migration Timeline

1. **Week 1-2**: Complete OpenRaft migration (Phase 1-2 minimum)
2. **Week 3**: Update RocksDB spec with these changes
3. **Week 3-4**: Implement RocksDB storage with openraft integration
4. **Week 4**: Test with openraft in 3-node cluster

## Status

- [x] Changes documented
- [ ] design.md updated
- [ ] spec.md updated
- [ ] Effort estimate revised
- [ ] OpenRaft migration Phase 1-2 complete (BLOCKER)
