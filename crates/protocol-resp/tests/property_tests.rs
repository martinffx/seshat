//! Property-based tests for RESP protocol using proptest
//!
//! These tests verify that the parser and encoder are:
//! 1. Inverses of each other (roundtrip property)
//! 2. Robust against arbitrary input (parser never panics)
//! 3. Properly enforce limits (depth, size)

use bytes::{Bytes, BytesMut};
use proptest::prelude::*;
use seshat_protocol_resp::{RespEncoder, RespParser, RespValue};

// ============================================================================
// PROPERTY GENERATORS
// ============================================================================

/// Generate arbitrary bytes for BulkString and other types that can contain any data
fn arb_bytes() -> impl Strategy<Value = Bytes> {
    // Generate Vec<u8> and convert to Bytes
    // Limit size to 1024 bytes for reasonable test performance
    prop::collection::vec(any::<u8>(), 0..=1024).prop_map(Bytes::from)
}

/// Generate arbitrary bytes for SimpleString/Error (no \r or \n)
fn arb_simple_bytes() -> impl Strategy<Value = Bytes> {
    // Filter out \r (13) and \n (10) since they're delimiters in RESP
    prop::collection::vec(
        any::<u8>().prop_filter("no CRLF", |&b| b != b'\r' && b != b'\n'),
        0..=1024,
    )
    .prop_map(Bytes::from)
}

/// Generate arbitrary integers (RESP2)
fn arb_integer() -> impl Strategy<Value = RespValue> {
    any::<i64>().prop_map(RespValue::Integer)
}

/// Generate arbitrary boolean values (RESP3)
fn arb_boolean() -> impl Strategy<Value = RespValue> {
    any::<bool>().prop_map(RespValue::Boolean)
}

/// Generate arbitrary bulk strings including null case (RESP2)
fn arb_bulk_string() -> impl Strategy<Value = RespValue> {
    prop_oneof![
        // None case (null bulk string)
        Just(RespValue::BulkString(None)),
        // Some case with arbitrary bytes
        arb_bytes().prop_map(|b| RespValue::BulkString(Some(b))),
    ]
}

/// Generate arbitrary doubles (RESP3)
fn arb_double() -> impl Strategy<Value = RespValue> {
    prop_oneof![
        // Regular finite numbers
        any::<f64>()
            .prop_filter("finite", |d| d.is_finite())
            .prop_map(RespValue::Double),
        // Special values
        Just(RespValue::Double(f64::INFINITY)),
        Just(RespValue::Double(f64::NEG_INFINITY)),
        Just(RespValue::Double(f64::NAN)),
    ]
}

/// Generate arbitrary big numbers (RESP3)
fn arb_big_number() -> impl Strategy<Value = RespValue> {
    // Generate string representation of large integers
    prop_oneof![
        // Positive big number
        "[0-9]{30,100}".prop_map(|s| RespValue::BigNumber(Bytes::from(s))),
        // Negative big number
        "-[0-9]{30,100}".prop_map(|s| RespValue::BigNumber(Bytes::from(s))),
    ]
}

/// Generate arbitrary bulk errors (RESP3)
fn arb_bulk_error() -> impl Strategy<Value = RespValue> {
    arb_bytes().prop_map(RespValue::BulkError)
}

/// Generate arbitrary verbatim strings (RESP3)
fn arb_verbatim_string() -> impl Strategy<Value = RespValue> {
    // Format is exactly 3 ASCII bytes
    let format_strategy = prop::array::uniform3(b'a'..=b'z');

    (format_strategy, arb_bytes())
        .prop_map(|(format, data)| RespValue::VerbatimString { format, data })
}

/// Generate arbitrary double values without NaN/Inf (for nested structures)
fn arb_double_no_nan() -> impl Strategy<Value = RespValue> {
    // Generate finite doubles only to avoid NaN comparison issues in nested structures
    prop::num::f64::NORMAL.prop_map(RespValue::Double)
}

/// Generate leaf (non-recursive) RESP values without NaN (for nested structures)
/// Excludes SimpleString and Error to avoid CRLF filter rejection issues
fn arb_leaf() -> impl Strategy<Value = RespValue> {
    prop_oneof![
        arb_integer(),
        arb_boolean(),
        arb_bulk_string(),
        Just(RespValue::Null),
        arb_double_no_nan(), // Use non-NaN version for nested structures
        arb_big_number(),
        arb_bulk_error(),
        arb_verbatim_string(),
    ]
}

