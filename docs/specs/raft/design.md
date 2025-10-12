# Technical Design: Raft Consensus Implementation

## Architecture Overview

This design implements Raft consensus for Seshat using a layered architecture: **Protocol (gRPC) → Raft Layer → Storage Layer**. This is NOT a typical web service architecture (no Router → Service → Repository pattern), as this is an internal distributed consensus system.

**Key Principle**: This design wraps the raft-rs library with application-specific logic, implementing the Storage trait with in-memory structures for Phase 1. All nodes are identical - no special roles.

## Domain Model

### Core Entities

#### 1. RaftNode
**Purpose**: Application-specific wrapper around raft-rs RawNode

**Responsibilities**:
- Initialize raft-rs RawNode with configuration
- Drive consensus via tick() and process Ready
- Handle message routing between nodes
- Coordinate with StateMachine for applying committed entries
- Manage leader transfer on graceful shutdown

**Key Methods**:
```rust
pub struct RaftNode {
    raw_node: RawNode<MemStorage>,
    storage: Arc<MemStorage>,
    state_machine: Arc<Mutex<StateMachine>>,
    config: RaftConfig,
    node_id: NodeId,
}

impl RaftNode {
    // Create new Raft node with bootstrap configuration
    pub fn new(config: RaftConfig, node_id: NodeId, peers: Vec<NodeId>) -> Result<Self>;

    // Process timer tick (call every 100ms)
    pub fn tick(&mut self) -> Result<()>;

    // Handle incoming Raft message
    pub fn step(&mut self, msg: raft::Message) -> Result<()>;

    // Propose client command
    pub async fn propose(&mut self, data: Vec<u8>) -> Result<()>;

    // Process ready state (persist → send → apply → advance)
    pub async fn handle_ready(&mut self) -> Result<Vec<raft::Message>>;

    // Check if this node is leader
    pub fn is_leader(&self) -> bool;

    // Get current leader ID
    pub fn leader_id(&self) -> Option<NodeId>;

    // Transfer leadership on shutdown
    pub async fn transfer_leader(&mut self, target: NodeId) -> Result<()>;
}
```

**State Transitions**:
- `Follower → Candidate`: Election timeout expires
- `Candidate → Leader`: Receives majority votes
- `Candidate → Follower`: Discovers higher term or new leader
- `Leader → Follower`: Discovers higher term

---

#### 2. MemStorage
**Purpose**: In-memory implementation of raft-rs Storage trait

**Responsibilities**:
- Store Raft log entries in Vec
- Track hard state (term, vote, commit)
- Provide snapshot access
- Implement all 6 Storage trait methods

**Data Structures**:
```rust
pub struct MemStorage {
    // Core Raft state
    hard_state: RwLock<RaftHardState>,
    conf_state: RwLock<ConfState>,

    // Log storage (entries[0] is dummy entry)
    entries: RwLock<Vec<Entry>>,

    // Snapshot (simplified for Phase 1)
    snapshot: RwLock<Snapshot>,
}

impl MemStorage {
    pub fn new() -> Self;

    // Append entries to log
    pub fn append(&self, entries: &[Entry]) -> Result<()>;

    // Set hard state
    pub fn set_hard_state(&self, hs: RaftHardState) -> Result<()>;

    // Set conf state
    pub fn set_conf_state(&self, cs: ConfState) -> Result<()>;

    // Compact log (remove entries before index)
    pub fn compact(&self, compact_index: u64) -> Result<()>;

    // Create snapshot at index
    pub fn create_snapshot(&self, index: u64, data: Vec<u8>) -> Result<()>;
}
```

**Storage Trait Implementation**:
```rust
impl Storage for MemStorage {
    fn initial_state(&self) -> raft::Result<RaftState> {
        // Return (HardState, ConfState)
        // On first boot: HardState::default(), ConfState with voters
        // After restart: Load persisted state
    }

    fn entries(&self, low: u64, high: u64, max_size: Option<u64>)
        -> raft::Result<Vec<Entry>> {
        // Return entries in range [low, high)
        // Respect max_size if provided
        // Return EntryError::Compacted if low < first_index
        // Return EntryError::Unavailable if high > last_index + 1
    }

    fn term(&self, index: u64) -> raft::Result<u64> {
        // Return term for entry at index
        // Special case: index 0 returns 0
        // Check snapshot.metadata if index < first_index
    }

    fn first_index(&self) -> raft::Result<u64> {
        // Return snapshot.metadata.index + 1
        // Or 1 if no snapshot
    }

    fn last_index(&self) -> raft::Result<u64> {
        // Return index of last entry
        // Or snapshot.metadata.index if log is empty
    }

    fn snapshot(&self, request_index: u64) -> raft::Result<Snapshot> {
        // Return current snapshot
        // For Phase 1: simplified, just return stored snapshot
    }
}
```

