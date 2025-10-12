# Core Data Structures

This document defines the key data structures used throughout Seshat, with their Rust representations and serialization formats.

## Schema Versioning

**Critical**: All persisted data includes version markers for future evolution.

```rust
// All serialized structures include a version field
const CURRENT_VERSION: u8 = 1;

// On deserialization:
// - If version > CURRENT_VERSION: refuse to start (upgrade required)
// - If version < CURRENT_VERSION: run migration
// - If version == CURRENT_VERSION: deserialize normally
```

---

## Cluster Metadata Structures

### ClusterMembership

**Purpose**: Track all nodes in the cluster with their addresses and states.

**Storage**: `system_data` column family, replicated via system Raft group.

```rust
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMembership {
    /// Schema version for future migrations
    pub version: u8,

    /// Map of node_id → node information
    pub members: HashMap<u64, NodeInfo>,

    /// Incremented on each membership change
    pub membership_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: u64,

    /// Network address for internal RPC (e.g., "kvstore-1:7379")
    /// Can be DNS name or IP:port
    pub addr: String,

    /// Current state of this node
    pub state: NodeState,

    /// Timestamp when node joined (milliseconds since epoch)
    pub joined_at: u64,

    /// Last heartbeat received (for health monitoring)
    pub last_heartbeat: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// Node is active and participating in Raft groups
    Active,

    /// Node is in the process of joining
    Joining,

    /// Node is being gracefully removed (shards migrating away)
    Leaving,

    /// Node is suspected unhealthy (consecutive failures)
    Suspected,

    /// Node has been removed from cluster
    Removed,
}
```

**Operations**:
- **Add Node**: Propose membership change via system Raft group
- **Remove Node**: Mark as `Leaving`, migrate shards, then mark `Removed`
- **Update Health**: Update `last_heartbeat` on successful RPC

**Invariants**:
- Each `node_id` appears at most once
- At least 3 nodes must be `Active` for quorum
- Cannot remove node until all shards migrated away

---

### ShardMap

**Purpose**: Track shard assignments and their replica placement.

**Storage**: `system_data` column family, replicated via system Raft group.

