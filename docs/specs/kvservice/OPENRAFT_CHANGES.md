# KV Service Spec Updates for OpenRaft Migration

## Summary of Changes

This document summarizes the required changes to the KV Service spec to align with the OpenRaft migration.

## Key Changes

###  1. RaftNode Section Update (design.md:126-140)

**Add new section after line 140**:

### OpenRaft API Changes

**IMPORTANT**: After OpenRaft migration, RaftNode uses openraft's async API.

**API Changes from raft-rs**:
```rust
// OLD (raft-rs - synchronous):
impl RaftNode {
    fn propose(&mut self, data: Vec<u8>) -> Result<()>  // Sync
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>       // Sync
    fn is_leader(&self) -> bool                         // Sync
    fn leader_id(&self) -> Option<u64>                  // Sync
}

// NEW (openraft - asynchronous):
impl RaftNode {
    async fn propose(&self, data: Vec<u8>) -> Result<ClientWriteResponse>  // Async!
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>                           // Still sync (direct StateMachine access)
    async fn is_leader(&self) -> bool                                      // Async!
    async fn leader_id(&self) -> Option<u64>                               // Async!
}
```

**Critical Leadership Check Contract**:
- `RaftNode::get()` does NOT check leadership internally (direct HashMap access)
- KvService MUST call `is_leader().await` before calling `get()`
- Without this check, followers will serve stale reads
- This is intentional in Phase 1 for simplicity (Phase 4 adds ReadIndex for linearizable reads)

**Error Type Changes**:
- raft-rs errors → openraft errors
- Different error variants and semantics
- KvServiceError must map from openraft error types

### 2. Command Handlers - Add async/await

**All handler signatures** (design.md:149-285) need async:

```rust
// OLD:
async fn handle_get(&self, key: Vec<u8>) -> Result<RespValue>

// NEW (already async - no change):
async fn handle_get(&self, key: Vec<u8>) -> Result<RespValue>

// Implementation changes:
async fn handle_get(&self, key: Vec<u8>) -> Result<RespValue, KvServiceError> {
    // MUST check leadership first (get() doesn't check internally)
    if !self.raft_node.is_leader().await {  // <-- .await added
        let leader_id = self.raft_node.leader_id().await;  // <-- .await added
        return Err(KvServiceError::NotLeader { leader_id });
    }

    // get() is sync (direct StateMachine access)
    match self.raft_node.get(&key) {
        Some(value) => Ok(RespValue::BulkString(value.into())),
        None => Ok(RespValue::Null),
    }
}

async fn handle_set(&self, key: Vec<u8>, value: Vec<u8>) -> Result<RespValue, KvServiceError> {
    // Validation
    validate_key_size(&key)?;
    validate_value_size(&value)?;

    // Serialize operation
    let operation = Operation::Set { key, value };
    let data = bincode::serialize(&operation)?;

    // Propose via Raft (async with openraft)
    self.raft_node.propose(data).await?;  // <-- .await added

    Ok(RespValue::SimpleString("OK".into()))
}

async fn handle_del(&self, keys: Vec<Vec<u8>>) -> Result<RespValue, KvServiceError> {
    // Validation
    for key in &keys {
        validate_key_size(key)?;
    }

    let mut deleted_count = 0;

    // Each key is a separate Raft proposal
    for key in keys {
        let operation = Operation::Del { key };
        let data = bincode::serialize(&operation)?;

        // Propose (async)
        match self.raft_node.propose(data).await {  // <-- .await added
            Ok(response) => {
                // Parse response to get deletion count (1 or 0)
                deleted_count += parse_del_result(&response)?;
            }
            Err(e) => {
                // Partial failure - return what we deleted so far with error
                return Err(KvServiceError::PartialFailure {
                    completed: deleted_count,
                    error: Box::new(e),
                });
            }
        }
    }

    Ok(RespValue::Integer(deleted_count))
}

async fn handle_exists(&self, keys: Vec<Vec<u8>>) -> Result<RespValue, KvServiceError> {
    // MUST check leadership (get() doesn't check internally)
    if !self.raft_node.is_leader().await {  // <-- .await added
        let leader_id = self.raft_node.leader_id().await;  // <-- .await added
        return Err(KvServiceError::NotLeader { leader_id });
    }

    let mut exists_count = 0;
    for key in &keys {
        if self.raft_node.get(key).is_some() {
            exists_count += 1;
        }
    }

    Ok(RespValue::Integer(exists_count))
}

async fn handle_ping(&self, message: Option<Vec<u8>>) -> Result<RespValue, KvServiceError> {
    // No async needed - no Raft interaction
    match message {
        Some(msg) => Ok(RespValue::BulkString(msg.into())),
        None => Ok(RespValue::SimpleString("PONG".into())),
    }
}
```

