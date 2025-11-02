# gRPC - Condensed Spec

## User Story
As a Raft node, I want to communicate with other nodes over gRPC so that I can participate in leader election, log replication, and consensus operations

## Key Criteria
1. RequestVote RPC completes within 10s with proper vote response
2. AppendEntries RPC persists log entries and acknowledges within 10s
3. InstallSnapshot RPC handles chunks up to 10MB successfully
4. Network failures propagate as RaftError types to openraft layer
5. Concurrent RPCs process without blocking
6. Connection timeout 5s, retry 3x with exponential backoff
7. Protobuf serialization using prost 0.14

## Critical Rules
- Port 7379 (internal Raft), distinct from 6379 (client RESP)
- Timeouts: 5s connection, 10s request, 30s InstallSnapshot
- Max message 10MB, retry 3x exponential backoff
- HashMap<NodeId, GrpcClient> connection pool
- tonic::Status -> RaftError mapping required
- TLS optional Phase 1, required Phase 4

## Scope
**IN**: gRPC service (RequestVote/AppendEntries/InstallSnapshot), server on 7379, client with pooling, RaftNetwork trait impl, retry logic, protobuf schemas, integration tests, replaces StubNetwork

**OUT**: TLS (Phase 4), load balancing, streaming snapshots, metrics (Phase 4), dynamic membership (Phase 3)

## Dependencies
- openraft 0.10+ (RaftNetwork trait)
- storage crate (persistence)
- common crate (NodeId, ClusterId)
- tonic 0.11+ / prost 0.14+ (MUST match versions)
- tokio, thiserror, tracing

## Files
```
crates/raft/proto/raft.proto              # Protobuf schema
crates/raft/src/grpc_server.rs            # Server impl
crates/raft/src/grpc_client.rs            # Client impl
crates/raft/src/network.rs                # RaftNetwork trait (replaces network_stub.rs)
```

## Architecture
- **Trait**: openraft::RaftNetwork<RaftTypeConfig>
- **Pool**: HashMap<NodeId, GrpcClient> with DNS discovery
- **Error**: tonic::Status -> openraft::RaftError
- **Tests**: Unit (protobuf), Integration (2-node), Chaos (11 tests)

## Alignment
Phase 1 MVP: 3-node cluster with Raft consensus. Enables internal node communication on 7379, supports leader election <2s and chaos tests.