/// Generate arbitrary arrays with depth limit (RESP2/RESP3)
fn arb_array(depth: u32) -> impl Strategy<Value = RespValue> {
    if depth == 0 {
        // At max depth, only generate null or empty arrays
        prop_oneof![
            Just(RespValue::Array(None)),
            Just(RespValue::Array(Some(vec![]))),
        ]
        .boxed()
    } else {
        prop_oneof![
            // Null array
            Just(RespValue::Array(None)),
            // Non-empty array with elements
            prop::collection::vec(arb_resp_value(depth - 1), 0..=10)
                .prop_map(|v| RespValue::Array(Some(v))),
        ]
        .boxed()
    }
}

/// Generate arbitrary maps with depth limit (RESP3)
fn arb_map(depth: u32) -> impl Strategy<Value = RespValue> {
    if depth == 0 {
        // At max depth, only generate empty maps
        Just(RespValue::Map(vec![])).boxed()
    } else {
        // Generate vector of key-value pairs
        prop::collection::vec(
            (arb_resp_value(depth - 1), arb_resp_value(depth - 1)),
            0..=10,
        )
        .prop_map(RespValue::Map)
        .boxed()
    }
}

/// Generate arbitrary sets with depth limit (RESP3)
fn arb_set(depth: u32) -> impl Strategy<Value = RespValue> {
    if depth == 0 {
        // At max depth, only generate empty sets
        Just(RespValue::Set(vec![])).boxed()
    } else {
        prop::collection::vec(arb_resp_value(depth - 1), 0..=10)
            .prop_map(RespValue::Set)
            .boxed()
    }
}

/// Generate arbitrary push messages with depth limit (RESP3)
fn arb_push(depth: u32) -> impl Strategy<Value = RespValue> {
    if depth == 0 {
        // At max depth, only generate empty push
        Just(RespValue::Push(vec![])).boxed()
    } else {
        prop::collection::vec(arb_resp_value(depth - 1), 0..=10)
            .prop_map(RespValue::Push)
            .boxed()
    }
}

/// Generate arbitrary RESP values with depth limit
fn arb_resp_value(depth: u32) -> impl Strategy<Value = RespValue> {
    if depth == 0 {
        arb_leaf().boxed()
    } else {
        prop_oneof![
            3 => arb_leaf(),
            1 => arb_array(depth),
            1 => arb_map(depth),
            1 => arb_set(depth),
            1 => arb_push(depth),
        ]
        .boxed()
    }
}

/// Generate arbitrary RESP value with reasonable depth (max 5)
fn arb_resp_value_shallow() -> impl Strategy<Value = RespValue> {
    arb_resp_value(5)
}

// ============================================================================
// ROUNDTRIP PROPERTY TESTS
// ============================================================================

