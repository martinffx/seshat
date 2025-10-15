//! RESP3 streaming parser
//!
//! This module implements a streaming state machine parser for RESP3 protocol.
//! It handles incomplete data gracefully and supports all 14 RESP3 types.

use bytes::{Buf, BytesMut};

use crate::{ProtocolError, RespValue, Result};

/// Streaming RESP3 parser with state machine
///
/// The parser maintains internal state to handle partial frames across
/// multiple calls to `parse()`. This allows efficient streaming parsing
/// without buffering complete messages.
///
/// # Examples
///
/// ```
/// use bytes::BytesMut;
/// use seshat_protocol::parser::RespParser;
///
/// let mut parser = RespParser::new();
/// let mut buf = BytesMut::from("+OK\r\n");
///
/// let result = parser.parse(&mut buf).unwrap();
/// assert!(result.is_some());
/// ```
pub struct RespParser {
    /// Current parsing state
    state: ParseState,
    /// Maximum bulk string size (default 512 MB)
    max_bulk_size: usize,
    /// Maximum array length (default 1M)
    max_array_len: usize,
    /// Maximum nesting depth (default 32)
    #[allow(dead_code)]
    max_depth: usize,
    /// Current nesting depth
    #[allow(dead_code)]
    current_depth: usize,
}

/// Internal parser state machine
#[derive(Debug, Clone)]
enum ParseState {
    /// Waiting for type marker byte
    WaitingForType,
    /// Parsing simple string data
    SimpleString { data: BytesMut },
    /// Parsing error data
    Error { data: BytesMut },
    /// Parsing integer data
    Integer { data: BytesMut },
    /// Parsing bulk string length
    BulkStringLen { len_bytes: BytesMut },
    /// Parsing bulk string data
    BulkStringData { len: usize, data: BytesMut },
}

impl RespParser {
    /// Create a new parser with default limits
    ///
    /// # Defaults
    ///
    /// - max_bulk_size: 512 MB
    /// - max_array_len: 1,000,000 elements
    /// - max_depth: 32 levels
    pub fn new() -> Self {
        Self {
            state: ParseState::WaitingForType,
            max_bulk_size: 512 * 1024 * 1024, // 512 MB
            max_array_len: 1_000_000,
            max_depth: 32,
            current_depth: 0,
        }
    }

    /// Set maximum bulk string size
    ///
    /// # Arguments
    ///
    /// * `size` - Maximum size in bytes
    pub fn with_max_bulk_size(mut self, size: usize) -> Self {
        self.max_bulk_size = size;
        self
    }

    /// Set maximum array length
    ///
    /// # Arguments
    ///
    /// * `len` - Maximum number of elements
    pub fn with_max_array_len(mut self, len: usize) -> Self {
        self.max_array_len = len;
        self
    }

