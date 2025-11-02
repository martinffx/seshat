# Technical Design: gRPC Network Layer for OpenRaft

## Overview

This design describes the gRPC network layer that replaces the `StubNetwork` placeholder created during the OpenRaft migration. The gRPC layer provides inter-node communication for Raft consensus, implementing the `RaftNetwork` trait with connection pooling, retry logic, and streaming support for snapshots.

**Current State**: StubNetwork placeholder exists (from OpenRaft Phase 4)
**Target State**: Production-ready gRPC transport with connection pooling and retry logic

## Technical Needs Analysis

### 1. Domain Model

The gRPC network layer's "domain" is distributed communication between Raft nodes. Core entities manage connections, message serialization, and error handling.

#### Core Entities

**OpenRaftNetwork (RaftNetwork Implementation)**
- **Responsibilities**:
  - Implements OpenRaft's `RaftNetwork<RaftTypeConfig>` trait
  - Manages connection pool to peer nodes
  - Coordinates retry logic for transient failures
  - Routes Raft RPC calls to appropriate gRPC clients
  - Converts gRPC errors to OpenRaft errors
- **Methods**:
  - `send_append_entries(target: u64, req: AppendEntriesRequest) -> Result<AppendEntriesResponse>`
  - `send_vote(target: u64, req: VoteRequest) -> Result<VoteResponse>`
  - `send_install_snapshot(target: u64, req: InstallSnapshotRequest) -> Result<InstallSnapshotResponse>`
  - `get_or_create_client(node_id: u64) -> Result<RaftGrpcClient>` (internal)
- **Dependencies**: ConnectionPool, RaftGrpcClient, common::NodeId

**RaftGrpcServer**
- **Responsibilities**:
  - Implements generated `raft_service_server::RaftService` trait from protobuf
  - Receives incoming Raft RPCs from peer nodes
  - Deserializes protobuf messages to OpenRaft types
  - Delegates to local RaftNode for processing
  - Serializes responses back to protobuf
  - Handles streaming for large snapshot transfers
- **Methods**:
  - `request_vote(&self, request: tonic::Request<proto::VoteRequest>) -> Result<tonic::Response<proto::VoteResponse>>`
  - `append_entries(&self, request: tonic::Request<proto::AppendEntriesRequest>) -> Result<tonic::Response<proto::AppendEntriesResponse>>`
  - `install_snapshot(&self, request: tonic::Request<Streaming<proto::InstallSnapshotRequest>>) -> Result<tonic::Response<proto::InstallSnapshotResponse>>`
- **Dependencies**: Arc<RaftNode>, protobuf conversion utilities

**RaftGrpcClient**
- **Responsibilities**:
  - Wraps tonic-generated gRPC client for a single peer
  - Implements retry logic with exponential backoff
  - Handles timeout enforcement per RPC type
  - Converts protobuf types to/from OpenRaft types
  - Manages streaming for snapshot installation
- **Methods**:
  - `call_vote(&mut self, req: VoteRequest) -> Result<VoteResponse>` (with retry)
  - `call_append_entries(&mut self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse>` (with retry)
  - `call_install_snapshot(&mut self, req: InstallSnapshotRequest) -> Result<InstallSnapshotResponse>` (streaming)
  - `is_connected(&self) -> bool`
  - `reconnect(&mut self) -> Result<()>`
- **Dependencies**: tonic::transport::Channel, RaftServiceClient (generated)

**ConnectionPool**
- **Responsibilities**:
  - Manages lazy connection establishment to peer nodes
  - Caches active gRPC channels per node_id
  - Handles connection health monitoring
  - Provides thread-safe access via RwLock or DashMap
  - Evicts stale/failed connections
- **Methods**:
  - `get_or_connect(node_id: u64, addr: String) -> Result<RaftGrpcClient>`
  - `remove(node_id: u64)` (for connection failures)
  - `health_check_all() -> Vec<(u64, bool)>`
- **Data Structure**: `HashMap<NodeId, RaftGrpcClient>` wrapped in `RwLock`
- **Dependencies**: tonic::transport::Channel, common::NodeId

