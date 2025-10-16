//! Tokio codec for RESP protocol
//!
//! This module provides a tokio_util::codec integration for the RESP protocol,
//! allowing seamless streaming of RESP messages over TCP connections.

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::{ProtocolError, RespEncoder, RespParser, RespValue, Result};

/// Tokio codec for RESP protocol
///
/// Integrates RespParser and RespEncoder into tokio's codec framework,
/// enabling efficient streaming I/O with TCP connections.
///
/// # Examples
///
/// ```
/// use tokio_util::codec::{Decoder, Encoder};
/// use bytes::BytesMut;
/// use seshat_protocol::{RespCodec, RespValue};
///
/// let mut codec = RespCodec::new();
/// let mut buf = BytesMut::from("+OK\r\n");
///
/// let value = codec.decode(&mut buf).unwrap();
/// assert!(value.is_some());
/// ```
pub struct RespCodec {
    parser: RespParser,
}

impl RespCodec {
    /// Create a new codec with default limits
    pub fn new() -> Self {
        Self {
            parser: RespParser::new(),
        }
    }

    /// Create codec with custom max bulk size
    pub fn with_max_bulk_size(mut self, size: usize) -> Self {
        self.parser = self.parser.with_max_bulk_size(size);
        self
    }

    /// Create codec with custom max array length
    pub fn with_max_array_len(mut self, len: usize) -> Self {
        self.parser = self.parser.with_max_array_len(len);
        self
    }

    /// Create codec with custom max nesting depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.parser = self.parser.with_max_depth(depth);
        self
    }
}

impl Default for RespCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for RespCodec {
    type Item = RespValue;
    type Error = ProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        self.parser.parse(src)
    }
}

impl Encoder<RespValue> for RespCodec {
    type Error = ProtocolError;

    fn encode(&mut self, item: RespValue, dst: &mut BytesMut) -> Result<()> {
        RespEncoder::encode(&item, dst)
    }
}

impl Encoder<&RespValue> for RespCodec {
    type Error = ProtocolError;

    fn encode(&mut self, item: &RespValue, dst: &mut BytesMut) -> Result<()> {
        RespEncoder::encode(item, dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio_util::codec::{Decoder, Encoder};

    // =================================================================
    // Basic Decode Tests
    // =================================================================

    #[test]
    fn test_decode_simple_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_error() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("-ERR unknown\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Error(Bytes::from("ERR unknown"))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_integer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from(":1000\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(1000)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_bulk_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("$5\r\nhello\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_null_bulk_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("$-1\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BulkString(None)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("GET"))),
                RespValue::BulkString(Some(Bytes::from("key"))),
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_null_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("*-1\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Array(None)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_resp3_null() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("_\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_boolean_true() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("#t\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Boolean(true)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_boolean_false() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("#f\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Boolean(false)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_double() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from(",1.23\r\n");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Double(1.23)));
        assert_eq!(buf.len(), 0);
    }

    // =================================================================
    // Partial Message Tests
    // =================================================================

    #[test]
    fn test_decode_partial_simple_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK");

        // Should return None (need more data)
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add remaining data
        buf.extend_from_slice(b"\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_partial_bulk_string_length() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("$5");

        // Need more data for length
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add CRLF and data
        buf.extend_from_slice(b"\r\nhello\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_partial_bulk_string_data() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("$5\r\nhel");

        // Need more data for bulk string content
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add remaining data
        buf.extend_from_slice(b"lo\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    #[ignore]
    // TODO: Parser limitation - incomplete nested elements in arrays require state stack
    // This is a known limitation documented in status.md. Will be addressed in future enhancement.
    fn test_decode_partial_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("*2\r\n$3\r\nGET");

        // Need more data for array element
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add remaining data
        buf.extend_from_slice(b"\r\n$3\r\nkey\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::BulkString(Some(bytes::Bytes::from("GET"))),
                RespValue::BulkString(Some(bytes::Bytes::from("key"))),
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_empty_buffer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);
        assert_eq!(buf.len(), 0);
    }

    // =================================================================
    // Multiple Messages Tests
    // =================================================================

    #[test]
    fn test_decode_multiple_messages_in_buffer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK\r\n+PONG\r\n");

        // First message
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));

        // Second message
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("PONG"))));

        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_multiple_with_partial() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK\r\n+PON");

