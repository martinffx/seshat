# gRPC Network Layer

## Overview

This feature implements a production-ready gRPC transport layer for OpenRaft RPC communication between Seshat nodes. It replaces the StubNetwork placeholder from the OpenRaft migration with full tonic/prost implementation, enabling multi-node Raft consensus for the Phase 1 goal: a 3-node distributed Redis-compatible KV store.

**Target Success Criteria:**
- Leader election completes in under 2 seconds
- All 11 chaos tests pass (network partitions, node failures, recovery)
- Performance exceeds 5,000 operations per second with sub-10ms p99 latency

**Port Assignment:** Internal Raft RPC uses port 7379 (resolved from initial requirement of 50051 to align with project architecture standards). This is distinct from the client-facing RESP protocol on port 6379.

The gRPC layer handles three core RPC types: RequestVote for leader election, AppendEntries for log replication and heartbeats, and InstallSnapshot for log compaction and state transfer to lagging nodes.

## User Story

As a Raft node, I want to communicate with other nodes over gRPC so that I can participate in leader election, log replication, and consensus operations.

## Acceptance Criteria

1. **RequestVote RPC**
   - GIVEN a 3-node cluster, WHEN node 1 sends RequestVote RPC to node 2
   - THEN node 2 receives the message, processes it against local state
   - AND returns a vote response within 10 seconds
   - AND updates its term if the candidate's term is higher
   - AND grants vote only if candidate's log is at least as up-to-date

2. **AppendEntries RPC**
   - GIVEN a leader node, WHEN it sends AppendEntries RPC to a follower
   - THEN the follower receives log entries and persists them
   - AND validates prev_log_index and prev_log_term match local log
   - AND appends new entries, overwriting conflicting entries
   - AND updates commit index if majority has acknowledged
   - AND returns success/failure response within 10 seconds

3. **InstallSnapshot RPC**
   - GIVEN a new node joining the cluster, WHEN the leader sends InstallSnapshot RPC
   - THEN the node receives snapshot data in chunks up to 10MB
   - AND assembles chunks into complete snapshot
   - AND applies snapshot to state machine
   - AND truncates log to snapshot's last_included_index
   - AND completes transfer within 30 seconds

4. **Error Propagation**
   - GIVEN network failure between nodes, WHEN gRPC connection fails
   - THEN the error propagates through NetworkError abstraction
   - AND converts to openraft::RaftError for handling by consensus layer
   - AND preserves context (node_id, RPC type, attempt count)
   - AND categorizes errors as transient (retryable) or permanent

5. **Concurrent Processing**
   - GIVEN multiple concurrent RPCs, WHEN nodes send messages simultaneously
   - THEN all messages process without blocking each other
   - AND connection pool supports concurrent access via RwLock
   - AND server handles requests concurrently via tokio
   - AND no single slow RPC blocks other operations

6. **Timeout and Retry**
   - GIVEN a connection timeout of 5 seconds, WHEN a peer is unreachable
   - THEN the client fails the request within 5 seconds
   - AND retries up to 3 times with exponential backoff
   - AND uses backoff intervals of 50ms, 100ms, 200ms
   - AND fails immediately for permanent errors (InvalidArgument, NotFound)
   - AND continues retrying for transient errors (Unavailable, DeadlineExceeded)

7. **Protobuf Serialization**
   - GIVEN protobuf serialization, WHEN messages are sent over the wire
   - THEN they encode efficiently using prost 0.14
   - AND achieve compact binary representation
   - AND support all Raft message types (Vote, AppendEntries, InstallSnapshot)
   - AND maintain version compatibility with tonic 0.11+

## Business Rules

