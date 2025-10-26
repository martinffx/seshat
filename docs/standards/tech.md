# Seshat Technical Standards

## Tech Stack

### Core Dependencies
- **Language**: Rust 1.90+ (2021 edition)
- **Async Runtime**: tokio 1.x (full features)
- **Consensus**: raft-rs 0.7+
- **Storage**: RocksDB 0.22+ (via rocksdb crate)
- **RPC Framework**: gRPC (tonic 0.11+ / prost 0.12+)
- **Serialization**: Protobuf (prost 0.12+) for all serialization (internal RPC and storage)
- **Error Handling**: thiserror (libraries), anyhow (binary)
- **Logging**: tracing + tracing-subscriber
- **Testing**: proptest (property tests), tokio-test
- **Observability**: OpenTelemetry (recommended for Phase 4)

### Dependency Rationale

**Why raft-rs?**
- Production-tested Raft implementation
- Flexible Storage trait for custom backends
- Used by TiKV and other distributed systems

**Why RocksDB?**
- Embedded key-value store (no separate process)
- Column family support for logical data separation
- Excellent write performance
- Snapshot support for log compaction

**Why gRPC/tonic?**
- Efficient binary protocol (Protobuf)
- HTTP/2 connection multiplexing
- Streaming support for large snapshots
- Good Rust ecosystem support

**Why OpenTelemetry?** (Phase 4)
- Vendor-neutral observability
- Supports metrics, traces, and logs
- Can export to Prometheus, Jaeger, etc.
- Future-proof for cloud-native deployments

## Architecture Layers

Seshat uses a layered architecture where each layer has clear responsibilities and dependencies flow downward.

```
┌─────────────────────────────────────┐
│   Protocol Layer (RESP/gRPC)        │  ← Client/Node communication
├─────────────────────────────────────┤
│   Service Layer (Command Execution) │  ← Business logic
├─────────────────────────────────────┤
│   Raft Layer (Consensus)            │  ← Distributed agreement
├─────────────────────────────────────┤
│   Storage Layer (RocksDB)           │  ← Persistent state
├─────────────────────────────────────┤
│   Transport Layer (Network I/O)     │  ← Physical communication
└─────────────────────────────────────┘
```

### 1. Protocol Layer
**Purpose**: Parse and serialize network protocols

**Responsibilities**:
- RESP protocol parsing (Redis wire format)
- RESP response serialization
- gRPC service definitions for internal RPC
- Protocol error handling

**Implementation**: `protocol/` crate

### 2. Service Layer
**Purpose**: Execute commands and coordinate operations

**Responsibilities**:
- Command validation (size limits, syntax)
- Route requests to appropriate Raft group
- Coordinate cross-shard operations (Phase 2+)
- Handle leader forwarding

**Implementation**: `seshat/` crate (main orchestration)

### 3. Raft Layer
**Purpose**: Provide distributed consensus

**Responsibilities**:
- Leader election
- Log replication
- Membership changes
- Snapshot creation and restoration
- State machine application

**Implementation**: `raft/` crate (wraps raft-rs)

### 4. Storage Layer
**Purpose**: Persistent data management

**Responsibilities**:
- RocksDB interaction
- Column family management
- Atomic batch writes
- Snapshot/checkpoint creation
- Storage metrics

**Implementation**: `storage/` crate

### 5. Transport Layer
**Purpose**: Network communication

**Responsibilities**:
- TCP connection management
- gRPC client/server
- Connection pooling
- Retry logic

**Implementation**: Integrated in `protocol/` crate

**Layer Dependencies**: Each layer only depends on layers below it, ensuring clean separation of concerns. See `docs/architecture/crates.md` for detailed module interaction.

## Node Characteristics
- Every node is architecturally identical
- No predefined special roles
- Dynamic leader election

## Network Topology
### Client-Facing Network
- Protocol: Redis RESP
- Default Port: 6379

### Internal RPC Network
- Protocol: gRPC
- Default Port: 7379

## Storage Model

### RocksDB Architecture

Each node runs a **single RocksDB instance** with multiple **column families** for logical data separation.

