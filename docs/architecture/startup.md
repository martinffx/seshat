# Node Startup and Cluster Formation

This document describes how Seshat nodes start up, form clusters, and join existing clusters.

## Two Startup Modes

Seshat supports two distinct startup modes:

1. **Bootstrap Mode**: Initialize a new cluster with founding members
2. **Join Mode**: Add a node to an existing cluster

---

## Bootstrap Mode: Forming a New Cluster

**When to use**: First time starting a cluster with no existing nodes.

**Requirements**:
- All founding nodes start with identical configuration
- Minimum 3 nodes for fault tolerance (required for majority quorum)
- All nodes must be reachable via network (DNS or IP)

### Configuration Example

All 3 founding nodes use the same configuration (except `id` and `internal_addr`):

```toml
[node]
id = 1  # Different for each node: 1, 2, 3
client_addr = "0.0.0.0:6379"      # Bind address for Redis clients
internal_addr = "0.0.0.0:7379"    # Bind address for internal Raft RPC
data_dir = "/var/lib/seshat"
# advertise_addr is auto-detected from hostname, or set explicitly:
# advertise_addr = "kvstore-1:7379"

[cluster]
bootstrap = true  # Enable bootstrap mode

# All founding members (same list on all nodes)
initial_members = [
  {id = 1, addr = "kvstore-1:7379"},
  {id = 2, addr = "kvstore-2:7379"},
  {id = 3, addr = "kvstore-3:7379"},
]

replication_factor = 3

[raft]
heartbeat_interval_ms = 100
election_timeout_min_ms = 500
election_timeout_max_ms = 1000
snapshot_interval_entries = 10000
```

### Bootstrap Process

```
Phase 1: Validation and Initialization
┌──────────────────────────────────────┐
│ Node 1 starts                        │
│ - Loads config                       │
│ - Validates bootstrap config         │
│ - Creates empty RocksDB              │
│ - Detects own address (hostname)     │
└──────────────────────────────────────┘

Phase 2: Raft Group Creation
┌──────────────────────────────────────┐
│ Create System Raft Group             │
│ - Initialize with 3 members          │
│ - Members: [1, 2, 3]                 │
│ - State: Follower                    │
└──────────────────────────────────────┘

Phase 3: Network Discovery
┌──────────────────────────────────────┐
│ Resolve DNS names                    │
│ - kvstore-1 → 10.0.0.1:7379         │
│ - kvstore-2 → 10.0.0.2:7379         │
│ - kvstore-3 → 10.0.0.3:7379         │
└──────────────────────────────────────┘

Phase 4: Leader Election
┌──────────────────────────────────────┐
│ Nodes exchange RequestVote RPCs      │
│ - One node becomes leader (~2s)      │
│ - Others remain followers            │
└──────────────────────────────────────┘

Phase 5: Initial Metadata
┌──────────────────────────────────────┐
│ Leader proposes initial metadata     │
│ - ClusterMembership (3 nodes)        │
│ - ShardMap (1 shard, 3 replicas)     │
│ - Replicate via Raft                 │
└──────────────────────────────────────┘

Phase 6: Ready to Serve
┌──────────────────────────────────────┐
│ Cluster formed! Accept client reqs   │
│ - /health returns 200                │
│ - /ready returns 200                 │
│ - Redis clients can connect          │
└──────────────────────────────────────┘
```

**Timeline**: Typically completes in 2-5 seconds.

### Configuration Validation

On bootstrap, the node validates:

```rust
fn validate_bootstrap_config(config: &Config) -> Result<()> {
    // Must have bootstrap enabled
    ensure!(config.cluster.bootstrap, "bootstrap must be true");

    // Must have at least 3 initial members
    ensure!(
        config.cluster.initial_members.len() >= 3,
        "Need at least 3 nodes for fault tolerance"
    );

    // This node must be in initial_members
    ensure!(
        config.cluster.initial_members.iter()
            .any(|m| m.id == config.node.id),
        "This node (id={}) must be in initial_members",
        config.node.id
    );

    // No duplicate node IDs
    let mut seen = HashSet::new();
    for member in &config.cluster.initial_members {
        ensure!(
            seen.insert(member.id),
            "Duplicate node ID: {}",
            member.id
        );
    }

    // Replication factor must be odd
    ensure!(
        config.cluster.replication_factor % 2 == 1,
        "replication_factor must be odd (3, 5, 7, etc.)"
    );

    // DNS resolution check (optional but recommended)
    for member in &config.cluster.initial_members {
        match tokio::net::lookup_host(&member.addr).await {
            Ok(_) => {},
            Err(e) => warn!("Cannot resolve {}: {}", member.addr, e),
        }
    }

    Ok(())
}
```