#### Service Responsibilities

**Connection Management Service**
- ConnectionPool manages peer connections with lazy initialization
- Connections established on first RPC call (not at startup)
- Failed connections removed from pool and retried on next call
- Health checks run periodically (future enhancement)

**Retry Coordination Service**
- RaftGrpcClient handles retry logic per RPC type
- Vote RPCs: No retry (election timeouts handle failures)
- AppendEntries RPCs: Retry 3 times with exponential backoff (50ms, 100ms, 200ms)
- InstallSnapshot RPCs: Retry 5 times with backoff (network instability tolerance)
- Transient errors (Unavailable, DeadlineExceeded) trigger retries
- Permanent errors (InvalidArgument, NotFound) fail immediately

**Error Conversion Service**
- NetworkError enum provides bidirectional conversions
- tonic::Status → NetworkError (maps gRPC codes to domain errors)
- NetworkError → openraft::RPCError (integrates with OpenRaft)
- Preserves error context for debugging (node_id, RPC type, attempt count)

### 2. Data Requirements

#### Persistence: In-Memory Only
- **Storage Type**: In-memory connection pool only
- **No Database**: gRPC layer is stateless - connection pool recreated on restart
- **Justification**: All persistent state (log, snapshots) managed by storage crate via OpenRaft
- **Data Volume**: Small - connection metadata per peer (max 10 connections in Phase 1)
- **Access Patterns**:
  - Read-heavy: Lookup peer connection from pool on every RPC
  - Write-rare: Connection establishment only on first call or after failure

#### Connection Metadata (Ephemeral)
```rust
struct ConnectionMetadata {
    node_id: u64,
    addr: String,
    channel: tonic::transport::Channel,
    client: RaftServiceClient<Channel>,
    established_at: Instant,
    last_used: Instant,
}
```

### 3. Router Layer (gRPC Service)

#### No Traditional HTTP Router
The gRPC server replaces the traditional HTTP router pattern used in service layers.

**RaftService gRPC Definition** (`crates/raft/proto/raft.proto`):
```protobuf
service RaftService {
  rpc RequestVote(VoteRequest) returns (VoteResponse);
  rpc AppendEntries(AppendEntriesRequest) returns (AppendEntriesResponse);
  rpc InstallSnapshot(stream InstallSnapshotRequest) returns (InstallSnapshotResponse);
}
```

**Server Configuration**:
- **Port**: 7379 (configurable)
- **Protocol**: HTTP/2 (gRPC requirement)
- **TLS**: Optional (Phase 4 - production readiness)
- **Concurrency**: Tokio runtime handles concurrent RPCs
- **Backpressure**: gRPC flow control built-in

**Server Lifecycle**:
1. seshat binary starts RaftGrpcServer on port 7379
2. Server binds to `0.0.0.0:7379` (all interfaces)
3. Server registers RaftService implementation
4. Incoming RPCs handled concurrently by tokio
5. Each RPC delegates to local RaftNode
6. Graceful shutdown on SIGTERM/SIGINT

### 4. Events & Async Patterns

#### No Event Bus in Phase 1
- Raft is inherently request-response (synchronous RPC semantics)
- No async event bus needed for core consensus
- Future Phase 4 may add metrics events for observability

#### Async Patterns Used
- All gRPC methods are async (tonic requirement)
- ConnectionPool uses async RwLock (tokio::sync::RwLock)
- Retry logic uses tokio::time::sleep for backoff
- Streaming snapshots use tonic::Streaming<T>

### 5. Dependencies

#### Upstream Crates (Seshat)
- **openraft**: Provides `RaftNetwork<RaftTypeConfig>` trait
- **common**: Provides `NodeId`, `Error`, configuration types

#### Downstream Crates (Seshat)
- **seshat binary**: Starts gRPC server on port 7379
- **RaftNode**: Uses OpenRaftNetwork for consensus operations