- **Port Assignment:** gRPC server MUST run on port 7379 (internal Raft RPC, distinct from port 6379 for client RESP protocol)
- **Connection Timeout:** Connection establishment MUST timeout after 5 seconds
- **Request Timeouts:** RequestVote and AppendEntries MUST timeout after 10 seconds; InstallSnapshot MUST timeout after 30 seconds due to large data transfer
- **Message Size:** Maximum message size MUST be 10MB to support snapshot chunks
- **Retry Logic:** Retry logic MUST use exponential backoff with maximum 3 attempts for AppendEntries, 5 attempts for InstallSnapshot
- **TLS/mTLS:** TLS/mTLS is OPTIONAL in Phase 1, will become REQUIRED in Phase 4
- **Version Compatibility:** All dependencies MUST use prost 0.14 (matches tonic 0.11 requirement) - CRITICAL for compatibility
- **Connection Pool:** Connection pool MUST maintain HashMap<NodeId, GrpcClient> for peer connections with lazy initialization
- **Peer Discovery:** Peer discovery MUST use DNS-based hostname:port format (e.g., "node1.internal:7379")
- **Error Mapping:** Error handling MUST map tonic::Status to openraft RaftError types through NetworkError abstraction
- **Async-First:** All network operations MUST be async (tonic requirement) using tokio runtime

## Scope

### Included

- **gRPC Service Definition:** Protocol Buffers schema for RequestVote, AppendEntries, and InstallSnapshot RPC messages in crates/raft/proto/
- **Server Implementation:** gRPC server for receiving Raft messages on port 7379, handling all three RPC types with proper request/response semantics
- **Client Implementation:** Client for sending messages to peers with connection management and lifecycle handling
- **RaftNetwork Trait:** Full implementation of openraft::RaftNetwork<RaftTypeConfig> trait enabling OpenRaft to use gRPC transport
- **Connection Pooling:** HashMap<NodeId, GrpcClient> with lazy connection establishment, connection reuse, and eviction of failed connections
- **Error Handling:** Comprehensive tonic::Status to RaftError mapping with NetworkError abstraction providing transient/permanent classification
- **Retry Logic:** Exponential backoff with max 3 attempts for AppendEntries (no retry for RequestVote as election timeouts handle failures)
- **Protocol Buffers:** Schema definitions and generated Rust types using prost-build with tonic integration
- **Type Conversions:** Bidirectional conversions between protobuf messages and OpenRaft types
- **Streaming Support:** InstallSnapshot uses gRPC streaming for efficient large snapshot transfer without loading entire snapshot into memory
- **Unit Tests:** Protobuf serialization/deserialization, error conversion, connection pool behavior
- **Integration Tests:** 2-node cluster message exchange, leader election, log replication
- **StubNetwork Replacement:** Complete replacement of placeholder StubNetwork with production OpenRaftNetwork

### Excluded

- **TLS/mTLS Authentication:** Deferred to Phase 4 - all node-to-node communication will be plaintext in Phase 1
- **Load Balancing:** Not needed for fixed 3-node cluster with static membership
- **Streaming for Large Snapshots:** InstallSnapshot uses chunked transfer, full streaming optimization deferred
- **Metrics/Observability:** OpenTelemetry metrics, Prometheus endpoints deferred to Phase 4
- **Dynamic Cluster Membership:** Adding/removing nodes via Raft configuration changes deferred to Phase 3
- **Client-Facing RESP Protocol:** Handled by separate protocol-resp crate on port 6379

## Architecture

### Components

The gRPC network layer consists of four primary components that work together to provide reliable inter-node communication:

#### OpenRaftNetwork

Implements the `openraft::RaftNetwork<RaftTypeConfig>` trait, serving as the bridge between OpenRaft's consensus logic and the gRPC transport layer. This is the primary interface that OpenRaft uses to send RPCs to peer nodes.

**Responsibilities:**
- Implements OpenRaft's RaftNetwork trait with required methods: send_vote, send_append_entries, send_install_snapshot
- Manages connection pool to peer nodes, retrieving or establishing connections as needed
- Coordinates retry logic for transient failures, delegating to RaftGrpcClient
- Routes Raft RPC calls to appropriate gRPC clients based on target node_id
- Converts gRPC errors to OpenRaft errors through NetworkError abstraction
- Provides thread-safe access to shared connection pool

**Key Behaviors:**
- Lazy connection initialization - connections established on first RPC to a peer, not at startup
- Failed connections evicted from pool and retried on next call
- All methods are async, returning futures that resolve to Result types

#### RaftGrpcServer

Receives incoming Raft RPCs from peer nodes, deserializes protobuf messages to OpenRaft types, delegates processing to the local RaftNode, and serializes responses back to protobuf.

