# KV Service Specification

## User Story

As a Redis client, I want to execute GET/SET/DEL commands over TCP so that I can store and retrieve key-value data in a distributed, fault-tolerant manner using strong consistency guarantees

## Acceptance Criteria

1. GIVEN the seshat binary starts WHEN the TCP server binds to port 6379 THEN Redis clients can successfully connect and establish sessions
2. GIVEN a Redis client sends a SET command WHEN the command is parsed by RespCodec THEN KvService receives a valid RespCommand::Set with key and value
3. GIVEN KvService receives a SET command WHEN it calls RaftNode::propose() THEN the operation is replicated to a quorum of nodes and committed via Raft consensus
4. GIVEN a SET operation is committed WHEN the StateMachine applies the entry THEN the key-value pair is persisted to the data_kv column family in RocksDB
5. GIVEN a Redis client sends a GET command WHEN the command is processed THEN it reads from the local StateMachine and returns the value in RESP format (eventual consistency - reads may be stale during leadership transitions with ~100ms window between is_leader() check and actual read operation)
6. GIVEN a Redis client sends a command WHEN the node is not the leader THEN it returns a MOVED error with the leader's node ID
7. GIVEN a Redis client sends a DEL command with one or more keys WHEN each key is committed via separate Raft proposals THEN the keys are removed from storage and the total deletion count is returned (matching Redis semantics where DEL is not atomic across multiple keys)
8. GIVEN a Redis client sends a DEL command with multiple keys WHEN a quorum loss occurs mid-operation THEN partial deletions are committed and the count reflects only successfully deleted keys
9. GIVEN a Redis client sends an EXISTS command WHEN checked against storage THEN it returns the count of existing keys
10. GIVEN a Redis client sends a PING command THEN it receives a PONG response (or the echo message if provided)
11. GIVEN any operation fails due to quorum loss WHEN processed THEN the client receives a NOQUORUM error response
12. GIVEN a key exceeds 256 bytes WHEN validated THEN the client receives 'ERR key too large' error
13. GIVEN a value exceeds 64KB WHEN validated THEN the client receives 'ERR value too large' error

## Business Rules

- Only Phase 1 commands are supported: GET, SET, DEL, EXISTS, PING
- All write operations (SET, DEL) must go through Raft consensus for strong consistency
- Read operations (GET, EXISTS) use eventual consistency model - reads are served from the local StateMachine and may be stale during leadership transitions (~100ms window). Linearizable reads (via ReadIndex) are deferred to Phase 4
- Operations must achieve a majority quorum (2 out of 3 nodes) to commit
- Non-leader nodes must redirect clients to the current leader using MOVED errors
- Maximum key size is 256 bytes (enforced before Raft proposal)
- Maximum value size is 64KB (enforced before Raft proposal)
- Failed operations return appropriate RESP error messages with context
- The leader node forwards write operations through the Raft log
- Committed operations are applied to the StateMachine in log order
- PING commands bypass Raft consensus and return immediately
- Request timeout is 30 seconds (from configuration)
- Raft RPC timeout is 5 seconds (from configuration)
- **DEL multi-key operations**: Each key is processed with a separate Raft proposal (not atomic across keys), matching Redis behavior where partial success is possible
- **DEL partial failures**: If a proposal fails mid-operation, the deletion count reflects only successfully committed deletions up to that point

## Scope

### Included
- TCP server initialization in seshat main binary on port 6379
- Integration with protocol-resp crate for RESP command parsing and response encoding
- KvService struct in kv crate for command routing and validation
- Command validation (key/value size limits, command syntax)
- Integration with raft crate (RaftNode::propose() for writes, read_local() for reads)
- Error handling for NOT_LEADER scenarios (return MOVED errors with leader ID)
- Error handling for NOQUORUM scenarios (cannot reach majority)
- Error handling for invalid commands (syntax errors, size violations)
- End-to-end request flow: TCP → RespCodec → KvService → RaftNode → StateMachine → RocksDB
- Response formatting in RESP protocol
- Support for GET, SET, DEL, EXISTS, PING commands
- Leader-only read path for strong consistency
- Write path through Raft consensus with quorum requirement
- Multi-key DEL with separate Raft proposals per key (non-atomic, allows partial success)

