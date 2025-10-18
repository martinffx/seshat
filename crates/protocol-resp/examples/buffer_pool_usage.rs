//! Example demonstrating BufferPool usage with RespEncoder
//!
//! This example shows how to use BufferPool to reduce allocations when
//! encoding multiple RESP messages.

use bytes::Bytes;
use seshat_protocol_resp::{BufferPool, RespEncoder, RespValue};

fn main() {
    // Create a buffer pool with 4KB buffers
    let mut pool = BufferPool::new(4096);

    println!("=== BufferPool Example ===\n");

    // Simulate encoding multiple RESP messages
    let messages = [
        RespValue::SimpleString(Bytes::from("OK")),
        RespValue::Integer(42),
        RespValue::BulkString(Some(Bytes::from("Hello, World!"))),
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::BulkString(Some(Bytes::from("mykey"))),
        ])),
        RespValue::Error(Bytes::from("ERR something went wrong")),
    ];

    for (i, message) in messages.iter().enumerate() {
        // Acquire a buffer from the pool
        let mut buf = pool.acquire();

        // Encode the RESP value
        RespEncoder::encode(message, &mut buf).unwrap();

        // Print the encoded message
        println!("Message {}: {:?}", i + 1, String::from_utf8_lossy(&buf[..]));
        println!("  Buffer capacity: {} bytes", buf.capacity());
        println!("  Buffer length: {} bytes", buf.len());

        // In a real application, you would send buf over the network here
        // Then return it to the pool when done

        // Return the buffer to the pool
        pool.release(buf);

        println!();
    }

    println!(
        "\nAll {} messages were encoded using a single pooled buffer!",
        messages.len()
    );
}