**Responsibilities:**
- Implements the generated `raft_service_server::RaftService` trait from protobuf
- Handles RequestVote, AppendEntries, and InstallSnapshot RPC invocations
- Deserializes protobuf messages to OpenRaft native types using conversion utilities
- Delegates to local RaftNode for actual consensus logic processing
- Serializes responses back to protobuf for transport
- Manages streaming for large snapshot transfers using tonic::Streaming

**Key Behaviors:**
- Binds to 0.0.0.0:7379 (all interfaces) on startup
- Registers with tonic server builder
- Handles requests concurrently using tokio's async runtime
- Graceful shutdown on SIGTERM/SIGINT signals

#### RaftGrpcClient

Wraps the tonic-generated gRPC client for a single peer connection, implementing retry logic, timeout enforcement, and type conversions for outbound RPCs.

**Responsibilities:**
- Wraps tonic-generated gRPC client for a single peer
- Implements retry logic with exponential backoff per RPC type
- Enforces timeout constraints: 500ms for RequestVote, 1000ms for AppendEntries, 30000ms for InstallSnapshot
- Converts protobuf types to/from OpenRaft types
- Manages streaming for snapshot installation
- Tracks connection health and supports reconnection

**Retry Semantics:**
- RequestVote: No retry (election timeouts handle failures)
- AppendEntries: Retry 3 times with exponential backoff (50ms, 100ms, 200ms)
- InstallSnapshot: Retry 5 times with backoff (network instability tolerance)
- Transient errors (Unavailable, DeadlineExceeded, ResourceExhausted) trigger retries
- Permanent errors (InvalidArgument, NotFound, PermissionDenied) fail immediately

#### ConnectionPool

Manages lazy connection establishment to peer nodes, caching active gRPC channels per node_id and providing thread-safe access.

**Responsibilities:**
- Maintains HashMap<NodeId, RaftGrpcClient> wrapped in RwLock for thread safety
- Manages lazy connection establishment (on first RPC, not startup)
- Caches active gRPC channels per node_id for reuse
- Provides health monitoring for connections
- Evicts stale or failed connections
- Supports concurrent access from multiple threads

**Key Behaviors:**
- `get_or_connect(node_id, addr)` - Returns existing connection or establishes new one
- `remove(node_id)` - Evicts failed connection from pool
- Bounded to cluster size (3-5 nodes in Phase 1)
- HTTP/2 connection reuse built into tonic

### Design Patterns

**Lazy Initialization:** Connections established on first RPC call to a peer, not at node startup. This reduces startup time and resource usage for unused connections.

**Connection Pooling:** HashMap<NodeId, GrpcClient> caches connections with RwLock for thread-safe concurrent access. Failed connections removed and retried on next call.

**Retry Coordination:** Per-RPC-type retry strategies handle transient failures gracefully while failing fast on permanent errors.

**Error Conversion Chain:** tonic::Status → NetworkError (with context) → openraft::RPCError → RaftError. Preserves node_id, RPC type, and attempt count.

## Technical Design

### RPC Endpoints

The gRPC service exposes three RPC endpoints, each with specific timeout and retry characteristics:

**RequestVote**
- Type: Unary RPC
- Timeout: 500ms (election timeout sensitive)
- Retry: No retry (election timeouts in OpenRaft handle failures)
- Purpose: Leader election vote requests
- Behavior: Candidate requests votes from peers; voters respond based on term and log completeness

**AppendEntries**
- Type: Unary RPC
- Timeout: 1000ms (1 second)
- Retry: 3 attempts with exponential backoff (50ms, 100ms, 200ms)
- Purpose: Log replication and heartbeats
- Behavior: Leader sends new entries or empty heartbeat; follower validates consistency and appends

**InstallSnapshot**
- Type: Streaming RPC (client-side streaming for chunks)
- Timeout: 30000ms (30 seconds - large data transfer)
- Retry: 5 attempts with backoff (network instability tolerance)
- Chunk Size: 64KB per message
- Purpose: Snapshot transfer for log compaction
- Behavior: Leader streams snapshot chunks to follower; follower assembles and applies

### Timeouts and Retries

**Timeout Configuration:**
- Connection establishment: 5 seconds
- RequestVote RPC: 500ms
- AppendEntries RPC: 1000ms
- InstallSnapshot RPC: 30000ms