### Docker Compose Example

```yaml
version: '3.8'

services:
  kvstore-1:
    image: seshat:latest
    hostname: kvstore-1  # Sets container hostname
    volumes:
      - ./data1:/var/lib/seshat
    ports:
      - "6379:6379"  # Redis clients
      - "7379:7379"  # Internal RPC
    environment:
      - NODE_ID=1
      - BOOTSTRAP=true
      - INITIAL_MEMBERS=kvstore-1:7379,kvstore-2:7379,kvstore-3:7379

  kvstore-2:
    image: seshat:latest
    hostname: kvstore-2
    volumes:
      - ./data2:/var/lib/seshat
    ports:
      - "6380:6379"
      - "7380:7379"
    environment:
      - NODE_ID=2
      - BOOTSTRAP=true
      - INITIAL_MEMBERS=kvstore-1:7379,kvstore-2:7379,kvstore-3:7379

  kvstore-3:
    image: seshat:latest
    hostname: kvstore-3
    volumes:
      - ./data3:/var/lib/seshat
    ports:
      - "6381:6379"
      - "7381:7379"
    environment:
      - NODE_ID=3
      - BOOTSTRAP=true
      - INITIAL_MEMBERS=kvstore-1:7379,kvstore-2:7379,kvstore-3:7379
```

**Start cluster**: `docker-compose up -d`

**Check health**:
```bash
redis-cli -h localhost -p 6379 PING
redis-cli -h localhost -p 6380 PING
redis-cli -h localhost -p 6381 PING
```

---

## Join Mode: Adding a Node to Existing Cluster

**When to use**: Adding a node to a running cluster (Phase 3+).

**Requirements**:
- At least one existing node's address
- Unique node ID not already in cluster
- Network connectivity to existing cluster

### Configuration Example

```toml
[node]
id = 4  # Must be unique
client_addr = "0.0.0.0:6379"
internal_addr = "0.0.0.0:7379"
data_dir = "/var/lib/seshat"
# advertise_addr auto-detected: "kvstore-4:7379"

[cluster]
bootstrap = false  # Explicitly disable bootstrap

# Address of any existing cluster member
join_addr = "kvstore-1:7379"

replication_factor = 3

[raft]
# Same as existing cluster
heartbeat_interval_ms = 100
election_timeout_min_ms = 500
election_timeout_max_ms = 1000
```

### Join Process

```
Phase 1: Connect to Cluster
┌──────────────────────────────────────┐
│ Node 4 starts                        │
│ - Loads config                       │
│ - Validates join config              │
│ - Resolves kvstore-1:7379            │
│ - Connects via gRPC                  │
└──────────────────────────────────────┘

Phase 2: Send Join Request
┌──────────────────────────────────────┐
│ Send JoinRequest to kvstore-1        │
│ - id: 4                              │
│ - addr: "kvstore-4:7379"             │
└──────────────────────────────────────┘

Phase 3: Leader Routing
┌──────────────────────────────────────┐
│ kvstore-1 checks if it's leader      │
│ - If yes: process request            │
│ - If no: redirect to current leader  │
└──────────────────────────────────────┘

Phase 4: Membership Change Proposal
┌──────────────────────────────────────┐
│ Leader proposes membership change    │
│ - Add node 4 to ClusterMembership    │
│ - Raft replicates to majority        │
│ - Once committed, add to Raft group  │
└──────────────────────────────────────┘

Phase 5: State Transfer
┌──────────────────────────────────────┐
│ Node 4 receives JoinResponse         │
│ - ClusterMembership (now includes 4) │
│ - ShardMap with assignments          │
│ - Latest snapshot (if needed)        │
└──────────────────────────────────────┘

Phase 6: Catch Up
┌──────────────────────────────────────┐
│ Node 4 applies snapshot              │
│ - Load data from snapshot            │
│ - Replay log entries after snapshot  │
│ - Join assigned Raft groups          │
└──────────────────────────────────────┘

Phase 7: Ready to Serve
┌──────────────────────────────────────┐
│ Node 4 is now part of cluster        │
│ - /health returns 200                │
│ - /ready returns 200 (after catch-up)│
│ - Can serve client requests          │
└──────────────────────────────────────┘
```