### Excluded
- Advanced Redis commands beyond Phase 1 scope (TTL, EXPIRE, etc.)
- Multi-shard clustering (Phase 2 feature)
- Stale reads from followers (Phase 1 uses leader-only reads)
- Dynamic cluster management (adding/removing nodes during runtime - Phase 3)
- Observability metrics and tracing (Phase 4 feature)
- SQL interface (Phase 5 feature)
- Redis Cluster protocol support (CLUSTER commands)
- Redis pub/sub functionality
- Redis transactions (MULTI/EXEC)
- Redis pipelining optimization (Phase 1 processes commands sequentially)
- Atomic multi-key DEL operations (Phase 2 optimization - can add Operation::DelMulti if needed)

## Dependencies

- protocol-resp crate (100% complete - provides RespCodec, RespCommand, RespValue for parsing and encoding)
- **openraft migration (BLOCKING)** - must complete before KV service implementation begins
- raft crate (in progress - provides RaftNode wrapper with openraft async APIs, gRPC transport, StateMachine, MemStorage)
- storage crate (in progress - provides MemStorage implementation of raft::Storage trait, will migrate to RocksDB)
- common crate (provides shared types: NodeId, Error, Result, configuration types)
- tokio 1.x - async runtime for all I/O operations
- prost - protobuf serialization (replaces bincode)
- seshat binary (orchestration - needs TCP server on port 6379 that routes to KvService)

## Technical Details

### Components

- **seshat/main.rs**: Main binary that starts TCP listener on port 6379
- **kv/src/service.rs**: KvService struct with handle_command() method
- **kv/src/handlers.rs**: Individual command handlers (handle_get, handle_set, handle_del, etc.)
- **kv/src/validation.rs**: Input validation (key/value size limits)
- **kv/src/error.rs**: KV-specific error types
- **seshat/src/server.rs**: TCP server using Tokio with RespCodec framing

### Integration Points

- **RespCodec::decode()**: From protocol-resp crate for parsing incoming RESP commands
- **RespCodec::encode()**: From protocol-resp crate for serializing RESP responses
- **RaftNode::propose(operation: Vec<u8>) -> Result<ClientWriteResponse>**: **ASYNC** - For write operations (SET, DEL)
- **RaftNode::get(key: &[u8]) -> Option<Vec<u8>>**: **SYNC** - Direct StateMachine access for reads (GET, EXISTS) - **WARNING: Does NOT check leadership internally**
- **RaftNode::is_leader() -> bool**: **ASYNC** - To check leadership status before reads
- **RaftNode::leader_id() -> Option<u64>**: **ASYNC** - To get current leader for MOVED errors
- **StateMachine::apply(entry: Entry)**: Applies committed operations to storage
- **Storage::get(cf: &str, key: &[u8])**: Reads from data_kv column family
- **Storage::put(cf: &str, key: &[u8], value: &[u8])**: Writes to data_kv column family

### Error Handling

- **MOVED errors when not leader**: Return `-MOVED {leader_id}\r\n` in RESP format
- **NOQUORUM errors when cannot reach majority**: Return `-(error) NOQUORUM\r\n`
- **Key size validation**: Return `-(error) ERR key too large\r\n` if key > 256 bytes
- **Value size validation**: Return `-(error) ERR value too large\r\n` if value > 64KB
- **Invalid command syntax**: Return `-(error) ERR unknown command\r\n`
- **Raft proposal timeout**: Return `-(error) ERR timeout\r\n` after 30 seconds
- **Storage errors**: Return `-(error) ERR storage failure\r\n` for RocksDB errors
- **Connection errors**: Close TCP connection and log error
- **DEL partial failure**: Return deletion count with error context (e.g., NOQUORUM after 2 of 3 keys deleted)

