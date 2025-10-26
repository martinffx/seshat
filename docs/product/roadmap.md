# Seshat Project Roadmap

## Overview
Our roadmap is structured into progressive phases. **IMPORTANT**: Phase 1 has been revised to reflect the OpenRaft migration dependency discovered during technical review.

## Phase 1: Single Shard Cluster (MVP) - REVISED

### Current Status
- **True Progress**: ~15-20% complete (1 of 5-6 major components)
- **Timeline**: 2-3 weeks remaining (vs original 1-2 weeks)
- **Total Effort**: 56-71 hours remaining

### Phase 1A: Foundation Migration (NEW - 2 weeks)
**Status**: ⏳ Not Started
**Priority**: BLOCKING - Must complete before other Phase 1 work

- ⏳ **OpenRaft Migration** (0% - 15-21h) - HIGHEST PRIORITY
  - Migrate from raft-rs 0.7 to openraft 0.10
  - Eliminate prost version conflicts (0.11 vs 0.14)
  - Convert to async-first architecture
  - Preserve StateMachine idempotency guarantees
  - **Blocks**: RocksDB storage, KV service integration
  - Spec: `docs/specs/openraft/`

### Phase 1B: Storage Layer (1 week)
**Status**: ⏳ Not Started
**Dependencies**: OpenRaft Phase 1-2 must complete first

- ⏳ **RocksDB Storage** (0% - 14-17h) - HIGH PRIORITY
  - Implement 6 column families (system_raft_log, system_raft_state, system_data, data_raft_log, data_raft_state, data_kv)
  - Integrate with openraft storage traits (RaftLogReader, RaftSnapshotBuilder, RaftStorage)
  - Async trait implementation
  - **Blocks**: KV service persistence
  - Spec: `docs/specs/rocksdb/` (revised for OpenRaft)

### Phase 1C: Service Integration (1 week)
**Status**: Partial (RESP only)
**Dependencies**: OpenRaft must be complete

- ✅ **RESP Protocol** (100% - DONE) - 487 tests passing
  - Full Redis protocol support (GET, SET, DEL, EXISTS, PING)
  - Property-based testing
  - Comprehensive integration tests

- ⏳ **KV Service Layer** (0% - 11-13h) - HIGH PRIORITY
  - Async command handlers with OpenRaft integration
  - Input validation (key/value size limits)
  - Error handling and leader redirection
  - **Depends on**: OpenRaft complete
  - Spec: `docs/specs/kvservice/` (revised for async API)

- ⏳ **Main Binary Orchestration** (0% - 8-10h) - HIGH PRIORITY
  - Wire RESP → KV → OpenRaft → Storage together
  - TCP server on port 6379
  - Tokio async runtime integration
  - 3-node cluster bootstrap

### Phase 1D: Validation (3-4 days)
**Status**: ⏳ Not Started
**Dependencies**: Phase 1C complete

- ⏳ **Integration Testing** (0% - 4-6h)
  - End-to-end request flows
  - 3-node cluster behavior validation
  - Leader election and failover testing

- ⏳ **Chaos Testing** (0% - 8-10h) - REQUIRED
  - 11 chaos test scenarios
  - Network partitions and node failures
  - 1+ hour cluster stability test
  - Pass all scenarios before Phase 1 complete

### Success Criteria
- [ ] 3-node cluster runs stably for 1+ hours
- [ ] Pass all 11 chaos test scenarios
- [ ] Performance: >5,000 ops/sec, <10ms p99 latency
- [ ] Zero prost dependency conflicts
- [ ] OpenRaft integration complete with 85+ tests passing

### Revised Estimates
- **Original Estimate**: 45-55 hours
- **Revised Estimate**: 56-71 hours (+24% increase)
- **Reason**: OpenRaft migration discovered as critical technical debt

## Phase 2: Multi-Shard Cluster
- Focus: Horizontal scalability
- Key Deliverables:
  - Consistent hashing
  - Shard allocation strategies
  - Dynamic shard rebalancing
  - Cross-shard operation support

## Phase 3: Dynamic Cluster Management
- Focus: Operational flexibility
- Key Deliverables:
  - Node addition/removal
  - Automatic rebalancing
  - Dynamic configuration updates
  - Advanced failure recovery

## Phase 4: Production Readiness
- Focus: Enterprise-grade features
- Key Deliverables:
  - Advanced monitoring (OpenTelemetry)
  - Comprehensive security model
  - Multi-datacenter replication
  - Compliance and certification
  - Performance optimizations
  - Linearizable reads (ReadIndex mechanism)

## Phase 5: SQL Interface
- Focus: Multi-protocol support
- Key Deliverables:
  - PostgreSQL wire protocol implementation
  - SQL service layer
  - Query planning and optimization
  - Transaction management
  - Schema validation

## Risk Assessment

### High Risks
1. **OpenRaft Learning Curve**: Async patterns and new API may slow development
2. **Integration Complexity**: Wiring 4 major async components with no incremental testing until complete
3. **Technical Debt Cascade**: OpenRaft may reveal additional incompatibilities

### Mitigations
- Spec-driven development with detailed task breakdown
- TDD approach with comprehensive test coverage
- Incremental validation gates after each phase
- Early architectural validation before implementation

## Recommended Next Steps

**Immediate (This Week)**:
1. Begin OpenRaft migration: `/spec:implement openraft`
2. Focus on Phase 1 (Type System) as foundation

**Week 2**:
1. Complete OpenRaft Phases 2-4 (Storage, State Machine, Network)
2. Parallel work on RocksDB spec finalization

**Week 3**:
1. Complete OpenRaft integration (Phases 5-6)
2. Begin RocksDB implementation with openraft traits

**Week 4**:
1. Complete RocksDB storage
2. Begin KV Service async implementation
3. Wire seshat binary integration

**Week 5** (if needed):
1. Complete integration testing
2. Run chaos test suite
3. Performance validation

## Progress Tracking

Use these commands to monitor progress:
- `/spec:progress openraft` - OpenRaft migration progress
- `/spec:progress rocksdb` - RocksDB storage progress
- `/spec:progress kvservice` - KV service progress
- `/product:progress` - Overall project progress

---

**Last Updated**: 2025-10-26
**Status**: Phase 1A - OpenRaft migration (Not Started)
**Next Milestone**: Complete OpenRaft Phase 1-2 (Type System + Storage Layer)