**Retry Configuration:**
- Max retries (AppendEntries): 3
- Max retries (InstallSnapshot): 5
- Initial backoff: 50ms
- Max backoff: 5000ms
- Backoff multiplier: 2.0x (exponential)

**Transient vs Permanent Errors:**

Transient (trigger retry):
- tonic::Code::Unavailable (service temporarily unavailable)
- tonic::Code::DeadlineExceeded (timeout-like condition)
- tonic::Code::ResourceExhausted (temporary resource limits)
- tonic::Code::Unknown (unknown errors might be transient)

Permanent (fail immediately):
- tonic::Code::InvalidArgument (bad request - bug in code)
- tonic::Code::NotFound (wrong endpoint)
- tonic::Code::PermissionDenied (auth failure)
- tonic::Code::Unimplemented (version mismatch)

### Error Handling

The error handling system provides comprehensive error propagation with context preservation:

**NetworkError Variants:**
- ConnectionFailed: Failed to connect to peer node
- Timeout: RPC call exceeded timeout
- RpcFailed: gRPC returned error status
- Serialization: Failed to serialize message
- Deserialization: Failed to deserialize message
- NodeNotFound: Node ID not in cluster membership
- MaxRetriesExceeded: All retry attempts exhausted

**Error Propagation Chain:**
```
tonic::Status (gRPC error from tonic)
    ↓
NetworkError (raft crate with context: node_id, RPC type, attempt)
    ↓  
openraft::RPCError<RaftTypeConfig> (OpenRaft error type)
    ↓
RaftError (raft crate unified error)
    ↓
Handled by OpenRaft consensus logic
```

**Context Preservation:**
All errors preserve node_id, RPC type (RequestVote/AppendEntries/InstallSnapshot), and attempt count for debugging and logging.

### Connection Pooling

**Strategy:**
- Lazy initialization: Connections created on first RPC, not startup
- Keep-alive: HTTP/2 connection reuse built into tonic
- Eviction: Failed connections removed from pool immediately
- Health: No periodic health checks in Phase 1 (future enhancement)

**Data Structure:**
- HashMap<NodeId, RaftGrpcClient> wrapped in tokio::sync::RwLock
- Read-heavy access pattern (lookup on every RPC)
- Write-rare pattern (establishment only on first call or after failure)

**Bounded Size:**
Pool bounded to cluster size (3-5 nodes in Phase 1), preventing unbounded growth.

## Dependencies

**External Libraries:**
- **openraft 0.10+**: Provides RaftNetwork<RaftTypeConfig> trait and consensus logic
- **tonic 0.11+**: gRPC framework for Rust (server/client implementation)
- **prost 0.14+**: Protocol Buffers serialization (MUST match tonic version - critical)
- **tokio 1.x**: Async runtime for connection management and concurrent RPC handling
- **tower 0.4+**: Middleware for gRPC (timeouts, retries via layers)
- **async-trait 0.1**: Async trait support for trait implementations

**Internal Crates:**
- **storage crate**: Provides persistent state (log entries, snapshots) for Raft
- **common crate**: Provides shared types (NodeId, ClusterId, Error types)

**Tech Stack:**
- Rust version: 1.90+
- Serialization: Protocol Buffers via prost
- Error handling: thiserror
- Logging: tracing with structured fields

## Testing Strategy

### Unit Tests

**Protobuf Serialization:**
- Round-trip serialization/deserialization for all message types
- Binary encoding size verification
- Field presence and type validation

**Error Handling:**
- NetworkError creation and display
- Error conversion chain: tonic::Status → NetworkError → RPCError
- Transient vs permanent error classification
- Context preservation (node_id, RPC type, attempt count)

**Connection Pool:**
- Concurrent get_or_connect operations
- Connection reuse verification
- Eviction of failed connections
- Thread-safe access patterns

**Retry Logic:**
- Transient error detection triggers retry
- Permanent error fails immediately
- Exponential backoff timing
- Max retry attempt enforcement

### Integration Tests

**2-Node Cluster:**
- Leader election via RequestVote RPC
- Log replication via AppendEntries RPC
- Node restart and reconnection

**3-Node Cluster:**
- Leader election with split vote scenarios
- Log replication to multiple followers
- Consensus with one node failure
- Leader handover on failure

