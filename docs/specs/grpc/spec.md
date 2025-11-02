# gRPC Specification

## User Story

As a Raft node, I want to communicate with other nodes over gRPC so that I can participate in leader election, log replication, and consensus operations

## Acceptance Criteria

1. GIVEN a 3-node cluster, WHEN node 1 sends RequestVote RPC to node 2, THEN node 2 receives the message, processes it, and returns a vote response within 10 seconds
2. GIVEN a leader node, WHEN it sends AppendEntries RPC to a follower, THEN the follower receives log entries, persists them, and acknowledges within 10 seconds
3. GIVEN a new node joining the cluster, WHEN the leader sends InstallSnapshot RPC, THEN the node receives snapshot chunks up to 10MB and applies them successfully
4. GIVEN network failure between nodes, WHEN gRPC connection fails, THEN the error is propagated to openraft layer with proper RaftError type
5. GIVEN multiple concurrent RPCs, WHEN nodes send messages simultaneously, THEN all messages are processed without blocking each other
6. GIVEN a connection timeout of 5 seconds, WHEN a peer is unreachable, THEN the client fails the request within 5 seconds and retries up to 3 times with exponential backoff
7. GIVEN protobuf serialization, WHEN messages are sent over the wire, THEN they are efficiently encoded/decoded using prost 0.14

## Business Rules

- gRPC server MUST run on port 7379 (internal Raft RPC, distinct from port 6379 for client RESP protocol)
- Connection timeout MUST be 5 seconds
- Request timeout MUST be 10 seconds for RequestVote and AppendEntries
- Request timeout MUST be 30 seconds for InstallSnapshot (larger snapshots need more time)
- Maximum message size MUST be 10MB to support snapshot chunks
- Retry logic MUST use exponential backoff with maximum 3 attempts
- TLS/mTLS is OPTIONAL in Phase 1, will become REQUIRED in Phase 4
- All version dependencies MUST use prost 0.14 (matches tonic 0.11 requirement)
- Connection pool MUST maintain HashMap<NodeId, GrpcClient> for peer connections
- Peer discovery MUST use DNS-based hostname:port format
- Error handling MUST map tonic::Status to openraft RaftError types

## Scope

### Included
- gRPC service definition for Raft RPC messages (RequestVote, AppendEntries, InstallSnapshot)
- Server implementation for receiving Raft messages on port 7379
- Client implementation for sending messages to peers
- Implementation of openraft::RaftNetwork<RaftTypeConfig> trait
- Connection pooling and lifecycle management (HashMap<NodeId, GrpcClient>)
- Error handling with proper tonic::Status to RaftError mapping
- Retry logic with exponential backoff (max 3 attempts)
- Protocol Buffers schema definitions in crates/raft/proto/
- Integration with storage crate for state persistence
- Unit tests for protobuf serialization/deserialization
- Integration tests for 2-node message exchange
- Replacement of StubNetwork placeholder with OpenRaftNetwork

### Excluded
- TLS/mTLS authentication (deferred to Phase 4)
- Load balancing across multiple endpoints (not needed for fixed 3-node cluster)
- Streaming for large snapshots (optimization for later phases)
- Metrics/observability (deferred to Phase 4)
- Dynamic cluster membership (Phase 3)
- Client-facing RESP protocol (handled by protocol-resp crate)

## Dependencies

- openraft crate 0.10+ for RaftNetwork trait and type config
- storage crate for state persistence (log entries, snapshots)
- common crate for shared types (NodeId, ClusterId)
- tonic 0.11+ for gRPC server/client implementation
- prost 0.14+ for Protocol Buffers serialization (MUST match tonic version)
- tokio async runtime for connection management
- thiserror for error type definitions
- tracing for structured logging

## Technical Details

**Port**: 7379 (internal Raft RPC)
**Protocol**: gRPC with tonic 0.11+ / prost 0.14+
**Async Runtime**: tokio

### RPC Types
- RequestVote (leader election)
- AppendEntries (log replication)
- InstallSnapshot (snapshot transfer)

### Trait Implementation
Implements `openraft::RaftNetwork<RaftTypeConfig>` trait

Replaces `StubNetwork` placeholder from openraft migration spec

### File Structure
```
crates/raft/
├── proto/
│   └── raft.proto              # Protocol Buffers schema
├── src/
│   ├── grpc_server.rs          # Server implementation
│   ├── grpc_client.rs          # Client implementation
│   └── network.rs              # RaftNetwork trait (replaces network_stub.rs)
```

### Connection Strategy
- **Pooling**: HashMap<NodeId, GrpcClient>
- **Discovery**: DNS-based hostname:port
- **Retry**: Exponential backoff, max 3 attempts

### Error Mapping
tonic::Status -> openraft::error::RaftError

### Testing Strategy
- **Unit tests**: Protobuf serialization, client/server initialization
- **Integration tests**: 2-node message exchange scenarios
- **Chaos tests**: Included in 11 chaos test suite (network partitions, failures)

## Conflicts Resolved

### Port Number Mismatch
- **Issue**: Raw requirements specified port 50051, but project context requires port 7379 for internal Raft RPC
- **Resolution**: Use port 7379 per project architecture standards

### Timeout Value Clarity
- **Issue**: InstallSnapshot needs longer timeout than standard 10s due to large message size
- **Resolution**: Added specific 30s timeout rule for InstallSnapshot operations

## Diagrams

<!-- Add sequence diagrams for RequestVote, AppendEntries, and InstallSnapshot RPC flows if needed -->

## Alignment

This feature aligns with: Phase 1 goal of building a 3-node distributed Redis-compatible KV store with Raft consensus. The gRPC implementation enables internal node-to-node communication on port 7379, separate from client-facing RESP on port 6379. This directly supports the success criteria of leader election <2s and passing 11 chaos tests.

## Implementation Guidance

This specification is ready for the `/spec:design` phase to generate:
1. Detailed protobuf schema with all message types
2. Server implementation architecture (async handlers, connection lifecycle)
3. Client implementation architecture (connection pooling, retry logic)
4. RaftNetwork trait implementation mapping
5. Error type definitions and conversion logic
6. Test scenarios for unit and integration testing
