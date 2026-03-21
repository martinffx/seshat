# RESP Protocol Specification

## Overview

The RESP Protocol feature implements Redis Serialization Protocol (RESP3) for the Seshat distributed key-value store, enabling modern Redis clients to communicate with Seshat using the latest protocol features. This is a protocol-only layer providing zero-copy parsing and encoding of RESP3 data types.

**Status:** ✅ COMPLETE
- Implementation: 100%
- Tests: 501 passing (429 unit, 33 integration, 22 property, 17 doctest)
- Time Spent: ~40 hours

**Crate:** `seshat-resp` (renamed from `protocol-resp`)

## User Story

As a Seshat distributed key-value store node, I want to implement the RESP protocol so that modern Redis clients can communicate with Seshat using the latest Redis protocol features.

## Acceptance Criteria

| ID | Description | Test Cases | Status |
|----|-------------|------------|--------|
| AC1 | Parse all 14 RESP3 data types from byte streams | 14 types | ✅ |
| AC2 | Encode all RESP3 data types to byte streams | 9 types | ✅ |
| AC3 | Parse inline commands for telnet compatibility | 4 scenarios | ✅ |
| AC4 | Support pipelined command execution | 3 scenarios | ✅ |
| AC5 | Handle protocol errors gracefully | 7 scenarios | ✅ |

### RESP3 Data Types Supported

**RESP2 Compatible (5 types):**
- SimpleString: `+OK\r\n`
- Error: `-ERR unknown command\r\n`
- Integer: `:1000\r\n`
- BulkString: `$5\r\nhello\r\n`
- Array: `*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n`

**RESP3 Only (9 types):**
- Null: `_\r\n`
- Boolean: `#t\r\n` or `#f\r\n`
- Double: `,1.23\r\n`
- BigNumber: `(3492890...\r\n`
- BulkError: `!21\r\nSYNTAX error\r\n`
- VerbatimString: `=15\r\ntxt:Some string\r\n`
- Map: `%2\r\n...`
- Set: `~3\r\n...`
- Push: `>4\r\n...`

## Business Rules

1. **Default Protocol**: All connections use RESP3 protocol
2. **Bulk String Limit**: Maximum 512 MB (configurable)
3. **Array Elements Limit**: Maximum 1M elements (configurable)
4. **Nested Depth Limit**: Maximum 32 levels (prevent stack overflow)
5. **Line Length Limit**: Maximum 1 MB for simple strings/errors
6. **Protocol Termination**: CRLF (`\r\n`) used as terminator
7. **Type Determination**: First byte determines data type
8. **Command Structure**: Client commands as arrays of bulk strings
9. **Inline Commands**: Support telnet interactions
10. **Error Handling**: Protocol errors must not crash connections

## Scope

### Included

- RESP3 protocol parser for 14 data types
- RESP3 protocol encoder for server responses
- Inline command parser for telnet compatibility
- Pipelined command support
- Protocol error detection and reporting
- Configurable size limits (bulk string, array length, depth)
- Streaming parser with partial data handling
- Tokio codec integration for async I/O
- Type-safe Rust enums for RESP3 values
- Property-based tests with proptest
- Command parsing for GET, SET, DEL, EXISTS, PING

### Excluded

- RESP2 protocol support (RESP3 only)
- HELLO command implementation
- Version negotiation logic
- Redis command execution (handled by KV service)
- Network layer handling (handled by TCP server)
- Authentication/authorization
- Pub/Sub push messages
- MONITOR command
- TLS/SSL support

## Architecture

### Design Pattern

Streaming State Machine Protocol Parser with Tokio codec integration.

```
Network Layer (tokio TcpStream)
         ↓
    RespCodec (Tokio Decoder/Encoder)
         ↓
    RespParser (Streaming State Machine)
         ↓
    RespValue (14 RESP3 types)
         ↓
    RespCommand (Application Commands)
         ↓
Command Handler Layer (separate feature)
```

### Core Components

**RespValue**
- Enum for all 14 RESP3 data types
- Uses `bytes::Bytes` for zero-copy string handling
- Implements Debug, Clone, PartialEq traits

