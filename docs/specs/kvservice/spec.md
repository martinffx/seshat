# KV Service Layer Specification

## Overview

KV service layer bridges the RESP protocol with Raft consensus for strong consistency. All state changes go through Raft - KvService never directly accesses storage.

## User Story

As a Redis client, I want to execute GET/SET/DEL commands over TCP so that I can store and retrieve key-value data in a distributed, fault-tolerant manner using strong consistency guarantees.

## Acceptance Criteria

1. **Initialization**: TCP server binds to port 6379, Redis clients connect successfully
2. **SET command**: Parsed RespCommand::Set → RaftNode::propose() → quorum commit → StateMachine apply→ data_kv CF
3. **GET command**: Leader reads from local StateMachine, returns RESP value or null
4. **DEL command**: Each key separate Raft proposal, returns deletion count
5. **EXISTS command**: Returns count of existing keys from StateMachine
6. **PING command**: Returns PONG immediately (bypasses Raft)
7. **Leader check**: Non-leader nodes return MOVED error with leader ID
8. **Quorum failure**: Returns NOQUORUM error when majority unavailable
9. **Key size limit**: Keys > 256 bytes rejected with 'ERR key too large'
10. **Value size limit**: Values > 64KB rejected with 'ERR value too large'
11. **Invalid command**: Unknown commands return 'ERR unknown command'
12. **Timeout**: Proposal timeout returns 'ERR timeout' after 30 seconds
13. **Partial DEL**: Multi-key DEL returns count of successfully deleted keys on failure

## Business Rules

- Only Phase 1 commands: GET, SET, DEL, EXISTS, PING
- All writes (SET, DEL) go through Raft consensus for strong consistency
- Reads (GET, EXISTS) from leader only - eventual consistency (~100ms race window)
- Quorum requirement: 2 of 3 nodes must commit
- Non-leader nodes redirect with MOVED errors
- Key size limit: 256 bytes (enforced before Raft proposal)
- Value size limit: 64KB (enforced before Raft proposal)
- Failed operations return RESP error messages with context
- PING bypasses Raft (fast path)
- Request timeout: 30 seconds
- Raft RPC timeout: 5 seconds
- Multi-key DEL: Each key separate proposal (non-atomic, partial success possible)

## Scope

### Included

- TCP server on port 6379 in seshat binary
- RespCodec integration for parsing and encoding
- KvService struct for command routing and validation
- Command handlers: GET, SET, DEL, EXISTS, PING
- Key/value validation (size limits, syntax)
- RaftNode integration: propose() for writes, get() for reads
- Leader redirection with MOVED errors
- Quorum failure handling with NOQUORUM errors
- End-to-end flow: TCP → RespCodec → KvService → RaftNode → StateMachine→ RocksDB

### Excluded

- Advanced Redis commands (TTL, EXPIRE, etc.) - Phase 2
- Multi-shard clustering - Phase 2
- Stale reads from followers - Phase 4
- Dynamic cluster management - Phase 3
- Observability metrics - Phase 4
- SQL interface - Phase 5
- Redis Cluster protocol (CLUSTER commands)
- Redis pub/sub, transactions (MULTI/EXEC)
- Pipelining optimization - Phase 1 processes sequentially
- Atomic multi-key DEL - Phase 2 optimization

## Architecture

### Distributed Systems Pattern

KvService is a distributed systems component that integrates with Raft consensus for strong consistency. Unlike traditional Router → Service → Repository patterns, all state changes go through Raft.

```
Client (TCP:6379) → RespCodec → KvService → RaftNode → StateMachine
                                            ↓
                                         Raft Log→ RocksDB
                                            ↓
                                       gRPC AppendEntries→ Followers
```

### Components

| Component | File | Responsibility |
|-----------|------|---------------|
| Server | seshat/src/server.rs | TCP listener, connection handling |
| KvService | kv/src/service.rs | Command routing, validation, Raft integration |
| Handlers | kv/src/handlers.rs | Individual command implementations |
| Validation | kv/src/validation.rs | Key/value size limits |
| Error | kv/src/error.rs | KV-specific error types |

### KvService Interface

KvService owns `Arc<RaftNode>` for thread-safe access across async tasks. It is stateless - all state lives in RaftNode's StateMachine.

Key responsibilities:
- Command routing: Dispatch RespCommand to correct handler
- Input validation: Check key/value sizes before Raft proposals
- Leader validation: Check is_leader() before reads
- Error formatting: Map internal errors to RESP format