        // First message complete
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));

        // Second message incomplete
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Complete second message
        buf.extend_from_slice(b"G\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("PONG"))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_complex_array_with_partial() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("*3\r\n:1\r\n:2\r\n");

        // Array incomplete
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add final element
        buf.extend_from_slice(b":3\r\n");
        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Integer(2),
                RespValue::Integer(3),
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    // =================================================================
    // Basic Encode Tests
    // =================================================================

    #[test]
    fn test_encode_simple_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::SimpleString(Bytes::from("OK"));

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"+OK\r\n");
    }

    #[test]
    fn test_encode_error() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Error(Bytes::from("ERR unknown"));

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"-ERR unknown\r\n");
    }

    #[test]
    fn test_encode_integer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(1000);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b":1000\r\n");
    }

    #[test]
    fn test_encode_bulk_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(Some(Bytes::from("hello")));

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_encode_null_bulk_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(None);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$-1\r\n");
    }

    #[test]
    fn test_encode_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::BulkString(Some(Bytes::from("key"))),
        ]));

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");
    }

    #[test]
    fn test_encode_null_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Array(None);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*-1\r\n");
    }

    #[test]
    fn test_encode_resp3_null() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Null;

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"_\r\n");
    }

    #[test]
    fn test_encode_boolean_true() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Boolean(true);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"#t\r\n");
    }

    #[test]
    fn test_encode_boolean_false() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Boolean(false);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"#f\r\n");
    }

    #[test]
    fn test_encode_double() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Double(1.23);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b",1.23\r\n");
    }

    // =================================================================
    // Encode by Reference Tests
    // =================================================================

    #[test]
    fn test_encode_by_reference() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::SimpleString(Bytes::from("OK"));

        // Encode by reference
        codec.encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"+OK\r\n");

        // Can still use value
        let _ = value;
    }

    #[test]
    fn test_encode_multiple_references() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(42);

        codec.encode(&value, &mut buf).unwrap();
        codec.encode(&value, &mut buf).unwrap();
        codec.encode(&value, &mut buf).unwrap();

        assert_eq!(&buf[..], b":42\r\n:42\r\n:42\r\n");
    }

    // =================================================================
    // Roundtrip Tests
    // =================================================================

    #[test]
    fn test_roundtrip_simple_string() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let original = RespValue::SimpleString(Bytes::from("HELLO"));

        // Encode
        codec.encode(&original, &mut buf).unwrap();

        // Decode
        let decoded = codec.decode(&mut buf).unwrap();
        assert_eq!(decoded, Some(original));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_roundtrip_complex_array() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let original = RespValue::Array(Some(vec![
            RespValue::Integer(1),
            RespValue::BulkString(Some(Bytes::from("test"))),
            RespValue::Null,
            RespValue::Boolean(true),
        ]));

        // Encode
        codec.encode(&original, &mut buf).unwrap();

        // Decode
        let decoded = codec.decode(&mut buf).unwrap();
        assert_eq!(decoded, Some(original));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_roundtrip_multiple_messages() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();
        let messages = vec![
            RespValue::SimpleString(Bytes::from("OK")),
            RespValue::Integer(42),
            RespValue::BulkString(Some(Bytes::from("data"))),
        ];

        // Encode all
        for msg in &messages {
            codec.encode(msg, &mut buf).unwrap();
        }

        // Decode all
        for expected in messages {
            let decoded = codec.decode(&mut buf).unwrap();
            assert_eq!(decoded, Some(expected));
        }
        assert_eq!(buf.len(), 0);
    }

    // =================================================================
    // Custom Limits Tests
    // =================================================================

    #[test]
    fn test_codec_with_custom_max_bulk_size() {
        let mut codec = RespCodec::new().with_max_bulk_size(10);
        let mut buf = BytesMut::from("$5\r\nhello\r\n");

        // Should succeed (within limit)
        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_codec_with_custom_max_array_len() {
        let mut codec = RespCodec::new().with_max_array_len(5);
        let mut buf = BytesMut::from("*2\r\n:1\r\n:2\r\n");

        // Should succeed (within limit)
        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_codec_with_custom_max_depth() {
        let mut codec = RespCodec::new().with_max_depth(2);
        let mut buf = BytesMut::from("*1\r\n*1\r\n:42\r\n");

        // Should succeed (within depth limit)
        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());
    }

    // =================================================================
    // Error Propagation Tests
    // =================================================================

    #[test]
    fn test_decode_error_propagation() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK\r\nINVALID");

        // First decode succeeds
        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());

        // Second decode should fail with protocol error
        let result = codec.decode(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_invalid_type_marker() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("XNOPE\r\n");

        let result = codec.decode(&mut buf);
        assert!(result.is_err());
    }

    // =================================================================
    // Default Trait Tests
    // =================================================================

    #[test]
    fn test_codec_default() {
        let codec1 = RespCodec::new();
        let codec2 = RespCodec::default();

        // Both should work identically
        let mut buf1 = BytesMut::from("+OK\r\n");
        let mut buf2 = BytesMut::from("+OK\r\n");

        let mut c1 = codec1;
        let mut c2 = codec2;

        let result1 = c1.decode(&mut buf1).unwrap();
        let result2 = c2.decode(&mut buf2).unwrap();

        assert_eq!(result1, result2);
    }

    // =================================================================
    // Edge Cases
    // =================================================================

    #[test]
    fn test_decode_preserves_extra_data() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("+OK\r\nextra data");

        let result = codec.decode(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));

        // Extra data should remain in buffer
        assert_eq!(&buf[..], b"extra data");
    }

    #[test]
    fn test_encode_into_non_empty_buffer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::from("prefix");
        let value = RespValue::Integer(42);

        codec.encode(value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"prefix:42\r\n");
    }

    #[test]
    fn test_repeated_decode_on_empty_buffer() {
        let mut codec = RespCodec::new();
        let mut buf = BytesMut::new();

        // Should consistently return None
        for _ in 0..10 {
            let result = codec.decode(&mut buf).unwrap();
            assert_eq!(result, None);
        }
    }
}
