//! Integration tests for RESP protocol
//!
//! These tests validate all components working together:
//! - Codec + Tokio TCP integration
//! - Pipelined commands
//! - Nested data structures
//! - Partial data stream handling
//! - Full command workflow
//! - Error handling across layers

use bytes::{BufMut, Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use seshat_protocol_resp::{RespCodec, RespCommand, RespValue};
use std::io;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Decoder, Framed};

// =================================================================
// Helper Functions
// =================================================================

/// Setup a TCP server that echoes back received values
async fn setup_echo_server() -> io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();

    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut framed = Framed::new(socket, RespCodec::new());

                while let Some(Ok(value)) = framed.next().await {
                    if framed.send(value).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    Ok(addr)
}

/// Setup a TCP server that responds to commands
async fn setup_command_server() -> io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();

    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut framed = Framed::new(socket, RespCodec::new());

                while let Some(Ok(value)) = framed.next().await {
                    // Try to parse as command
                    let response = match RespCommand::from_value(value) {
                        Ok(RespCommand::Ping { message }) => match message {
                            Some(msg) => RespValue::BulkString(Some(msg)),
                            None => RespValue::SimpleString(Bytes::from("PONG")),
                        },
                        Ok(RespCommand::Get { .. }) => {
                            RespValue::BulkString(Some(Bytes::from("value")))
                        }
                        Ok(RespCommand::Set { .. }) => RespValue::SimpleString(Bytes::from("OK")),
                        Ok(RespCommand::Del { keys }) => RespValue::Integer(keys.len() as i64),
                        Ok(RespCommand::Exists { keys }) => RespValue::Integer(keys.len() as i64),
                        Err(_) => RespValue::Error(Bytes::from("ERR unknown command")),
                    };

                    if framed.send(response).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    Ok(addr)
}

/// Send a value and receive a response
async fn send_receive(
    framed: &mut Framed<TcpStream, RespCodec>,
    value: RespValue,
) -> io::Result<RespValue> {
    framed.send(value).await.map_err(io::Error::other)?;
    framed
        .next()
        .await
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed"))?
        .map_err(io::Error::other)
}

// =================================================================
// Test 1: Codec + Tokio TCP Integration
// =================================================================

#[tokio::test]
async fn test_codec_with_tokio() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send a simple string command
    let command = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from("PING")))]));

    framed.send(command.clone()).await.unwrap();

    // Receive echo
    let response = framed.next().await.unwrap().unwrap();
    assert_eq!(response, command);
}

#[tokio::test]
async fn test_codec_roundtrip_multiple_types() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    let test_values = vec![
        RespValue::SimpleString(Bytes::from("OK")),
        RespValue::Integer(42),
        RespValue::BulkString(Some(Bytes::from("hello"))),
        RespValue::Null,
        RespValue::Boolean(true),
        RespValue::Error(Bytes::from("ERR test")),
    ];

    for value in test_values {
        let response = send_receive(&mut framed, value.clone()).await.unwrap();
        assert_eq!(response, value);
    }
}

#[tokio::test]
async fn test_codec_with_large_bulk_string() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Create a large bulk string (1 MB)
    let large_data = vec![b'x'; 1024 * 1024];
    let value = RespValue::BulkString(Some(Bytes::from(large_data.clone())));

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);
}

// =================================================================
// Test 2: Pipelined Command Execution
// =================================================================

#[tokio::test]
async fn test_pipelined_commands() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send 5 commands without waiting for responses
    let commands = vec![
        RespValue::SimpleString(Bytes::from("CMD1")),
        RespValue::SimpleString(Bytes::from("CMD2")),
        RespValue::SimpleString(Bytes::from("CMD3")),
        RespValue::SimpleString(Bytes::from("CMD4")),
        RespValue::SimpleString(Bytes::from("CMD5")),
    ];

    for cmd in &commands {
        framed.send(cmd.clone()).await.unwrap();
    }

    // Receive all responses in correct order
    for expected in commands {
        let response = framed.next().await.unwrap().unwrap();
        assert_eq!(response, expected);
    }
}

#[tokio::test]
async fn test_pipelined_mixed_types() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Pipeline different types of values
    let commands = vec![
        RespValue::SimpleString(Bytes::from("OK")),
        RespValue::Integer(100),
        RespValue::BulkString(Some(Bytes::from("data"))),
        RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)])),
        RespValue::Null,
    ];

    // Send all at once
    for cmd in &commands {
        framed.send(cmd.clone()).await.unwrap();
    }

    // Receive in order
    for expected in commands {
        let response = framed.next().await.unwrap().unwrap();
        assert_eq!(response, expected);
    }
}

