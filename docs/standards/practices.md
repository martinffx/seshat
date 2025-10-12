# Seshat Development Practices

## Test-Driven Development (TDD)
Workflow:
1. Write Test
2. Write Minimal Implementation
3. Refactor
4. Repeat

## Logging and Observability
- Use `tracing` crate for structured logging
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Contextual logging with span and event tracking
- Structured log output (JSON preferred)

## Configuration Management
- Validate all configuration on startup
- Fail fast if configuration is invalid
- Support environment-based configuration
- Provide clear error messages for misconfiguration

## Versioning and Compatibility
- Version markers in all persisted data structures
- Explicit version compatibility checks
- Backward compatibility preservation

## Resource Management
- Enforce resource limits from day 1
- Configurable CPU, memory, and network constraints
- Graceful degradation under resource pressure

## Chaos Testing Requirements
Mandatory chaos test scenarios:
1. Network partition
2. Node failure during leader election
3. Slow network conditions
4. Concurrent client operations
5. Storage media failure simulation
6. Clock skew simulation
7. High-latency network
8. Sudden leader termination
9. Membership change during operation
10. Rapid cluster reconfiguration
11. Storage saturation

## Error Handling
- Use `anyhow` for application-level errors
- Use `thiserror` for library-level errors
- Comprehensive error context
- Predictable error propagation