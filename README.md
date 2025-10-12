# Seshat

> *"She Who Is The Scribe"* - Egyptian goddess of wisdom, knowledge, and writing

Seshat is a distributed, Redis-compatible key-value store built with Raft consensus in Rust.

## Features (Planned)

- **Strong consistency** via Raft consensus
- **Redis protocol compatibility** (RESP)
- **Horizontal scalability** through sharding
- **Fault tolerance** (survives node failures)
- **Log compaction** with RocksDB snapshots
- **Dynamic cluster membership**

## Architecture

- **Consensus**: Raft via `raft-rs`
- **Storage**: RocksDB
- **Protocol**: Redis RESP
- **Transport**: gRPC (tonic)
- **Language**: Rust

## Project Status

🚧 **Phase 1: Single Shard Cluster** - In Development

This is a learning project to deeply understand distributed consensus systems.

## Quick Start
```bash
# Build
cargo build

# Run single node (development)
cargo run -p seshat -- --config config/node1.toml

# Run 3-node cluster (Docker Compose)
docker-compose up

## Documentation

Product Requirements
Architecture
Development Guide

## Etymology
Seshat was the ancient Egyptian goddess of wisdom, knowledge, and writing. She was the keeper of royal records and libraries, responsible for recording the names of pharaohs and their deeds. Her role as divine scribe makes her the perfect patron for a distributed database focused on preserving and retrieving data with strong consistency.

## License
MIT