#[tokio::test]
async fn test_pipelined_no_interference() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send commands with unique identifiers
    let mut commands = Vec::new();
    for i in 0..10 {
        commands.push(RespValue::BulkString(Some(Bytes::from(format!(
            "command-{i}"
        )))));
    }

    // Pipeline all commands
    for cmd in &commands {
        framed.send(cmd.clone()).await.unwrap();
    }

    // Verify each response matches its command (no mixing)
    for expected in commands {
        let response = framed.next().await.unwrap().unwrap();
        assert_eq!(response, expected);
    }
}

// =================================================================
// Test 3: Nested Data Structures
// =================================================================

#[tokio::test]
async fn test_nested_arrays() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Create deeply nested arrays
    let inner = RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]));

    let middle = RespValue::Array(Some(vec![
        inner.clone(),
        RespValue::SimpleString(Bytes::from("middle")),
    ]));

    let outer = RespValue::Array(Some(vec![
        middle.clone(),
        RespValue::SimpleString(Bytes::from("outer")),
        inner,
    ]));

    let response = send_receive(&mut framed, outer.clone()).await.unwrap();
    assert_eq!(response, outer);
}

#[tokio::test]
async fn test_arrays_containing_arrays() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Array of arrays
    let value = RespValue::Array(Some(vec![
        RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)])),
        RespValue::Array(Some(vec![RespValue::Integer(3), RespValue::Integer(4)])),
        RespValue::Array(Some(vec![RespValue::Integer(5), RespValue::Integer(6)])),
    ]));

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);
}

#[tokio::test]
async fn test_maps_with_complex_values() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Map with arrays as values
    let value = RespValue::Map(vec![
        (
            RespValue::SimpleString(Bytes::from("key1")),
            RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)])),
        ),
        (
            RespValue::SimpleString(Bytes::from("key2")),
            RespValue::BulkString(Some(Bytes::from("value"))),
        ),
        (
            RespValue::SimpleString(Bytes::from("key3")),
            RespValue::Map(vec![(
                RespValue::SimpleString(Bytes::from("nested")),
                RespValue::Boolean(true),
            )]),
        ),
    ]);

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);
}

#[tokio::test]
async fn test_deep_nesting() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Create a deeply nested structure (within reasonable limits)
    let mut value = RespValue::Integer(42);
    for _ in 0..10 {
        value = RespValue::Array(Some(vec![value]));
    }

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);
}

#[tokio::test]
async fn test_mixed_nested_structures() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Complex structure mixing arrays, maps, and sets
    let value = RespValue::Array(Some(vec![
        RespValue::Map(vec![(
            RespValue::SimpleString(Bytes::from("users")),
            RespValue::Set(vec![
                RespValue::SimpleString(Bytes::from("alice")),
                RespValue::SimpleString(Bytes::from("bob")),
            ]),
        )]),
        RespValue::Array(Some(vec![
            RespValue::Integer(1),
            RespValue::Integer(2),
            RespValue::Integer(3),
        ])),
        RespValue::BulkString(Some(Bytes::from("metadata"))),
    ]));

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);
}

// =================================================================
// Test 4: Partial Data Stream Handling
// =================================================================

#[tokio::test]
async fn test_partial_frame_simple_string() {
    use tokio::io::AsyncWriteExt;

    let addr = setup_echo_server().await.unwrap();
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Send partial data
    stream.write_all(b"+OK").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Complete the frame
    stream.write_all(b"\r\n").await.unwrap();

    // Read response
    let mut framed = Framed::new(stream, RespCodec::new());
    let response = framed.next().await.unwrap().unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("OK")));
}

#[tokio::test]
async fn test_partial_frame_bulk_string() {
    use tokio::io::AsyncWriteExt;

    let addr = setup_echo_server().await.unwrap();
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Send length
    stream.write_all(b"$5\r\n").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Send partial data
    stream.write_all(b"hel").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Complete data
    stream.write_all(b"lo\r\n").await.unwrap();

    // Read response
    let mut framed = Framed::new(stream, RespCodec::new());
    let response = framed.next().await.unwrap().unwrap();
    assert_eq!(response, RespValue::BulkString(Some(Bytes::from("hello"))));
}

#[tokio::test]
async fn test_gradual_data_arrival() {
    // Test codec handles gradual data arrival across multiple decode calls
    let mut codec = RespCodec::new();
    let mut buf = BytesMut::new();

    // First chunk - incomplete
    buf.put_slice(b"+HEL");
    assert_eq!(codec.decode(&mut buf).unwrap(), None);

    // Second chunk - still incomplete
    buf.put_slice(b"LO");
    assert_eq!(codec.decode(&mut buf).unwrap(), None);

    // Final chunk - completes message
    buf.put_slice(b"\r\n");
    let result = codec.decode(&mut buf).unwrap();
    assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("HELLO"))));
}