#### External Libraries
- **tonic 0.11+**: gRPC framework for Rust
- **prost 0.14+**: Protobuf serialization (matches OpenRaft requirement)
- **tokio 1.x**: Async runtime
- **tower 0.4+**: Middleware for gRPC (timeouts, retries)
- **async-trait 0.1**: Async trait support

### 6. Error Handling Strategy

#### Error Types

**NetworkError** (crates/raft/src/network/error.rs):
```rust
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Connection failed to node {node_id}: {source}")]
    ConnectionFailed {
        node_id: u64,
        #[source]
        source: tonic::transport::Error,
    },

    #[error("RPC timeout after {timeout_ms}ms to node {node_id}")]
    Timeout {
        node_id: u64,
        timeout_ms: u64,
    },

    #[error("RPC failed to node {node_id}: {status}")]
    RpcFailed {
        node_id: u64,
        status: tonic::Status,
    },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Node {node_id} not found in cluster membership")]
    NodeNotFound { node_id: u64 },

    #[error("Max retries ({max_retries}) exceeded for node {node_id}")]
    MaxRetriesExceeded {
        node_id: u64,
        max_retries: u32,
    },
}
```

#### Error Propagation Chain

```
tonic::Status (gRPC error)
    ↓ (convert in RaftGrpcClient)
NetworkError (raft crate)
    ↓ (convert in OpenRaftNetwork)
openraft::RPCError<RaftTypeConfig> (openraft crate)
    ↓ (handled by OpenRaft)
RaftError (raft crate - if OpenRaft propagates up)
    ↓ (convert in service layer)
KvServiceError / SqlServiceError
    ↓ (format for client)
RespValue::Error / SQL Error Response
```

#### Retry Semantics

**Transient Errors (Retry)**:
- `tonic::Code::Unavailable` → Retry with backoff
- `tonic::Code::DeadlineExceeded` → Retry with increased timeout
- `tonic::Code::ResourceExhausted` → Retry with backoff
- `tonic::Code::Unknown` → Retry once (may be transient)

**Permanent Errors (Fail Fast)**:
- `tonic::Code::InvalidArgument` → Fail immediately (bad request)
- `tonic::Code::NotFound` → Fail immediately (wrong endpoint)
- `tonic::Code::PermissionDenied` → Fail immediately (auth failure)
- `tonic::Code::Unimplemented` → Fail immediately (version mismatch)

**Retry Configuration**:
```rust
struct RetryConfig {
    max_retries: u32,          // 3 for AppendEntries, 5 for snapshots
    initial_backoff_ms: u64,   // 50ms
    max_backoff_ms: u64,       // 5000ms (5 seconds)
    backoff_multiplier: f64,   // 2.0 (exponential)
}
```

### 7. Integration Points

#### Initialization Flow (seshat binary)
```rust
// crates/seshat/src/main.rs
async fn main() -> Result<()> {
    // 1. Load configuration
    let config = Config::load()?;

    // 2. Initialize storage
    let storage = OpenRaftMemStorage::new()?;

    // 3. Create network layer
    let network = OpenRaftNetwork::new(
        config.node_id,
        config.cluster_membership.clone(),
    );

    // 4. Create Raft node
    let raft_node = RaftNode::new(
        config.node_id,
        Arc::new(config.raft_config),
        Arc::new(network),
        Arc::new(storage),
    ).await?;

    // 5. Start gRPC server
    let grpc_server = RaftGrpcServer::new(Arc::clone(&raft_node));
    let grpc_addr = format!("0.0.0.0:{}", config.raft_port);

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(RaftServiceServer::new(grpc_server))
            .serve(grpc_addr.parse().unwrap())
            .await
    });

    // 6. Start RESP server (client-facing)
    // ... KV service initialization ...

    Ok(())
}
```

#### RaftNode Integration
```rust
// RaftNode uses OpenRaftNetwork via OpenRaft's Raft instance
impl RaftNode {
    async fn new(
        id: u64,
        config: Arc<Config>,
        network: Arc<OpenRaftNetwork>,
        storage: Arc<OpenRaftMemStorage>,
    ) -> Result<Self> {
        // OpenRaft automatically uses network for RPCs
        let raft = Raft::new(id, config, network, storage).await?;
        Ok(Self { raft, storage })
    }
}
```