**RespParser**
- Streaming state machine parser
- Handles incremental parsing
- Supports partial data streams
- Zero-copy parsing strategies

**RespEncoder**
- Converts Rust types to RESP3 wire format
- Streaming encoding support
- Efficient BytesMut usage

**RespCodec**
- Tokio Decoder/Encoder integration
- Works with `tokio_util::codec::Framed`
- Async I/O support

**ProtocolError**
- Comprehensive error types using thiserror
- 12+ variants covering all error cases
- Descriptive error messages with context

**RespCommand**
- Enum for Redis commands (GET, SET, DEL, EXISTS, PING)
- Case-insensitive command matching
- Argument validation

## Technical Design

### Performance Requirements

- **Throughput**: >50,000 commands/sec per node
- **Latency**: Parser overhead <100μs per command
- **Memory**: Zero-copy parsing with `bytes::Bytes`
- **Backpressure**: Handle partial frames gracefully

### Size Limits (Configurable)

| Limit | Default | Purpose |
|-------|---------|---------|
| Bulk String | 512 MB | Prevent memory exhaustion |
| Array Elements | 1M | Prevent large allocations |
| Nested Depth | 32 levels | Prevent stack overflow |
| Line Length | 1 MB | Prevent long simple strings |

### Tokio Integration

- `RespCodec` implements `tokio_util::codec::Decoder` and `Encoder`
- Works with `Framed` wrapper for async I/O
- Supports pipelined commands (multiple frames in buffer)

### Command Parsing

Commands parsed from RESP arrays into `RespCommand` enum:
- GET key → `RespCommand::Get { key }`
- SET key value → `RespCommand::Set { key, value }`
- DEL key [key...] → `RespCommand::Del { keys }`
- EXISTS key [key...] → `RespCommand::Exists { keys }`
- PING → `RespCommand::Ping`

## Dependencies

### External Libraries

- **tokio**: Async runtime for TCP I/O
- **bytes**: Zero-copy byte handling with `Bytes` type
- **thiserror**: Error type definitions
- **tokio-util**: Codec framework integration

### Internal Dependencies

- **common crate**: Shared types (future integration)

## Testing Strategy

### Unit Tests (429 tests)

- Parse/encode each RESP3 data type
- Validate type conversions
- Edge case handling
- Error condition coverage

### Integration Tests (33 tests)

- Pipelining scenarios
- Nested data structures
- Partial data stream handling
- Tokio codec integration

### Property Tests (22 tests)

- Generate random RESP3 values with proptest
- Fuzz testing with invalid inputs
- Roundtrip encoding/decoding validation
- 5,632+ generated test cases

### Doctests (17 tests)

- Documentation examples
- Type usage patterns

## Success Metrics

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| RESP3 Types | 14/14 | 14/14 | ✅ |
| Test Coverage | >400 tests | 501 tests | ✅ |
| Protocol Compliance | 100% | 100% | ✅ |
| Crashes on Malformed Input | 0 | 0 | ✅ |
| Error Handling | Graceful | Graceful | ✅ |

## File Organization

```
crates/resp/
├── src/
│   ├── lib.rs              # Public API exports
│   ├── types.rs            # RespValue enum (14 types)
│   ├── parser.rs           # Streaming parser state machine
│   ├── encoder.rs          # RespValue serializer
│   ├── codec.rs            # Tokio Decoder/Encoder integration
│   ├── inline.rs           # Inline command parser
│   ├── command.rs          # RespCommand enum
│   └── error.rs           # Protocol error types
├── tests/
│   ├── parser_tests.rs
│   ├── encoder_tests.rs
│   ├── codec_tests.rs
│   ├── property_tests.rs   # proptest
│   └── integration_tests.rs
└── benches/
    └── resp_benchmark.rs  # criterion benchmarks
```

## Alignment

This feature aligns with **Phase 1 MVP** by providing the protocol foundation for client communication. RESP3 enables:
- Modern Redis client compatibility
- Efficient binary data handling
- Streaming parser for memory efficiency
- Graceful error handling

The RESP protocol layer sits at the bottom of the client-facing stack, enabling Redis clients to communicate with the Seshat KV service.