**Benefits of Column Families**:
- Independent compaction strategies per data type
- Atomic batch writes across multiple CFs
- Efficient prefix scans within a CF
- Separate memory budgets per CF

### Phase 1 Column Families

**For Single Shard Cluster (3 nodes, 1 data shard):**

| Column Family | Purpose | Data Type | Size Estimate |
|---------------|---------|-----------|---------------|
| `system_raft_log` | System Raft group log entries | VersionedLogEntry | ~10MB (compacted) |
| `system_raft_state` | System Raft group hard state | RaftHardState | <1KB |
| `system_data` | Cluster metadata, membership | ClusterMembership, ShardMap | ~100KB |
| `data_raft_log` | Data shard Raft log entries | VersionedLogEntry | ~100MB (compacted) |
| `data_raft_state` | Data shard hard state | RaftHardState | <1KB |
| `data_kv` | Actual key-value data | StoredValue | User data size |

**Storage Layout**:
```
/var/lib/seshat/
├── rocksdb/
│   ├── CURRENT
│   ├── MANIFEST-000001
│   ├── OPTIONS-000003
│   ├── 000001.log  (WAL)
│   ├── 000002.sst  (system_raft_log)
│   ├── 000003.sst  (system_raft_state)
│   ├── 000004.sst  (system_data)
│   ├── 000005.sst  (data_raft_log)
│   ├── 000006.sst  (data_raft_state)
│   └── 000007.sst  (data_kv)
└── snapshots/
    └── snapshot-20250112-120000/  (checkpoint)
```

### Column Family Details

#### 1. `system_raft_log`
**Purpose**: Store Raft log entries for the system Raft group

**Key Format**: `log:{index}` (e.g., `log:142`)

**Value Format**: Bincode-serialized `VersionedLogEntry`

**Compaction**: Truncated after snapshot (keep only entries after `last_included_index`)

**Size**: Grows until snapshot, then resets to ~0

**Example**:
```rust
// Write log entry (in raft crate, not storage crate)
let entry = VersionedLogEntry {
    version: 1,
    term: 5,
    index: 142,
    entry_type: EntryType::Normal,
    data: membership_change.encode_to_vec(),
};
let serialized = entry.encode_to_vec();
storage.put("system_raft_log", b"log:142", &serialized)?;
```

#### 2. `system_raft_state`
**Purpose**: Store Raft hard state (must survive restarts)

**Key Format**: `state` (single entry)

**Value Format**: Bincode-serialized `RaftHardState`

**Update Frequency**: On every vote or term change

**Size**: <1KB (fixed)

**Durability Requirement**: **Must be synced to disk before responding to RPCs**

**Example**:
```rust
// Update hard state (in raft crate, not storage crate)
let hard_state = RaftHardState {
    version: 1,
    term: 5,
    vote: Some(1),  // Voted for node 1
    commit: 142,
};
let serialized = hard_state.encode_to_vec();
storage.put("system_raft_state", b"state", &serialized)?;
storage.sync()?;  // CRITICAL: fsync before responding
```

#### 3. `system_data`
**Purpose**: Store cluster metadata (membership, shard assignments)

**Key Format**: `membership` and `shardmap`

**Value Format**: Protobuf-serialized `ClusterMembership` / `ShardMap`

**Update Frequency**: On membership changes, shard rebalancing

**Size**: ~100KB (bounded)

**Example**:
```rust
// Store cluster membership (in raft crate, not storage crate)
let membership = ClusterMembership {
    version: 1,
    members: HashMap::from([
        (1, NodeInfo { id: 1, addr: "kvstore-1:7379".into(), ... }),
        (2, NodeInfo { id: 2, addr: "kvstore-2:7379".into(), ... }),
        (3, NodeInfo { id: 3, addr: "kvstore-3:7379".into(), ... }),
    ]),
    membership_version: 1,
};
let serialized = membership.encode_to_vec();
storage.put("system_data", b"membership", &serialized)?;
```

#### 4. `data_raft_log`
**Purpose**: Store Raft log entries for data shard