### 8. Performance Considerations

#### Connection Pooling Strategy
- **Lazy Initialization**: Connections created on first RPC (not at startup)
- **Keep-Alive**: HTTP/2 connection reuse (built-in with tonic)
- **Timeout per RPC Type**:
  - RequestVote: 500ms (election timeout sensitive)
  - AppendEntries: 1000ms (heartbeat/replication)
  - InstallSnapshot: 30000ms (large data transfer)

#### Streaming for Large Snapshots
- InstallSnapshot uses streaming to avoid loading entire snapshot in memory
- Chunk size: 64KB per message
- Flow control via gRPC backpressure
- Receiver writes chunks directly to storage

#### Memory Management
- Connection pool bounded to cluster size (small in Phase 1: 3-5 nodes)
- No unbounded buffers for messages
- Streaming prevents large snapshot memory spikes

### 9. Observability

#### Tracing Instrumentation
```rust
#[tracing::instrument(skip(self, req), fields(target = %target, term = %req.term))]
async fn send_append_entries(
    &self,
    target: u64,
    req: AppendEntriesRequest,
) -> Result<AppendEntriesResponse> {
    tracing::debug!("Sending AppendEntries to node {}", target);
    // ... implementation ...
}
```

#### Metrics (Future - Phase 4)
- `raft_rpc_requests_total{type, status, target_node}`: Counter
- `raft_rpc_duration_seconds{type, target_node}`: Histogram
- `raft_network_errors_total{type, error_code, target_node}`: Counter
- `raft_connection_pool_size`: Gauge

### 10. Testing Strategy

#### Unit Tests
- Connection pool: concurrent get_or_connect
- Retry logic: transient vs permanent error handling
- Error conversions: tonic::Status → NetworkError → RPCError
- Timeout enforcement per RPC type

#### Integration Tests
- 2-node cluster: leader election via gRPC
- 3-node cluster: log replication via AppendEntries
- Snapshot transfer: streaming InstallSnapshot
- Network partition simulation: connection failures

#### Chaos Tests (Future - Phase 1D)
- Network partition during election
- Slow network (inject latency)
- Connection failures mid-RPC
- Snapshot transfer interruption

## Architecture Diagrams

### Component Interaction

```
┌─────────────────────────────────────────────────────────┐
│                    Seshat Binary                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │  RaftGrpcServer (port 7379)                        │ │
│  │  - Receives RPCs from peers                        │ │
│  │  - Delegates to local RaftNode                     │ │
│  └─────────────────┬──────────────────────────────────┘ │
│                    │                                     │
│  ┌─────────────────▼──────────────────────────────────┐ │
│  │  RaftNode (OpenRaft)                               │ │
│  │  - Consensus logic                                 │ │
│  │  - Uses OpenRaftNetwork for outbound RPCs          │ │
│  └─────────────────┬──────────────────────────────────┘ │
│                    │                                     │
│  ┌─────────────────▼──────────────────────────────────┐ │
│  │  OpenRaftNetwork (RaftNetwork trait)               │ │
│  │  - send_append_entries()                           │ │
│  │  - send_vote()                                     │ │
│  │  - send_install_snapshot()                         │ │
│  └─────────────────┬──────────────────────────────────┘ │
│                    │                                     │
│  ┌─────────────────▼──────────────────────────────────┐ │
│  │  ConnectionPool                                    │ │
│  │  - HashMap<NodeId, RaftGrpcClient>                 │ │
│  │  - Lazy connection establishment                   │ │
│  └─────────────────┬──────────────────────────────────┘ │
│                    │                                     │
│  ┌─────────────────▼──────────────────────────────────┐ │
│  │  RaftGrpcClient (per peer)                         │ │
│  │  - tonic Channel                                   │ │
│  │  - Retry logic                                     │ │
│  │  - Timeout enforcement                             │ │
│  └─────────────────┬──────────────────────────────────┘ │
└────────────────────┼──────────────────────────────────────┘
                     │
                     │ gRPC (HTTP/2)
                     ▼
┌─────────────────────────────────────────────────────────┐
│              Peer Node (port 7379)                       │
│  ┌────────────────────────────────────────────────────┐ │
│  │  RaftGrpcServer                                    │ │
│  │  - Receives RPC                                    │ │
│  │  - Processes via RaftNode                          │ │
│  │  - Returns response                                │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### RPC Flow: AppendEntries

```
Leader Node                          Follower Node
     │                                      │
     │  1. OpenRaft triggers replication    │
     │     (internal)                       │
     │                                      │
     │  2. Calls network.send_append_entries()
     ├─────────────────────────────────────▶│
     │     AppendEntriesRequest             │
     │                                      │
     │  3. ConnectionPool.get_or_connect()  │
     │     (lookup or establish connection) │
     │                                      │
     │  4. RaftGrpcClient.call_append_entries()
     │     (with retry logic)               │
     │                                      │
     │  5. gRPC call over HTTP/2            │
     ├─────────────────────────────────────▶│
     │     Serialized protobuf              │
     │                                      │
     │                                      │ 6. RaftGrpcServer receives
     │                                      │    tonic::Request
     │                                      │
     │                                      │ 7. Delegates to RaftNode
     │                                      │    (OpenRaft processes)
     │                                      │
     │  8. gRPC response                    │
     │◀─────────────────────────────────────┤
     │     AppendEntriesResponse            │
     │                                      │
     │  9. Return to OpenRaft               │
     │                                      │