**Invariants**:
- `entries[0]` is always a dummy entry (standard raft-rs convention)
- `first_index() == snapshot.metadata.index + 1`
- `last_index() == first_index() + entries.len() - 1` (excluding dummy)
- All entries in range `[first_index, last_index]` are available

---

#### 3. StateMachine
**Purpose**: Apply committed log entries to in-memory key-value store

**Responsibilities**:
- Apply SET/DEL operations to HashMap
- Maintain operation ordering
- Generate snapshot data (Phase 1: serialize HashMap)
- Restore from snapshot

**Implementation**:
```rust
pub struct StateMachine {
    // In-memory key-value store
    data: HashMap<Vec<u8>, Vec<u8>>,

    // Last applied index (for idempotency)
    last_applied: u64,
}

impl StateMachine {
    pub fn new() -> Self;

    // Apply committed entry
    pub fn apply(&mut self, entry: &Entry) -> Result<Vec<u8>>;

    // Get value (for read operations)
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>>;

    // Check if key exists
    pub fn exists(&self, key: &[u8]) -> bool;

    // Create snapshot of current state
    pub fn snapshot(&self) -> Result<Vec<u8>>;

    // Restore from snapshot
    pub fn restore(&mut self, data: &[u8]) -> Result<()>;

    // Get last applied index
    pub fn last_applied(&self) -> u64;
}
```

**Operation Types** (defined in protocol crate):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Set { key: Vec<u8>, value: Vec<u8> },
    Del { key: Vec<u8> },
    // Phase 2+: Exists, Scan, etc.
}

impl Operation {
    // Apply operation to state machine
    pub fn apply(&self, data: &mut HashMap<Vec<u8>, Vec<u8>>) -> Result<Vec<u8>>;
}
```

**Apply Logic Flow**:
1. Deserialize Operation from entry.data
2. Check `entry.index > last_applied` (idempotency)
3. Execute operation on HashMap
4. Update `last_applied = entry.index`
5. Return result bytes

**Snapshot Format** (Phase 1):
```rust
#[derive(Serialize, Deserialize)]
struct SnapshotData {
    version: u8,
    last_applied: u64,
    data: HashMap<Vec<u8>, Vec<u8>>,
}
```

---

### Configuration Entities

#### NodeConfig
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: u64,                    // Must be > 0
    pub client_addr: String,        // e.g., "0.0.0.0:6379"
    pub internal_addr: String,      // e.g., "0.0.0.0:7379"
    pub data_dir: PathBuf,
    pub advertise_addr: Option<String>,  // Auto-detect if None
}
```

#### ClusterConfig
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub bootstrap: bool,
    pub initial_members: Vec<InitialMember>,
    pub replication_factor: usize,  // Must be 3 for Phase 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialMember {
    pub id: u64,
    pub addr: String,  // e.g., "kvstore-1:7379"
}
```

#### RaftConfig
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    pub heartbeat_interval_ms: u64,     // 100
    pub election_timeout_min_ms: u64,   // 500
    pub election_timeout_max_ms: u64,   // 1000
    pub snapshot_interval_entries: u64, // 10_000
    pub snapshot_interval_bytes: u64,   // 100MB
    pub max_log_size_bytes: u64,        // 500MB
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 100,
            election_timeout_min_ms: 500,
            election_timeout_max_ms: 1000,
            snapshot_interval_entries: 10_000,
            snapshot_interval_bytes: 100 * 1024 * 1024,
            max_log_size_bytes: 500 * 1024 * 1024,
        }
    }
}
```

**Validation Rules**:
```rust
impl NodeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.id == 0 {
            return Err(Error::ConfigError("node_id must be > 0".into()));
        }
        // Validate addresses are parseable
        // Validate data_dir is writable
        Ok(())
    }
}

impl ClusterConfig {
    pub fn validate(&self, node_id: u64) -> Result<()> {
        // Check initial_members.len() >= 3
        // Check no duplicate IDs
        // Check this node_id is in initial_members
        // Check replication_factor == 3 (Phase 1)
        Ok(())
    }
}

impl RaftConfig {
    pub fn validate(&self) -> Result<()> {
        // Check election_timeout_min >= heartbeat_interval * 2
        // Check election_timeout_max > election_timeout_min
        Ok(())
    }
}
```