**Timeline**:
- Fast path (small cluster, no snapshot): 5-10 seconds
- Slow path (large dataset, snapshot transfer): 10-30 minutes

### Join Request/Response Protocol

```rust
// Internal gRPC message
#[derive(Debug, Clone)]
pub struct JoinRequest {
    /// New node's unique ID
    pub node_id: u64,

    /// New node's advertised address (e.g., "kvstore-4:7379")
    pub addr: String,
}

#[derive(Debug, Clone)]
pub struct JoinResponse {
    /// Success or error
    pub status: JoinStatus,

    /// Current cluster membership (all nodes)
    pub membership: ClusterMembership,

    /// Shard assignments
    pub shard_map: ShardMap,

    /// Snapshot if needed (large clusters)
    pub snapshot: Option<SnapshotMetadata>,
}

#[derive(Debug, Clone, Copy)]
pub enum JoinStatus {
    /// Join accepted, node is now part of cluster
    Accepted,

    /// Redirect to current leader
    NotLeader { leader_id: u64 },

    /// Node ID already in use
    DuplicateNodeId,

    /// Cluster is not ready (e.g., no quorum)
    ClusterNotReady,
}
```

### Configuration Validation

```rust
fn validate_join_config(config: &Config) -> Result<()> {
    // Must have join_addr
    ensure!(
        config.cluster.join_addr.is_some(),
        "join_addr required in join mode"
    );

    // Must NOT have bootstrap enabled
    ensure!(
        !config.cluster.bootstrap,
        "Cannot have bootstrap=true in join mode"
    );

    // Node ID must be > 0
    ensure!(config.node.id > 0, "node_id must be positive");

    // Resolve join_addr
    let join_addr = config.cluster.join_addr.as_ref().unwrap();
    tokio::net::lookup_host(join_addr).await
        .context("Cannot resolve join_addr")?;

    Ok(())
}
```

### Command Line Example

```bash
# Start new node joining existing cluster
./seshat \
  --id=4 \
  --client-addr=0.0.0.0:6379 \
  --internal-addr=0.0.0.0:7379 \
  --data-dir=/var/lib/seshat \
  --join=kvstore-1:7379

# Or using config file
./seshat --config=/etc/seshat.toml
```

---

## Address Discovery and Announcement

When a node starts, it needs to know its own address to announce to the cluster.

### Option 1: Auto-detect from Hostname (Recommended)

**Kubernetes/Docker**: Pod/container hostname is set automatically.

```rust
use std::process::Command;

fn get_my_advertise_addr(internal_port: u16) -> String {
    // Read system hostname
    let hostname = Command::new("hostname")
        .output()
        .expect("Failed to get hostname")
        .stdout;

    let hostname = String::from_utf8(hostname)
        .unwrap()
        .trim()
        .to_string();

    format!("{}:{}", hostname, internal_port)
}

// Kubernetes StatefulSet example:
// - Pod name: kvstore-2
// - Hostname: kvstore-2
// - Advertise address: "kvstore-2:7379"
```

### Option 2: Environment Variable (Explicit)

```yaml
# Kubernetes
env:
- name: POD_NAME
  valueFrom:
    fieldRef:
      fieldPath: metadata.name  # "kvstore-2"

- name: ADVERTISE_ADDR
  value: "$(POD_NAME):7379"
```

```rust
fn get_my_advertise_addr() -> String {
    std::env::var("ADVERTISE_ADDR")
        .or_else(|_| {
            let pod_name = std::env::var("POD_NAME")?;
            let port = std::env::var("INTERNAL_PORT")?;
            Ok(format!("{}:{}", pod_name, port))
        })
        .expect("Cannot determine advertise address")
}
```

