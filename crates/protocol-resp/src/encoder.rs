//! RESP protocol encoder
//!
//! This module implements encoding of RespValue types into their wire format
//! representation according to the RESP (REdis Serialization Protocol) specification.

use crate::error::Result;
use crate::types::RespValue;
use bytes::{BufMut, BytesMut};

/// RESP protocol encoder for serializing RespValue types to bytes
pub struct RespEncoder;

impl RespEncoder {
    /// Encode a RespValue into the provided BytesMut buffer
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::{RespEncoder, RespValue};
    /// use bytes::{Bytes, BytesMut};
    ///
    /// let mut buf = BytesMut::new();
    /// let value = RespValue::SimpleString(Bytes::from("OK"));
    /// RespEncoder::encode(&value, &mut buf).unwrap();
    /// assert_eq!(&buf[..], b"+OK\r\n");
    /// ```
    pub fn encode(value: &RespValue, buf: &mut BytesMut) -> Result<()> {
        match value {
            // RESP2 Types
            RespValue::SimpleString(s) => {
                buf.put_u8(b'+');
                buf.put_slice(s);
                buf.put_slice(b"\r\n");
            }

            RespValue::Error(e) => {
                buf.put_u8(b'-');
                buf.put_slice(e);
                buf.put_slice(b"\r\n");
            }

            RespValue::Integer(i) => {
                buf.put_u8(b':');
                buf.put_slice(i.to_string().as_bytes());
                buf.put_slice(b"\r\n");
            }

            RespValue::BulkString(None) => {
                buf.put_slice(b"$-1\r\n");
            }

            RespValue::BulkString(Some(s)) => {
                buf.put_u8(b'$');
                buf.put_slice(s.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                buf.put_slice(s);
                buf.put_slice(b"\r\n");
            }

            RespValue::Array(None) => {
                buf.put_slice(b"*-1\r\n");
            }

            RespValue::Array(Some(arr)) => {
                buf.put_u8(b'*');
                buf.put_slice(arr.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                for elem in arr {
                    Self::encode(elem, buf)?;
                }
            }

            // RESP3 Types
            RespValue::Null => {
                buf.put_slice(b"_\r\n");
            }

            RespValue::Boolean(true) => {
                buf.put_slice(b"#t\r\n");
            }

            RespValue::Boolean(false) => {
                buf.put_slice(b"#f\r\n");
            }

            RespValue::Double(d) => {
                buf.put_u8(b',');
                if d.is_infinite() {
                    if *d > 0.0 {
                        buf.put_slice(b"inf");
                    } else {
                        buf.put_slice(b"-inf");
                    }
                } else if d.is_nan() {
                    buf.put_slice(b"nan");
                } else {
                    buf.put_slice(d.to_string().as_bytes());
                }
                buf.put_slice(b"\r\n");
            }

            RespValue::BigNumber(n) => {
                buf.put_u8(b'(');
                buf.put_slice(n);
                buf.put_slice(b"\r\n");
            }

            RespValue::BulkError(e) => {
                buf.put_u8(b'!');
                buf.put_slice(e.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                buf.put_slice(e);
                buf.put_slice(b"\r\n");
            }

            RespValue::VerbatimString { format, data } => {
                // Length includes format (3 bytes) + colon (1 byte) + data
                let total_len = 4 + data.len();
                buf.put_u8(b'=');
                buf.put_slice(total_len.to_string().as_bytes());
                buf.put_slice(b"\r\n");
                buf.put_slice(&format[..]);
                buf.put_u8(b':');
                buf.put_slice(data);
                buf.put_slice(b"\r\n");
            }

            RespValue::Map(pairs) => {
                buf.put_u8(b'%');
                buf.put_slice(pairs.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                for (key, value) in pairs {
                    Self::encode(key, buf)?;
                    Self::encode(value, buf)?;
                }
            }

            RespValue::Set(elements) => {
                buf.put_u8(b'~');
                buf.put_slice(elements.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                for elem in elements {
                    Self::encode(elem, buf)?;
                }
            }

            RespValue::Push(elements) => {
                buf.put_u8(b'>');
                buf.put_slice(elements.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                for elem in elements {
                    Self::encode(elem, buf)?;
                }
            }
        }

        Ok(())
    }

    /// Convenience method to encode a simple OK response
    pub fn encode_ok(buf: &mut BytesMut) {
        buf.put_slice(b"+OK\r\n");
    }

    /// Convenience method to encode an error message
    pub fn encode_error(msg: &str, buf: &mut BytesMut) {
        buf.put_u8(b'-');
        buf.put_slice(msg.as_bytes());
        buf.put_slice(b"\r\n");
    }

    /// Convenience method to encode a null value
    pub fn encode_null(buf: &mut BytesMut) {
        buf.put_slice(b"_\r\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    // ===== SimpleString Tests =====

    #[test]
    fn test_encode_simple_string() {
        let mut buf = BytesMut::new();
        let value = RespValue::SimpleString(Bytes::from("OK"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"+OK\r\n");
    }

    #[test]
    fn test_encode_simple_string_with_spaces() {
        let mut buf = BytesMut::new();
        let value = RespValue::SimpleString(Bytes::from("Hello World"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"+Hello World\r\n");
    }

    #[test]
    fn test_encode_simple_string_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::SimpleString(Bytes::from(""));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"+\r\n");
    }

    // ===== Error Tests =====

    #[test]
    fn test_encode_error() {
        let mut buf = BytesMut::new();
        let value = RespValue::Error(Bytes::from("ERR unknown command"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"-ERR unknown command\r\n");
    }

    #[test]
    fn test_encode_error_simple() {
        let mut buf = BytesMut::new();
        let value = RespValue::Error(Bytes::from("Error"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"-Error\r\n");
    }

    // ===== Integer Tests =====

    #[test]
    fn test_encode_integer_positive() {
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(123);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b":123\r\n");
    }

    #[test]
    fn test_encode_integer_negative() {
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(-456);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b":-456\r\n");
    }

    #[test]
    fn test_encode_integer_zero() {
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(0);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b":0\r\n");
    }

    #[test]
    fn test_encode_integer_large() {
        let mut buf = BytesMut::new();
        let value = RespValue::Integer(9223372036854775807); // i64::MAX
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b":9223372036854775807\r\n");
    }

    // ===== BulkString Tests =====

    #[test]
    fn test_encode_bulk_string() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(Some(Bytes::from("hello")));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_encode_bulk_string_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(Some(Bytes::from("")));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$0\r\n\r\n");
    }

    #[test]
    fn test_encode_bulk_string_null() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(None);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$-1\r\n");
    }

    #[test]
    fn test_encode_bulk_string_with_crlf() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkString(Some(Bytes::from("hello\r\nworld")));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"$12\r\nhello\r\nworld\r\n");
    }

    #[test]
    fn test_encode_bulk_string_binary() {
        let mut buf = BytesMut::new();
        let data = vec![0x00, 0x01, 0x02, 0xff];
        let value = RespValue::BulkString(Some(Bytes::from(data.clone())));
        RespEncoder::encode(&value, &mut buf).unwrap();

        let expected = b"$4\r\n";
        assert_eq!(&buf[..expected.len()], expected);
        assert_eq!(&buf[expected.len()..expected.len() + 4], &data[..]);
        assert_eq!(&buf[expected.len() + 4..], b"\r\n");
    }

    // ===== Array Tests =====

    #[test]
    fn test_encode_array_simple() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::BulkString(Some(Bytes::from("key"))),
        ]));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");
    }

    #[test]
    fn test_encode_array_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![]));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*0\r\n");
    }

    #[test]
    fn test_encode_array_null() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(None);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*-1\r\n");
    }

    #[test]
    fn test_encode_array_nested() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![
            RespValue::Integer(1),
            RespValue::Array(Some(vec![RespValue::Integer(2), RespValue::Integer(3)])),
            RespValue::Integer(4),
        ]));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*3\r\n:1\r\n*2\r\n:2\r\n:3\r\n:4\r\n");
    }

    #[test]
    fn test_encode_array_mixed_types() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![
            RespValue::SimpleString(Bytes::from("OK")),
            RespValue::Integer(42),
            RespValue::BulkString(Some(Bytes::from("data"))),
            RespValue::BulkString(None),
        ]));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*4\r\n+OK\r\n:42\r\n$4\r\ndata\r\n$-1\r\n");
    }

    #[test]
    fn test_encode_array_with_null_elements() {
        let mut buf = BytesMut::new();
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("a"))),
            RespValue::BulkString(None),
            RespValue::BulkString(Some(Bytes::from("b"))),
        ]));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"*3\r\n$1\r\na\r\n$-1\r\n$1\r\nb\r\n");
    }

    // ===== RESP3 Types Tests (for basic compatibility) =====

    #[test]
    fn test_encode_null() {
        let mut buf = BytesMut::new();
        let value = RespValue::Null;
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"_\r\n");
    }

    #[test]
    fn test_encode_boolean_true() {
        let mut buf = BytesMut::new();
        let value = RespValue::Boolean(true);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"#t\r\n");
    }

    #[test]
    fn test_encode_boolean_false() {
        let mut buf = BytesMut::new();
        let value = RespValue::Boolean(false);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"#f\r\n");
    }

    // ===== Roundtrip Tests =====

    #[test]
    fn test_roundtrip_simple_string() {
        use crate::parser::RespParser;

        let original = RespValue::SimpleString(Bytes::from("Hello"));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_bulk_string() {
        use crate::parser::RespParser;

        let original = RespValue::BulkString(Some(Bytes::from("test\r\ndata")));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_array() {
        use crate::parser::RespParser;

        let original = RespValue::Array(Some(vec![
            RespValue::Integer(123),
            RespValue::BulkString(Some(Bytes::from("test"))),
            RespValue::BulkString(None),
        ]));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_null_bulk_string() {
        use crate::parser::RespParser;

        let original = RespValue::BulkString(None);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_null_array() {
        use crate::parser::RespParser;

        let original = RespValue::Array(None);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    // ===== Convenience Methods Tests =====

    #[test]
    fn test_encode_ok_convenience() {
        let mut buf = BytesMut::new();
        RespEncoder::encode_ok(&mut buf);
        assert_eq!(&buf[..], b"+OK\r\n");
    }

    #[test]
    fn test_encode_error_convenience() {
        let mut buf = BytesMut::new();
        RespEncoder::encode_error("ERR test error", &mut buf);
        assert_eq!(&buf[..], b"-ERR test error\r\n");
    }

    #[test]
    fn test_encode_null_convenience() {
        let mut buf = BytesMut::new();
        RespEncoder::encode_null(&mut buf);
        assert_eq!(&buf[..], b"_\r\n");
    }

    // ===== Multiple Encodings Tests =====

    #[test]
    fn test_encode_multiple_values() {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&RespValue::SimpleString(Bytes::from("OK")), &mut buf).unwrap();
        RespEncoder::encode(&RespValue::Integer(42), &mut buf).unwrap();
        RespEncoder::encode(&RespValue::BulkString(Some(Bytes::from("data"))), &mut buf).unwrap();

        assert_eq!(&buf[..], b"+OK\r\n:42\r\n$4\r\ndata\r\n");
    }

    // ===== RESP3 Extended Tests =====

    #[test]
    fn test_encode_double_normal() {
        let mut buf = BytesMut::new();
        let value = RespValue::Double(123.456);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..6], b",123.4");
        assert!(buf.ends_with(b"\r\n"));
    }

    #[test]
    fn test_encode_double_infinity() {
        let mut buf = BytesMut::new();
        let value = RespValue::Double(f64::INFINITY);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b",inf\r\n");
    }

    #[test]
    fn test_encode_double_neg_infinity() {
        let mut buf = BytesMut::new();
        let value = RespValue::Double(f64::NEG_INFINITY);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b",-inf\r\n");
    }

    #[test]
    fn test_encode_double_nan() {
        let mut buf = BytesMut::new();
        let value = RespValue::Double(f64::NAN);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b",nan\r\n");
    }

    #[test]
    fn test_encode_big_number() {
        let mut buf = BytesMut::new();
        let value =
            RespValue::BigNumber(Bytes::from("3492890328409238509324850943850943825024385"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(
            &buf[..],
            b"(3492890328409238509324850943850943825024385\r\n"
        );
    }

    #[test]
    fn test_encode_big_number_negative() {
        let mut buf = BytesMut::new();
        let value =
            RespValue::BigNumber(Bytes::from("-3492890328409238509324850943850943825024385"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(
            &buf[..],
            b"(-3492890328409238509324850943850943825024385\r\n"
        );
    }

    #[test]
    fn test_encode_bulk_error() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkError(Bytes::from("SYNTAX invalid syntax"));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"!21\r\nSYNTAX invalid syntax\r\n");
    }

    #[test]
    fn test_encode_bulk_error_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::BulkError(Bytes::from(""));
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"!0\r\n\r\n");
    }

    #[test]
    fn test_encode_verbatim_string_txt() {
        let mut buf = BytesMut::new();
        let value = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("Some string"),
        };
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"=15\r\ntxt:Some string\r\n");
    }

    #[test]
    fn test_encode_verbatim_string_mkd() {
        let mut buf = BytesMut::new();
        let value = RespValue::VerbatimString {
            format: *b"mkd",
            data: Bytes::from("# Markdown"),
        };
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"=14\r\nmkd:# Markdown\r\n");
    }

    #[test]
    fn test_encode_map_simple() {
        let mut buf = BytesMut::new();
        let value = RespValue::Map(vec![
            (
                RespValue::SimpleString(Bytes::from("first")),
                RespValue::Integer(1),
            ),
            (
                RespValue::SimpleString(Bytes::from("second")),
                RespValue::Integer(2),
            ),
        ]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"%2\r\n+first\r\n:1\r\n+second\r\n:2\r\n");
    }

    #[test]
    fn test_encode_map_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::Map(vec![]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"%0\r\n");
    }

    #[test]
    fn test_encode_map_with_null_value() {
        let mut buf = BytesMut::new();
        let value = RespValue::Map(vec![(
            RespValue::BulkString(Some(Bytes::from("key"))),
            RespValue::Null,
        )]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"%1\r\n$3\r\nkey\r\n_\r\n");
    }

    #[test]
    fn test_encode_set_simple() {
        let mut buf = BytesMut::new();
        let value = RespValue::Set(vec![
            RespValue::SimpleString(Bytes::from("orange")),
            RespValue::SimpleString(Bytes::from("apple")),
        ]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"~2\r\n+orange\r\n+apple\r\n");
    }

    #[test]
    fn test_encode_set_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::Set(vec![]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"~0\r\n");
    }

    #[test]
    fn test_encode_push_simple() {
        let mut buf = BytesMut::new();
        let value = RespValue::Push(vec![
            RespValue::SimpleString(Bytes::from("pubsub")),
            RespValue::SimpleString(Bytes::from("message")),
            RespValue::BulkString(Some(Bytes::from("channel"))),
            RespValue::BulkString(Some(Bytes::from("Hello!"))),
        ]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(
            &buf[..],
            b">4\r\n+pubsub\r\n+message\r\n$7\r\nchannel\r\n$6\r\nHello!\r\n"
        );
    }

    #[test]
    fn test_encode_push_empty() {
        let mut buf = BytesMut::new();
        let value = RespValue::Push(vec![]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b">0\r\n");
    }

    // ===== RESP3 Roundtrip Tests =====

    #[test]
    fn test_roundtrip_double() {
        use crate::parser::RespParser;

        let original = RespValue::Double(123.456);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_big_number() {
        use crate::parser::RespParser;

        let original =
            RespValue::BigNumber(Bytes::from("3492890328409238509324850943850943825024385"));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_bulk_error() {
        use crate::parser::RespParser;

        let original = RespValue::BulkError(Bytes::from("SYNTAX invalid syntax"));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_verbatim_string() {
        use crate::parser::RespParser;

        let original = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("Some string"),
        };
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_map() {
        use crate::parser::RespParser;

        let original = RespValue::Map(vec![
            (
                RespValue::SimpleString(Bytes::from("key1")),
                RespValue::Integer(100),
            ),
            (
                RespValue::BulkString(Some(Bytes::from("key2"))),
                RespValue::Null,
            ),
        ]);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_set() {
        use crate::parser::RespParser;

        let original = RespValue::Set(vec![
            RespValue::Integer(1),
            RespValue::Integer(2),
            RespValue::Integer(3),
        ]);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_push() {
        use crate::parser::RespParser;

        let original = RespValue::Push(vec![
            RespValue::SimpleString(Bytes::from("pubsub")),
            RespValue::SimpleString(Bytes::from("message")),
        ]);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&original, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        assert_eq!(original, decoded);
    }

    // ===== Complex Nested Structure Tests =====

    #[test]
    fn test_encode_nested_map_with_array() {
        let mut buf = BytesMut::new();
        let value = RespValue::Map(vec![(
            RespValue::SimpleString(Bytes::from("items")),
            RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Integer(2),
                RespValue::Integer(3),
            ])),
        )]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(&buf[..], b"%1\r\n+items\r\n*3\r\n:1\r\n:2\r\n:3\r\n");
    }

    #[test]
    fn test_encode_set_with_bulk_strings() {
        let mut buf = BytesMut::new();
        let value = RespValue::Set(vec![
            RespValue::BulkString(Some(Bytes::from("alpha"))),
            RespValue::BulkString(Some(Bytes::from("beta"))),
            RespValue::BulkString(Some(Bytes::from("gamma"))),
        ]);
        RespEncoder::encode(&value, &mut buf).unwrap();
        assert_eq!(
            &buf[..],
            b"~3\r\n$5\r\nalpha\r\n$4\r\nbeta\r\n$5\r\ngamma\r\n"
        );
    }
}
