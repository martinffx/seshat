# KV Service (Condensed Spec)

## User Story

As a Redis client, I want to execute GET/SET/DEL commands over TCP so that I can store and retrieve key-value data in a distributed, fault-tolerant manner using strong consistency guarantees

## Key Acceptance Criteria

1. TCP server on port 6379 accepts Redis client connections
2. SET commands replicate via Raft consensus to quorum of nodes
3. GET commands read from leader's local StateMachine
4. Non-leader nodes return MOVED errors with leader ID
5. Key/value size limits enforced (256 bytes / 64KB)
6. NOQUORUM errors when majority unavailable

## Critical Business Rules

- Only Phase 1 commands: GET, SET, DEL, EXISTS, PING
- All writes go through Raft consensus (strong consistency)
- Leader-only reads (no stale reads in Phase 1)
- Majority quorum required (2/3 nodes)
- Max key: 256 bytes, max value: 64KB
- Request timeout: 30s, Raft RPC timeout: 5s

## Dependencies

- protocol-resp crate (100% complete - RespCodec, RespCommand, RespValue)
- raft crate (in progress - RaftNode, gRPC transport, StateMachine)
- storage crate (in progress - MemStorage, migrating to RocksDB)
- common crate (NodeId, Error, Result, config types)
- seshat binary (TCP server orchestration)

## Main Components

- **seshat/main.rs**: TCP listener on port 6379
- **kv/src/service.rs**: KvService with handle_command()
- **kv/src/handlers.rs**: Command handlers (handle_get, handle_set, etc.)
- **kv/src/validation.rs**: Size limit validation
- **seshat/src/server.rs**: Tokio TCP server with RespCodec

## Request Flow

### Write Path (SET)
```
TCP → RespCodec::decode() → KvService → validate → RaftNode::propose()
→ Raft replication → StateMachine::apply() → Storage::put() → RocksDB
→ RespCodec::encode() → TCP response
```

### Read Path (GET)
```
TCP → RespCodec::decode() → KvService → RaftNode::is_leader()
→ RaftNode::read_local() → StateMachine::get() → RespCodec::encode() → TCP response
```

## Error Handling

- **NOT_LEADER**: `-MOVED {leader_id}\r\n`
- **NOQUORUM**: `-(error) NOQUORUM\r\n`
- **Key too large**: `-(error) ERR key too large\r\n`
- **Value too large**: `-(error) ERR value too large\r\n`
- **Timeout**: `-(error) ERR timeout\r\n`

## Success Criteria

- redis-cli connects and executes GET/SET/DEL/EXISTS/PING
- Writes replicate to all 3 nodes via Raft
- Leader failure → new leader → operations continue
- Performance: >5,000 ops/sec, GET <5ms p99, SET <10ms p99