---

## Data Persistence

**Type**: In-memory only (Phase 1)

**Data Structures**:

### In-Memory Storage Layout

```rust
// MemStorage internal structure
pub struct MemStorage {
    // Hard state: term, vote, commit
    hard_state: RwLock<RaftHardState> {
        version: 1,
        term: u64,
        vote: Option<u64>,
        commit: u64,
    },

    // Configuration state: cluster members
    conf_state: RwLock<ConfState> {
        voters: Vec<u64>,      // [1, 2, 3]
        learners: Vec<u64>,    // []
        voters_outgoing: Vec<u64>,
        learners_next: Vec<u64>,
        auto_leave: bool,
    },

    // Log entries: Vec with dummy entry at [0]
    entries: RwLock<Vec<Entry>> {
        // entries[0] = dummy Entry { term: 0, index: 0, ... }
        // entries[1] = Entry { term: 1, index: 1, data: ... }
        // entries[n] = Entry { term: t, index: n, data: ... }
    },

    // Snapshot (simplified for Phase 1)
    snapshot: RwLock<Snapshot> {
        metadata: SnapshotMetadata {
            conf_state: ConfState,
            index: u64,  // last_included_index
            term: u64,   // last_included_term
        },
        data: Vec<u8>,  // Serialized StateMachine
    },
}

// StateMachine internal structure
pub struct StateMachine {
    data: HashMap<Vec<u8>, Vec<u8>>,  // Key-value store
    last_applied: u64,                 // Last applied log index
}
```

**Memory Estimates** (3-node cluster):
- Hard state: ~100 bytes per node
- Conf state: ~100 bytes per node
- Log entries: ~1KB per entry × 10,000 = ~10MB per node (before compaction)
- State machine: User data size (unbounded, but limited by resource constraints)
- Total baseline: ~10-20MB per node + user data

**Compaction Strategy** (Phase 1 - simplified):
- Trigger: Every 10,000 entries
- Process:
  1. Create snapshot of StateMachine
  2. Store snapshot in MemStorage
  3. Remove log entries before snapshot index
  4. Keep entries after snapshot index
- No persistent checkpoint in Phase 1 (just in-memory)

---

## Protocols

### Internal RPC (gRPC)

**Framework**: tonic 0.11+ (gRPC) + prost 0.12+ (Protobuf)

**Port**: 7379

**Protobuf Definition** (protocol/proto/raft.proto):

```protobuf
syntax = "proto3";

package raft;

// Raft RPC service
service RaftService {
    // Request vote from candidate
    rpc RequestVote(RequestVoteRequest) returns (RequestVoteResponse);

    // Append entries from leader
    rpc AppendEntries(AppendEntriesRequest) returns (AppendEntriesResponse);

    // Install snapshot from leader
    rpc InstallSnapshot(stream InstallSnapshotRequest) returns (InstallSnapshotResponse);
}

// RequestVote RPC
message RequestVoteRequest {
    uint64 term = 1;              // Candidate's term
    uint64 candidate_id = 2;       // Candidate requesting vote
    uint64 last_log_index = 3;     // Index of candidate's last log entry
    uint64 last_log_term = 4;      // Term of candidate's last log entry
}

message RequestVoteResponse {
    uint64 term = 1;              // Current term for candidate to update itself
    bool vote_granted = 2;         // True means candidate received vote
}

// AppendEntries RPC
message AppendEntriesRequest {
    uint64 term = 1;              // Leader's term
    uint64 leader_id = 2;          // Leader's node ID
    uint64 prev_log_index = 3;     // Index of log entry immediately preceding new ones
    uint64 prev_log_term = 4;      // Term of prev_log_index entry
    repeated LogEntry entries = 5; // Log entries to store (empty for heartbeat)
    uint64 leader_commit = 6;      // Leader's commit index
}

message AppendEntriesResponse {
    uint64 term = 1;              // Current term for leader to update itself
    bool success = 2;              // True if follower contained entry matching prev_log_index and prev_log_term
    uint64 reject_hint = 3;        // Hint for leader to retry with lower index
}

// InstallSnapshot RPC
message InstallSnapshotRequest {
    uint64 term = 1;              // Leader's term
    uint64 leader_id = 2;          // Leader's node ID
    uint64 last_included_index = 3; // Snapshot replaces all entries up through and including this index
    uint64 last_included_term = 4;  // Term of last_included_index
    bytes data = 5;                // Snapshot data chunk
    bool done = 6;                 // True if this is the last chunk
}

message InstallSnapshotResponse {
    uint64 term = 1;              // Current term for leader to update itself
}

// Log entry
message LogEntry {
    uint64 term = 1;              // Term when entry was received by leader
    uint64 index = 2;              // Position in the log
    bytes data = 3;                // Serialized Operation (Set/Del)
    EntryType entry_type = 4;      // Entry type
}

enum EntryType {
    ENTRY_TYPE_NORMAL = 0;         // Normal command
    ENTRY_TYPE_CONF_CHANGE = 1;    // Configuration change
    ENTRY_TYPE_NOOP = 2;           // No-op (leader establishes authority)
}
```

