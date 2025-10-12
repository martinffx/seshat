# Seshat Rust Coding Style Guide

## Language Edition
- Rust 2021 Edition
- Strict compiler warnings enabled
- Rustfmt for consistent formatting

## Async Programming
- Exclusively use `tokio` async runtime
- Prefer `.await` over blocking calls
- Minimize runtime overhead
- Use async channels for communication

## Error Handling
- `anyhow::Result` for application errors
- `thiserror::Error` for library errors
- Descriptive error messages
- Context preservation

## Module Structure
Align with `crates/` layout:
- `seshat-core/`: Core distributed systems logic
- `seshat-storage/`: Storage engine implementations
- `seshat-network/`: Networking and RPC
- `seshat-consensus/`: Raft implementation
- `seshat-proto/`: Protobuf definitions

## Code Organization
- Keep functions small (<50 lines)
- Explicit type annotations
- Comprehensive documentation
- Implement `Debug`, `Clone` where appropriate
- Prefer composition over inheritance

## Performance Considerations
- Use `#[inline]` judiciously
- Minimize allocations
- Prefer borrowed types
- Use zero-cost abstractions

## Testing
- 100% test coverage goal
- Property-based testing preferred
- Mutation testing recommended
- Benchmark critical paths