//! Inline command parser for telnet-style commands
//!
//! This module provides parsing for inline commands (telnet-style) that are sent
//! as plain text rather than RESP protocol. For example: "GET key\r\n" instead of
//! "*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n".
//!
//! The parser handles:
//! - Basic whitespace-separated arguments
//! - Quoted strings with spaces: "SET key \"value with spaces\""
//! - Escape sequences in quoted strings: "SET key \"quote\\\"here\""
//! - Single and double quotes
//!
//! # Wire Format
//!
//! Inline commands must end with CRLF (\r\n). The parser rejects commands that:
//! - Don't end with \r\n
//! - Have unclosed quotes
//! - Are empty or contain only whitespace
//! - Contain invalid UTF-8
//!
//! # Examples
//!
//! ```
//! use seshat_protocol_resp::inline::InlineCommandParser;
//! use seshat_protocol_resp::RespValue;
//!
//! // Basic command
//! let result = InlineCommandParser::parse(b"GET mykey\r\n").unwrap();
//! if let RespValue::Array(Some(elements)) = result {
//!     assert_eq!(elements.len(), 2);
//! }
//!
//! // Command with quoted string
//! let result = InlineCommandParser::parse(b"SET key \"value with spaces\"\r\n").unwrap();
//! if let RespValue::Array(Some(elements)) = result {
//!     assert_eq!(elements.len(), 3);
//! }
//!
//! // Command with escape sequences
//! let result = InlineCommandParser::parse(b"SET key \"quote\\\"here\"\r\n").unwrap();
//! ```

use bytes::Bytes;

use crate::{ProtocolError, RespValue, Result};

/// Inline command parser for telnet-style commands
///
/// This parser converts telnet-style inline commands into RESP array format,
/// making them compatible with the standard RESP command processing pipeline.
///
/// The parser is stateless and thread-safe.
pub struct InlineCommandParser;

impl InlineCommandParser {
    /// Parse an inline command into a RespValue::Array
    ///
    /// Converts telnet-style commands like "GET key\r\n" into RESP array format.
    /// The result is a RespValue::Array containing BulkString elements.
    ///
    /// # Arguments
    ///
    /// * `line` - The raw command bytes including trailing \r\n
    ///
    /// # Returns
    ///
    /// * `Ok(RespValue::Array)` - Parsed command as array of bulk strings
    /// * `Err(ProtocolError::InvalidLength)` - If missing or invalid CRLF termination
    /// * `Err(ProtocolError::Utf8)` - If command contains invalid UTF-8
    /// * `Err(ProtocolError::EmptyCommand)` - If command is empty or whitespace-only
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::inline::InlineCommandParser;
    ///
    /// let result = InlineCommandParser::parse(b"GET key\r\n").unwrap();
    /// ```
    pub fn parse(line: &[u8]) -> Result<RespValue> {
        // Verify line ends with \r\n (minimum valid command is "\r\n" which is 2 bytes)
        if line.len() < 2 {
            return Err(ProtocolError::InvalidLength);
        }

        let len = line.len();
        if !Self::has_crlf_terminator(line) {
            return Err(ProtocolError::InvalidLength);
        }

        // Strip \r\n terminator
        let command_bytes = &line[..len - 2];

        // Convert to UTF-8 string (will propagate Utf8 error if invalid)
        let command_str = std::str::from_utf8(command_bytes)?;

        // Split into tokens (respecting quotes and escapes)
        let tokens = Self::split_inline_command(command_str)?;

        // Convert tokens to RespValue::Array of BulkStrings
        let elements: Vec<RespValue> = tokens
            .into_iter()
            .map(|token| RespValue::BulkString(Some(Bytes::from(token))))
            .collect();

        Ok(RespValue::Array(Some(elements)))
    }

    /// Check if line ends with CRLF (\r\n)
    #[inline]
    fn has_crlf_terminator(line: &[u8]) -> bool {
        let len = line.len();
        len >= 2 && line[len - 2] == b'\r' && line[len - 1] == b'\n'
    }