**Message Mapping**:
- Protobuf `LogEntry` ↔ raft-rs `Entry`
- Protobuf `RequestVoteRequest` ↔ raft-rs `Message::MsgRequestVote`
- Protobuf `AppendEntriesRequest` ↔ raft-rs `Message::MsgAppend`
- Protobuf `InstallSnapshotRequest` ↔ raft-rs `Message::MsgSnapshot`

**Note**: gRPC client/server implementation is NOT included in this spec. This design only defines the Protobuf schemas. Implementation will be in a separate `grpc-transport` spec.

---

## Module Structure

### Crate: `raft`

**Dependencies**:
```toml
[dependencies]
# Seshat crates
common = { path = "../common" }

# External
raft = "0.7"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
```

**Module Layout**:
```
raft/
├── src/
│   ├── lib.rs              // Public API, re-exports
│   ├── node.rs             // RaftNode implementation
│   ├── storage.rs          // MemStorage + Storage trait
│   ├── state_machine.rs    // StateMachine
│   └── config.rs           // Configuration types (moved from common)
├── tests/
│   ├── storage_tests.rs    // Storage trait tests
│   ├── state_machine_tests.rs  // State machine tests
│   └── config_tests.rs     // Config validation tests
└── Cargo.toml
```

**Public API** (src/lib.rs):
```rust
// Re-export main types
pub use node::RaftNode;
pub use storage::MemStorage;
pub use state_machine::StateMachine;
pub use config::{NodeConfig, ClusterConfig, RaftConfig};

// Re-export common types
pub use common::{NodeId, Term, LogIndex, Error, Result};
```

---

### Crate: `protocol`

**Dependencies**:
```toml
[dependencies]
common = { path = "../common" }

tonic = "0.11"
prost = "0.12"
serde = { version = "1.0", features = ["derive"] }

[build-dependencies]
tonic-build = "0.11"
```

**Module Layout**:
```
protocol/
├── proto/
│   └── raft.proto          // Raft RPC definitions
├── src/
│   ├── lib.rs              // Re-exports generated code
│   └── operations.rs       // Operation enum (Set/Del)
├── build.rs                // Protobuf code generation
└── Cargo.toml
```

**build.rs**:
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["proto/raft.proto"], &["proto"])?;
    Ok(())
}
```

**operations.rs**:
```rust
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Set { key: Vec<u8>, value: Vec<u8> },
    Del { key: Vec<u8> },
}

impl Operation {
    pub fn apply(&self, data: &mut HashMap<Vec<u8>, Vec<u8>>) -> crate::Result<Vec<u8>> {
        match self {
            Operation::Set { key, value } => {
                data.insert(key.clone(), value.clone());
                Ok(b"OK".to_vec())
            }
            Operation::Del { key } => {
                let existed = data.remove(key).is_some();
                Ok(if existed { b"1" } else { b"0" }.to_vec())
            }
        }
    }

    pub fn serialize(&self) -> crate::Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| Error::Serialization(e.to_string()))
    }

    pub fn deserialize(data: &[u8]) -> crate::Result<Self> {
        bincode::deserialize(data).map_err(|e| Error::Serialization(e.to_string()))
    }
}
```

---

### Crate: `common`

**Updates Required**:
```rust
// src/types.rs (existing)
pub type NodeId = u64;
pub type Term = u64;
pub type LogIndex = u64;

// src/errors.rs (add new variants)
#[derive(Debug, Error)]
pub enum Error {
    #[error("Not leader: current leader is node {leader:?}")]
    NotLeader { leader: Option<NodeId> },

    #[error("No quorum available")]
    NoQuorum,