### 3. Error Types Update (design.md:309-338)

**Add OpenRaft error variant**:

```rust
#[derive(Debug, Error)]
pub enum KvServiceError {
    /// Key exceeds 256 bytes
    #[error("key too large: {size} bytes (max 256)")]
    KeyTooLarge { size: usize },

    /// Value exceeds 64KB
    #[error("value too large: {size} bytes (max 64KB)")]
    ValueTooLarge { size: usize },

    /// Write on follower, redirect to leader
    #[error("not leader, redirect to node {leader_id:?}")]
    NotLeader { leader_id: Option<u64> },

    /// Cannot reach Raft majority
    #[error("no quorum available")]
    NoQuorum,

    /// Raft proposal rejected
    #[error("proposal failed: {0}")]
    ProposalFailed(String),

    /// Operation serialization failed
    #[error("serialization error: {0}")]
    SerializationError(#[from] bincode::Error),

    /// OpenRaft error (replaces RaftError)
    #[error("raft error: {0}")]
    OpenRaftError(Box<dyn std::error::Error + Send + Sync>),

    /// Partial multi-key operation failure (for DEL with multiple keys)
    #[error("partial failure: completed {completed} operations before error")]
    PartialFailure {
        completed: i64,
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

// Implement From for openraft errors
impl From<openraft::error::ClientWriteError<RaftTypeConfig>> for KvServiceError {
    fn from(err: openraft::error::ClientWriteError<RaftTypeConfig>) -> Self {
        match err {
            openraft::error::ClientWriteError::ForwardToLeader(fwd) => {
                KvServiceError::NotLeader {
                    leader_id: fwd.leader_id,
                }
            }
            openraft::error::ClientWriteError::ChangeMembershipError(_) => {
                KvServiceError::OpenRaftError(Box::new(err))
            }
            _ => KvServiceError::OpenRaftError(Box::new(err)),
        }
    }
}
```

### 4. Data Flow Updates

**Write Path** (design.md:768-792):

Update Step 9-18 to show async:

```
Step 9:  KvService calls raft_node.propose(data).await  // <-- async
Step 10: RaftNode checks is_leader().await              // <-- async (if needed)
...
```

**Read Path** (design.md:795-813):

Update Step 5-8:

```
Step 5:  KvService MUST check raft_node.is_leader().await  // <-- async
Step 6:  If not leader → return NotLeader error → client gets MOVED response
Step 7:  If leader → call raft_node.get(b"foo")            // <-- sync (direct HashMap)
Step 8:  RaftNode reads from StateMachine HashMap (no additional leadership check)
```

### 5. Concurrency Model Update (design.md:939-955)

**Add async runtime section**:

```rust
**Async Runtime**: Tokio 1.x for all I/O operations

**OpenRaft Async Integration**:
- All RaftNode methods that interact with openraft are async
- get() remains sync (direct StateMachine access with RwLock)
- KvService handlers await on propose(), is_leader(), leader_id()
- No blocking operations in async contexts

**State Machine Locking**: StateMachine uses RwLock for concurrent reads, exclusive writes on apply

**Propose Concurrency**: Multiple concurrent `propose().await` calls are safe - openraft serializes them into log order

**Read Concurrency**: Reads are concurrent-safe via RwLock read locks on StateMachine HashMap
```