    /// Split inline command into tokens, respecting quotes and escapes
    ///
    /// This function implements a simple state machine that:
    /// 1. Splits on whitespace (space, tab) when outside quotes
    /// 2. Preserves whitespace inside quotes (single or double)
    /// 3. Handles escape sequences (\n, \t, \r, \\, \", \') inside quotes
    /// 4. Rejects commands with unclosed quotes
    /// 5. Rejects empty commands
    ///
    /// # Arguments
    ///
    /// * `line` - The command string (without \r\n)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - Vector of command tokens
    /// * `Err(ProtocolError::InvalidLength)` - If quotes are unclosed
    /// * `Err(ProtocolError::EmptyCommand)` - If command is empty or whitespace-only
    ///
    /// # State Machine
    ///
    /// - **Outside quotes**: Whitespace splits tokens, quotes start quoted region
    /// - **Inside quotes**: All chars preserved, backslash triggers escape processing
    /// - **Escape processing**: Next char determines escape sequence or literal
    fn split_inline_command(line: &str) -> Result<Vec<String>> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut chars = line.chars().peekable();
        let mut in_quotes = false;
        let mut quote_char = '\0';

        while let Some(ch) = chars.next() {
            match ch {
                // Whitespace outside quotes: token separator
                ' ' | '\t' if !in_quotes => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                }

                // Quote character outside quotes: start quoted region
                '"' | '\'' if !in_quotes => {
                    in_quotes = true;
                    quote_char = ch;
                }

                // Matching quote character inside quotes: end quoted region
                '"' | '\'' if in_quotes && ch == quote_char => {
                    in_quotes = false;
                    quote_char = '\0';
                    // Push token immediately, even if empty (empty quoted strings are valid)
                    tokens.push(current_token.clone());
                    current_token.clear();
                }

                // Backslash inside quotes: process escape sequence
                '\\' if in_quotes => {
                    if let Some(next_ch) = chars.next() {
                        match next_ch {
                            'n' => current_token.push('\n'),
                            't' => current_token.push('\t'),
                            'r' => current_token.push('\r'),
                            '\\' => current_token.push('\\'),
                            '"' => current_token.push('"'),
                            '\'' => current_token.push('\''),
                            _ => {
                                // Unknown escape sequence: preserve both backslash and char
                                current_token.push('\\');
                                current_token.push(next_ch);
                            }
                        }
                    } else {
                        // Backslash at end of string: keep literal backslash
                        current_token.push('\\');
                    }
                }

                // Regular character: add to current token
                _ => {
                    current_token.push(ch);
                }
            }
        }

        // Validate final state
        if in_quotes {
            return Err(ProtocolError::InvalidLength);
        }

        // Add final token if present
        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        // Reject empty commands
        if tokens.is_empty() {
            return Err(ProtocolError::EmptyCommand);
        }

        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic command parsing tests

    #[test]
    fn test_parse_simple_get_command() {
        let result = InlineCommandParser::parse(b"GET mykey\r\n");
        assert!(result.is_ok(), "Simple GET command should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2);
            assert_eq!(
                elements[0].as_bytes().map(|b| b.as_ref()),
                Some(b"GET".as_ref())
            );
            assert_eq!(
                elements[1].as_bytes().map(|b| b.as_ref()),
                Some(b"mykey".as_ref())
            );
        } else {
            panic!("Expected Array, got {value:?}");
        }
    }

    #[test]
    fn test_parse_simple_set_command() {
        let result = InlineCommandParser::parse(b"SET key value\r\n");
        assert!(result.is_ok(), "Simple SET command should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[0].as_bytes().map(|b| b.as_ref()),
                Some(b"SET".as_ref())
            );
            assert_eq!(
                elements[1].as_bytes().map(|b| b.as_ref()),
                Some(b"key".as_ref())
            );
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value".as_ref())
            );
        } else {
            panic!("Expected Array, got {value:?}");
        }
    }

    #[test]
    fn test_parse_ping_command() {
        let result = InlineCommandParser::parse(b"PING\r\n");
        assert!(result.is_ok(), "PING command should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 1);
            assert_eq!(
                elements[0].as_bytes().map(|b| b.as_ref()),
                Some(b"PING".as_ref())
            );
        } else {
            panic!("Expected Array, got {value:?}");
        }
    }

    #[test]
    fn test_parse_command_with_multiple_spaces() {
        let result = InlineCommandParser::parse(b"GET    key\r\n");
        assert!(result.is_ok(), "Command with multiple spaces should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2, "Multiple spaces should be collapsed");
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_command_with_leading_spaces() {
        let result = InlineCommandParser::parse(b"  GET key\r\n");
        assert!(result.is_ok(), "Command with leading spaces should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2);
            assert_eq!(
                elements[0].as_bytes().map(|b| b.as_ref()),
                Some(b"GET".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_command_with_trailing_spaces() {
        let result = InlineCommandParser::parse(b"GET key  \r\n");
        assert!(result.is_ok(), "Command with trailing spaces should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }

    // Quoted string tests

    #[test]
    fn test_parse_double_quoted_string() {
        let result = InlineCommandParser::parse(b"SET key \"value with spaces\"\r\n");
        assert!(
            result.is_ok(),
            "Command with double-quoted string should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value with spaces".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_single_quoted_string() {
        let result = InlineCommandParser::parse(b"SET key 'value with spaces'\r\n");
        assert!(
            result.is_ok(),
            "Command with single-quoted string should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value with spaces".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_empty_quoted_string() {
        let result = InlineCommandParser::parse(b"SET key \"\"\r\n");
        assert!(
            result.is_ok(),
            "Command with empty quoted string should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_multiple_quoted_strings() {
        let result =
            InlineCommandParser::parse(b"MSET \"key 1\" \"value 1\" \"key 2\" \"value 2\"\r\n");
        assert!(
            result.is_ok(),
            "Command with multiple quoted strings should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 5);
            assert_eq!(
                elements[1].as_bytes().map(|b| b.as_ref()),
                Some(b"key 1".as_ref())
            );
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value 1".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    // Escape sequence tests

    #[test]
    fn test_parse_escaped_quote() {
        let result = InlineCommandParser::parse(b"SET key \"quote\\\"here\"\r\n");
        assert!(
            result.is_ok(),
            "Command with escaped quote should parse: {result:?}"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"quote\"here".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_escaped_backslash() {
        let result = InlineCommandParser::parse(b"SET key \"back\\\\slash\"\r\n");
        assert!(
            result.is_ok(),
            "Command with escaped backslash should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"back\\slash".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_escaped_newline() {
        let result = InlineCommandParser::parse(b"SET key \"line\\nbreak\"\r\n");
        assert!(result.is_ok(), "Command with escaped newline should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"line\nbreak".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_escaped_tab() {
        let result = InlineCommandParser::parse(b"SET key \"tab\\there\"\r\n");
        assert!(result.is_ok(), "Command with escaped tab should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"tab\there".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_multiple_escape_sequences() {
        let result = InlineCommandParser::parse(b"SET key \"\\\"test\\\"\\n\"\r\n");
        assert!(
            result.is_ok(),
            "Command with multiple escape sequences should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"\"test\"\n".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    // Error handling tests

    #[test]
    fn test_parse_empty_command_fails() {
        let result = InlineCommandParser::parse(b"\r\n");
        assert!(result.is_err(), "Empty command should fail");
    }

    #[test]
    fn test_parse_only_whitespace_fails() {
        let result = InlineCommandParser::parse(b"   \r\n");
        assert!(result.is_err(), "Only whitespace should fail");
    }

    #[test]
    fn test_parse_unclosed_double_quote_fails() {
        let result = InlineCommandParser::parse(b"SET key \"unclosed\r\n");
        assert!(result.is_err(), "Unclosed double quote should fail");
    }

    #[test]
    fn test_parse_unclosed_single_quote_fails() {
        let result = InlineCommandParser::parse(b"SET key 'unclosed\r\n");
        assert!(result.is_err(), "Unclosed single quote should fail");
    }

    #[test]
    fn test_parse_invalid_utf8_fails() {
        let invalid_utf8 = b"SET key \xff\xfe\r\n";
        let result = InlineCommandParser::parse(invalid_utf8);
        assert!(result.is_err(), "Invalid UTF-8 should fail");
    }

    #[test]
    fn test_parse_missing_crlf_fails() {
        let result = InlineCommandParser::parse(b"GET key");
        assert!(result.is_err(), "Missing CRLF should fail");
    }

    #[test]
    fn test_parse_only_cr_fails() {
        let result = InlineCommandParser::parse(b"GET key\r");
        assert!(result.is_err(), "Only CR without LF should fail");
    }

    #[test]
    fn test_parse_only_lf_fails() {
        let result = InlineCommandParser::parse(b"GET key\n");
        assert!(result.is_err(), "Only LF without CR should fail");
    }

    // Edge case tests

    #[test]
    fn test_parse_command_with_numbers() {
        let result = InlineCommandParser::parse(b"GETRANGE key 0 10\r\n");
        assert!(result.is_ok(), "Command with numbers should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 4);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"0".as_ref())
            );
            assert_eq!(
                elements[3].as_bytes().map(|b| b.as_ref()),
                Some(b"10".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_case_sensitive() {
        let result = InlineCommandParser::parse(b"get KEY\r\n");
        assert!(result.is_ok(), "Commands should be case-sensitive");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(
                elements[0].as_bytes().map(|b| b.as_ref()),
                Some(b"get".as_ref())
            );
            assert_eq!(
                elements[1].as_bytes().map(|b| b.as_ref()),
                Some(b"KEY".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_special_characters() {
        let result = InlineCommandParser::parse(b"SET key@123 value!@#$%\r\n");
        assert!(
            result.is_ok(),
            "Command with special characters should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[1].as_bytes().map(|b| b.as_ref()),
                Some(b"key@123".as_ref())
            );
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value!@#$%".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_unicode_in_quoted_string() {
        let result = InlineCommandParser::parse("SET key \"hello 世界\"\r\n".as_bytes());
        assert!(result.is_ok(), "Unicode in quoted string should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some("hello 世界".as_bytes())
            );
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_mixed_quotes_and_unquoted() {
        let result = InlineCommandParser::parse(b"MSET key1 \"value 1\" key2 value2\r\n");
        assert!(
            result.is_ok(),
            "Mixed quoted and unquoted args should parse"
        );

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 5);
            assert_eq!(
                elements[2].as_bytes().map(|b| b.as_ref()),
                Some(b"value 1".as_ref())
            );
            assert_eq!(
                elements[4].as_bytes().map(|b| b.as_ref()),
                Some(b"value2".as_ref())
            );
        } else {
            panic!("Expected Array");
        }
    }

    // Helper function tests

    #[test]
    fn test_split_inline_command_basic() {
        let result = InlineCommandParser::split_inline_command("GET key");
        assert!(result.is_ok());
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], "GET");
        assert_eq!(tokens[1], "key");
    }

    #[test]
    fn test_split_inline_command_with_quotes() {
        let result = InlineCommandParser::split_inline_command("SET key \"value with spaces\"");
        assert!(result.is_ok());
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[2], "value with spaces");
    }

    #[test]
    fn test_split_inline_command_empty_fails() {
        let result = InlineCommandParser::split_inline_command("");
        assert!(result.is_err());
    }

    #[test]
    fn test_split_inline_command_unclosed_quote_fails() {
        let result = InlineCommandParser::split_inline_command("SET key \"unclosed");
        assert!(result.is_err());
    }

    // Additional edge case tests for completeness

    #[test]
    fn test_parse_tab_separated() {
        let result = InlineCommandParser::parse(b"GET\tkey\r\n");
        assert!(result.is_ok(), "Tab-separated command should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }

    #[test]
    fn test_parse_mixed_whitespace() {
        let result = InlineCommandParser::parse(b"GET  \t key\r\n");
        assert!(result.is_ok(), "Mixed whitespace should parse");

        let value = result.unwrap();
        if let RespValue::Array(Some(elements)) = value {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("Expected Array");
        }
    }
}