### Data Flow

#### Write Path

1. Client sends 'SET foo bar' over TCP connection
2. Tokio TCP listener receives bytes
3. RespCodec::decode() parses to RespCommand::Set { key: b'foo', value: b'bar' }
4. KvService::handle_command(RespCommand::Set) called
5. KvService::validate_key_size(b'foo') - check <= 256 bytes
6. KvService::validate_value_size(b'bar') - check <= 64KB
7. KvService creates Operation::Set { key: b'foo', value: b'bar' }
8. Serialize operation with protobuf: let data = operation.encode_to_vec()
9. KvService calls raft_node.propose(data).await (async with openraft)
10. RaftNode checks leadership internally (openraft handles this)
11. openraft appends entry to local Raft log
12. openraft replicates entry to followers via gRPC (AppendEntries RPC)
13. Once majority commits, openraft calls StateMachine::apply(entry)
14. StateMachine deserializes operation from entry.data using protobuf
15. StateMachine::apply_set(key, value) calls Storage::put('data_kv', key, value)
16. RocksDB writes to data_kv column family
17. RaftNode::propose() returns Ok(ClientWriteResponse)
18. KvService returns RespValue::SimpleString('OK')
19. RespCodec::encode() serializes to '+OK\r\n'
20. Bytes sent back to client over TCP

#### Read Path

1. Client sends 'GET foo' over TCP connection
2. Tokio TCP listener receives bytes
3. RespCodec::decode() parses to RespCommand::Get { key: b'foo' }
4. KvService::handle_command(RespCommand::Get) called
5. KvService MUST check raft_node.is_leader().await (async with openraft)
6. If not leader: return NotLeader(leader_id) → format MOVED error
7. If leader: KvService calls RaftNode::get(b'foo') (sync - direct HashMap access)
8. RaftNode accesses StateMachine (in-memory HashMap in Phase 1) - NO internal leadership check
9. StateMachine::get(b'foo') returns Option<Vec<u8>>
10. If Some(value): return RespValue::BulkString(value)
11. If None: return RespValue::Null
12. RespCodec::encode() serializes to `$3\r\nbar\r\n` or `$-1\r\n`
13. Bytes sent back to client over TCP

**IMPORTANT**: There is a race condition window (~100ms) between Step 5 (is_leader() check) and Step 7 (get() call) where leadership may change. This provides eventual consistency. Phase 4 will add ReadIndex for linearizable reads.

#### Multi-Key DEL Path

1. Client sends 'DEL key1 key2 key3' over TCP connection
2. Tokio TCP listener receives bytes
3. RespCodec::decode() parses to RespCommand::Del { keys: vec![b'key1', b'key2', b'key3'] }
4. KvService::handle_command(RespCommand::Del) called
5. KvService validates all key sizes <= 256 bytes
6. deleted_count = 0
7. For each key:
   - Create Operation::Del { key }
   - Serialize operation with protobuf: operation.encode_to_vec()
   - Call RaftNode::propose(serialized_op).await (async with openraft)
   - Wait for commit
   - If success: parse result (b"1" or b"0"), add to deleted_count
   - If failure (NotLeader, NoQuorum, timeout): stop processing, return error with partial count
8. Return RespValue::Integer(deleted_count)
9. RespCodec::encode() serializes to `:N\r\n` (where N = successfully deleted keys)
10. Bytes sent back to client over TCP

### Architecture Layers

- **Layer 1 (Protocol)**: protocol-resp crate parses RESP commands and encodes responses
- **Layer 2 (Service)**: kv crate validates commands and routes to Raft
- **Layer 3 (Consensus)**: raft crate replicates writes and manages state machine
- **Layer 4 (Storage)**: storage crate persists data to RocksDB (or MemStorage in development)
- **Layer 5 (Transport)**: TCP server in seshat binary handles client connections