    #[error("Raft error: {0}")]
    Raft(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// Convert raft-rs errors to our Error type
impl From<raft::Error> for Error {
    fn from(e: raft::Error) -> Self {
        Error::Raft(e.to_string())
    }
}
```

---

## Component Architecture

### Layer Responsibilities

```
┌─────────────────────────────────────────────────────────┐
│  Protocol Layer (gRPC)                                   │
│  - Parse Protobuf messages                               │
│  - Serialize responses                                   │
│  - Handle transport (NOT in this spec)                   │
└─────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────┐
│  Raft Layer                                              │
│  - RaftNode: Drive consensus                             │
│  - MemStorage: Store log/state                           │
│  - StateMachine: Apply committed entries                 │
└─────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────┐
│  Storage Layer (In-Memory)                               │
│  - Vec<Entry> for log                                    │
│  - HashMap<Vec<u8>, Vec<u8>> for KV data                │
└─────────────────────────────────────────────────────────┘
```

### Data Flow

#### Bootstrap Flow
```
1. Load NodeConfig, ClusterConfig, RaftConfig from TOML
2. Validate all configurations
3. Create MemStorage
4. Initialize with voters = [1, 2, 3]
5. Create StateMachine
6. Create RaftNode with RawNode
7. If bootstrap mode: immediately become candidate (all nodes do this)
8. raft-rs handles leader election
```

#### Write Flow (SET key value)
```
1. Client sends command to node
2. Node checks if leader:
   - If follower: return NotLeader error with leader_id
   - If leader: proceed
3. Serialize Operation::Set { key, value }
4. RaftNode.propose(data)
5. raft-rs appends to log, replicates to followers
6. Once majority commits:
   - RaftNode.handle_ready() returns Ready with committed_entries
   - For each entry: StateMachine.apply(entry)
   - StateMachine deserializes Operation and updates HashMap
   - Return success to client
```

#### Read Flow (GET key)
```
1. Client sends command to node
2. Node checks if leader:
   - If follower: return NotLeader error with leader_id
   - If leader: proceed (Phase 1: no follower reads)
3. StateMachine.get(key)
4. Return value to client
```

#### Heartbeat Flow
```
1. Leader's RaftNode.tick() called every 100ms
2. raft-rs generates MsgHeartbeat for each follower
3. RaftNode.handle_ready() returns messages to send
4. Send AppendEntries RPC to followers (empty entries)
5. Followers respond with success
6. Leader updates follower progress
```

#### Leader Election Flow
```
1. Follower's election timeout expires (500-1000ms)
2. Follower becomes candidate, increments term
3. MemStorage.set_hard_state(new term, vote for self)
4. Send RequestVote RPCs to all peers
5. Collect votes:
   - If majority: become leader
   - If discover higher term: revert to follower
   - If timeout: start new election
6. New leader sends empty AppendEntries (establishes authority)
```

#### Snapshot Creation Flow (Phase 1 - simplified)
```
1. After 10,000 log entries applied
2. RaftNode checks log size
3. StateMachine.snapshot() → serialize HashMap
4. MemStorage.create_snapshot(index, data)
5. MemStorage.compact(index) → remove old log entries
6. raft-rs updates snapshot metadata
```

---

## Testing Strategy

### Unit Tests (Per Module)

#### 1. Storage Tests (storage_tests.rs)
```rust
#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        // Create MemStorage
        // Verify initial_state returns default HardState and ConfState
    }

    #[test]
    fn test_append_entries() {
        // Append entries to empty log
        // Verify first_index, last_index, entries() work
    }

    #[test]
    fn test_entries_range() {
        // Append 10 entries
        // Query entries(3, 7) → should return [3, 4, 5, 6]
    }

    #[test]
    fn test_entries_max_size() {
        // Append large entries
        // Query with max_size → should respect limit
    }

    #[test]
    fn test_term_lookup() {
        // Append entries with different terms
        // Verify term(index) returns correct term
    }

    #[test]
    fn test_compaction() {
        // Append 100 entries
        // Compact at index 50
        // Verify first_index == 51
        // Verify entries(1, 50) returns Compacted error
    }

    #[test]
    fn test_snapshot_creation() {
        // Create snapshot with data
        // Verify snapshot() returns correct metadata and data
    }

    #[test]
    fn test_hard_state_persistence() {
        // Set hard_state
        // Read back via initial_state
        // Verify term, vote, commit are correct
    }
}
```

#### 2. State Machine Tests (state_machine_tests.rs)
```rust
#[cfg(test)]
mod state_machine_tests {
    use super::*;

    #[test]
    fn test_apply_set() {
        // Create state machine
        // Apply SET foo bar
        // Verify get(foo) == bar
    }