### 6. Testing Strategy Updates (design.md:957-1007)

**Add async test requirements**:

```rust
// All tests use #[tokio::test]
#[tokio::test]
async fn test_handle_set_validates_key_size() {
    let kv_service = setup_test_service().await;
    let result = kv_service.handle_set(vec![0u8; 257], vec![1, 2, 3]).await;
    assert!(matches!(result, Err(KvServiceError::KeyTooLarge { .. })));
}

#[tokio::test]
async fn test_handle_get_on_follower_returns_not_leader() {
    let kv_service = setup_follower_service().await;
    let result = kv_service.handle_get(b"foo".to_vec()).await;
    assert!(matches!(result, Err(KvServiceError::NotLeader { .. })));
}
```

### 7. Dependencies Update (spec.md)

**Add to "Depends On" section**:
- openraft migration (BLOCKING) - must complete before KV service implementation
- tokio 1.x - async runtime (already present)
- async-trait - for trait implementations (if needed)

**Update "Used By" section**:
- seshat binary - integrates KvService with tokio TCP listener

### 8. Implementation Dependencies (design.md:1093-1113)

**Update to reflect OpenRaft**:

```markdown
### Requires (After OpenRaft Migration)

- ✓ `RaftNode::propose()` → **NOW ASYNC**: `async fn propose(&self, data: Vec<u8>) -> Result<ClientWriteResponse>`
- ✓ `RaftNode::get()` → **STILL SYNC**: `fn get(&self, key: &[u8]) -> Option<Vec<u8>>`
- ✓ `RaftNode::is_leader()` → **NOW ASYNC**: `async fn is_leader(&self) -> bool`
- ✓ `RaftNode::leader_id()` → **NOW ASYNC**: `async fn leader_id(&self) -> Option<u64>`
- ✓ `Operation::serialize()` - DONE (no change)
- ✓ `Operation::deserialize()` - DONE (no change)
- ✓ `RespCommand` and `RespValue` - DONE (no change)

### Blockers

**OpenRaft migration MUST be complete** before KV Service implementation begins. Specifically:
- OpenRaft Phase 1-6 complete
- RaftNode API migrated to openraft
- StateMachine integrated with openraft
- All raft crate tests passing with openraft
```

### 9. Phase 1 Limitations Update (design.md:1146-1167)

**Add async clarification**:

```markdown
**Read Consistency - Leadership Transition Race**:
- Reads may be stale during leadership transitions (window of ~100ms)
- While KvService checks `is_leader().await` before reads, leadership can change before `get()` completes
- `is_leader()` is async (checks openraft state)
- `get()` is sync (direct StateMachine HashMap access, no internal leadership check)
- **Race window**: Between `is_leader().await` returning true and `get()` executing
- Old leader may serve reads briefly after losing leadership
- New leader may not have all committed entries immediately after election
- Clients may observe "time travel": newer value → older value → newer value
- Phase 4 will add linearizable reads via ReadIndex to eliminate this race
```

### 10. Estimated Effort Update

**Old**: 10-12 hours

**New**: 11-13 hours
- +1 hour for async integration with openraft
- Updating all tests to use #[tokio::test]
- Error type mapping from openraft errors

## Implementation Notes

1. **KvService handlers already async**: The signatures are already `async fn`, so main change is adding `.await` to RaftNode calls

2. **Critical Leadership Check**: Document that `get()` does NOT check leadership - this is KvService's responsibility

3. **Error Mapping**: Must implement `From<openraft::error::ClientWriteError>` for KvServiceError

4. **Testing**: All tests need `#[tokio::test]` and `.await` on async calls

## Migration Timeline

1. **Week 1-2**: Complete OpenRaft migration (all 6 phases)
2. **Week 3**: Update KV Service spec with these changes
3. **Week 3-4**: Implement KV Service with async openraft integration
4. **Week 4**: End-to-end testing with redis-cli

## Status

- [x] Changes documented
- [ ] design.md updated
- [ ] spec.md updated
- [ ] Effort estimate revised
- [ ] OpenRaft migration complete (BLOCKER)