proptest! {
    #[test]
    fn prop_roundtrip_simple_string(bytes in arb_simple_bytes()) {
        let value = RespValue::SimpleString(bytes);
        let mut buf = BytesMut::new();

        // Encode
        RespEncoder::encode(&value, &mut buf).unwrap();

        // Decode
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Error roundtrips correctly
    #[test]
    fn prop_roundtrip_error(bytes in arb_simple_bytes()) {
        let value = RespValue::Error(bytes);
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Integer roundtrips correctly
    #[test]
    fn prop_roundtrip_integer(i in any::<i64>()) {
        let value = RespValue::Integer(i);
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Boolean roundtrips correctly
    #[test]
    fn prop_roundtrip_boolean(b in any::<bool>()) {
        let value = RespValue::Boolean(b);
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Null roundtrips correctly
    #[test]
    fn prop_roundtrip_null(_dummy in 0..1u8) {
        let value = RespValue::Null;
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: BulkString roundtrips correctly (including null case)
    #[test]
    fn prop_roundtrip_bulk_string(value in arb_bulk_string()) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Array roundtrips correctly
    #[test]
    fn prop_roundtrip_array(value in arb_array(3)) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Double roundtrips correctly (special handling for NaN)
    #[test]
    fn prop_roundtrip_double(value in arb_double()) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        // Special handling for NaN (NaN != NaN)
        match (&value, &decoded) {
            (RespValue::Double(a), RespValue::Double(b)) => {
                if a.is_nan() {
                    prop_assert!(b.is_nan());
                } else {
                    prop_assert_eq!(value, decoded);
                }
            }
            _ => prop_assert_eq!(value, decoded),
        }
    }

    /// Test: BigNumber roundtrips correctly
    #[test]
    fn prop_roundtrip_big_number(value in arb_big_number()) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: BulkError roundtrips correctly
    #[test]
    fn prop_roundtrip_bulk_error(bytes in arb_bytes()) {
        let value = RespValue::BulkError(bytes);
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: VerbatimString roundtrips correctly
    #[test]
    fn prop_roundtrip_verbatim_string(value in arb_verbatim_string()) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Map roundtrips correctly
    #[test]
    fn prop_roundtrip_map(value in arb_map(3)) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Set roundtrips correctly
    #[test]
    fn prop_roundtrip_set(value in arb_set(3)) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Push roundtrips correctly
    #[test]
    fn prop_roundtrip_push(value in arb_push(3)) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Complex nested structures roundtrip correctly
    #[test]
    fn prop_roundtrip_nested_structures(value in arb_resp_value_shallow()) {
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        // Special handling for NaN in nested structures
        match (&value, &decoded) {
            (RespValue::Double(a), RespValue::Double(b)) if a.is_nan() && b.is_nan() => {
                // Both are NaN, consider equal
            }
            _ => prop_assert_eq!(value, decoded),
        }
    }
}

// ============================================================================
// PARSER ROBUSTNESS TESTS
// ============================================================================

proptest! {
    /// Test: Parser never panics on arbitrary input
    #[test]
    fn prop_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..=1024)) {
        let mut buf = BytesMut::from(&bytes[..]);
        let mut parser = RespParser::new();

        // Should return Ok or Err, never panic
        let _ = parser.parse(&mut buf);

        // If we get here, parser didn't panic
        prop_assert!(true);
    }

    /// Test: Parser respects bulk string size limits
    #[test]
    fn prop_parser_respects_bulk_size_limit(size in 1000usize..2000usize) {
        let max_size = 500;
        let mut parser = RespParser::new().with_max_bulk_size(max_size);

        // Try to parse bulk string larger than limit
        let mut buf = BytesMut::from(format!("${size}\r\n").as_bytes());

        let result = parser.parse(&mut buf);

        if size > max_size {
            // Should error
            prop_assert!(result.is_err());
        }
    }

    /// Test: Parser respects array length limits
    #[test]
    fn prop_parser_respects_array_limit(size in 100usize..200usize) {
        let max_len = 50;
        let mut parser = RespParser::new().with_max_array_len(max_len);

        // Try to parse array larger than limit
        let mut buf = BytesMut::from(format!("*{size}\r\n").as_bytes());

        let result = parser.parse(&mut buf);

        if size > max_len {
            // Should error
            prop_assert!(result.is_err());
        }
    }

    /// Test: Parser respects depth limits
    #[test]
    fn prop_parser_respects_depth_limit(depth in 5usize..10usize) {
        let max_depth = 3;
        let mut parser = RespParser::new().with_max_depth(max_depth);

        // Build deeply nested array
        let mut buf = BytesMut::new();
        for _ in 0..depth {
            buf.extend_from_slice(b"*1\r\n");
        }
        buf.extend_from_slice(b":1\r\n");

        let result = parser.parse(&mut buf);

        if depth > max_depth {
            // Should error
            prop_assert!(result.is_err());
        }
    }
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

proptest! {
    /// Test: Empty values roundtrip correctly
    #[test]
    fn prop_empty_values(_dummy in 0..1u8) {
        // Empty simple string
        let value = RespValue::SimpleString(Bytes::from(""));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);

        // Empty bulk string
        let value = RespValue::BulkString(Some(Bytes::from("")));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);

        // Empty array
        let value = RespValue::Array(Some(vec![]));
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);
    }

    /// Test: Large integers roundtrip correctly
    #[test]
    fn prop_large_integers(i in any::<i64>()) {
        let value = RespValue::Integer(i);
        let mut buf = BytesMut::new();

        RespEncoder::encode(&value, &mut buf).unwrap();

        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();

        prop_assert_eq!(value, decoded);
    }

    /// Test: Null handling for different types
    #[test]
    fn prop_null_handling(_dummy in 0..1u8) {
        // RESP3 Null
        let value = RespValue::Null;
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);

        // Null bulk string
        let value = RespValue::BulkString(None);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);

        // Null array
        let value = RespValue::Array(None);
        let mut buf = BytesMut::new();
        RespEncoder::encode(&value, &mut buf).unwrap();
        let mut parser = RespParser::new();
        let decoded = parser.parse(&mut buf).unwrap().unwrap();
        prop_assert_eq!(value, decoded);
    }
}
