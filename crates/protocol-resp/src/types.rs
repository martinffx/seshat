//! RESP3 data types
//!
//! This module defines all 14 RESP3 data types for the Redis Serialization Protocol.
//! It uses `bytes::Bytes` for zero-copy efficiency.

use bytes::Bytes;

/// RESP3 value type representing all 14 data types
///
/// # RESP2 Compatible Types (5)
///
/// - **SimpleString**: `+OK\r\n`
/// - **Error**: `-ERR unknown command\r\n`
/// - **Integer**: `:1000\r\n`
/// - **BulkString**: `$5\r\nhello\r\n` or `$-1\r\n` for null
/// - **Array**: `*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n` or `*-1\r\n` for null
///
/// # RESP3 Only Types (9)
///
/// - **Null**: `_\r\n`
/// - **Boolean**: `#t\r\n` or `#f\r\n`
/// - **Double**: `,1.23\r\n` or `,inf\r\n` or `,-inf\r\n` or `,nan\r\n`
/// - **BigNumber**: `(3492890328409238509324850943850943825024385\r\n`
/// - **BulkError**: `!21\r\nSYNTAX invalid syntax\r\n`
/// - **VerbatimString**: `=15\r\ntxt:Some string\r\n`
/// - **Map**: `%2\r\n+first\r\n:1\r\n+second\r\n:2\r\n`
/// - **Set**: `~5\r\n+orange\r\n+apple\r\n...\r\n`
/// - **Push**: `>4\r\n+pubsub\r\n+message\r\n...\r\n`
#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    // RESP2 Compatible Types
    /// Simple string: `+OK\r\n`
    SimpleString(Bytes),

    /// Error: `-ERR unknown command\r\n`
    Error(Bytes),

    /// Integer: `:1000\r\n`
    Integer(i64),

    /// Bulk string: `$5\r\nhello\r\n` (or `$-1\r\n` for null)
    BulkString(Option<Bytes>),

    /// Array: `*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n` (or `*-1\r\n` for null)
    Array(Option<Vec<RespValue>>),

    // RESP3-Only Types
    /// Null: `_\r\n`
    Null,

    /// Boolean: `#t\r\n` or `#f\r\n`
    Boolean(bool),

    /// Double: `,1.23\r\n` or `,inf\r\n` or `,-inf\r\n` or `,nan\r\n`
    Double(f64),

    /// Big number: `(3492890328409238509324850943850943825024385\r\n`
    BigNumber(Bytes),

    /// Bulk error: `!21\r\nSYNTAX invalid syntax\r\n`
    BulkError(Bytes),

    /// Verbatim string: `=15\r\ntxt:Some string\r\n`
    ///
    /// The format field is exactly 3 bytes, typically:
    /// - `b"txt"` for plain text
    /// - `b"mkd"` for markdown
    VerbatimString {
        /// Format identifier (exactly 3 bytes, e.g., b"txt" or b"mkd")
        format: [u8; 3],
        /// String data
        data: Bytes,
    },

    /// Map: `%2\r\n+first\r\n:1\r\n+second\r\n:2\r\n`
    ///
    /// Uses Vec to preserve insertion order
    Map(Vec<(RespValue, RespValue)>),

    /// Set: `~5\r\n+orange\r\n+apple\r\n...\r\n`
    ///
    /// Uses Vec to preserve insertion order
    Set(Vec<RespValue>),

    /// Push: `>4\r\n+pubsub\r\n+message\r\n...\r\n`
    ///
    /// Used for out-of-band data like pub/sub messages
    Push(Vec<RespValue>),
}

impl RespValue {
    /// Returns true if this is a null value.
    ///
    /// Checks for RESP2 `$-1\r\n` (BulkString(None)), RESP2 `*-1\r\n` (Array(None)),
    /// and RESP3 `_\r\n` (Null).
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::types::RespValue;
    ///
    /// assert!(RespValue::Null.is_null());
    /// assert!(RespValue::BulkString(None).is_null());
    /// assert!(RespValue::Array(None).is_null());
    /// assert!(!RespValue::Integer(42).is_null());
    /// ```
    pub fn is_null(&self) -> bool {
        matches!(
            self,
            RespValue::Null | RespValue::BulkString(None) | RespValue::Array(None)
        )
    }