### Option 3: Explicit Configuration

```toml
[node]
id = 2
internal_addr = "0.0.0.0:7379"  # Bind address
advertise_addr = "kvstore-2:7379"  # Explicitly set
```

**Recommendation**: Use Option 1 (auto-detect) for production, Option 3 for development simplicity.

---

## Restart Behavior

### Normal Restart

When a node restarts:

```
1. Load configuration (same as before)
2. Open RocksDB at data_dir
3. Read RaftHardState from *_raft_state CFs
4. Restore term, vote, commit index
5. Load latest snapshot (if exists)
6. Replay log entries after snapshot
7. Rejoin Raft groups
8. Resume normal operation
```

**Duration**: 10-30 seconds depending on log size.

### Restart After Crash

Same as normal restart, but:
- RocksDB recovery may take longer (WAL replay)
- Raft catch-up may be needed (download missing entries from leader)

---

## Error Scenarios and Recovery

### Bootstrap Failures

**Scenario 1: Only 1-2 nodes start**
```
Error: Cannot form quorum (need 2/3 nodes)
Action: Wait for 3rd node to start
Timeout: 30 seconds, then exit
```

**Scenario 2: DNS resolution fails**
```
Error: Cannot resolve kvstore-2:7379
Action: Retry with exponential backoff (1s, 2s, 4s, 8s)
Timeout: 30 seconds, then exit with error
```

**Scenario 3: Network partition during bootstrap**
```
Error: Nodes cannot reach each other
Action: Election timeout, retry, eventually exit
Resolution: Fix network, restart nodes
```

### Join Failures

**Scenario 1: Cluster unreachable**
```
Error: Cannot connect to kvstore-1:7379
Action: Retry 5 times with 2s delay
Fallback: Try other known nodes (if configured)
```

**Scenario 2: Node ID already in use**
```
Error: JoinResponse::DuplicateNodeId
Action: Exit with error
Resolution: Choose different node ID
```

**Scenario 3: Snapshot transfer timeout**
```
Error: Timeout downloading 50GB snapshot
Action: Resume transfer from last checkpoint
Retry: Up to 3 attempts
```

---

## Health Checks During Startup

Expose status endpoints:

```bash
# GET /health
# - 503 during startup (not ready)
# - 200 once healthy

# GET /ready
# - 503 during startup or catch-up
# - 200 once ready to serve traffic

# Check from load balancer
curl http://kvstore-1:8080/ready
# 200 OK → Route traffic to this node
# 503 Service Unavailable → Don't route yet
```

---

## Operational Recommendations

### Development

Use bootstrap mode with Docker Compose:
```bash
docker-compose up -d
# All 3 nodes start together, cluster forms automatically
```

### Production

1. **Initial deployment**: Bootstrap with 3 nodes
2. **Scale up**: Add nodes via join mode
3. **Upgrades**: Rolling restart (nodes rejoin automatically)
4. **Disaster recovery**: Restore from backup, bootstrap new cluster

### Kubernetes StatefulSet

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: seshat
spec:
  replicas: 3
  serviceName: seshat
  template:
    spec:
      containers:
      - name: seshat
        image: seshat:latest
        env:
        - name: POD_NAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: NODE_ID
          # Extract ordinal: seshat-0 → 0, seshat-1 → 1
          value: "$(POD_NAME##*-)"
        - name: BOOTSTRAP
          value: "true"  # For initial deployment
        - name: INITIAL_MEMBERS
          value: "seshat-0.seshat:7379,seshat-1.seshat:7379,seshat-2.seshat:7379"
```

**Scale up**:
```bash
kubectl scale statefulset seshat --replicas=5
# New pods seshat-3 and seshat-4 automatically join
```

---

This startup design ensures:
- **Simplicity**: Two clear modes (bootstrap vs join)
- **Safety**: Extensive validation prevents misconfigurations
- **Flexibility**: Works with Docker, Kubernetes, or bare metal
- **Observability**: Health checks expose startup status
- **Resilience**: Automatic retry and recovery mechanisms
