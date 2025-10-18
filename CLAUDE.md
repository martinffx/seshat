# Seshat: Distributed Redis-Compatible Key-Value Store

## Project Overview

Seshat is a distributed, Redis-compatible key-value store built in Rust with Raft consensus for learning distributed systems fundamentals.

**Core Principle**: Every node is identical - no special roles, no separate gateway/metadata/data tiers.

## Quick Start

### Prerequisites
- **Rust 1.90+** (2021 edition)
- RocksDB (installed via cargo)
- Protobuf compiler (`protoc`)

### Local Development

**Using mise (recommended)**:
```bash
# Install dependencies
mise install

# Build the project
mise build

# Run tests
mise test

# Format code
mise format

# Lint code
mise lint

# Start 5-node cluster
mise start

# Connect to node 1
mise redis-cli
```

**Using cargo directly**:
```bash
# Build all crates
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy
```

### Current Status
**Phase 1 (MVP)**: In planning - foundational architecture defined

**Available mise tasks**: Run `mise tasks` to see all available commands

## Available Slash Commands

### Product Management
- `/product:init <project_name>`: Initialize project documentation
- `/product:roadmap [update]`: View/update development roadmap
- `/product:progress [verbose]`: Track overall product progress

### Feature Development (Spec-Driven Development)
- `/spec:create <feature_name>`: Create feature specification
- `/spec:design <feature_name>`: Generate technical design
- `/spec:plan <feature_name>`: Generate implementation tasks
- `/spec:implement <feature_name> [task_id]`: Implement feature with TDD
- `/spec:progress <feature_name>`: Update task progress

### Quality Assurance
- `/validate`: Run build, lint, and test pipeline
- `/commit`: Create well-crafted git commit
- `/review [branch]`: Comprehensive code review

## Documentation Structure

```
docs/
├── architecture/         # System architecture and design
│   ├── crates.md        # Crate responsibilities and interactions
│   ├── data-structures.md  # Core types and serialization
│   └── startup.md       # Bootstrap and join modes
├── product/             # Product definition and roadmap
│   ├── product.md       # Features, users, success metrics
│   └── roadmap.md       # 4-phase development plan
├── standards/           # Technical standards and practices
│   ├── tech.md         # Tech stack, architecture, storage
│   ├── practices.md    # TDD workflow, chaos testing
│   └── style.md        # Code style and conventions
└── specs/              # Feature specifications
    └── README.md       # Specification workflow guide
```

### Key Documentation

- **Start here**: `docs/product/product.md` - Product definition and Phase 1 goals
- **Architecture**: `docs/architecture/crates.md` - How the 5 crates fit together
- **Data design**: `docs/architecture/data-structures.md` - Core types with examples
- **Tech decisions**: `docs/standards/tech.md` - Storage, networking, observability
- **Development**: `docs/standards/practices.md` - TDD workflow and 11 chaos tests

## Architecture Overview

### Tech Stack
- **Rust 1.90+** (2021 edition)
- **Consensus**: raft-rs (wraps with custom storage)
- **Storage**: RocksDB with 6 column families
- **Client Protocol**: Redis RESP2 on port 6379
- **Internal RPC**: gRPC (tonic) on port 7379
- **Observability**: OpenTelemetry + Prometheus (Phase 4)

### Crate Structure
```
seshat/          - Main binary, orchestration
raft/            - Raft consensus wrapper
storage/         - RocksDB persistence layer
protocol-resp/   - RESP and gRPC protocols
common/          - Shared types and utilities
```

### Phase 1 Features (MVP)
- ✅ Raft consensus with leader election
- ✅ Redis commands: GET, SET, DEL, EXISTS, PING
- ✅ RocksDB persistent storage (6 column families)
- ✅ gRPC internal communication
- ✅ Log compaction and snapshots
- ✅ 3-node cluster with fault tolerance

### Success Criteria
- 3-node cluster runs stably for 1+ hours
- **Pass all 11 chaos tests** (network partitions, failures, etc.)
- Performance: >5,000 ops/sec, <10ms p99 latency

## Development Workflow

### Spec-Driven Development (SDD)

1. **Create spec**: `/spec:create raft-consensus`
   - Analyst agent gathers requirements

2. **Design architecture**: `/spec:design raft-consensus`
   - Architect agent creates technical design

3. **Generate tasks**: `/spec:plan raft-consensus`
   - Break into dependency-ordered tasks

4. **Implement with TDD**: `/spec:implement raft-consensus`
   - Coder agent follows: Test → Code → Refactor

5. **Track progress**: `/spec:progress raft-consensus`
   - Update completion status

### Testing Strategy
- **Unit tests**: Per-crate functionality
- **Integration tests**: Full request flows
- **Property tests**: Edge cases with `proptest`
- **Chaos tests**: 11 scenarios (partitions, failures, restarts)
- **Performance tests**: `redis-benchmark` compatibility

## Learning Objectives

This project focuses on **deep understanding** of:
- Raft consensus in practice (not just theory)
- Trade-offs in distributed systems design
- Network partitions and split-brain scenarios
- Log compaction and snapshot strategies
- Where distributed systems complexity actually lives

## Future Roadmap

- **Phase 2**: Multi-shard cluster (horizontal scalability)
- **Phase 3**: Dynamic cluster management (add/remove nodes)
- **Phase 4**: Production readiness (metrics, monitoring, upgrades)
- **Phase 5**: SQL interface (PostgreSQL wire protocol)

## Next Steps

Start implementing your first feature:
```bash
/spec:create storage-layer    # RocksDB column family implementation
# OR
/spec:create resp-protocol    # Redis protocol parser
# OR
/spec:create raft-integration # Wrap raft-rs with storage
```

## Contributing

1. Read `docs/standards/practices.md` for TDD workflow
2. Follow layered architecture (Protocol → Service → Raft → Storage)
3. All features must pass chaos tests
4. Document architectural decisions in `docs/architecture/`