#[tokio::test]
async fn test_incomplete_then_complete_array() {
    // Test codec with incomplete array that gets completed
    let mut codec = RespCodec::new();
    let mut buf = BytesMut::new();

    // Send array header and first element (incomplete)
    buf.extend_from_slice(b"*2\r\n:1\r\n");

    // Should return None - array is incomplete
    assert!(codec.decode(&mut buf).unwrap().is_none());

    // Send second element to complete the array
    buf.extend_from_slice(b":2\r\n");

    // Now should decode successfully
    let result = codec.decode(&mut buf).unwrap();
    assert_eq!(
        result,
        Some(RespValue::Array(Some(vec![
            RespValue::Integer(1),
            RespValue::Integer(2)
        ])))
    );
}

#[tokio::test]
async fn test_multiple_partial_frames() {
    // Test codec receiving multiple frames that arrive in partial chunks
    let mut codec = RespCodec::new();
    let mut buf = BytesMut::new();

    // Add first frame in parts
    buf.extend_from_slice(b"+FIR");
    assert!(codec.decode(&mut buf).unwrap().is_none()); // Incomplete

    buf.extend_from_slice(b"ST\r\n");
    let response1 = codec.decode(&mut buf).unwrap();
    assert_eq!(
        response1,
        Some(RespValue::SimpleString(Bytes::from("FIRST")))
    );

    // Add second frame in parts
    buf.extend_from_slice(b":10");
    assert!(codec.decode(&mut buf).unwrap().is_none()); // Incomplete

    buf.extend_from_slice(b"0\r\n");
    let response2 = codec.decode(&mut buf).unwrap();
    assert_eq!(response2, Some(RespValue::Integer(100)));
}

// =================================================================
// Test 5: Full Command Workflow
// =================================================================

#[tokio::test]
async fn test_full_command_workflow_get() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send GET command
    let command = RespValue::Array(Some(vec![
        RespValue::BulkString(Some(Bytes::from("GET"))),
        RespValue::BulkString(Some(Bytes::from("mykey"))),
    ]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::BulkString(Some(Bytes::from("value"))));
}

#[tokio::test]
async fn test_full_command_workflow_set() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send SET command
    let command = RespValue::Array(Some(vec![
        RespValue::BulkString(Some(Bytes::from("SET"))),
        RespValue::BulkString(Some(Bytes::from("mykey"))),
        RespValue::BulkString(Some(Bytes::from("myvalue"))),
    ]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("OK")));
}

#[tokio::test]
async fn test_full_command_workflow_del() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send DEL command with multiple keys
    let command = RespValue::Array(Some(vec![
        RespValue::BulkString(Some(Bytes::from("DEL"))),
        RespValue::BulkString(Some(Bytes::from("key1"))),
        RespValue::BulkString(Some(Bytes::from("key2"))),
        RespValue::BulkString(Some(Bytes::from("key3"))),
    ]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::Integer(3));
}

#[tokio::test]
async fn test_full_command_workflow_exists() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send EXISTS command
    let command = RespValue::Array(Some(vec![
        RespValue::BulkString(Some(Bytes::from("EXISTS"))),
        RespValue::BulkString(Some(Bytes::from("key1"))),
        RespValue::BulkString(Some(Bytes::from("key2"))),
    ]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::Integer(2));
}

#[tokio::test]
async fn test_full_command_workflow_ping_no_message() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send PING command
    let command = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from("PING")))]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("PONG")));
}

#[tokio::test]
async fn test_full_command_workflow_ping_with_message() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send PING command with message
    let command = RespValue::Array(Some(vec![
        RespValue::BulkString(Some(Bytes::from("PING"))),
        RespValue::BulkString(Some(Bytes::from("hello"))),
    ]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert_eq!(response, RespValue::BulkString(Some(Bytes::from("hello"))));
}

#[tokio::test]
async fn test_parse_encode_roundtrip_all_commands() {
    // Test that we can parse → command → encode for all commands
    let commands = vec![
        (
            RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("GET"))),
                RespValue::BulkString(Some(Bytes::from("key"))),
            ])),
            RespCommand::Get {
                key: Bytes::from("key"),
            },
        ),
        (
            RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("SET"))),
                RespValue::BulkString(Some(Bytes::from("key"))),
                RespValue::BulkString(Some(Bytes::from("value"))),
            ])),
            RespCommand::Set {
                key: Bytes::from("key"),
                value: Bytes::from("value"),
            },
        ),
        (
            RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("DEL"))),
                RespValue::BulkString(Some(Bytes::from("key1"))),
                RespValue::BulkString(Some(Bytes::from("key2"))),
            ])),
            RespCommand::Del {
                keys: vec![Bytes::from("key1"), Bytes::from("key2")],
            },
        ),
        (
            RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("EXISTS"))),
                RespValue::BulkString(Some(Bytes::from("key"))),
            ])),
            RespCommand::Exists {
                keys: vec![Bytes::from("key")],
            },
        ),
        (
            RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from("PING")))])),
            RespCommand::Ping { message: None },
        ),
    ];

    for (value, expected_cmd) in commands {
        // Parse value to command
        let cmd = RespCommand::from_value(value.clone()).unwrap();
        assert_eq!(cmd, expected_cmd);

        // Encode back to bytes and verify roundtrip
        let mut buf = BytesMut::new();
        seshat_protocol_resp::RespEncoder::encode(&value, &mut buf).unwrap();

        // Parse back
        let mut parser = seshat_protocol_resp::RespParser::new();
        let parsed = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(parsed, value);
    }
}