### Performance Considerations

- **TCP connection pooling**: Tokio handles concurrent connections efficiently
- **Zero-copy parsing**: RespCodec uses bytes::Bytes to avoid allocations
- **Async I/O**: All network operations use tokio::net::TcpListener and async/await
- **Batching**: Raft batches log entries internally for replication efficiency
- **Leader reads**: Avoid network round-trip by reading from local StateMachine
- **Connection limits**: Max 10,000 concurrent client connections (from configuration)
- **Request timeout**: 30 seconds prevents client connections from hanging indefinitely
- **Raft RPC timeout**: 5 seconds for internal node communication
- **Multi-key DEL performance**: N keys require N Raft consensus rounds (Phase 2 can optimize with batching)

### Testing Strategy

- **Unit tests**: KvService command handlers with mock RaftNode
- **Integration tests**: Full request flow with in-memory Raft cluster
- **End-to-end tests**: Redis client (redis-cli or redis-rs) against running cluster
- **Error scenario tests**: NOT_LEADER, NOQUORUM, size limit violations
- **Concurrency tests**: Multiple concurrent SET operations maintain consistency
- **Chaos tests**: Leader failure during SET operation (no data loss)
- **Performance tests**: redis-benchmark compatibility, measure ops/sec and latency
- **Multi-key DEL tests**: Verify correct counting with partial success scenarios

## Success Criteria

- redis-cli can connect to any node in the cluster on port 6379
- GET/SET/DEL/EXISTS/PING commands execute successfully with correct RESP responses
- Write operations (SET, DEL) replicate to all 3 nodes via Raft consensus
- Read operations (GET, EXISTS) return consistent values from the leader
- Non-leader nodes correctly redirect clients with MOVED errors
- Key size and value size limits are enforced with appropriate error messages
- NOQUORUM errors returned when majority of nodes are unavailable
- Cluster passes end-to-end integration test: SET on node 1 → GET from node 2 returns same value
- Cluster survives leader failure: Kill leader → new leader elected → SET/GET continue working
- Performance targets met: >5,000 ops/sec, GET <5ms p99, SET <10ms p99
- Multi-key DEL returns accurate count matching number of successfully deleted keys
- Multi-key DEL handles partial failures correctly (returns count + error for remaining keys)

## Future Enhancements

- **Phase 2**: Multi-shard support (route commands to appropriate shard based on key hash)
- **Phase 2**: Cross-shard commands (MGET, MSET)
- **Phase 2**: Batched DEL operations (Operation::DelMulti for atomic multi-key deletion)
- **Phase 3**: Dynamic membership (add/remove nodes via CLUSTER commands)
- **Phase 4**: Linearizable reads via ReadIndex (eliminates leadership race condition)
- **Phase 4**: Follower reads with bounded staleness (trade consistency for read scalability)
- **Phase 4**: Observability (OpenTelemetry metrics, distributed tracing)
- **Phase 5**: SQL interface (parallel service layer using same Raft/storage infrastructure)

## Estimated Effort

**11-13 hours** (updated from 10-12 hours to account for OpenRaft migration integration)
- 1 hour: KvService struct and basic setup
- 2 hours: Command handler implementation with async/await
- 2 hours: Validation logic and error types (including OpenRaft error mapping)
- 3 hours: Unit tests (including async test setup with #[tokio::test])
- 2 hours: Integration tests with single-node Raft cluster
- 1 hour: Property tests for boundary conditions
- 1-2 hours: seshat binary integration and end-to-end testing
- +1 hour: OpenRaft async integration (updating all RaftNode calls with .await, error mapping)

## Alignment

This feature aligns with: Phase 1 MVP goals from product vision: Enable Redis clients to execute basic commands (GET, SET, DEL, EXISTS, PING) against a distributed, fault-tolerant 3-node cluster with strong consistency guarantees via Raft consensus