**Phase 1**: Single shard, all nodes are replicas.
**Phase 2+**: Multiple shards with subset replication.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMap {
    /// Schema version
    pub version: u8,

    /// All shards in the cluster
    pub shards: Vec<ShardInfo>,

    /// Number of replicas per shard (typically 3)
    pub replication_factor: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardInfo {
    /// Unique shard identifier (0, 1, 2, ...)
    pub id: u64,

    /// Hash range this shard covers (start, end) inclusive
    /// Phase 1: (0, u32::MAX) - single shard covers all keys
    /// Phase 2+: Partitioned ranges
    pub range: (u32, u32),

    /// Node IDs that replicate this shard
    pub replicas: Vec<u64>,

    /// Current leader node_id (cached, not source of truth)
    /// Source of truth is Raft state, this is for routing optimization
    pub leader: Option<u64>,

    /// Shard state
    pub state: ShardState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardState {
    /// Shard is healthy and serving requests
    Active,

    /// Shard is being split into two shards
    Splitting,

    /// Shard is being merged with another
    Merging,

    /// Shard is being rebalanced (replicas changing)
    Rebalancing,
}
```

**Key Routing Algorithm**:
```rust
fn route_key(key: &[u8], shard_map: &ShardMap) -> u64 {
    // 1. Hash the key using FNV1a (fast, good distribution)
    let hash = fnv1a_hash(key);

    // 2. Find shard covering this hash range
    for shard in &shard_map.shards {
        if hash >= shard.range.0 && hash <= shard.range.1 {
            return shard.id;
        }
    }

    panic!("No shard covers hash {} - ShardMap is invalid", hash);
}
```

**Phase 1 Example**:
```rust
ShardMap {
    version: 1,
    shards: vec![
        ShardInfo {
            id: 0,
            range: (0, u32::MAX),  // Covers all keys
            replicas: vec![1, 2, 3],  // All 3 nodes
            leader: Some(1),
            state: ShardState::Active,
        }
    ],
    replication_factor: 3,
}
```

**Phase 2 Example** (4 shards, 3 nodes):
```rust
ShardMap {
    version: 1,
    shards: vec![
        ShardInfo { id: 0, range: (0x00000000, 0x3FFFFFFF), replicas: vec![1, 2, 3], ... },
        ShardInfo { id: 1, range: (0x40000000, 0x7FFFFFFF), replicas: vec![1, 2, 3], ... },
        ShardInfo { id: 2, range: (0x80000000, 0xBFFFFFFF), replicas: vec![1, 2, 3], ... },
        ShardInfo { id: 3, range: (0xC0000000, 0xFFFFFFFF), replicas: vec![1, 2, 3], ... },
    ],
    replication_factor: 3,
}
```

---

## Raft Data Structures

### VersionedLogEntry

**Purpose**: Raft log entries with schema versioning for future evolution.

**Storage**: `*_raft_log` column families.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedLogEntry {
    /// Schema version (start at 1, increment on breaking changes)
    pub version: u8,

    /// Raft term
    pub term: u64,

    /// Log index
    pub index: u64,

    /// Entry type
    pub entry_type: EntryType,

    /// Serialized entry data (depends on entry_type)
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    /// Normal state machine command (e.g., SET foo bar)
    Normal,

    /// Configuration change (add/remove node)
    ConfigChange,

    /// No-op entry (for new leader to commit)
    Noop,
}
```

**Serialization**:
```rust
// Write to RocksDB
let entry = VersionedLogEntry {
    version: 1,
    term: 5,
    index: 142,
    entry_type: EntryType::Normal,
    data: bincode::serialize(&SetCommand { key: b"foo", value: b"bar" })?,
};

let key = format!("log:{}", entry.index);
let value = bincode::serialize(&entry)?;
storage.put("data_raft_log", key, value)?;

// Read from RocksDB
let value = storage.get("data_raft_log", key)?;
let entry: VersionedLogEntry = bincode::deserialize(&value)?;

// Version check
match entry.version {
    1 => process_v1(&entry),
    2 => process_v2(&entry),  // Future version
    _ => return Err("Unsupported log entry version"),
}
```

---

### RaftHardState

**Purpose**: Persistent Raft state that must survive restarts.

**Storage**: `*_raft_state` column families.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftHardState {
    /// Schema version
    pub version: u8,

    /// Current term
    pub term: u64,

    /// Candidate voted for in current term (if any)
    pub vote: Option<u64>,

    /// Index of highest committed log entry
    pub commit: u64,
}
```

**Persistence Guarantee**: Written to disk before responding to any Raft RPC.

---

### SnapshotMetadata

**Purpose**: Track snapshots for log compaction.

**Storage**: `system_data` and `data_kv` column families.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Schema version
    pub version: u8,

    /// Last log index included in this snapshot
    pub last_included_index: u64,

    /// Term of last_included_index
    pub last_included_term: u64,

    /// Cluster membership at snapshot time
    pub membership: ClusterMembership,

    /// Timestamp when snapshot created
    pub created_at: u64,

    /// Size of snapshot on disk (bytes)
    pub size_bytes: u64,
}
```

**Snapshot Creation**:
1. RocksDB checkpoint (hard links, atomic)
2. Record metadata with `last_included_index`
3. Truncate log entries before `last_included_index`

---

## Key-Value Data Structures

### StoredValue

**Purpose**: Wrap user values with metadata.

**Storage**: `data_kv` column family.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredValue {
    /// Schema version
    pub version: u8,

    /// User data (arbitrary bytes)
    pub data: Vec<u8>,

    /// Timestamp when created (milliseconds since epoch)
    pub created_at: u64,

    /// Optional TTL expiration (Phase 3+)
    pub expires_at: Option<u64>,
}
```

**Serialization**:
```rust
// SET foo bar
let value = StoredValue {
    version: 1,
    data: b"bar".to_vec(),
    created_at: current_timestamp_ms(),
    expires_at: None,
};

storage.put("data_kv", b"foo", bincode::serialize(&value)?)?;

// GET foo
let bytes = storage.get("data_kv", b"foo")?;
let stored: StoredValue = bincode::deserialize(&bytes)?;

// Check version
if stored.version != 1 {
    return Err("Unsupported value version");
}

// Check TTL (Phase 3+)
if let Some(expires) = stored.expires_at {
    if current_timestamp_ms() > expires {
        return Ok(None);  // Expired
    }
}

return Ok(Some(stored.data));
```

---

## Configuration Structures

### NodeConfig

**Purpose**: Node-specific configuration loaded from TOML.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier (must be > 0)
    pub id: u64,

    /// Address to bind for client connections (e.g., "0.0.0.0:6379")
    pub client_addr: String,

    /// Address to bind for internal RPC (e.g., "0.0.0.0:7379")
    pub internal_addr: String,

    /// Data directory path
    pub data_dir: String,

    /// Optional: Advertised address for internal RPC
    /// If not set, auto-detect from hostname
    pub advertise_addr: Option<String>,
}
```

### ClusterConfig

**Purpose**: Cluster-wide configuration.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Initial cluster members (bootstrap mode only)
    pub initial_members: Vec<InitialMember>,

    /// Bootstrap mode: create new cluster
    pub bootstrap: bool,

    /// Join mode: existing node to join
    pub join_addr: Option<String>,

    /// Number of replicas per shard
    pub replication_factor: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialMember {
    pub id: u64,
    pub addr: String,  // e.g., "kvstore-1:7379"
}
```

### RaftConfig

**Purpose**: Raft timing and compaction parameters.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Heartbeat interval (milliseconds)
    pub heartbeat_interval_ms: u64,

    /// Election timeout range
    pub election_timeout_min_ms: u64,
    pub election_timeout_max_ms: u64,

    /// Snapshot triggers
    pub snapshot_interval_entries: u64,    // e.g., 10,000
    pub snapshot_interval_bytes: u64,      // e.g., 100MB
    pub max_log_size_bytes: u64,           // e.g., 500MB - force compaction

    /// Snapshot transfer
    pub snapshot_chunk_size: usize,        // e.g., 1MB chunks
    pub snapshot_transfer_timeout_secs: u64,
}
```

---

## Size Limits and Validation

```rust
pub struct Limits {
    // Memory limits
    pub max_memory_per_raft_log_mb: usize,     // 512MB per Raft group
    pub max_concurrent_client_connections: usize,  // 10,000

    // Data size limits (Phase 1)
    pub max_key_size_bytes: usize,             // 256B
    pub max_value_size_bytes: usize,           // 64KB
    pub max_keys_per_node: usize,              // 10M keys

    // Timeouts
    pub request_timeout_secs: u64,             // 30s
    pub raft_rpc_timeout_secs: u64,            // 5s
}

// Validation example
fn validate_set_request(key: &[u8], value: &[u8], limits: &Limits) -> Result<()> {
    if key.len() > limits.max_key_size_bytes {
        return Err(Error::KeyTooLarge {
            size: key.len(),
            max: limits.max_key_size_bytes
        });
    }
    if value.len() > limits.max_value_size_bytes {
        return Err(Error::ValueTooLarge {
            size: value.len(),
            max: limits.max_value_size_bytes
        });
    }
    Ok(())
}
```

---

## Error Types

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Key too large: {size} bytes (max: {max})")]
    KeyTooLarge { size: usize, max: usize },

    #[error("Value too large: {size} bytes (max: {max})")]
    ValueTooLarge { size: usize, max: usize },

    #[error("Not leader: current leader is node {leader}")]
    NotLeader { leader: u64 },

    #[error("No quorum: cluster has {alive}/{total} nodes alive")]
    NoQuorum { alive: usize, total: usize },

    #[error("Unsupported version: {version} (current: {current})")]
    UnsupportedVersion { version: u8, current: u8 },

    #[error("Storage error: {0}")]
    Storage(#[from] rocksdb::Error),

    #[error("Raft error: {0}")]
    Raft(String),

    #[error("Protocol error: {0}")]
    Protocol(String),
}
```

---

## Metrics and Observability

```rust
pub struct NodeMetrics {
    // Raft metrics (per group)
    pub raft_term: u64,
    pub raft_commit_index: u64,
    pub raft_applied_index: u64,
    pub raft_is_leader: bool,
    pub raft_log_size_bytes: u64,

    // Storage metrics
    pub storage_db_size_bytes: u64,
    pub storage_num_keys: u64,

    // Request metrics
    pub requests_total: u64,
    pub request_errors_total: u64,
    pub requests_in_flight: usize,

    // Health metrics
    pub node_health_score: u8,  // 0-100
    pub cluster_nodes_healthy: usize,
}
```

---

## Type Aliases and Constants

```rust
// Common type aliases
pub type NodeId = u64;
pub type Term = u64;
pub type LogIndex = u64;
pub type Key = Vec<u8>;
pub type Value = Vec<u8>;
pub type Result<T> = std::result::Result<T, Error>;

// Constants
pub const CURRENT_VERSION: u8 = 1;
pub const MIN_ELECTION_TIMEOUT_MS: u64 = 500;
pub const MAX_ELECTION_TIMEOUT_MS: u64 = 1000;
pub const HEARTBEAT_INTERVAL_MS: u64 = 100;
pub const SNAPSHOT_INTERVAL_ENTRIES: u64 = 10_000;
```

---

This data structure design ensures:
- **Evolvability**: Version fields enable schema migrations
- **Type Safety**: Strong Rust types prevent invalid states
- **Serializability**: All structures are Serde-compatible
- **Testability**: Clear invariants and validation rules
- **Observability**: Rich metrics and error types