// =================================================================
// Test 6: Error Handling Integration
// =================================================================

#[tokio::test]
async fn test_malformed_command_error() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send invalid command (not an array)
    let command = RespValue::SimpleString(Bytes::from("INVALID"));

    // This will fail at command parsing level
    // Server should respond with error
    let response = send_receive(&mut framed, command).await.unwrap();
    assert!(matches!(response, RespValue::Error(_)));
}

#[tokio::test]
async fn test_unknown_command_error() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send unknown command
    let command = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from(
        "UNKNOWN",
    )))]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert!(matches!(response, RespValue::Error(_)));
}

#[tokio::test]
async fn test_wrong_arity_error() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send GET with wrong number of arguments
    let command = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from("GET")))]));

    let response = send_receive(&mut framed, command).await.unwrap();
    assert!(matches!(response, RespValue::Error(_)));
}

#[tokio::test]
async fn test_codec_decode_error_propagation() {
    use tokio_util::codec::Decoder;

    let mut codec = RespCodec::new();
    let mut buf = BytesMut::from("INVALID\r\n");

    // Should propagate error from parser
    let result = codec.decode(&mut buf);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_type_marker_error() {
    use tokio_util::codec::Decoder;

    let mut codec = RespCodec::new();
    let mut buf = BytesMut::from("XINVALID\r\n");

    // Invalid type marker 'X' should cause error
    let result = codec.decode(&mut buf);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_recovery() {
    let addr = setup_command_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send invalid command
    let invalid = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from(
        "BADCMD",
    )))]));
    let response = send_receive(&mut framed, invalid).await.unwrap();
    assert!(matches!(response, RespValue::Error(_)));

    // Send valid command - connection should still work
    let valid = RespValue::Array(Some(vec![RespValue::BulkString(Some(Bytes::from("PING")))]));
    let response = send_receive(&mut framed, valid).await.unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("PONG")));
}

// =================================================================
// Additional Integration Tests
// =================================================================

#[tokio::test]
async fn test_concurrent_connections() {
    let addr = setup_echo_server().await.unwrap();

    // Spawn multiple concurrent connections
    let mut handles = vec![];
    for i in 0..5 {
        let addr = addr.clone();
        let handle = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut framed = Framed::new(stream, RespCodec::new());

            let value = RespValue::Integer(i);
            let response = send_receive(&mut framed, value.clone()).await.unwrap();
            assert_eq!(response, value);
        });
        handles.push(handle);
    }

    // Wait for all connections to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_large_pipelined_commands() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Pipeline 100 commands
    let count = 100;
    let mut commands = Vec::new();
    for i in 0..count {
        commands.push(RespValue::Integer(i));
    }

    // Send all commands
    for cmd in &commands {
        framed.send(cmd.clone()).await.unwrap();
    }

    // Receive all responses
    for expected in commands {
        let response = framed.next().await.unwrap().unwrap();
        assert_eq!(response, expected);
    }
}

#[tokio::test]
async fn test_connection_close_handling() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(&addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send a value
    framed
        .send(RespValue::SimpleString(Bytes::from("TEST")))
        .await
        .unwrap();

    // Receive response
    let response = framed.next().await.unwrap().unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("TEST")));

    // Close connection
    drop(framed);

    // Reconnect and verify it works
    let stream = TcpStream::connect(&addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    framed
        .send(RespValue::SimpleString(Bytes::from("AFTER")))
        .await
        .unwrap();
    let response = framed.next().await.unwrap().unwrap();
    assert_eq!(response, RespValue::SimpleString(Bytes::from("AFTER")));
}

#[tokio::test]
async fn test_binary_data_integrity() {
    let addr = setup_echo_server().await.unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, RespCodec::new());

    // Send binary data with null bytes and special characters
    let binary_data = vec![0x00, 0x01, 0xFF, 0xFE, 0x0D, 0x0A, 0x7F];
    let value = RespValue::BulkString(Some(Bytes::from(binary_data.clone())));

    let response = send_receive(&mut framed, value.clone()).await.unwrap();
    assert_eq!(response, value);

    // Verify the actual bytes
    if let RespValue::BulkString(Some(data)) = response {
        assert_eq!(data.as_ref(), binary_data.as_slice());
    }
}