    /// Extract byte data from string-like types.
    ///
    /// Returns `Some(&Bytes)` for:
    /// - SimpleString
    /// - BulkString (if not null)
    /// - Error
    /// - BigNumber
    /// - BulkError
    /// - VerbatimString
    ///
    /// Returns `None` for all other types.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::types::RespValue;
    /// use bytes::Bytes;
    ///
    /// let value = RespValue::SimpleString(Bytes::from("OK"));
    /// assert_eq!(value.as_bytes(), Some(&Bytes::from("OK")));
    ///
    /// let value = RespValue::Integer(42);
    /// assert_eq!(value.as_bytes(), None);
    /// ```
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            RespValue::SimpleString(b) => Some(b),
            RespValue::BulkString(Some(b)) => Some(b),
            RespValue::Error(b) => Some(b),
            RespValue::BigNumber(b) => Some(b),
            RespValue::BulkError(b) => Some(b),
            RespValue::VerbatimString { data, .. } => Some(data),
            _ => None,
        }
    }

    /// Extract integer value.
    ///
    /// Returns `Some(i64)` for Integer type, `None` for all others.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::types::RespValue;
    ///
    /// let value = RespValue::Integer(1000);
    /// assert_eq!(value.as_integer(), Some(1000));
    ///
    /// let value = RespValue::Null;
    /// assert_eq!(value.as_integer(), None);
    /// ```
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            RespValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Extract array elements, consuming the value.
    ///
    /// Returns `Some(Vec<RespValue>)` for Array(Some), `None` for all others.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::types::RespValue;
    /// use bytes::Bytes;
    ///
    /// let value = RespValue::Array(Some(vec![
    ///     RespValue::SimpleString(Bytes::from("GET")),
    ///     RespValue::SimpleString(Bytes::from("key")),
    /// ]));
    /// let elements = value.into_array();
    /// assert!(elements.is_some());
    /// assert_eq!(elements.unwrap().len(), 2);
    /// ```
    pub fn into_array(self) -> Option<Vec<RespValue>> {
        match self {
            RespValue::Array(Some(arr)) => Some(arr),
            _ => None,
        }
    }

    /// Calculate approximate wire size in bytes.
    ///
    /// This provides an estimate for buffer allocation. The actual wire size
    /// may vary slightly due to integer encoding and formatting overhead.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::types::RespValue;
    /// use bytes::Bytes;
    ///
    /// let value = RespValue::SimpleString(Bytes::from("OK"));
    /// let size = value.size_estimate();
    /// assert!(size > 0);
    /// ```
    pub fn size_estimate(&self) -> usize {
        match self {
            RespValue::SimpleString(b) | RespValue::Error(b) => b.len() + 10,
            RespValue::BulkString(Some(b)) => b.len() + 15,
            RespValue::BulkString(None) => 5,
            RespValue::Integer(_) => 15,
            RespValue::Array(Some(arr)) => {
                arr.iter().map(|v| v.size_estimate()).sum::<usize>() + 10
            }
            RespValue::Array(None) => 5,
            RespValue::Map(pairs) => {
                pairs
                    .iter()
                    .map(|(k, v)| k.size_estimate() + v.size_estimate())
                    .sum::<usize>()
                    + 10
            }
            RespValue::Set(items) => items.iter().map(|v| v.size_estimate()).sum::<usize>() + 10,
            RespValue::BigNumber(b) => b.len() + 10,
            RespValue::BulkError(b) => b.len() + 15,
            RespValue::VerbatimString { data, .. } => data.len() + 20,
            RespValue::Push(items) => items.iter().map(|v| v.size_estimate()).sum::<usize>() + 10,
            // Other types (Null, Boolean, Double)
            _ => 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RESP2 Type Construction Tests

    #[test]
    fn test_simple_string_construction() {
        let value = RespValue::SimpleString(Bytes::from("OK"));
        match value {
            RespValue::SimpleString(s) => assert_eq!(s, "OK"),
            _ => panic!("Expected SimpleString"),
        }
    }

    #[test]
    fn test_error_construction() {
        let value = RespValue::Error(Bytes::from("ERR unknown command"));
        match value {
            RespValue::Error(e) => assert_eq!(e, "ERR unknown command"),
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_integer_construction() {
        let value = RespValue::Integer(1000);
        match value {
            RespValue::Integer(i) => assert_eq!(i, 1000),
            _ => panic!("Expected Integer"),
        }
    }

    #[test]
    fn test_integer_negative() {
        let value = RespValue::Integer(-42);
        match value {
            RespValue::Integer(i) => assert_eq!(i, -42),
            _ => panic!("Expected Integer"),
        }
    }

    #[test]
    fn test_bulk_string_construction() {
        let value = RespValue::BulkString(Some(Bytes::from("hello")));
        match value {
            RespValue::BulkString(Some(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected BulkString with data"),
        }
    }

    #[test]
    fn test_bulk_string_null() {
        let value = RespValue::BulkString(None);
        assert_eq!(value, RespValue::BulkString(None));
    }

    #[test]
    fn test_bulk_string_empty() {
        let value = RespValue::BulkString(Some(Bytes::from("")));
        match value {
            RespValue::BulkString(Some(s)) => assert_eq!(s.len(), 0),
            _ => panic!("Expected empty BulkString"),
        }
    }

    #[test]
    fn test_array_construction() {
        let value = RespValue::Array(Some(vec![
            RespValue::SimpleString(Bytes::from("GET")),
            RespValue::BulkString(Some(Bytes::from("key"))),
        ]));
        match value {
            RespValue::Array(Some(arr)) => {
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], RespValue::SimpleString(Bytes::from("GET")));
            }
            _ => panic!("Expected Array with data"),
        }
    }

    #[test]
    fn test_array_null() {
        let value = RespValue::Array(None);
        assert_eq!(value, RespValue::Array(None));
    }

    #[test]
    fn test_array_empty() {
        let value = RespValue::Array(Some(Vec::new()));
        match value {
            RespValue::Array(Some(arr)) => assert_eq!(arr.len(), 0),
            _ => panic!("Expected empty Array"),
        }
    }

    // RESP3 Type Construction Tests

    #[test]
    fn test_null_construction() {
        let value = RespValue::Null;
        assert_eq!(value, RespValue::Null);
    }

    #[test]
    fn test_boolean_true() {
        let value = RespValue::Boolean(true);
        match value {
            RespValue::Boolean(b) => assert!(b),
            _ => panic!("Expected Boolean"),
        }
    }

    #[test]
    fn test_boolean_false() {
        let value = RespValue::Boolean(false);
        match value {
            RespValue::Boolean(b) => assert!(!b),
            _ => panic!("Expected Boolean"),
        }
    }

    #[test]
    fn test_double_construction() {
        let value = RespValue::Double(1.23);
        match value {
            RespValue::Double(d) => assert!((d - 1.23).abs() < f64::EPSILON),
            _ => panic!("Expected Double"),
        }
    }

    #[test]
    fn test_double_infinity() {
        let value = RespValue::Double(f64::INFINITY);
        match value {
            RespValue::Double(d) => assert!(d.is_infinite() && d.is_sign_positive()),
            _ => panic!("Expected Double"),
        }
    }

    #[test]
    fn test_double_negative_infinity() {
        let value = RespValue::Double(f64::NEG_INFINITY);
        match value {
            RespValue::Double(d) => assert!(d.is_infinite() && d.is_sign_negative()),
            _ => panic!("Expected Double"),
        }
    }

    #[test]
    fn test_double_nan() {
        let value = RespValue::Double(f64::NAN);
        match value {
            RespValue::Double(d) => assert!(d.is_nan()),
            _ => panic!("Expected Double"),
        }
    }

    #[test]
    fn test_big_number_construction() {
        let value =
            RespValue::BigNumber(Bytes::from("3492890328409238509324850943850943825024385"));
        match value {
            RespValue::BigNumber(n) => {
                assert_eq!(n, "3492890328409238509324850943850943825024385")
            }
            _ => panic!("Expected BigNumber"),
        }
    }

    #[test]
    fn test_bulk_error_construction() {
        let value = RespValue::BulkError(Bytes::from("SYNTAX invalid syntax"));
        match value {
            RespValue::BulkError(e) => assert_eq!(e, "SYNTAX invalid syntax"),
            _ => panic!("Expected BulkError"),
        }
    }

    #[test]
    fn test_verbatim_string_construction() {
        let value = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("Some string"),
        };
        match value {
            RespValue::VerbatimString { format, data } => {
                assert_eq!(format, *b"txt");
                assert_eq!(data, "Some string");
            }
            _ => panic!("Expected VerbatimString"),
        }
    }

    #[test]
    fn test_verbatim_string_format_exactly_three_bytes() {
        let value = RespValue::VerbatimString {
            format: *b"mkd",
            data: Bytes::from("# Markdown"),
        };
        match value {
            RespValue::VerbatimString { format, .. } => {
                assert_eq!(format.len(), 3);
                assert_eq!(format, *b"mkd");
            }
            _ => panic!("Expected VerbatimString"),
        }
    }

    #[test]
    fn test_map_construction() {
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
        match value {
            RespValue::Map(pairs) => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0].0, RespValue::SimpleString(Bytes::from("first")));
                assert_eq!(pairs[0].1, RespValue::Integer(1));
            }
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_map_preserves_insertion_order() {
        let value = RespValue::Map(vec![
            (
                RespValue::SimpleString(Bytes::from("z")),
                RespValue::Integer(3),
            ),
            (
                RespValue::SimpleString(Bytes::from("a")),
                RespValue::Integer(1),
            ),
            (
                RespValue::SimpleString(Bytes::from("m")),
                RespValue::Integer(2),
            ),
        ]);
        match value {
            RespValue::Map(pairs) => {
                // Order should be z, a, m (insertion order, not sorted)
                assert_eq!(pairs[0].0, RespValue::SimpleString(Bytes::from("z")));
                assert_eq!(pairs[1].0, RespValue::SimpleString(Bytes::from("a")));
                assert_eq!(pairs[2].0, RespValue::SimpleString(Bytes::from("m")));
            }
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_map_empty() {
        let value = RespValue::Map(Vec::new());
        match value {
            RespValue::Map(pairs) => assert_eq!(pairs.len(), 0),
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_set_construction() {
        let value = RespValue::Set(vec![
            RespValue::SimpleString(Bytes::from("orange")),
            RespValue::SimpleString(Bytes::from("apple")),
            RespValue::SimpleString(Bytes::from("banana")),
        ]);
        match value {
            RespValue::Set(elements) => {
                assert_eq!(elements.len(), 3);
                assert_eq!(elements[0], RespValue::SimpleString(Bytes::from("orange")));
            }
            _ => panic!("Expected Set"),
        }
    }

    #[test]
    fn test_set_preserves_insertion_order() {
        let value = RespValue::Set(vec![
            RespValue::SimpleString(Bytes::from("z")),
            RespValue::SimpleString(Bytes::from("a")),
            RespValue::SimpleString(Bytes::from("m")),
        ]);
        match value {
            RespValue::Set(elements) => {
                // Order should be z, a, m (insertion order, not sorted)
                assert_eq!(elements[0], RespValue::SimpleString(Bytes::from("z")));
                assert_eq!(elements[1], RespValue::SimpleString(Bytes::from("a")));
                assert_eq!(elements[2], RespValue::SimpleString(Bytes::from("m")));
            }
            _ => panic!("Expected Set"),
        }
    }

    #[test]
    fn test_set_empty() {
        let value = RespValue::Set(Vec::new());
        match value {
            RespValue::Set(elements) => assert_eq!(elements.len(), 0),
            _ => panic!("Expected Set"),
        }
    }

    #[test]
    fn test_push_construction() {
        let value = RespValue::Push(vec![
            RespValue::SimpleString(Bytes::from("pubsub")),
            RespValue::SimpleString(Bytes::from("message")),
            RespValue::SimpleString(Bytes::from("channel")),
            RespValue::BulkString(Some(Bytes::from("Hello, World!"))),
        ]);
        match value {
            RespValue::Push(elements) => {
                assert_eq!(elements.len(), 4);
                assert_eq!(elements[0], RespValue::SimpleString(Bytes::from("pubsub")));
            }
            _ => panic!("Expected Push"),
        }
    }

    #[test]
    fn test_push_empty() {
        let value = RespValue::Push(Vec::new());
        match value {
            RespValue::Push(elements) => assert_eq!(elements.len(), 0),
            _ => panic!("Expected Push"),
        }
    }

    // Equality Tests

    #[test]
    fn test_simple_string_equality() {
        let v1 = RespValue::SimpleString(Bytes::from("OK"));
        let v2 = RespValue::SimpleString(Bytes::from("OK"));
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_simple_string_inequality() {
        let v1 = RespValue::SimpleString(Bytes::from("OK"));
        let v2 = RespValue::SimpleString(Bytes::from("ERROR"));
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_integer_equality() {
        let v1 = RespValue::Integer(42);
        let v2 = RespValue::Integer(42);
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_array_equality() {
        let v1 = RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]));
        let v2 = RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]));
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_nested_array_equality() {
        let v1 = RespValue::Array(Some(vec![RespValue::Array(Some(vec![
            RespValue::Integer(1),
        ]))]));
        let v2 = RespValue::Array(Some(vec![RespValue::Array(Some(vec![
            RespValue::Integer(1),
        ]))]));
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_null_equality() {
        let v1 = RespValue::Null;
        let v2 = RespValue::Null;
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_bulk_string_null_equality() {
        let v1 = RespValue::BulkString(None);
        let v2 = RespValue::BulkString(None);
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_different_types_not_equal() {
        let v1 = RespValue::SimpleString(Bytes::from("42"));
        let v2 = RespValue::Integer(42);
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_null_and_bulk_string_null_not_equal() {
        let v1 = RespValue::Null;
        let v2 = RespValue::BulkString(None);
        assert_ne!(v1, v2);
    }

    // Clone Tests

    #[test]
    fn test_simple_string_clone() {
        let v1 = RespValue::SimpleString(Bytes::from("OK"));
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_array_clone() {
        let v1 = RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]));
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_verbatim_string_clone() {
        let v1 = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("hello"),
        };
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_map_clone() {
        let v1 = RespValue::Map(vec![(
            RespValue::SimpleString(Bytes::from("key")),
            RespValue::Integer(1),
        )]);
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    // Debug Formatting Tests

    #[test]
    fn test_debug_simple_string() {
        let value = RespValue::SimpleString(Bytes::from("OK"));
        let debug = format!("{value:?}");
        assert!(debug.contains("SimpleString"));
        assert!(debug.contains("OK"));
    }

    #[test]
    fn test_debug_integer() {
        let value = RespValue::Integer(42);
        let debug = format!("{value:?}");
        assert!(debug.contains("Integer"));
        assert!(debug.contains("42"));
    }

    #[test]
    fn test_debug_null() {
        let value = RespValue::Null;
        let debug = format!("{value:?}");
        assert!(debug.contains("Null"));
    }

    #[test]
    fn test_debug_array() {
        let value = RespValue::Array(Some(vec![RespValue::Integer(1)]));
        let debug = format!("{value:?}");
        assert!(debug.contains("Array"));
        assert!(debug.contains("Integer"));
    }

    #[test]
    fn test_debug_verbatim_string() {
        let value = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("hello"),
        };
        let debug = format!("{value:?}");
        assert!(debug.contains("VerbatimString"));
        assert!(debug.contains("format"));
    }

    // Type Size Tests (informational)

    #[test]
    fn test_enum_size() {
        let size = std::mem::size_of::<RespValue>();
        // This is informational - RespValue should be reasonably sized
        // Typical size is around 32-48 bytes depending on platform
        println!("RespValue size: {size} bytes");
        // We don't assert a specific size, but this helps track enum size
        assert!(size > 0);
    }

    #[test]
    fn test_bytes_is_cheap_to_clone() {
        let data = Bytes::from("hello world");
        let clone1 = data.clone();
        let clone2 = data.clone();

        // All three should point to the same underlying data (zero-copy)
        // We can verify by checking pointer equality
        assert_eq!(data.as_ptr(), clone1.as_ptr());
        assert_eq!(data.as_ptr(), clone2.as_ptr());
    }

    // Helper Method Tests

    // is_null() tests

    #[test]
    fn test_is_null_true_for_null() {
        assert!(RespValue::Null.is_null());
    }

    #[test]
    fn test_is_null_true_for_bulk_string_none() {
        assert!(RespValue::BulkString(None).is_null());
    }

    #[test]
    fn test_is_null_true_for_array_none() {
        assert!(RespValue::Array(None).is_null());
    }

    #[test]
    fn test_is_null_false_for_integer() {
        assert!(!RespValue::Integer(42).is_null());
    }

    #[test]
    fn test_is_null_false_for_simple_string() {
        assert!(!RespValue::SimpleString(Bytes::from("OK")).is_null());
    }

    #[test]
    fn test_is_null_false_for_bulk_string_some() {
        assert!(!RespValue::BulkString(Some(Bytes::from("data"))).is_null());
    }

    #[test]
    fn test_is_null_false_for_array_some() {
        assert!(!RespValue::Array(Some(vec![])).is_null());
    }

    #[test]
    fn test_is_null_false_for_boolean() {
        assert!(!RespValue::Boolean(true).is_null());
    }

    // as_bytes() tests

    #[test]
    fn test_as_bytes_from_simple_string() {
        let value = RespValue::SimpleString(Bytes::from("OK"));
        assert_eq!(value.as_bytes(), Some(&Bytes::from("OK")));
    }

    #[test]
    fn test_as_bytes_from_bulk_string_some() {
        let value = RespValue::BulkString(Some(Bytes::from("hello")));
        assert_eq!(value.as_bytes(), Some(&Bytes::from("hello")));
    }

    #[test]
    fn test_as_bytes_from_error() {
        let value = RespValue::Error(Bytes::from("ERR message"));
        assert_eq!(value.as_bytes(), Some(&Bytes::from("ERR message")));
    }

    #[test]
    fn test_as_bytes_from_big_number() {
        let value = RespValue::BigNumber(Bytes::from("123456789"));
        assert_eq!(value.as_bytes(), Some(&Bytes::from("123456789")));
    }

    #[test]
    fn test_as_bytes_from_bulk_error() {
        let value = RespValue::BulkError(Bytes::from("SYNTAX error"));
        assert_eq!(value.as_bytes(), Some(&Bytes::from("SYNTAX error")));
    }

    #[test]
    fn test_as_bytes_from_verbatim_string() {
        let value = RespValue::VerbatimString {
            format: *b"txt",
            data: Bytes::from("content"),
        };
        assert_eq!(value.as_bytes(), Some(&Bytes::from("content")));
    }

    #[test]
    fn test_as_bytes_none_for_bulk_string_none() {
        let value = RespValue::BulkString(None);
        assert_eq!(value.as_bytes(), None);
    }

    #[test]
    fn test_as_bytes_none_for_integer() {
        let value = RespValue::Integer(42);
        assert_eq!(value.as_bytes(), None);
    }

    #[test]
    fn test_as_bytes_none_for_null() {
        let value = RespValue::Null;
        assert_eq!(value.as_bytes(), None);
    }

    #[test]
    fn test_as_bytes_none_for_array() {
        let value = RespValue::Array(Some(vec![]));
        assert_eq!(value.as_bytes(), None);
    }

    // as_integer() tests

    #[test]
    fn test_as_integer_extracts_value() {
        let value = RespValue::Integer(1000);
        assert_eq!(value.as_integer(), Some(1000));
    }

    #[test]
    fn test_as_integer_extracts_negative() {
        let value = RespValue::Integer(-42);
        assert_eq!(value.as_integer(), Some(-42));
    }

    #[test]
    fn test_as_integer_extracts_zero() {
        let value = RespValue::Integer(0);
        assert_eq!(value.as_integer(), Some(0));
    }

    #[test]
    fn test_as_integer_none_for_simple_string() {
        let value = RespValue::SimpleString(Bytes::from("42"));
        assert_eq!(value.as_integer(), None);
    }

    #[test]
    fn test_as_integer_none_for_null() {
        let value = RespValue::Null;
        assert_eq!(value.as_integer(), None);
    }

    // into_array() tests

    #[test]
    fn test_into_array_extracts_elements() {
        let arr = vec![
            RespValue::SimpleString(Bytes::from("GET")),
            RespValue::SimpleString(Bytes::from("key")),
        ];
        let value = RespValue::Array(Some(arr.clone()));
        let extracted = value.into_array();
        assert_eq!(extracted, Some(arr));
    }

    #[test]
    fn test_into_array_extracts_empty_array() {
        let value = RespValue::Array(Some(vec![]));
        let extracted = value.into_array();
        assert_eq!(extracted, Some(vec![]));
    }

    #[test]
    fn test_into_array_none_for_array_none() {
        let value = RespValue::Array(None);
        let extracted = value.into_array();
        assert_eq!(extracted, None);
    }

    #[test]
    fn test_into_array_none_for_integer() {
        let value = RespValue::Integer(42);
        let extracted = value.into_array();
        assert_eq!(extracted, None);
    }

    #[test]
    fn test_into_array_none_for_simple_string() {
        let value = RespValue::SimpleString(Bytes::from("OK"));
        let extracted = value.into_array();
        assert_eq!(extracted, None);
    }

    #[test]
    fn test_into_array_consumes_value() {
        let arr = vec![RespValue::Integer(1), RespValue::Integer(2)];
        let value = RespValue::Array(Some(arr.clone()));
        let extracted = value.into_array();
        assert_eq!(extracted, Some(arr));
        // value is consumed, can't use it again
    }

    // size_estimate() tests

    #[test]
    fn test_size_estimate_simple_string() {
        let value = RespValue::SimpleString(Bytes::from("OK"));
        let size = value.size_estimate();
        // 2 bytes + ~10 overhead = ~12
        assert!((12..=15).contains(&size));
    }

    #[test]
    fn test_size_estimate_bulk_string_some() {
        let value = RespValue::BulkString(Some(Bytes::from("hello")));
        let size = value.size_estimate();
        // 5 bytes + ~15 overhead = ~20
        assert!((20..=25).contains(&size));
    }

    #[test]
    fn test_size_estimate_bulk_string_none() {
        let value = RespValue::BulkString(None);
        let size = value.size_estimate();
        assert_eq!(size, 5);
    }

    #[test]
    fn test_size_estimate_integer() {
        let value = RespValue::Integer(42);
        let size = value.size_estimate();
        assert_eq!(size, 15);
    }

    #[test]
    fn test_size_estimate_error() {
        let value = RespValue::Error(Bytes::from("ERR message"));
        let size = value.size_estimate();
        // 11 bytes + ~10 overhead = ~21
        assert!((21..=25).contains(&size));
    }

    #[test]
    fn test_size_estimate_array_some() {
        let value = RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]));
        let size = value.size_estimate();
        // 2 integers (15 each) + ~10 overhead = ~40
        assert!((40..=50).contains(&size));
    }

    #[test]
    fn test_size_estimate_array_none() {
        let value = RespValue::Array(None);
        let size = value.size_estimate();
        assert_eq!(size, 5);
    }

    #[test]
    fn test_size_estimate_nested_array() {
        let inner = vec![RespValue::Integer(1), RespValue::Integer(2)];
        let outer = vec![
            RespValue::Array(Some(inner)),
            RespValue::SimpleString(Bytes::from("test")),
        ];
        let value = RespValue::Array(Some(outer));
        let size = value.size_estimate();
        // Inner array: 2*15 + 10 = 40
        // SimpleString: 4 + 10 = 14
        // Outer array: 40 + 14 + 10 = 64
        assert!((60..=70).contains(&size));
    }

    #[test]
    fn test_size_estimate_map() {
        let value = RespValue::Map(vec![
            (
                RespValue::SimpleString(Bytes::from("key")),
                RespValue::Integer(1),
            ),
            (
                RespValue::SimpleString(Bytes::from("key2")),
                RespValue::Integer(2),
            ),
        ]);
        let size = value.size_estimate();
        // key: 3+10=13, value: 15
        // key2: 4+10=14, value: 15
        // Total: 13+15+14+15+10 = 67
        assert!((65..=75).contains(&size));
    }

    #[test]
    fn test_size_estimate_set() {
        let value = RespValue::Set(vec![
            RespValue::SimpleString(Bytes::from("a")),
            RespValue::SimpleString(Bytes::from("b")),
        ]);
        let size = value.size_estimate();
        // a: 1+10=11, b: 1+10=11
        // Total: 11+11+10 = 32
        assert!((30..=35).contains(&size));
    }

    #[test]
    fn test_size_estimate_null() {
        let value = RespValue::Null;
        let size = value.size_estimate();
        assert_eq!(size, 10);
    }

    #[test]
    fn test_size_estimate_boolean() {
        let value = RespValue::Boolean(true);
        let size = value.size_estimate();
        assert_eq!(size, 10);
    }

    #[test]
    fn test_size_estimate_double() {
        let value = RespValue::Double(1.23);
        let size = value.size_estimate();
        assert_eq!(size, 10);
    }

    #[test]
    fn test_size_estimate_push() {
        let value = RespValue::Push(vec![RespValue::Integer(1)]);
        let size = value.size_estimate();
        // 1 integer (15) + 10 overhead = 25
        assert_eq!(size, 25);
    }
}