    #[test]
    fn test_apply_del() {
        // SET foo bar
        // DEL foo
        // Verify get(foo) == None
    }

    #[test]
    fn test_operation_ordering() {
        // Apply: SET foo 1, SET foo 2, SET foo 3
        // Verify get(foo) == 3 (last wins)
    }

    #[test]
    fn test_idempotency() {
        // Apply entry at index 5
        // Try to apply index 5 again
        // Should be no-op (last_applied tracking)
    }

    #[test]
    fn test_snapshot_create() {
        // SET 100 keys
        // Create snapshot
        // Verify data is serialized
    }

    #[test]
    fn test_snapshot_restore() {
        // Create state machine with data
        // Create snapshot
        // Restore to new state machine
        // Verify data matches
    }

    #[test]
    fn test_exists() {
        // SET foo bar
        // Verify exists(foo) == true
        // Verify exists(nonexistent) == false
    }
}
```

#### 3. Config Tests (config_tests.rs)
```rust
#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_node_config_validation() {
        // Valid config → Ok
        // node_id = 0 → Error
        // Invalid address → Error
    }

    #[test]
    fn test_cluster_config_validation() {
        // Valid config → Ok
        // Less than 3 members → Error
        // Duplicate IDs → Error
        // This node not in initial_members → Error
    }

    #[test]
    fn test_raft_config_validation() {
        // Valid config → Ok
        // election_timeout < heartbeat → Error
        // election_timeout_max < election_timeout_min → Error
    }

    #[test]
    fn test_config_defaults() {
        // Verify RaftConfig::default() has correct values
    }
}
```

#### 4. Protobuf Tests (protocol crate)
```rust
#[cfg(test)]
mod protobuf_tests {
    use super::*;

    #[test]
    fn test_request_vote_roundtrip() {
        // Create RequestVoteRequest
        // Serialize to bytes
        // Deserialize back
        // Verify all fields match
    }

    #[test]
    fn test_append_entries_roundtrip() {
        // Test with empty entries (heartbeat)
        // Test with multiple entries
    }

    #[test]
    fn test_operation_serialization() {
        // Serialize Operation::Set
        // Deserialize back
        // Verify fields match
    }
}
```

#### 5. RaftNode Tests (node_tests.rs)
```rust
#[cfg(test)]
mod node_tests {
    use super::*;

    #[test]
    fn test_node_initialization() {
        // Create RaftNode with valid config
        // Verify node_id, config, storage, state_machine are set
    }

    #[test]
    fn test_is_leader() {
        // Create node
        // Initially should be false
        // (Testing actual election requires multi-node, deferred to chaos tests)
    }

    #[test]
    fn test_propose_as_follower() {
        // Create node (will be follower)
        // Try to propose
        // Should return NotLeader error
    }
}
```

### Property-Based Tests (with proptest)
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_storage_entries_never_panics(
        low in 0u64..1000,
        high in 0u64..1000,
    ) {
        let storage = MemStorage::new();
        // Should never panic, always return Result
        let _ = storage.entries(low, high, None);
    }

    #[test]
    fn test_state_machine_arbitrary_keys(
        key in prop::collection::vec(any::<u8>(), 0..256),
        value in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let mut sm = StateMachine::new();
        let op = Operation::Set { key: key.clone(), value: value.clone() };
        // Should handle arbitrary byte sequences
        let _ = sm.apply_operation(&op);
    }
}
```

### Test Utilities
```rust
// tests/common/mod.rs
pub fn make_test_config() -> (NodeConfig, ClusterConfig, RaftConfig) {
    let node = NodeConfig {
        id: 1,
        client_addr: "127.0.0.1:6379".into(),
        internal_addr: "127.0.0.1:7379".into(),
        data_dir: PathBuf::from("/tmp/seshat-test"),
        advertise_addr: None,
    };

    let cluster = ClusterConfig {
        bootstrap: true,
        initial_members: vec![
            InitialMember { id: 1, addr: "127.0.0.1:7379".into() },
            InitialMember { id: 2, addr: "127.0.0.1:7380".into() },
            InitialMember { id: 3, addr: "127.0.0.1:7381".into() },
        ],
        replication_factor: 3,
    };

    let raft = RaftConfig::default();

    (node, cluster, raft)
}

pub fn make_test_entry(term: u64, index: u64, data: Vec<u8>) -> Entry {
    let mut entry = Entry::default();
    entry.term = term;
    entry.index = index;
    entry.data = data.into();
    entry.entry_type = EntryType::EntryNormal;
    entry
}
```

