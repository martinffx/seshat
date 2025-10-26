# RocksDB Storage Layer - Lite Specification

## Feature Overview

RocksDB-based storage layer providing persistent, durable, and performant storage for Seshat's distributed key-value store. Supports Raft consensus requirements with 6 strategically designed column families.

## Key Acceptance Criteria

1. Initialize 6 column families with optimized RocksDB configuration
2. Achieve <1ms p99 latency for local storage operations
3. Ensure atomic, gap-free Raft log entries across column families
4. Create RocksDB checkpoints with metadata preservation
5. Enable node restart with full data recovery and consistency

## Critical Technical Details

### Column Families
- `system_raft_log`: System Raft log entries (compacted, ~10MB)
- `system_raft_state`: Persistent Raft hard state
- `system_data`: Cluster metadata
- `data_raft_log`: Data shard log entries
- `data_raft_state`: Data shard hard state
- `data_kv`: User key-value data

### Performance Targets
- Single key ops: <1ms p99 latency
- Throughput: >5,000 ops/sec
- Snapshot creation: <10s (100MB)

## Dependencies
- Dependencies: rocksdb, bincode, serde, thiserror
- Integrated via: raft crate, kv crate
- Storage trait implementation for raft-rs

## Implementation Notes
- No Raft semantic understanding
- Version fields for future schema evolution
- Atomic batch writes
- Hard link-based snapshots
- Thread-safe concurrent operations

## Alignment
Phase 1 MVP storage foundation - enables cluster recovery, Raft consensus durability, and key-value data persistence.