# Data Flow Architecture

This document illustrates how data flows through Seshat from network to disk and back.

## Table of Contents

- [High-Level Architecture](#high-level-architecture)
- [Write Path (SET command)](#write-path-set-command)
- [Read Path (GET command)](#read-path-get-command)
- [Cluster Replication](#cluster-replication)
- [Storage Layer Details](#storage-layer-details)

## High-Level Architecture

```mermaid
graph TD
    Client[Redis Client<br/>redis-cli] -->|TCP :6379<br/>RESP2| Protocol

    subgraph "protocol/ crate"
        Protocol[RESP Parser/<br/>Serializer]
    end

    Protocol --> KVService

    subgraph "seshat/ crate"
        KVService[KVService<br/>Business Logic]
    end

    KVService --> Raft

    subgraph "raft/ crate"
        Raft[Raft Consensus<br/>Leader Election<br/>Log Replication]
    end

    Raft -->|gRPC :7379| Peer1[Peer Node 1]
    Raft -->|gRPC :7379| Peer2[Peer Node 2]

    Raft --> Storage

    subgraph "storage/ crate"
        Storage[RocksDB Storage]
        Storage --> CF1[kv_data CF]
        Storage --> CF2[raft_log CF]
        Storage --> CF3[raft_state CF]
        Storage --> CF4[snapshots CF]
        Storage --> CF5[metadata CF]
        Storage --> CF6[tombstones CF]
    end

    CF1 --> Disk[(Disk)]
    CF2 --> Disk
    CF3 --> Disk
    CF4 --> Disk
    CF5 --> Disk
    CF6 --> Disk

    style Client fill:#e1f5ff
    style Protocol fill:#fff3e0
    style KVService fill:#f3e5f5
    style Raft fill:#e8f5e9
    style Storage fill:#fce4ec
    style Disk fill:#333,color:#fff
```

## Write Path (SET command)

```mermaid
sequenceDiagram
    participant C as Client
    participant P as Protocol<br/>(RESP)
    participant K as KVService
    participant R as Raft
    participant S as Storage<br/>(RocksDB)
    participant N as Other Nodes

    C->>P: SET foo "bar"<br/>(TCP :6379)
    activate P
    P->>P: Parse RESP2
    P->>K: Command::Set{key, value}
    deactivate P

    activate K
    K->>K: Validate command
    K->>R: propose(Set{foo, bar})
    deactivate K

    activate R
    R->>S: append_entry(raft_log)
    activate S
    S->>S: Write to raft_log CF
    S-->>R: Ok
    deactivate S

    R->>N: AppendEntries RPC<br/>(gRPC :7379)
    activate N
    N-->>R: Success (majority)
    deactivate N

    R->>R: Commit log entry
    R->>S: apply(Set{foo, bar})
    activate S
    S->>S: Write to kv_data CF
    S->>S: fsync()
    S-->>R: Applied
    deactivate S

    R-->>K: Success
    deactivate R

    activate K
    K-->>P: Response::Ok
    deactivate K

    activate P
    P->>P: Serialize RESP2
    P-->>C: +OK\r\n
    deactivate P
```

## Read Path (GET command)

```mermaid
sequenceDiagram
    participant C as Client
    participant P as Protocol<br/>(RESP)
    participant K as KVService
    participant R as Raft
    participant S as Storage<br/>(RocksDB)

    C->>P: GET foo<br/>(TCP :6379)
    activate P
    P->>P: Parse RESP2
    P->>K: Command::Get{key}
    deactivate P

    activate K
    K->>R: read(foo)
    deactivate K

    activate R
    R->>R: Check if leader
    R->>S: get(kv_data, "foo")
    activate S
    S->>S: Read from kv_data CF
    S-->>R: Some(b"bar")
    deactivate S

    R-->>K: Some(value)
    deactivate R

    activate K
    K-->>P: Response::Value("bar")
    deactivate K

    activate P
    P->>P: Serialize RESP2
    P-->>C: $3\r\nbar\r\n
    deactivate P
```

## Cluster Replication

```mermaid
graph LR
    subgraph "Node 1 (Leader)"
        L1[Raft Leader]
        LS1[(RocksDB)]
        L1 --> LS1
    end

    subgraph "Node 2 (Follower)"
        F1[Raft Follower]
        FS1[(RocksDB)]
        F1 --> FS1
    end

    subgraph "Node 3 (Follower)"
        F2[Raft Follower]
        FS2[(RocksDB)]
        F2 --> FS2
    end

    L1 -->|1. AppendEntries<br/>gRPC :7379| F1
    L1 -->|1. AppendEntries<br/>gRPC :7379| F2

    F1 -.->|2. ACK| L1
    F2 -.->|2. ACK| L1

    L1 -->|3. Commit<br/>(after majority)| LS1
    F1 -->|3. Apply| FS1
    F2 -->|3. Apply| FS2

    style L1 fill:#4caf50,color:#fff
    style F1 fill:#2196f3,color:#fff
    style F2 fill:#2196f3,color:#fff
```

## Storage Layer Details

```mermaid
graph TB
    subgraph "RocksDB Storage Engine"
        direction TB

        subgraph "Application Data"
            KV[kv_data CF<br/>Key-Value Pairs<br/>key → value]
            Tomb[tombstones CF<br/>Deleted Keys<br/>key → timestamp]
        end

        subgraph "Raft Consensus"
            Log[raft_log CF<br/>Replicated Log<br/>index → entry]
            State[raft_state CF<br/>Persistent State<br/>term, voted_for]
            Snap[snapshots CF<br/>Compacted State<br/>index → snapshot]
        end

        subgraph "Cluster Management"
            Meta[metadata CF<br/>Cluster Config<br/>node_id, peers]
        end
    end

    KV -.->|Compaction| Snap
    Log -.->|Truncation| Snap
    Tomb -.->|GC after TTL| KV

    style KV fill:#e3f2fd
    style Tomb fill:#f3e5f5
    style Log fill:#fff3e0
    style State fill:#fff3e0
    style Snap fill:#fff3e0
    style Meta fill:#e8f5e9
```

## KV-to-Raft Interface

The interface between the key-value layer and Raft consensus is defined through the `Operation` type and the `RaftNode` API.

```mermaid
graph TB
    subgraph "Protocol Layer (seshat_protocol)"
        Op[Operation enum<br/>• Set {key, value}<br/>• Del {key}]
        Ser[serialize: Operation → Vec<u8>]
        Deser[deserialize: Vec<u8> → Operation]
        Apply[apply: HashMap → Result]
    end

    subgraph "Raft Layer (seshat_raft)"
        RN[RaftNode]
        Propose[propose Vec<u8>]
        Ready[handle_ready]
        SM[StateMachine]
        SMApply[apply index, data]
        Get[get key]
    end

    subgraph "Application Layer (seshat)"
        KV[KVService]
    end

    KV -->|"1. Create Operation"| Op
    Op -->|"2. Serialize"| Ser
    Ser -->|"3. propose data"| Propose
    Propose --> RN

    RN -->|"4. Replicate to majority"| Ready
    Ready -->|"5. Committed entries"| SMApply
    SMApply -->|"6. Deserialize"| Deser
    Deser -->|"7. Execute"| Apply
    Apply --> SM

    KV -->|"Read: get key"| Get
    Get --> SM

    style Op fill:#e3f2fd
    style RN fill:#e8f5e9
    style SM fill:#fce4ec
    style KV fill:#f3e5f5
```

### Key Interfaces

#### 1. Operation API (protocol crate)

```rust
pub enum Operation {
    Set { key: Vec<u8>, value: Vec<u8> },
    Del { key: Vec<u8> },
}

impl Operation {
    // Serialize to bytes for Raft log
    pub fn serialize(&self) -> Result<Vec<u8>>;

    // Deserialize from Raft log entry
    pub fn deserialize(bytes: &[u8]) -> Result<Operation>;

    // Apply to state HashMap
    pub fn apply(&self, state: &mut HashMap) -> Result<Vec<u8>>;
}
```

#### 2. RaftNode API (raft crate)

```rust
pub struct RaftNode {
    id: u64,
    raw_node: RawNode<MemStorage>,
    state_machine: StateMachine,
}

impl RaftNode {
    // Propose a command for consensus (writes)
    pub fn propose(&mut self, data: Vec<u8>) -> Result<()>;

    // Process Raft ready state (drive consensus)
    pub fn handle_ready(&mut self) -> Result<Vec<Message>>;

    // Read from state machine (reads)
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>>;

    // Check leadership (route requests)
    pub fn is_leader(&self) -> bool;
    pub fn leader_id(&self) -> Option<u64>;

    // Drive Raft timing
    pub fn tick(&mut self) -> Result<()>;
}
```

#### 3. StateMachine API (raft crate)

```rust
pub struct StateMachine {
    data: HashMap<Vec<u8>, Vec<u8>>,
    last_applied: u64,
}

impl StateMachine {
    // Apply committed log entry
    pub fn apply(&mut self, index: u64, data: &[u8]) -> Result<Vec<u8>>;

    // Read current state
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    pub fn exists(&self, key: &[u8]) -> bool;

    // Snapshots for log compaction
    pub fn snapshot(&self) -> Result<Vec<u8>>;
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<()>;

    // Progress tracking
    pub fn last_applied(&self) -> u64;
}
```

### Write Path: SET Command

```mermaid
sequenceDiagram
    participant KV as KVService
    participant Op as Operation
    participant RN as RaftNode
    participant SM as StateMachine
    participant HM as HashMap

    KV->>Op: Create Set{key, value}
    Op->>Op: serialize() → Vec<u8>

    KV->>RN: propose(serialized_data)
    Note over RN: Only succeeds if leader

    RN->>RN: raw_node.propose(data)
    Note over RN: Added to Raft log

    RN->>RN: handle_ready()
    Note over RN: Replicate & commit

    RN->>SM: apply(index, data)

    SM->>Op: deserialize(data)
    Op-->>SM: Operation::Set

    SM->>Op: apply(&mut HashMap)
    Op->>HM: insert(key, value)
    HM-->>Op: ()
    Op-->>SM: Ok(b"OK")

    SM->>SM: last_applied = index
    SM-->>RN: Ok(b"OK")
    RN-->>KV: Success
```

### Read Path: GET Command

```mermaid
sequenceDiagram
    participant KV as KVService
    participant RN as RaftNode
    participant SM as StateMachine
    participant HM as HashMap

    KV->>RN: is_leader()
    RN-->>KV: true

    KV->>RN: get(key)
    RN->>SM: get(key)
    SM->>HM: get(key)
    HM-->>SM: Some(value)
    SM-->>RN: Some(value)
    RN-->>KV: Some(value)
```

### Data Transformations

```mermaid
graph LR
    subgraph "Client Request"
        CR[Redis RESP<br/>SET foo bar]
    end

    subgraph "Protocol Parsing"
        CMD[Command::Set<br/>{key: foo, value: bar}]
    end

    subgraph "Operation Creation"
        OP[Operation::Set<br/>{key: [102,111,111], value: [98,97,114]}]
    end

    subgraph "Serialization"
        BYTES[Vec<u8><br/>[0,3,102,111,111,3,98,97,114]]
    end

    subgraph "Raft Log Entry"
        ENTRY[Entry<br/>{index: 5, data: bytes}]
    end

    subgraph "State Machine"
        HM[HashMap<br/>foo → bar]
    end

    subgraph "Client Response"
        RESP[Redis RESP<br/>+OK\r\n]
    end

    CR --> CMD
    CMD --> OP
    OP --> BYTES
    BYTES --> ENTRY
    ENTRY -.Commit & Apply.-> BYTES
    BYTES --> OP
    OP --> HM
    HM --> RESP
```

### Interface Contract

**KVService responsibilities:**
- Parse client commands into `Operation` types
- Call `propose()` for writes (returns error if not leader)
- Call `get()` for reads (leader serves from local state)
- Handle leadership changes (redirect to current leader)
- Serialize responses back to client protocol

**RaftNode responsibilities:**
- Accept proposals via `propose()` (leader only)
- Replicate entries to majority via `handle_ready()`
- Apply committed entries to `StateMachine`
- Track leadership status for request routing
- Provide read access via `get()` (linearizable on leader)

**StateMachine responsibilities:**
- Deserialize `Operation` from log entry data
- Execute operations on internal `HashMap`
- Enforce idempotency (reject duplicate indexes)
- Track `last_applied` index for snapshots
- Provide snapshot/restore for log compaction

### Error Handling

```mermaid
graph TD
    KV[KVService receives SET]

    KV --> Check{Is Leader?}
    Check -->|No| Redirect[Return: MOVED leader_id]
    Check -->|Yes| Propose[propose data]

    Propose --> PropResult{Result?}
    PropResult -->|Err| PropFail[Return: ERR not leader]
    PropResult -->|Ok| Ready[handle_ready]

    Ready --> Commit{Committed?}
    Commit -->|No| Wait[Wait for next ready]
    Commit -->|Yes| Apply[apply to StateMachine]

    Apply --> ApplyResult{Result?}
    ApplyResult -->|Ok| Success[Return: +OK]
    ApplyResult -->|Err| Fail[Return: ERR message]

    Wait --> Ready

    style Redirect fill:#ffebee
    style PropFail fill:#ffebee
    style Fail fill:#ffebee
    style Success fill:#e8f5e9
```

## Data Flow Summary

### Write Path Layers

1. **Network → Protocol** (TCP :6379)
   - RESP2 parsing
   - Command deserialization

2. **Protocol → KVService** (in-process)
   - Command validation
   - Business logic

3. **KVService → Raft** (in-process)
   - Consensus proposal
   - Leader election check

4. **Raft → Storage** (in-process)
   - Log append (raft_log CF)
   - State machine apply (kv_data CF)

5. **Raft → Peers** (gRPC :7379)
   - AppendEntries RPC
   - Replication to followers

6. **Storage → Disk** (RocksDB)
   - Write-ahead log (WAL)
   - SSTable compaction
   - fsync for durability

### Read Path Layers

1. **Network → Protocol** (TCP :6379)
   - RESP2 parsing

2. **Protocol → KVService** (in-process)
   - Command routing

3. **KVService → Raft** (in-process)
   - Leadership check
   - Read-index for linearizability (optional)

4. **Raft → Storage** (in-process)
   - Read from kv_data CF

5. **Storage → Disk** (RocksDB)
   - Block cache lookup
   - SSTable read if cache miss

6. **Response path reverses up the stack**

## Performance Considerations

### Write Latency Components

- **Network parsing**: ~0.1ms (RESP2 is simple)
- **Raft append**: ~0.5ms (WAL write)
- **Network replication**: ~1-2ms (gRPC + network RTT)
- **State machine apply**: ~0.5ms (RocksDB write)
- **Total**: ~2-3ms typical, ~10ms p99

### Read Latency Components

- **Network parsing**: ~0.1ms
- **RocksDB read**: ~0.1ms (cache hit), ~1ms (SSD seek)
- **Total**: ~0.2ms typical (cached), ~1-2ms (disk)

### Optimization Opportunities

1. **Batch writes**: Group multiple commands into single Raft proposal
2. **Read cache**: In-memory LRU for hot keys
3. **Follower reads**: Stale reads from followers (eventual consistency)
4. **Pipeline**: Async RESP2 pipelining for throughput