**Snapshot Transfer:**
- InstallSnapshot streaming to lagging node
- Snapshot application and log truncation
- Recovery after snapshot installation

### Chaos Tests

**Network Partitions:**
- Partition during leader election
- Partition during log replication
- Healing and recovery after partition

**Failure Scenarios:**
- Slow network (injected latency)
- Connection failures mid-RPC
- Snapshot transfer interruption
- Multiple simultaneous node failures

## Integration Points

### Initialization Flow

1. **Load Configuration**: Read cluster membership and node addresses from config.toml
2. **Initialize Storage**: Create OpenRaftMemStorage for persistent log and state
3. **Create Network Layer**: Instantiate OpenRaftNetwork with connection pool and cluster membership
4. **Create Raft Node**: Wrap openraft::Raft with network and storage
5. **Start gRPC Server**: Bind RaftGrpcServer to 0.0.0.0:7379
6. **Start RESP Server**: Start client-facing KV service on port 6379

### RaftNode Integration

RaftNode uses OpenRaftNetwork via OpenRaft's Raft instance. When consensus operations require RPC (leader election, log replication), OpenRaft automatically calls the network layer's methods.

**Automatic Usage:**
- No explicit network calls from application code
- OpenRaft invokes send_vote(), send_append_entries(), send_install_snapshot() as needed
- Network layer handles all connection management transparently

**Error Handling:**
- Network errors returned as RPCError to OpenRaft
- OpenRaft handles retry logic at consensus level (election timeouts, etc.)
- Application code receives high-level consensus results

## File Organization

```
crates/raft/
├── proto/
│   └── raft.proto              # Protocol Buffers schema definitions
├── src/
│   ├── network/
│   │   ├── mod.rs              # OpenRaftNetwork struct and trait impl
│   │   ├── error.rs            # NetworkError enum and variants
│   │   └── error_conversions.rs # tonic::Status → NetworkError → RPCError
│   ├── proto/
│   │   ├── mod.rs              # Generated protobuf code module
│   │   └── conversions.rs      # OpenRaft ↔ protobuf type conversions
│   ├── grpc_server.rs          # RaftGrpcServer service implementation
│   ├── grpc_client.rs          # RaftGrpcClient with retry logic
│   ├── connection_pool.rs      # ConnectionPool with HashMap
│   └── lib.rs                  # Crate exports and re-exports
└── tests/
    ├── proto_conversions_test.rs   # Protobuf serialization tests
    ├── network_error_test.rs       # Error handling tests
    ├── error_conversion_test.rs    # Error chain tests
    ├── connection_pool_test.rs     # Pool concurrent access tests
    └── integration/
        ├── two_node_test.rs        # 2-node cluster tests
        ├── three_node_test.rs      # 3-node cluster tests
        └── chaos_test.rs           # Chaos scenario tests
```

## Future Enhancements

### Phase 3
- **Dynamic Cluster Membership**: Add/remove nodes via Raft configuration changes without restart
- **Service Discovery Integration**: Replace static hostname:port with DNS-SD or Consul
- **Connection Health Checks**: Automatic reconnection and health monitoring

### Phase 4
- **TLS/mTLS**: Secure node-to-node communication with certificate-based authentication
- **OpenTelemetry Metrics**: Request latency, retry counts, connection pool size, RPC rates
- **Prometheus Endpoint**: HTTP endpoint exposing network metrics for monitoring

### Optimizations
- **Streaming Snapshots**: Full streaming for large snapshots (replace chunked InstallSnapshot)
- **Circuit Breaker**: Pattern for failing nodes to prevent cascading retries
- **Adaptive Timeouts**: Adjust based on measured network latencies
- **Compression**: gzip compression for AppendEntries messages to reduce bandwidth

## Alignment

This feature directly supports the Phase 1 goal of building a 3-node distributed Redis-compatible KV store with Raft consensus. The gRPC implementation enables the internal node-to-node communication required for distributed consensus, operating on port 7379 separate from client-facing operations on port 6379.

The implementation replaces the StubNetwork placeholder from the OpenRaft migration with production-ready transport, unblocking multi-node cluster testing and the 11 chaos test suite validation. Success criteria of leader election under 2 seconds and passing all chaos tests depend on this layer's reliability and performance.