---

## File Structure

```
seshat/
├── crates/
│   ├── raft/
│   │   ├── src/
│   │   │   ├── lib.rs            // Public API
│   │   │   ├── node.rs           // RaftNode implementation
│   │   │   ├── storage.rs        // MemStorage + Storage trait
│   │   │   ├── state_machine.rs  // StateMachine
│   │   │   └── config.rs         // Config types and validation
│   │   ├── tests/
│   │   │   ├── storage_tests.rs
│   │   │   ├── state_machine_tests.rs
│   │   │   ├── config_tests.rs
│   │   │   └── node_tests.rs
│   │   └── Cargo.toml
│   │
│   ├── protocol/
│   │   ├── proto/
│   │   │   └── raft.proto        // Protobuf definitions
│   │   ├── src/
│   │   │   ├── lib.rs            // Re-export generated code
│   │   │   └── operations.rs     // Operation enum
│   │   ├── build.rs              // Protobuf codegen
│   │   ├── tests/
│   │   │   └── protobuf_tests.rs
│   │   └── Cargo.toml
│   │
│   └── common/
│       ├── src/
│       │   ├── lib.rs
│       │   ├── types.rs          // Type aliases
│       │   └── errors.rs         // Error enum
│       └── Cargo.toml
│
└── docs/
    └── specs/
        └── raft/
            ├── spec.md           // Feature specification
            └── design.md         // This file
```

---

## Dependencies

### Crate Dependency Graph
```
raft crate depends on:
  - common (NodeId, Error types)
  - raft-rs 0.7+ (RawNode, Storage trait)
  - tokio 1.x (async runtime)
  - tracing (logging)
  - serde, bincode (serialization)
  - thiserror (error handling)

protocol crate depends on:
  - common (Error types)
  - tonic 0.11+ (gRPC framework)
  - prost 0.12+ (Protobuf serialization)
  - tonic-build (build-time codegen)

common crate depends on:
  - serde (serialization)
  - thiserror (Error derive)
```

### External Libraries
```toml
# raft-rs: Core consensus algorithm
raft = "0.7"

# Async runtime
tokio = { version = "1", features = ["full"] }

# gRPC
tonic = "0.11"
prost = "0.12"
tonic-build = "0.11"  # build dependency

# Serialization
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Error handling
thiserror = "1.0"

# Testing
proptest = "1.0"  # dev dependency
tokio-test = "0.4"  # dev dependency
```

---

## Implementation Notes

### raft-rs Integration Points

**RawNode vs Raft**:
- Use `RawNode` for message-level control (recommended)
- `RawNode` gives us Ready struct to process
- We control message sending/receiving

**Ready Processing Pattern**:
```rust
pub async fn handle_ready(&mut self) -> Result<Vec<Message>> {
    if !self.raw_node.has_ready() {
        return Ok(vec![]);
    }

    let mut ready = self.raw_node.ready();

    // 1. Persist hard state and entries BEFORE sending messages
    if let Some(hs) = ready.hs() {
        self.storage.set_hard_state(hs.clone())?;
    }
    if !ready.entries().is_empty() {
        self.storage.append(ready.entries())?;
    }

    // 2. Send messages to other nodes
    let messages = ready.take_messages();

    // 3. Apply committed entries to state machine
    if let Some(committed) = ready.committed_entries.take() {
        for entry in committed {
            if entry.data.is_empty() {
                // Empty entry (from new leader)
                continue;
            }
            let result = self.state_machine.lock().await.apply(&entry)?;
            // Store result for client response (Phase 2)
        }
    }

    // 4. Advance raft state
    let mut light_rd = self.raw_node.advance(ready);

    // Handle light ready if any
    if let Some(commit) = light_rd.commit_index() {
        self.storage.set_hard_state_commit(commit)?;
    }

    self.raw_node.advance_apply();

    Ok(messages)
}
```

**Tick Pattern**:
```rust
// Call this every 100ms from tokio timer
pub fn tick(&mut self) -> Result<()> {
    self.raw_node.tick();
    Ok(())
}
```