**Key Format**: `log:{index}`

**Value Format**: Protobuf-serialized `VersionedLogEntry`

**Compaction**: Snapshot every 10,000 entries or 100MB

**Size**: 0-100MB (cyclic)

#### 5. `data_raft_state`
**Purpose**: Store data shard Raft hard state

**Key Format**: `state`

**Value Format**: Protobuf-serialized `RaftHardState`

**Durability**: fsync required

#### 6. `data_kv`
**Purpose**: Store actual user key-value data

**Key Format**: Raw user key (arbitrary bytes)

**Value Format**: Protobuf-serialized `StoredValue`

**Size**: Unbounded (user data)

**Compaction**: RocksDB automatic compaction

**Example**:
```rust
// SET foo bar (in state machine, not storage crate)
let value = StoredValue {
    version: 1,
    data: b"bar".to_vec(),
    created_at: current_timestamp_ms(),
    expires_at: None,
};
let serialized = value.encode_to_vec();
storage.put("data_kv", b"foo", &serialized)?;

// GET foo (in state machine, not storage crate)
let bytes = storage.get("data_kv", b"foo")?;
let stored: StoredValue = StoredValue::decode(&bytes[..])?;
// Returns: b"bar"
```

### Phase 2+ Column Families

**For Multi-Shard Cluster**: Add per-shard column families:
- `shard_0_raft_log`, `shard_0_raft_state`, `shard_0_kv`
- `shard_1_raft_log`, `shard_1_raft_state`, `shard_1_kv`
- etc.

### Data Structure Reference

All data structures are documented in detail in `docs/architecture/data-structures.md`, including:
- `VersionedLogEntry`: Raft log entry format
- `RaftHardState`: Persistent Raft state
- `ClusterMembership`: Node registry
- `ShardMap`: Shard assignments
- `StoredValue`: User data wrapper

### Storage Configuration

```rust
// RocksDB options per column family
let opts = Options::default();
opts.set_compression_type(CompressionType::Lz4);  // Fast compression
opts.set_write_buffer_size(64 * 1024 * 1024);     // 64MB write buffer
opts.set_max_write_buffer_number(3);
opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB SST files

// Raft log CFs: optimize for sequential writes
let raft_log_opts = opts.clone();
raft_log_opts.set_compaction_style(DBCompactionStyle::Level);

// Data KV CF: optimize for point lookups
let kv_opts = opts.clone();
kv_opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(4));  // Hash prefix
kv_opts.set_memtable_prefix_bloom_ratio(0.2);
```

### Snapshot Strategy

**Trigger**: Every 10,000 log entries OR 100MB log size

**Method**: RocksDB checkpoint (hard links, atomic)

**Process**:
1. Create checkpoint: `storage.create_checkpoint("snapshots/snapshot-{timestamp}")?`
2. Record metadata: `last_included_index`, `last_included_term`
3. Truncate log: Delete entries before `last_included_index`
4. Update Raft state with new snapshot metadata

**Restoration**:
1. Copy checkpoint to data directory
2. Open RocksDB
3. Read `last_included_index` from snapshot metadata
4. Replay log entries after snapshot

## Observability (Phase 4)

### Metrics with OpenTelemetry

**Recommended**: Use OpenTelemetry for vendor-neutral observability.

```rust
use opentelemetry::metrics::{Meter, Counter, Histogram};
use opentelemetry_prometheus::exporter;

// Initialize on startup
let exporter = exporter()
    .with_registry(prometheus::default_registry())
    .build()?;

// Define metrics
let meter = global::meter("seshat");

let request_counter = meter
    .u64_counter("requests_total")
    .with_description("Total requests processed")
    .init();

let request_duration = meter
    .f64_histogram("request_duration_seconds")
    .with_description("Request latency")
    .init();

// Use in code
request_counter.add(1, &[KeyValue::new("command", "GET")]);
request_duration.record(0.005, &[KeyValue::new("command", "GET")]);
```