```

## Implementation Phases

### Phase 1: Protobuf Definitions (1-2 hours)
- Define raft.proto with RequestVote, AppendEntries, InstallSnapshot messages
- Generate Rust code with prost-build
- Create type conversions: OpenRaft types ↔ protobuf types

### Phase 2: RaftGrpcClient (3-4 hours)
- Implement client wrapper around generated tonic client
- Add retry logic with exponential backoff
- Implement timeout enforcement per RPC type
- Unit tests for retry behavior

### Phase 3: ConnectionPool (2-3 hours)
- Implement connection pool with lazy initialization
- Thread-safe access with RwLock or DashMap
- Connection health tracking
- Unit tests for concurrent access

### Phase 4: OpenRaftNetwork (2-3 hours)
- Implement RaftNetwork trait
- Integrate ConnectionPool
- Route RPCs to appropriate clients
- Error conversion to OpenRaft types

### Phase 5: RaftGrpcServer (3-4 hours)
- Implement gRPC service trait
- Delegate to local RaftNode
- Handle streaming for snapshots
- Integration tests with 2-node cluster

### Phase 6: Integration & Testing (4-6 hours)
- Wire into seshat binary
- 3-node cluster integration tests
- Leader election via gRPC
- Log replication validation
- Snapshot transfer test

**Total Estimated Effort**: 15-22 hours

## Success Criteria

- [ ] 3-node cluster performs leader election via gRPC
- [ ] Log replication works via AppendEntries RPCs
- [ ] Snapshot transfer completes via streaming InstallSnapshot
- [ ] Retry logic handles transient connection failures
- [ ] Connection pool reuses connections efficiently
- [ ] No prost version conflicts (all use 0.14)
- [ ] All unit tests pass (coverage >80%)
- [ ] Integration tests pass (2-node and 3-node clusters)

## Future Enhancements (Post-Phase 1)

- **TLS Support** (Phase 4): Mutual TLS for inter-node authentication
- **Compression** (Phase 4): gRPC message compression (gzip)
- **Connection Health Checks** (Phase 3): Periodic health checks and connection eviction
- **Metrics** (Phase 4): OpenTelemetry instrumentation for observability
- **Dynamic Membership** (Phase 3): Update connection pool on membership changes

---

**Created**: 2025-10-26
**Feature**: grpc-network-layer
**Phase**: 1B (Post-OpenRaft Migration)
**Priority**: HIGH (blocks 3-node cluster testing)
**Estimated Effort**: 15-22 hours