### Operation Enum

Operations serialized with protobuf before Raft proposal:

```
Operation::Set { key, value }  // Insert or update
Operation::Del { key }         // Delete key
```

## Operations

### Write Path (SET, DEL)

1. Validate key/value sizes (≤256 bytes, ≤64KB)
2. Create Operation enum
3. Serialize with protobuf: `operation.encode_to_vec()`
4. Call `raft_node.propose(data).await`
5. Wait for quorum commit
6. Return OK or error

### Read Path (GET, EXISTS)

1. Call `raft_node.is_leader().await`
2. If not leader: return MOVED error
3. Call `raft_node.get(key)` (sync, direct StateMachine access)
4. Return value or null

**Race Condition**: ~100ms window between is_leader() check and get() where leadership may change. Phase 4 will add ReadIndex for linearizable reads.

### Multi-Key DEL

Each key processed with separate Raft proposal:
1. Validate all key sizes
2. For each key: create Operation::Del, propose, wait for commit
3. Accumulate deletion count
4. On failure: stop, return partial count with error

**Non-atomic**: Partial success possible, matches Redis semantics.

### PING Command

Bypasses Raft consensus entirely:
- If message provided: return message
- Otherwise: return PONG

## Error Handling

| Error | RESP Response |
|-------|---------------|
| Not leader | `-MOVED {leader_id}\r\n` |
| No quorum | `-ERR NOQUORUM\r\n` |
| Key too large | `-ERR key too large\r\n` |
| Value too large | `-ERR value too large\r\n` |
| Unknown command | `-ERR unknown command\r\n` |
| Timeout | `-ERR timeout\r\n` |
| Storage failure | `-ERR storage failure\r\n` |

## Dependencies

### Blocking Dependencies

- **gRPC (seshat-2pz)**: Multi-node Raft communication - BLOCKING
- **RocksDB (seshat-5dw)**: Persistent storage - BLOCKING

### Required Dependencies

- **protocol-resp crate**: RespCodec, RespCommand, RespValue (100% complete)
- **seshat-storage crate**: RaftTypeConfig, Operation types
- **tokio 1.x**: Async runtime for all I/O operations

## Integration Points

### From protocol-resp crate

- `RespCodec::decode()` - Parse incoming RESP commands
- `RespCodec::encode()` - Serialize RESP responses

### From RaftNode

- `propose(operation: Vec<u8>) -> Result<ClientWriteResponse>` - **ASYNC** write path
- `get(key: &[u8]) -> Option<Vec<u8>>` - **SYNC** read from StateMachine
- `is_leader() -> bool` - **ASYNC** check leadership
- `leader_id() -> Option<u64>` - **ASYNC** get leader for MOVED errors

### From StateMachine

- `apply(entry: Entry)` - Apply committed operations to storage
- `get(key: &[u8]) -> Option<Vec<u8>>` - Read from in-memory HashMap

## Performance Requirements

| Metric | Target |
|--------|--------|
| Throughput | >5,000 ops/sec |
| GET latency (p99) | <5ms |
| SET latency (p99) | <10ms |
| Concurrent connections | 10,000 max |
| Request timeout | 30 seconds |
| Raft RPC timeout | 5 seconds |

### Optimizations

- Zero-copy parsing with bytes::Bytes
- Leader reads avoid network round-trip
- Async I/O with tokio
- Connection pooling via tokio

## Testing Strategy

- **Unit tests**: KvService handlers with mock RaftNode
- **Integration tests**: In-memory Raft cluster end-to-end
- **End-to-end tests**: redis-cli against running cluster
- **Error tests**: NOT_LEADER, NOQUORUM, size violations
- **Concurrency tests**: Multiple concurrent SET operations
- **Chaos tests**: Leader failure during SET operation
- **Performance tests**: redis-benchmark compatibility

## Future Enhancements

- **Phase 2**: Multi-shard support, cross-shard commands (MGET, MSET), batched DEL
- **Phase 3**: Dynamic membership (add/remove nodes)
- **Phase 4**: Linearizable reads via ReadIndex, follower reads, observability
- **Phase 5**: SQL interface

## Estimated Effort

**11-13 hours** across 4 phases:
- Phase 1 (Validation): 2-3 hours
- Phase 2 (Handlers): 4-5 hours
- Phase 3 (Raft Integration): 3-4 hours
- Phase 4 (TCP Server): 2-3 hours