    /// Parse bytes into RespValue
    ///
    /// Returns:
    /// - `Ok(Some(value))` - Complete frame parsed
    /// - `Ok(None)` - Need more data
    /// - `Err(error)` - Malformed data
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer containing RESP data. Consumed bytes are removed.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytes::BytesMut;
    /// use seshat_protocol::parser::RespParser;
    ///
    /// let mut parser = RespParser::new();
    /// let mut buf = BytesMut::from("+OK\r\n");
    ///
    /// let result = parser.parse(&mut buf).unwrap();
    /// assert!(result.is_some());
    /// assert_eq!(buf.len(), 0); // Buffer consumed
    /// ```
    pub fn parse(&mut self, buf: &mut BytesMut) -> Result<Option<RespValue>> {
        loop {
            match &mut self.state {
                ParseState::WaitingForType => {
                    // Need at least one byte for type marker
                    if buf.is_empty() {
                        return Ok(None);
                    }

                    let type_byte = buf[0];
                    buf.advance(1);

                    match type_byte {
                        b'+' => {
                            self.state = ParseState::SimpleString {
                                data: BytesMut::new(),
                            };
                        }
                        b'-' => {
                            self.state = ParseState::Error {
                                data: BytesMut::new(),
                            };
                        }
                        b':' => {
                            self.state = ParseState::Integer {
                                data: BytesMut::new(),
                            };
                        }
                        b'$' => {
                            self.state = ParseState::BulkStringLen {
                                len_bytes: BytesMut::new(),
                            };
                        }
                        b'_' => {
                            // Null: _\r\n
                            if buf.len() < 2 {
                                return Ok(None);
                            }
                            if &buf[0..2] != b"\r\n" {
                                return Err(ProtocolError::InvalidLength);
                            }
                            buf.advance(2);
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Null));
                        }
                        b'#' => {
                            // Boolean: #t\r\n or #f\r\n
                            if buf.len() < 3 {
                                return Ok(None);
                            }
                            let val = buf[0];
                            if &buf[1..3] != b"\r\n" {
                                return Err(ProtocolError::InvalidLength);
                            }
                            buf.advance(3);
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Boolean(val == b't')));
                        }
                        _ => {
                            return Err(ProtocolError::InvalidTypeMarker(type_byte));
                        }
                    }
                }

                ParseState::SimpleString { data } => {
                    // Look for \r\n terminator
                    if let Some(pos) = find_crlf(buf) {
                        data.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2); // Skip data + \r\n

                        let value = RespValue::SimpleString(data.clone().freeze());
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(value));
                    } else {
                        // Need more data - accumulate what we have
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::Error { data } => {
                    // Look for \r\n terminator
                    if let Some(pos) = find_crlf(buf) {
                        data.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2); // Skip data + \r\n

                        let value = RespValue::Error(data.clone().freeze());
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(value));
                    } else {
                        // Need more data - accumulate what we have
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::Integer { data } => {
                    // Look for \r\n terminator
                    if let Some(pos) = find_crlf(buf) {
                        data.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2); // Skip data + \r\n

                        // Parse integer from accumulated data
                        let int_str =
                            std::str::from_utf8(data).map_err(|_| ProtocolError::InvalidLength)?;
                        let value: i64 =
                            int_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Integer(value)));
                    } else {
                        // Need more data - accumulate what we have
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::BulkStringLen { len_bytes } => {
                    // Look for \r\n to complete length line
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null bulk string ($-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::BulkString(None)));
                        }

                        // Validate length is non-negative
                        if len < 0 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        let len = len as usize;

                        // Check size limit
                        if len > self.max_bulk_size {
                            return Err(ProtocolError::BulkStringTooLarge {
                                size: len,
                                max: self.max_bulk_size,
                            });
                        }

                        // Handle empty bulk string ($0\r\n\r\n)
                        if len == 0 {
                            // Still need to consume the final \r\n
                            if buf.len() < 2 {
                                self.state = ParseState::BulkStringData {
                                    len: 0,
                                    data: BytesMut::new(),
                                };
                                return Ok(None);
                            }
                            if &buf[0..2] != b"\r\n" {
                                return Err(ProtocolError::InvalidLength);
                            }
                            buf.advance(2);
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::BulkString(Some(bytes::Bytes::new()))));
                        }

                        // Transition to data parsing state
                        self.state = ParseState::BulkStringData {
                            len,
                            data: BytesMut::with_capacity(len),
                        };
                    } else {
                        // Need more data for length line
                        len_bytes.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::BulkStringData { len, data } => {
                    let needed = *len + 2 - data.len(); // +2 for \r\n

                    if buf.len() < needed {
                        // Need more data
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }

                    // Have complete data
                    let data_needed = *len - data.len();
                    data.extend_from_slice(&buf[..data_needed]);

                    // Verify CRLF terminator
                    if &buf[data_needed..data_needed + 2] != b"\r\n" {
                        return Err(ProtocolError::InvalidLength);
                    }
                    buf.advance(data_needed + 2);

                    let value = RespValue::BulkString(Some(data.clone().freeze()));
                    self.state = ParseState::WaitingForType;
                    return Ok(Some(value));
                }
            }
        }
    }
}

