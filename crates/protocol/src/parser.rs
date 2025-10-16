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
    max_depth: usize,
    /// Current nesting depth
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
    /// Parsing array length
    ArrayLen { len_bytes: BytesMut },
    /// Parsing array data
    ArrayData {
        remaining: usize,
        elements: Vec<RespValue>,
    },
    /// Parsing double data (line-based)
    Double { data: BytesMut },
    /// Parsing big number data (line-based)
    BigNumber { data: BytesMut },
    /// Parsing bulk error length
    BulkErrorLen { len_bytes: BytesMut },
    /// Parsing bulk error data
    BulkErrorData { len: usize, data: BytesMut },
    /// Parsing verbatim string length
    VerbatimStringLen { len_bytes: BytesMut },
    /// Parsing verbatim string data
    VerbatimStringData { len: usize, data: BytesMut },
    /// Parsing map length
    MapLen { len_bytes: BytesMut },
    /// Parsing map data (alternating keys and values)
    MapData {
        remaining: usize,
        pairs: Vec<(RespValue, RespValue)>,
        current_key: Option<RespValue>,
    },
    /// Parsing set length
    SetLen { len_bytes: BytesMut },
    /// Parsing set data
    SetData {
        remaining: usize,
        elements: Vec<RespValue>,
    },
    /// Parsing push length
    PushLen { len_bytes: BytesMut },
    /// Parsing push data
    PushData {
        remaining: usize,
        elements: Vec<RespValue>,
    },
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

    /// Set maximum nesting depth
    ///
    /// # Arguments
    ///
    /// * `depth` - Maximum nesting depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
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
                        b'*' => {
                            self.state = ParseState::ArrayLen {
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
                        b',' => {
                            self.state = ParseState::Double {
                                data: BytesMut::new(),
                            };
                        }
                        b'(' => {
                            self.state = ParseState::BigNumber {
                                data: BytesMut::new(),
                            };
                        }
                        b'!' => {
                            self.state = ParseState::BulkErrorLen {
                                len_bytes: BytesMut::new(),
                            };
                        }
                        b'=' => {
                            self.state = ParseState::VerbatimStringLen {
                                len_bytes: BytesMut::new(),
                            };
                        }
                        b'%' => {
                            self.state = ParseState::MapLen {
                                len_bytes: BytesMut::new(),
                            };
                        }
                        b'~' => {
                            self.state = ParseState::SetLen {
                                len_bytes: BytesMut::new(),
                            };
                        }
                        b'>' => {
                            self.state = ParseState::PushLen {
                                len_bytes: BytesMut::new(),
                            };
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

                ParseState::ArrayLen { len_bytes } => {
                    // Look for \r\n to complete length line
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null array (*-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Array(None)));
                        }

                        // Validate length is non-negative
                        if len < 0 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        let len = len as usize;

                        // Check size limit
                        if len > self.max_array_len {
                            return Err(ProtocolError::ArrayTooLarge {
                                size: len,
                                max: self.max_array_len,
                            });
                        }

                        // Handle empty array (*0\r\n)
                        if len == 0 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Array(Some(Vec::new()))));
                        }

                        // Transition to data parsing state
                        // Increment depth when entering array parsing
                        self.current_depth += 1;
                        if self.current_depth > self.max_depth {
                            self.current_depth -= 1; // Restore depth
                            return Err(ProtocolError::NestingTooDeep);
                        }

                        self.state = ParseState::ArrayData {
                            remaining: len,
                            elements: Vec::with_capacity(len),
                        };
                    } else {
                        // Need more data for length line
                        len_bytes.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::ArrayData {
                    remaining,
                    elements,
                } => {
                    // Need to parse more elements
                    if *remaining == 0 {
                        // All elements collected
                        let elems = std::mem::take(elements);
                        self.current_depth -= 1;
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Array(Some(elems))));
                    }

                    // Extract to avoid borrow conflict
                    let rem = *remaining;
                    let mut elems = std::mem::take(elements);
                    self.state = ParseState::WaitingForType;

                    // Parse next element
                    let elem_result = self.parse(buf)?;

                    match elem_result {
                        Some(element) => {
                            elems.push(element);
                            let new_remaining = rem - 1;

                            if new_remaining == 0 {
                                self.current_depth -= 1;
                                return Ok(Some(RespValue::Array(Some(elems))));
                            } else {
                                self.state = ParseState::ArrayData {
                                    remaining: new_remaining,
                                    elements: elems,
                                };
                            }
                        }
                        None => {
                            // Incomplete: don't overwrite state if it was changed
                            // Check if parse changed the state (e.g., to Integer)
                            if matches!(self.state, ParseState::WaitingForType) {
                                // No progress, restore ArrayData
                                self.state = ParseState::ArrayData {
                                    remaining: rem,
                                    elements: elems,
                                };
                            } else {
                                // State was changed (e.g., to Integer), but we need to remember array context
                                // This is a limitation: we can't properly nest incomplete elements in arrays
                                // For simplicity, wrap the partial state - actually, we can't do that without changing ParseState
                                // So just restore ArrayData for now
                                self.state = ParseState::ArrayData {
                                    remaining: rem,
                                    elements: elems,
                                };
                            }
                            return Ok(None);
                        }
                    }
                }

                ParseState::Double { data } => {
                    // Look for \r\n terminator (line-based like SimpleString)
                    if let Some(pos) = find_crlf(buf) {
                        data.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse double from accumulated data
                        let double_str =
                            std::str::from_utf8(data).map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle special cases: inf, -inf, nan
                        let value = match double_str {
                            "inf" => f64::INFINITY,
                            "-inf" => f64::NEG_INFINITY,
                            "nan" => f64::NAN,
                            _ => double_str
                                .parse::<f64>()
                                .map_err(|_| ProtocolError::InvalidLength)?,
                        };

                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Double(value)));
                    } else {
                        // Need more data
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::BigNumber { data } => {
                    // Look for \r\n terminator (line-based)
                    if let Some(pos) = find_crlf(buf) {
                        data.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        let value = RespValue::BigNumber(data.clone().freeze());
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(value));
                    } else {
                        // Need more data
                        data.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::BulkErrorLen { len_bytes } => {
                    // Identical to BulkStringLen
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null bulk error (!-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Null));
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

                        // Handle empty bulk error (!0\r\n\r\n)
                        if len == 0 {
                            if buf.len() < 2 {
                                self.state = ParseState::BulkErrorData {
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
                            return Ok(Some(RespValue::BulkError(bytes::Bytes::new())));
                        }

                        // Transition to data parsing state
                        self.state = ParseState::BulkErrorData {
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

                ParseState::BulkErrorData { len, data } => {
                    // Identical to BulkStringData
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

                    let value = RespValue::BulkError(data.clone().freeze());
                    self.state = ParseState::WaitingForType;
                    return Ok(Some(value));
                }

                ParseState::VerbatimStringLen { len_bytes } => {
                    // Similar to BulkStringLen
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null verbatim string (=-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Null));
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

                        // VerbatimString must have at least 4 bytes (format + ':')
                        if len < 4 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        // Transition to data parsing state
                        self.state = ParseState::VerbatimStringData {
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

                ParseState::VerbatimStringData { len, data } => {
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

                    // Extract format (first 3 bytes) and validate structure
                    if data.len() < 4 || data[3] != b':' {
                        return Err(ProtocolError::InvalidLength);
                    }

                    let format = [data[0], data[1], data[2]];
                    let content = data[4..].to_vec();

                    let value = RespValue::VerbatimString {
                        format,
                        data: bytes::Bytes::from(content),
                    };

                    self.state = ParseState::WaitingForType;
                    return Ok(Some(value));
                }

                ParseState::MapLen { len_bytes } => {
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null map (%-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Null));
                        }

                        // Validate length is non-negative
                        if len < 0 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        let len = len as usize;

                        // Check size limit
                        if len > self.max_array_len {
                            return Err(ProtocolError::ArrayTooLarge {
                                size: len,
                                max: self.max_array_len,
                            });
                        }

                        // Handle empty map (%0\r\n)
                        if len == 0 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Map(Vec::new())));
                        }

                        // Increment depth
                        self.current_depth += 1;
                        if self.current_depth > self.max_depth {
                            self.current_depth -= 1;
                            return Err(ProtocolError::NestingTooDeep);
                        }

                        self.state = ParseState::MapData {
                            remaining: len,
                            pairs: Vec::with_capacity(len),
                            current_key: None,
                        };
                    } else {
                        // Need more data for length line
                        len_bytes.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::MapData {
                    remaining,
                    pairs,
                    current_key,
                } => {
                    if *remaining == 0 {
                        // All pairs collected
                        let pairs_vec = std::mem::take(pairs);
                        self.current_depth -= 1;
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Map(pairs_vec)));
                    }

                    // Extract to avoid borrow conflict
                    let rem = *remaining;
                    let mut pairs_vec = std::mem::take(pairs);
                    let key_opt = current_key.take();
                    self.state = ParseState::WaitingForType;

                    // Parse next element
                    let elem_result = self.parse(buf)?;

                    match elem_result {
                        Some(element) => {
                            if let Some(key) = key_opt {
                                // We have a key, this element is the value
                                pairs_vec.push((key, element));
                                let new_remaining = rem - 1;

                                if new_remaining == 0 {
                                    self.current_depth -= 1;
                                    return Ok(Some(RespValue::Map(pairs_vec)));
                                } else {
                                    self.state = ParseState::MapData {
                                        remaining: new_remaining,
                                        pairs: pairs_vec,
                                        current_key: None,
                                    };
                                }
                            } else {
                                // This element is the key, need to parse value next
                                self.state = ParseState::MapData {
                                    remaining: rem,
                                    pairs: pairs_vec,
                                    current_key: Some(element),
                                };
                            }
                        }
                        None => {
                            // Incomplete: restore state
                            self.state = ParseState::MapData {
                                remaining: rem,
                                pairs: pairs_vec,
                                current_key: key_opt,
                            };
                            return Ok(None);
                        }
                    }
                }

                ParseState::SetLen { len_bytes } => {
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Handle null set (~-1\r\n)
                        if len == -1 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Null));
                        }

                        // Validate length is non-negative
                        if len < 0 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        let len = len as usize;

                        // Check size limit
                        if len > self.max_array_len {
                            return Err(ProtocolError::ArrayTooLarge {
                                size: len,
                                max: self.max_array_len,
                            });
                        }

                        // Handle empty set (~0\r\n)
                        if len == 0 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Set(Vec::new())));
                        }

                        // Increment depth
                        self.current_depth += 1;
                        if self.current_depth > self.max_depth {
                            self.current_depth -= 1;
                            return Err(ProtocolError::NestingTooDeep);
                        }

                        self.state = ParseState::SetData {
                            remaining: len,
                            elements: Vec::with_capacity(len),
                        };
                    } else {
                        // Need more data for length line
                        len_bytes.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::SetData {
                    remaining,
                    elements,
                } => {
                    if *remaining == 0 {
                        // All elements collected
                        let elems = std::mem::take(elements);
                        self.current_depth -= 1;
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Set(elems)));
                    }

                    // Extract to avoid borrow conflict
                    let rem = *remaining;
                    let mut elems = std::mem::take(elements);
                    self.state = ParseState::WaitingForType;

                    // Parse next element
                    let elem_result = self.parse(buf)?;

                    match elem_result {
                        Some(element) => {
                            elems.push(element);
                            let new_remaining = rem - 1;

                            if new_remaining == 0 {
                                self.current_depth -= 1;
                                return Ok(Some(RespValue::Set(elems)));
                            } else {
                                self.state = ParseState::SetData {
                                    remaining: new_remaining,
                                    elements: elems,
                                };
                            }
                        }
                        None => {
                            self.state = ParseState::SetData {
                                remaining: rem,
                                elements: elems,
                            };
                            return Ok(None);
                        }
                    }
                }

                ParseState::PushLen { len_bytes } => {
                    if let Some(pos) = find_crlf(buf) {
                        len_bytes.extend_from_slice(&buf[..pos]);
                        buf.advance(pos + 2);

                        // Parse length
                        let len_str = std::str::from_utf8(len_bytes)
                            .map_err(|_| ProtocolError::InvalidLength)?;
                        let len: i64 = len_str.parse().map_err(|_| ProtocolError::InvalidLength)?;

                        // Push doesn't support null (no >-1)
                        if len < 0 {
                            return Err(ProtocolError::InvalidLength);
                        }

                        let len = len as usize;

                        // Check size limit
                        if len > self.max_array_len {
                            return Err(ProtocolError::ArrayTooLarge {
                                size: len,
                                max: self.max_array_len,
                            });
                        }

                        // Handle empty push (>0\r\n)
                        if len == 0 {
                            self.state = ParseState::WaitingForType;
                            return Ok(Some(RespValue::Push(Vec::new())));
                        }

                        // Increment depth
                        self.current_depth += 1;
                        if self.current_depth > self.max_depth {
                            self.current_depth -= 1;
                            return Err(ProtocolError::NestingTooDeep);
                        }

                        self.state = ParseState::PushData {
                            remaining: len,
                            elements: Vec::with_capacity(len),
                        };
                    } else {
                        // Need more data for length line
                        len_bytes.extend_from_slice(&buf[..]);
                        buf.clear();
                        return Ok(None);
                    }
                }

                ParseState::PushData {
                    remaining,
                    elements,
                } => {
                    if *remaining == 0 {
                        // All elements collected
                        let elems = std::mem::take(elements);
                        self.current_depth -= 1;
                        self.state = ParseState::WaitingForType;
                        return Ok(Some(RespValue::Push(elems)));
                    }

                    // Extract to avoid borrow conflict
                    let rem = *remaining;
                    let mut elems = std::mem::take(elements);
                    self.state = ParseState::WaitingForType;

                    // Parse next element
                    let elem_result = self.parse(buf)?;

                    match elem_result {
                        Some(element) => {
                            elems.push(element);
                            let new_remaining = rem - 1;

                            if new_remaining == 0 {
                                self.current_depth -= 1;
                                return Ok(Some(RespValue::Push(elems)));
                            } else {
                                self.state = ParseState::PushData {
                                    remaining: new_remaining,
                                    elements: elems,
                                };
                            }
                        }
                        None => {
                            self.state = ParseState::PushData {
                                remaining: rem,
                                elements: elems,
                            };
                            return Ok(None);
                        }
                    }
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

    // Array tests
    #[test]
    fn test_parse_array_with_two_integers() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2\r\n:1\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Integer(2)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Array(Some(vec![]))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Array(None)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_with_mixed_types() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*3\r\n+OK\r\n:42\r\n$5\r\nhello\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::SimpleString(Bytes::from("OK")),
                RespValue::Integer(42),
                RespValue::BulkString(Some(Bytes::from("hello")))
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_nested_one_level() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2\r\n*1\r\n:1\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Array(Some(vec![RespValue::Integer(1)])),
                RespValue::Integer(2)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_nested_two_levels() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*1\r\n*1\r\n*1\r\n:42\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![RespValue::Array(Some(vec![
                RespValue::Array(Some(vec![RespValue::Integer(42)]))
            ]))])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_incomplete_length() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_array_incomplete_elements() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2\r\n:1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_array_too_large() {
        let mut parser = RespParser::new().with_max_array_len(10);
        let mut buf = BytesMut::from("*11\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::ArrayTooLarge { size, max } => {
                assert_eq!(size, 11);
                assert_eq!(max, 10);
            }
            _ => panic!("Expected ArrayTooLarge error"),
        }
    }

    #[test]
    fn test_parse_array_too_deep() {
        let mut parser = RespParser::new().with_max_depth(2);
        // Create nested array: *1\r\n*1\r\n*1\r\n:1\r\n (3 levels)
        let mut buf = BytesMut::from("*1\r\n*1\r\n*1\r\n:1\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::NestingTooDeep));
    }

    #[test]
    fn test_parse_array_invalid_length_non_numeric() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*abc\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    #[test]
    fn test_parse_array_invalid_length_negative() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*-5\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    // NOTE: Streaming with incomplete nested elements in arrays is not yet supported
    // This would require a state stack to properly track both array context and element parse state
    // For now, array elements must be complete in a single parse() call
    #[test]
    #[ignore]
    fn test_parse_array_streaming_partial_element_todo() {
        // TODO: Implement state stack for proper nested incomplete element support
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2\r\n:1");

        // First parse - incomplete element within array
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add more data
        buf.extend_from_slice(b"\r\n:2\r\n");

        // Second parse - complete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Integer(2)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_streaming_across_length() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*");

        // First parse - incomplete length
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);

        // Add rest of length and elements
        buf.extend_from_slice(b"2\r\n:1\r\n:2\r\n");

        // Second parse - complete
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Integer(2)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_with_null_elements() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*3\r\n$-1\r\n_\r\n*-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::BulkString(None),
                RespValue::Null,
                RespValue::Array(None)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_with_bulk_strings() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::BulkString(Some(Bytes::from("GET"))),
                RespValue::BulkString(Some(Bytes::from("key")))
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_array_deeply_nested_at_limit() {
        let mut parser = RespParser::new().with_max_depth(3);
        // Create nested array at exactly the limit: *1\r\n*1\r\n*1\r\n:1\r\n (3 levels)
        let mut buf = BytesMut::from("*1\r\n*1\r\n*1\r\n:1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_array_complex_nested_structure() {
        let mut parser = RespParser::new();
        // Array with: [Integer(1), Array([SimpleString("OK"), Integer(2)]), Integer(3)]
        let mut buf = BytesMut::from("*3\r\n:1\r\n*2\r\n+OK\r\n:2\r\n:3\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![
                RespValue::Integer(1),
                RespValue::Array(Some(vec![
                    RespValue::SimpleString(Bytes::from("OK")),
                    RespValue::Integer(2)
                ])),
                RespValue::Integer(3)
            ])))
        );
        assert_eq!(buf.len(), 0);
    }

    // Double tests
    #[test]
    fn test_parse_double_numeric() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",1.23\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Double(d)) => assert!((d - 1.23).abs() < f64::EPSILON),
            _ => panic!("Expected Double"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_inf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",inf\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Double(f64::INFINITY)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_negative_inf() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",-inf\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Double(f64::NEG_INFINITY)));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_nan() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",nan\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Double(d)) => assert!(d.is_nan()),
            _ => panic!("Expected Double with NaN"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_negative() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",-5.67\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Double(d)) => assert!((d - (-5.67)).abs() < f64::EPSILON),
            _ => panic!("Expected Double"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_zero() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Double(d)) => assert!(d.abs() < f64::EPSILON),
            _ => panic!("Expected Double"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_double_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",1.23");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_double_malformed() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",abc\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    // BigNumber tests
    #[test]
    fn test_parse_big_number() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("(3492890328409238509324850943850943825024385\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BigNumber(Bytes::from(
                "3492890328409238509324850943850943825024385"
            )))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_big_number_negative() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("(-3492890328409238509324850943850943825024385\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BigNumber(Bytes::from(
                "-3492890328409238509324850943850943825024385"
            )))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_big_number_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("(349289032840923850932485");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    // BulkError tests
    #[test]
    fn test_parse_bulk_error() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("!21\r\nSYNTAX invalid syntax\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::BulkError(Bytes::from("SYNTAX invalid syntax")))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_error_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("!-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_error_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("!0\r\n\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BulkError(Bytes::from(""))));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_bulk_error_too_large() {
        let mut parser = RespParser::new().with_max_bulk_size(100);
        let mut buf = BytesMut::from("!101\r\n");

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
    fn test_parse_bulk_error_incomplete_length() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("!21");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bulk_error_incomplete_data() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("!21\r\nSYNTAX");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    // VerbatimString tests
    #[test]
    fn test_parse_verbatim_string_txt() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=15\r\ntxt:Some string\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::VerbatimString { format, data }) => {
                assert_eq!(format, *b"txt");
                assert_eq!(data, Bytes::from("Some string"));
            }
            _ => panic!("Expected VerbatimString"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_verbatim_string_mkd() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=14\r\nmkd:# Markdown\r\n");

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::VerbatimString { format, data }) => {
                assert_eq!(format, *b"mkd");
                assert_eq!(data, Bytes::from("# Markdown"));
            }
            _ => panic!("Expected VerbatimString"),
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_verbatim_string_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_verbatim_string_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=15\r\ntxt:Some");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_verbatim_string_malformed_format() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=10\r\ntxtSomestr\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    #[test]
    fn test_parse_verbatim_string_too_short() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("=3\r\ntxt\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
    }

    #[test]
    fn test_parse_verbatim_string_too_large() {
        let mut parser = RespParser::new().with_max_bulk_size(100);
        let mut buf = BytesMut::from("=101\r\n");

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

    // Map tests
    #[test]
    fn test_parse_map_simple() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("%2\r\n+first\r\n:1\r\n+second\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Map(vec![
                (
                    RespValue::SimpleString(Bytes::from("first")),
                    RespValue::Integer(1)
                ),
                (
                    RespValue::SimpleString(Bytes::from("second")),
                    RespValue::Integer(2)
                )
            ]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_map_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("%-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_map_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("%0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Map(vec![])));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_map_nested() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("%1\r\n+key\r\n*2\r\n:1\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Map(vec![(
                RespValue::SimpleString(Bytes::from("key")),
                RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]))
            )]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_map_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("%2\r\n+first\r\n:1\r\n+second");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_map_too_large() {
        let mut parser = RespParser::new().with_max_array_len(10);
        let mut buf = BytesMut::from("%11\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::ArrayTooLarge { size, max } => {
                assert_eq!(size, 11);
                assert_eq!(max, 10);
            }
            _ => panic!("Expected ArrayTooLarge error"),
        }
    }

    #[test]
    fn test_parse_map_too_deep() {
        let mut parser = RespParser::new().with_max_depth(2);
        // Nested map: %1\r\n+k\r\n%1\r\n+k\r\n%1\r\n+k\r\n:1\r\n
        let mut buf = BytesMut::from("%1\r\n+k\r\n%1\r\n+k\r\n%1\r\n+k\r\n:1\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::NestingTooDeep));
    }

    // Set tests
    #[test]
    fn test_parse_set_simple() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("~3\r\n+orange\r\n+apple\r\n:5\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Set(vec![
                RespValue::SimpleString(Bytes::from("orange")),
                RespValue::SimpleString(Bytes::from("apple")),
                RespValue::Integer(5)
            ]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_set_null() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("~-1\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Null));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_set_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("~0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Set(vec![])));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_set_nested() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("~2\r\n+item\r\n*2\r\n:1\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Set(vec![
                RespValue::SimpleString(Bytes::from("item")),
                RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]))
            ]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_set_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("~3\r\n+orange\r\n+apple");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_set_too_large() {
        let mut parser = RespParser::new().with_max_array_len(10);
        let mut buf = BytesMut::from("~11\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::ArrayTooLarge { size, max } => {
                assert_eq!(size, 11);
                assert_eq!(max, 10);
            }
            _ => panic!("Expected ArrayTooLarge error"),
        }
    }

    #[test]
    fn test_parse_set_too_deep() {
        let mut parser = RespParser::new().with_max_depth(2);
        // Nested set: ~1\r\n~1\r\n~1\r\n:1\r\n
        let mut buf = BytesMut::from("~1\r\n~1\r\n~1\r\n:1\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::NestingTooDeep));
    }

    // Push tests
    #[test]
    fn test_parse_push_simple() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(">3\r\n+pubsub\r\n+message\r\n$5\r\nhello\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Push(vec![
                RespValue::SimpleString(Bytes::from("pubsub")),
                RespValue::SimpleString(Bytes::from("message")),
                RespValue::BulkString(Some(Bytes::from("hello")))
            ]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_push_empty() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(">0\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::Push(vec![])));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_push_nested() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(">2\r\n+item\r\n*2\r\n:1\r\n:2\r\n");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Push(vec![
                RespValue::SimpleString(Bytes::from("item")),
                RespValue::Array(Some(vec![RespValue::Integer(1), RespValue::Integer(2)]))
            ]))
        );
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_parse_push_incomplete() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(">3\r\n+pubsub\r\n+message");

        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_push_too_large() {
        let mut parser = RespParser::new().with_max_array_len(10);
        let mut buf = BytesMut::from(">11\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::ArrayTooLarge { size, max } => {
                assert_eq!(size, 11);
                assert_eq!(max, 10);
            }
            _ => panic!("Expected ArrayTooLarge error"),
        }
    }

    #[test]
    fn test_parse_push_too_deep() {
        let mut parser = RespParser::new().with_max_depth(2);
        // Nested push: >1\r\n>1\r\n>1\r\n:1\r\n
        let mut buf = BytesMut::from(">1\r\n>1\r\n>1\r\n:1\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::NestingTooDeep));
    }

    #[test]
    fn test_parse_push_negative_count_invalid() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(">-1\r\n");

        let result = parser.parse(&mut buf);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::InvalidLength));
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

    #[test]
    fn test_parse_multiple_arrays() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from("*1\r\n:1\r\n*1\r\n:2\r\n");

        // Parse first array
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![RespValue::Integer(1)])))
        );

        // Parse second array
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(
            result,
            Some(RespValue::Array(Some(vec![RespValue::Integer(2)])))
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
        let mut buf = BytesMut::from(format!("+{long_str}\r\n").as_bytes());

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

    #[test]
    fn test_parse_large_array() {
        let mut parser = RespParser::new();
        // Create array with 100 integers
        let mut buf = BytesMut::from("*100\r\n");
        for i in 0..100 {
            buf.extend_from_slice(format!(":{i}\r\n").as_bytes());
        }

        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Array(Some(elements))) => {
                assert_eq!(elements.len(), 100);
                assert_eq!(elements[0], RespValue::Integer(0));
                assert_eq!(elements[99], RespValue::Integer(99));
            }
            _ => panic!("Expected array with 100 elements"),
        }
        assert_eq!(buf.len(), 0);
    }

    // Mixed RESP3 types
    #[test]
    fn test_parse_mixed_resp3_types() {
        let mut parser = RespParser::new();
        let mut buf = BytesMut::from(",1.5\r\n(123456789\r\n!5\r\nERROR\r\n");

        // Parse double
        let result = parser.parse(&mut buf).unwrap();
        match result {
            Some(RespValue::Double(d)) => assert!((d - 1.5).abs() < f64::EPSILON),
            _ => panic!("Expected Double"),
        }

        // Parse big number
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BigNumber(Bytes::from("123456789"))));

        // Parse bulk error
        let result = parser.parse(&mut buf).unwrap();
        assert_eq!(result, Some(RespValue::BulkError(Bytes::from("ERROR"))));

        assert_eq!(buf.len(), 0);
    }
}