**Export Prometheus metrics**:
```rust
// Expose /metrics endpoint
let metrics_addr = SocketAddr::from(([0, 0, 0, 0], 9090));
let metrics_service = PrometheusExporter::default();
warp::serve(warp::path("metrics").map(move || metrics_service.export()))
    .run(metrics_addr)
    .await;
```

### Structured Logging with Tracing

```rust
use tracing::{info, warn, error, debug, instrument};

// Initialize on startup
tracing_subscriber::fmt()
    .with_target(true)
    .with_thread_ids(true)
    .with_level(true)
    .with_file(true)
    .with_line_number(true)
    .json()  // JSON format for parsing
    .init();

// Instrument functions
#[instrument(skip(self), fields(node_id = %self.id, term = %term))]
async fn become_leader(&mut self, term: u64) {
    info!("Became leader");
}

// Log with structured fields
debug!(
    index = %entry.index,
    term = %entry.term,
    size = %entry.data.len(),
    "Applying log entry"
);
```

### Health Check Endpoints

```rust
// GET /health - Liveness probe
// Returns 200 if node is running, 503 if unhealthy
async fn health_check(state: Arc<NodeState>) -> Result<impl Reply, Rejection> {
    if state.can_reach_quorum() {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::SERVICE_UNAVAILABLE)
    }
}

// GET /ready - Readiness probe
// Returns 200 if ready to serve traffic, 503 if still catching up
async fn readiness_check(state: Arc<NodeState>) -> Result<impl Reply, Rejection> {
    if state.is_ready() && state.lag() < 100 {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::SERVICE_UNAVAILABLE)
    }
}
```

### Key Metrics to Track

**Raft Metrics**:
- `raft_term{node_id, raft_group}`: Current term
- `raft_is_leader{node_id, raft_group}`: 1 if leader, 0 otherwise
- `raft_commit_index{node_id, raft_group}`: Committed log index
- `raft_log_size_bytes{node_id, raft_group}`: Log size before compaction

**Storage Metrics**:
- `storage_db_size_bytes{node_id}`: RocksDB total size
- `storage_num_keys{node_id, shard}`: Key count per shard
- `storage_snapshot_duration_seconds`: Histogram of snapshot times

**Request Metrics**:
- `requests_total{node_id, command, status}`: Request counter
- `request_duration_seconds{node_id, command}`: Histogram of latencies
- `requests_in_flight{node_id}`: Current concurrent requests

**Health Metrics**:
- `cluster_nodes_healthy`: Number of healthy nodes
- `cluster_shards_healthy`: Number of shards with quorum

## Design Principles

### Architectural Principles
- **Every node is identical**: No special roles, simplifies operations
- **Layered architecture**: Clear separation of concerns, testable in isolation
- **Immutable infrastructure**: Nodes are stateless except for data directory
- **Predictable state transitions**: All state changes go through Raft

### Code Quality Principles
- **Type safety**: Use Rust's type system to prevent invalid states
- **Error handling**: All errors propagate with context (anyhow/thiserror)
- **Testing**: Unit tests, integration tests, property tests, chaos tests
- **Documentation**: Every public API documented, architecture docs maintained

### Operational Principles
- **Observability first**: Metrics and logs from day 1
- **Fail fast**: Validate configuration on startup, panic on invariant violations
- **Graceful degradation**: Continue serving if possible, reject requests if not
- **Automation**: Health checks, auto-restart, but no auto-remediation (Phase 1)

### Performance Principles
- **Performance-conscious design**: Profile before optimizing, but be aware of O(n²) algorithms
- **Resource limits**: Enforce limits from day 1 (key/value sizes, connections, memory)
- **Async by default**: Use tokio for all I/O, avoid blocking operations
- **Zero-copy where possible**: Use `bytes::Bytes` for data passing

## Related Documentation

- **Architecture**: `docs/architecture/crates.md` - Crate responsibilities and interactions
- **Data Structures**: `docs/architecture/data-structures.md` - Core types and serialization
- **Startup**: `docs/architecture/startup.md` - Bootstrap and join modes
- **Practices**: `docs/standards/practices.md` - TDD workflow and chaos testing
- **Style**: `docs/standards/style.md` - Code conventions