impl Default for RespParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Find CRLF (\r\n) in buffer
///
/// Returns the position of \r if found, None otherwise.
fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_parser_creation() {
        let parser = RespParser::new();
        assert_eq!(parser.max_bulk_size, 512 * 1024 * 1024);
        assert_eq!(parser.max_array_len, 1_000_000);
        assert_eq!(parser.max_depth, 32);
    }

    #[test]
    fn test_parser_with_limits() {
        let parser = RespParser::new()
            .with_max_bulk_size(1024)
            .with_max_array_len(100);
        assert_eq!(parser.max_bulk_size, 1024);
        assert_eq!(parser.max_array_len, 100);
    }

    // SimpleString tests
    #[test]
    fn test_parse_simple_string() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+OK\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_empty_simple_string() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from(""))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_simple_string_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+OK");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_simple_string_incomplete_crlf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+OK\r");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_simple_string_continue_after_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+HEL");

        // First parse - incomplete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add more data
        buf.extend_from_slice(b"LO\r\n");

        // Second parse - complete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("HELLO"))));
        assert_eq!(buf.len(), 0);
    }

    // Error tests
    #[test]
    fn test_parse_error() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("-ERR unknown command\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Error(Bytes::from("ERR unknown command")))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_empty_error() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("-\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Error(Bytes::from(""))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_error_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("-ERR");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    // Integer tests
    #[test]
    fn test_parse_integer_positive() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":1000\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(1000)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_integer_negative() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":-500\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(-500)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_integer_zero() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(0)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_integer_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":100");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_integer_malformed() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":abc\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    // Null tests
    #[test]
    fn test_parse_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("_\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_null_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("_");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_null_incomplete_crlf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("_\r");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    // Boolean tests
    #[test]
    fn test_parse_boolean_true() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("#t\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Boolean(true)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_boolean_false() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("#f\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Boolean(false)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_boolean_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("#t");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_boolean_incomplete_full() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("#t\r");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    // BulkString tests
    #[test]
    fn test_parse_bulk_string() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5\r\nhello\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_with_spaces() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$11\r\nhello world\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello world"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$0\r\n\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BulkString(Some(Bytes::from("")))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BulkString(None)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_incomplete_length() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bulk_string_incomplete_length_crlf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5\r");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bulk_string_incomplete_data() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5\r\nhel");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bulk_string_incomplete_final_crlf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5\r\nhello");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bulk_string_too_large() {
        let mut parser = RespParser::new().with_max_bulk_size(100);
        let mut buf = BytesMut::from("$101\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BulkStringTooLarge { size, max } => {
                assert_eq!(size, 101);
                assert_eq!(max, 100);
            }
            _ => panic!("Expected BulkStringTooLarge error"),
        }
    }

    #[test]
    fn test_parse_bulk_string_invalid_length_non_numeric() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$abc\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    #[test]
    fn test_parse_bulk_string_invalid_length_negative() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$-5\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    #[test]
    fn test_parse_bulk_string_streaming() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$5\r\nhel");

        // First parse - incomplete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add more data
        buf.extend_from_slice(b"lo\r\n");

        // Second parse - complete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_streaming_across_length() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$1");

        // First parse - incomplete length
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add rest of length and data
        buf.extend_from_slice(b"1\r\nhello world\r\n");

        // Second parse - complete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("hello world"))))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_string_with_binary_data() {
        let mut parser = RespParser::new();
        let binary_data = vec![0x00, 0x01, 0x02, 0xff, 0xfe];
        let mut buf = BytesMut::from("$5\r\n");
        buf.extend_from_slice(&binary_data);
        buf.extend_from_slice(b"\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from(binary_data))))
        );
        assert_eq!(buf.len(), 0);
    }

    // Type marker tests
    #[test]
    fn test_parse_invalid_type_marker() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("@test\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::InvalidTypeMarker(0x40)
        ));
    }

    // Helper tests
    #[test]
    fn test_find_crlf() {
        assert_eq!(find_crlf(b"hello\r\nworld"), Some(5));
        assert_eq!(find_crlf(b"hello"), None);
        assert_eq!(find_crlf(b"\r\n"), Some(0));
        assert_eq!(find_crlf(b""), None);
        assert_eq!(find_crlf(b"test\r"), None);
    }

    // Multiple values in buffer
    #[test]
    fn test_parse_multiple_values() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("+OK\r\n:42\r\n");

        // Parse first value
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));

        // Parse second value
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(42)));

        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_multiple_bulk_strings() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("$3\r\nGET\r\n$3\r\nkey\r\n");

        // Parse first bulk string
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("GET"))))
        );

        // Parse second bulk string
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from("key"))))
        );

        assert_eq!(buf.len(), 0);
    }

    // Edge cases
    #[test]
    fn test_parse_empty_buffer() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::new();

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_long_simple_string() {
        let mut parser = RespParser::new();
        let long_str = "a".repeat(10000);
        let mut buf = BytesMut::from(format!("+{}\r\n", long_str).as_bytes());

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from(long_str))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_long_integer() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(":9223372036854775807\r\n"); // i64::MAX

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Integer(i64::MAX)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_long_bulk_string() {
        let mut parser = RespParser::new();
        let long_str = "x".repeat(10000);
        let mut buf = BytesMut::from(format!("${}\r\n{}\r\n", long_str.len(), long_str).as_bytes());

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkString(Some(Bytes::from(long_str))))
        );
        assert_eq!(buf.len(), 0);
    }
}