**Bootstrap Pattern**:
```rust
pub fn new_bootstrap(
    config: RaftConfig,
    node_id: NodeId,
    peers: Vec<NodeId>,
) -> Result<Self> {
    let storage = MemStorage::new();

    // Create initial ConfState with all voters
    let conf_state = ConfState {
        voters: peers.clone(),
        ..Default::default()
    };
    storage.set_conf_state(conf_state)?;

    // Create raft Config
    let raft_config = raft::Config {
        id: node_id,
        election_tick: 10,  // 10 ticks * 100ms = 1s election timeout
        heartbeat_tick: 1,   // 1 tick * 100ms = 100ms heartbeat
        max_size_per_msg: 1024 * 1024,
        max_inflight_msgs: 256,
        ..Default::default()
    };

    // Create RawNode
    let raw_node = RawNode::new(&raft_config, storage.clone())?;

    let state_machine = Arc::new(Mutex::new(StateMachine::new()));

    Ok(Self {
        raw_node,
        storage: Arc::new(storage),
        state_machine,
        config,
        node_id,
    })
}
```

### Logging Strategy

**Use tracing for structured logging**:
```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(self), fields(node_id = %self.node_id))]
pub async fn handle_ready(&mut self) -> Result<Vec<Message>> {
    debug!("Processing ready");

    // ...

    if !ready.entries().is_empty() {
        info!(
            count = ready.entries().len(),
            first_index = ready.entries()[0].index,
            last_index = ready.entries().last().unwrap().index,
            "Appending entries to log"
        );
    }

    if let Some(committed) = ready.committed_entries.take() {
        info!(
            count = committed.len(),
            "Applying committed entries"
        );
        for entry in committed {
            debug!(
                index = entry.index,
                term = entry.term,
                "Applying entry"
            );
            // ...
        }
    }
}
```

### Error Handling Patterns

**Convert raft-rs errors**:
```rust
impl From<raft::Error> for Error {
    fn from(e: raft::Error) -> Self {
        match e {
            raft::Error::StepPeerNotFound => Error::NotLeader { leader: None },
            _ => Error::Raft(e.to_string()),
        }
    }
}
```

**Propagate with context**:
```rust
pub fn append(&self, entries: &[Entry]) -> Result<()> {
    self.entries.write()
        .unwrap()
        .extend_from_slice(entries);
    Ok(())
}
```

### Configuration Loading

**Load from TOML**:
```rust
// In seshat binary (not this crate)
let config_str = std::fs::read_to_string("config.toml")?;
let config: Config = toml::from_str(&config_str)?;

// Validate
config.node.validate()?;
config.cluster.validate(config.node.id)?;
config.raft.validate()?;

// Create RaftNode
let raft_node = RaftNode::new_bootstrap(
    config.raft,
    config.node.id,
    config.cluster.initial_members.iter().map(|m| m.id).collect(),
)?;
```

---

## Success Criteria

This design is considered complete when:

1. **MemStorage Implementation**: All 6 Storage trait methods implemented and tested
2. **Storage Tests Pass**: initial_state, entries, term, first_index, last_index, snapshot all work correctly
3. **State Machine Works**: SET and DEL operations apply correctly to HashMap
4. **State Machine Tests Pass**: apply, get, exists, snapshot, restore all work correctly
5. **Protobuf Compiles**: .proto file generates valid Rust code via tonic-build
6. **Protobuf Tests Pass**: RequestVote, AppendEntries, InstallSnapshot roundtrip serialization works
7. **RaftNode Initializes**: Can create RaftNode with valid config and bootstrap cluster
8. **Config Validation**: All validation rules implemented and tested
9. **Config Tests Pass**: Valid configs accepted, invalid configs rejected with clear errors
10. **No Unwraps**: All error paths return Result, no panics in normal operation
11. **100% Test Coverage**: All public APIs have unit tests
12. **Documentation**: All public types and methods have doc comments

**NOT required for this design**:
- gRPC client/server implementation (separate spec)
- Multi-node cluster tests (separate chaos testing spec)
- Leader election tests (requires multi-node setup)
- Network partition tests (chaos testing)

---

## References

- **raft-rs Documentation**: https://github.com/tikv/raft-rs
- **Raft Paper**: https://raft.github.io/raft.pdf
- **tonic Documentation**: https://github.com/hyperium/tonic
- **Product Spec**: `/Users/martinrichards/code/seshat/docs/product/product.md`
- **Architecture**: `/Users/martinrichards/code/seshat/docs/architecture/crates.md`
- **Data Structures**: `/Users/martinrichards/code/seshat/docs/architecture/data-structures.md`
- **Tech Standards**: `/Users/martinrichards/code/seshat/docs/standards/tech.md`
- **Development Practices**: `/Users/martinrichards/code/seshat/docs/standards/practices.md`
- **Feature Spec**: `/Users/martinrichards/code/seshat/docs/specs/raft/spec.md`
