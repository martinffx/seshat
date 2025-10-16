//! RESP command parsing
//!
//! This module provides command parsing for Redis RESP protocol commands.
//! It converts parsed RESP values into strongly-typed command structures.

use bytes::Bytes;

use crate::error::{ProtocolError, Result};
use crate::types::RespValue;

/// Redis command types supported by Seshat
///
/// # Supported Commands
///
/// - **GET**: Retrieve value for a single key
/// - **SET**: Set value for a single key
/// - **DEL**: Delete one or more keys
/// - **EXISTS**: Check existence of one or more keys
/// - **PING**: Connection test with optional message
#[derive(Debug, Clone, PartialEq)]
pub enum RespCommand {
    /// GET key
    ///
    /// # Examples
    ///
    /// ```text
    /// *2\r\n$3\r\nGET\r\n$3\r\nkey\r\n
    /// ```
    Get { key: Bytes },

    /// SET key value
    ///
    /// # Examples
    ///
    /// ```text
    /// *3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n
    /// ```
    Set { key: Bytes, value: Bytes },

    /// DEL key [key ...]
    ///
    /// # Examples
    ///
    /// ```text
    /// *3\r\n$3\r\nDEL\r\n$4\r\nkey1\r\n$4\r\nkey2\r\n
    /// ```
    Del { keys: Vec<Bytes> },

    /// EXISTS key [key ...]
    ///
    /// # Examples
    ///
    /// ```text
    /// *3\r\n$6\r\nEXISTS\r\n$4\r\nkey1\r\n$4\r\nkey2\r\n
    /// ```
    Exists { keys: Vec<Bytes> },

    /// PING [message]
    ///
    /// # Examples
    ///
    /// ```text
    /// *1\r\n$4\r\nPING\r\n
    /// *2\r\n$4\r\nPING\r\n$5\r\nhello\r\n
    /// ```
    Ping { message: Option<Bytes> },
}

impl RespCommand {
    /// Parse a RESP command from a RespValue
    ///
    /// Commands must be RESP arrays with the first element being the command name
    /// and subsequent elements being the command arguments.
    ///
    /// # Errors
    ///
    /// - `ProtocolError::ExpectedArray` - Input is not an array
    /// - `ProtocolError::EmptyCommand` - Array is empty
    /// - `ProtocolError::InvalidCommandName` - First element is not a string
    /// - `ProtocolError::UnknownCommand` - Command name is not recognized
    /// - `ProtocolError::WrongArity` - Wrong number of arguments
    /// - `ProtocolError::InvalidKey` - Key argument is not a string
    /// - `ProtocolError::InvalidValue` - Value argument is not a string
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol::command::RespCommand;
    /// use seshat_protocol::types::RespValue;
    /// use bytes::Bytes;
    ///
    /// let value = RespValue::Array(Some(vec![
    ///     RespValue::BulkString(Some(Bytes::from("GET"))),
    ///     RespValue::BulkString(Some(Bytes::from("mykey"))),
    /// ]));
    ///
    /// let command = RespCommand::from_value(value).unwrap();
    /// assert!(matches!(command, RespCommand::Get { .. }));
    /// ```
    pub fn from_value(value: RespValue) -> Result<Self> {
        // Extract array from value
        let elements = value.into_array().ok_or(ProtocolError::ExpectedArray)?;

        // Check for empty command
        if elements.is_empty() {
            return Err(ProtocolError::EmptyCommand);
        }

        // Extract command name
        let cmd_name = elements[0]
            .as_bytes()
            .ok_or(ProtocolError::InvalidCommandName)?;

        // Convert to uppercase for case-insensitive matching
        let cmd_str = std::str::from_utf8(cmd_name)
            .map_err(|_| ProtocolError::InvalidCommandName)?
            .to_uppercase();

        // Parse based on command name
        match cmd_str.as_str() {
            "GET" => Self::parse_get(&elements),
            "SET" => Self::parse_set(&elements),
            "DEL" => Self::parse_del(&elements),
            "EXISTS" => Self::parse_exists(&elements),
            "PING" => Self::parse_ping(&elements),
            _ => Err(ProtocolError::UnknownCommand {
                command: cmd_str.to_string(),
            }),
        }
    }

    /// Parse GET command
    fn parse_get(elements: &[RespValue]) -> Result<Self> {
        if elements.len() != 2 {
            return Err(ProtocolError::WrongArity {
                command: "GET",
                expected: 2,
                got: elements.len(),
            });
        }

        let key = elements[1]
            .as_bytes()
            .ok_or(ProtocolError::InvalidKey)?
            .clone();

        Ok(RespCommand::Get { key })
    }

    /// Parse SET command
    fn parse_set(elements: &[RespValue]) -> Result<Self> {
        if elements.len() != 3 {
            return Err(ProtocolError::WrongArity {
                command: "SET",
                expected: 3,
                got: elements.len(),
            });
        }

        let key = elements[1]
            .as_bytes()
            .ok_or(ProtocolError::InvalidKey)?
            .clone();

        let value = elements[2]
            .as_bytes()
            .ok_or(ProtocolError::InvalidValue)?
            .clone();

        Ok(RespCommand::Set { key, value })
    }

    /// Parse DEL command
    fn parse_del(elements: &[RespValue]) -> Result<Self> {
        if elements.len() < 2 {
            return Err(ProtocolError::WrongArity {
                command: "DEL",
                expected: 2,
                got: elements.len(),
            });
        }

        let mut keys = Vec::with_capacity(elements.len() - 1);
        for element in &elements[1..] {
            let key = element.as_bytes().ok_or(ProtocolError::InvalidKey)?.clone();
            keys.push(key);
        }

        Ok(RespCommand::Del { keys })
    }

    /// Parse EXISTS command
    fn parse_exists(elements: &[RespValue]) -> Result<Self> {
        if elements.len() < 2 {
            return Err(ProtocolError::WrongArity {
                command: "EXISTS",
                expected: 2,
                got: elements.len(),
            });
        }

        let mut keys = Vec::with_capacity(elements.len() - 1);
        for element in &elements[1..] {
            let key = element.as_bytes().ok_or(ProtocolError::InvalidKey)?.clone();
            keys.push(key);
        }

        Ok(RespCommand::Exists { keys })
    }

    /// Parse PING command
    fn parse_ping(elements: &[RespValue]) -> Result<Self> {
        match elements.len() {
            1 => Ok(RespCommand::Ping { message: None }),
            2 => {
                let message = elements[1].as_bytes().cloned();
                Ok(RespCommand::Ping { message })
            }
            _ => Err(ProtocolError::WrongArity {
                command: "PING",
                expected: 2,
                got: elements.len(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create bulk string arrays
    fn bulk_array(strings: &[&str]) -> RespValue {
        RespValue::Array(Some(
            strings
                .iter()
                .map(|s| RespValue::BulkString(Some(Bytes::from(s.to_string()))))
                .collect(),
        ))
    }

    // Helper function to create simple string arrays
    fn simple_array(strings: &[&str]) -> RespValue {
        RespValue::Array(Some(
            strings
                .iter()
                .map(|s| RespValue::SimpleString(Bytes::from(s.to_string())))
                .collect(),
        ))
    }

    // GET Command Tests

    #[test]
    fn test_get_command_valid() {
        let value = bulk_array(&["GET", "mykey"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Get { key } => {
                assert_eq!(key, Bytes::from("mykey"));
            }
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_get_command_case_insensitive() {
        let variations = vec!["GET", "get", "Get", "gEt"];

        for variation in variations {
            let value = bulk_array(&[variation, "key"]);
            let cmd = RespCommand::from_value(value).unwrap();
            assert!(matches!(cmd, RespCommand::Get { .. }));
        }
    }

    #[test]
    fn test_get_command_with_simple_string() {
        let value = simple_array(&["GET", "mykey"]);
        let cmd = RespCommand::from_value(value).unwrap();
        assert!(matches!(cmd, RespCommand::Get { .. }));
    }

    #[test]
    fn test_get_command_empty_key() {
        let value = bulk_array(&["GET", ""]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Get { key } => {
                assert_eq!(key, Bytes::from(""));
            }
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_get_command_wrong_arity_too_few() {
        let value = bulk_array(&["GET"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "GET");
                assert_eq!(expected, 2);
                assert_eq!(got, 1);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_get_command_wrong_arity_too_many() {
        let value = bulk_array(&["GET", "key1", "key2"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "GET");
                assert_eq!(expected, 2);
                assert_eq!(got, 3);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_get_command_invalid_key_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::Integer(42),
        ]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidKey));
    }

    // SET Command Tests

    #[test]
    fn test_set_command_valid() {
        let value = bulk_array(&["SET", "mykey", "myvalue"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Set { key, value } => {
                assert_eq!(key, Bytes::from("mykey"));
                assert_eq!(value, Bytes::from("myvalue"));
            }
            _ => panic!("Expected SET command"),
        }
    }

    #[test]
    fn test_set_command_case_insensitive() {
        let variations = vec!["SET", "set", "Set", "sEt"];

        for variation in variations {
            let value = bulk_array(&[variation, "key", "value"]);
            let cmd = RespCommand::from_value(value).unwrap();
            assert!(matches!(cmd, RespCommand::Set { .. }));
        }
    }

    #[test]
    fn test_set_command_empty_value() {
        let value = bulk_array(&["SET", "key", ""]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Set { value, .. } => {
                assert_eq!(value, Bytes::from(""));
            }
            _ => panic!("Expected SET command"),
        }
    }

    #[test]
    fn test_set_command_binary_data() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("SET"))),
            RespValue::BulkString(Some(Bytes::from("key"))),
            RespValue::BulkString(Some(Bytes::from(vec![0xff, 0xfe, 0xfd]))),
        ]));
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Set { value, .. } => {
                assert_eq!(value, Bytes::from(vec![0xff, 0xfe, 0xfd]));
            }
            _ => panic!("Expected SET command"),
        }
    }

    #[test]
    fn test_set_command_wrong_arity_too_few() {
        let value = bulk_array(&["SET", "key"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "SET");
                assert_eq!(expected, 3);
                assert_eq!(got, 2);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_set_command_wrong_arity_too_many() {
        let value = bulk_array(&["SET", "key", "value", "extra"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "SET");
                assert_eq!(expected, 3);
                assert_eq!(got, 4);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_set_command_invalid_key_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("SET"))),
            RespValue::Integer(42),
            RespValue::BulkString(Some(Bytes::from("value"))),
        ]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidKey));
    }

    #[test]
    fn test_set_command_invalid_value_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("SET"))),
            RespValue::BulkString(Some(Bytes::from("key"))),
            RespValue::Integer(42),
        ]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidValue));
    }

    // DEL Command Tests

    #[test]
    fn test_del_command_single_key() {
        let value = bulk_array(&["DEL", "key1"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Del { keys } => {
                assert_eq!(keys.len(), 1);
                assert_eq!(keys[0], Bytes::from("key1"));
            }
            _ => panic!("Expected DEL command"),
        }
    }

    #[test]
    fn test_del_command_multiple_keys() {
        let value = bulk_array(&["DEL", "key1", "key2", "key3"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Del { keys } => {
                assert_eq!(keys.len(), 3);
                assert_eq!(keys[0], Bytes::from("key1"));
                assert_eq!(keys[1], Bytes::from("key2"));
                assert_eq!(keys[2], Bytes::from("key3"));
            }
            _ => panic!("Expected DEL command"),
        }
    }

    #[test]
    fn test_del_command_case_insensitive() {
        let variations = vec!["DEL", "del", "Del", "dEl"];

        for variation in variations {
            let value = bulk_array(&[variation, "key"]);
            let cmd = RespCommand::from_value(value).unwrap();
            assert!(matches!(cmd, RespCommand::Del { .. }));
        }
    }

    #[test]
    fn test_del_command_wrong_arity() {
        let value = bulk_array(&["DEL"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "DEL");
                assert_eq!(expected, 2);
                assert_eq!(got, 1);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_del_command_invalid_key_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("DEL"))),
            RespValue::BulkString(Some(Bytes::from("key1"))),
            RespValue::Integer(42),
        ]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidKey));
    }

    // EXISTS Command Tests

    #[test]
    fn test_exists_command_single_key() {
        let value = bulk_array(&["EXISTS", "key1"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Exists { keys } => {
                assert_eq!(keys.len(), 1);
                assert_eq!(keys[0], Bytes::from("key1"));
            }
            _ => panic!("Expected EXISTS command"),
        }
    }

    #[test]
    fn test_exists_command_multiple_keys() {
        let value = bulk_array(&["EXISTS", "key1", "key2", "key3"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Exists { keys } => {
                assert_eq!(keys.len(), 3);
                assert_eq!(keys[0], Bytes::from("key1"));
                assert_eq!(keys[1], Bytes::from("key2"));
                assert_eq!(keys[2], Bytes::from("key3"));
            }
            _ => panic!("Expected EXISTS command"),
        }
    }

    #[test]
    fn test_exists_command_case_insensitive() {
        let variations = vec!["EXISTS", "exists", "Exists", "eXiStS"];

        for variation in variations {
            let value = bulk_array(&[variation, "key"]);
            let cmd = RespCommand::from_value(value).unwrap();
            assert!(matches!(cmd, RespCommand::Exists { .. }));
        }
    }

    #[test]
    fn test_exists_command_wrong_arity() {
        let value = bulk_array(&["EXISTS"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "EXISTS");
                assert_eq!(expected, 2);
                assert_eq!(got, 1);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_exists_command_invalid_key_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("EXISTS"))),
            RespValue::BulkString(Some(Bytes::from("key1"))),
            RespValue::Integer(42),
        ]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidKey));
    }

    // PING Command Tests

    #[test]
    fn test_ping_command_no_message() {
        let value = bulk_array(&["PING"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Ping { message } => {
                assert_eq!(message, None);
            }
            _ => panic!("Expected PING command"),
        }
    }

    #[test]
    fn test_ping_command_with_message() {
        let value = bulk_array(&["PING", "hello"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Ping { message } => {
                assert_eq!(message, Some(Bytes::from("hello")));
            }
            _ => panic!("Expected PING command"),
        }
    }

    #[test]
    fn test_ping_command_case_insensitive() {
        let variations = vec!["PING", "ping", "Ping", "pInG"];

        for variation in variations {
            let value = bulk_array(&[variation]);
            let cmd = RespCommand::from_value(value).unwrap();
            assert!(matches!(cmd, RespCommand::Ping { .. }));
        }
    }

    #[test]
    fn test_ping_command_empty_message() {
        let value = bulk_array(&["PING", ""]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Ping { message } => {
                assert_eq!(message, Some(Bytes::from("")));
            }
            _ => panic!("Expected PING command"),
        }
    }

    #[test]
    fn test_ping_command_wrong_arity() {
        let value = bulk_array(&["PING", "msg1", "msg2"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::WrongArity {
                command,
                expected,
                got,
            } => {
                assert_eq!(command, "PING");
                assert_eq!(expected, 2);
                assert_eq!(got, 3);
            }
            _ => panic!("Expected WrongArity error"),
        }
    }

    #[test]
    fn test_ping_command_invalid_message_type() {
        let value = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(Bytes::from("PING"))),
            RespValue::Integer(42),
        ]));
        let cmd = RespCommand::from_value(value).unwrap();

        // PING with non-string message should result in None
        match cmd {
            RespCommand::Ping { message } => {
                assert_eq!(message, None);
            }
            _ => panic!("Expected PING command"),
        }
    }

    // Error Condition Tests

    #[test]
    fn test_expected_array_error() {
        let value = RespValue::SimpleString(Bytes::from("GET"));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::ExpectedArray));
    }

    #[test]
    fn test_expected_array_error_integer() {
        let value = RespValue::Integer(42);
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::ExpectedArray));
    }

    #[test]
    fn test_expected_array_error_null() {
        let value = RespValue::Null;
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::ExpectedArray));
    }

    #[test]
    fn test_expected_array_error_array_none() {
        let value = RespValue::Array(None);
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::ExpectedArray));
    }

    #[test]
    fn test_empty_command_error() {
        let value = RespValue::Array(Some(vec![]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::EmptyCommand));
    }

    #[test]
    fn test_invalid_command_name_integer() {
        let value = RespValue::Array(Some(vec![RespValue::Integer(42)]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidCommandName));
    }

    #[test]
    fn test_invalid_command_name_null() {
        let value = RespValue::Array(Some(vec![RespValue::Null]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidCommandName));
    }

    #[test]
    fn test_invalid_command_name_bulk_string_none() {
        let value = RespValue::Array(Some(vec![RespValue::BulkString(None)]));
        let err = RespCommand::from_value(value).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidCommandName));
    }

    #[test]
    fn test_unknown_command() {
        let value = bulk_array(&["UNKNOWN"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::UnknownCommand { command } => {
                assert_eq!(command, "UNKNOWN");
            }
            _ => panic!("Expected UnknownCommand error"),
        }
    }

    #[test]
    fn test_unknown_command_similar_to_get() {
        let value = bulk_array(&["GETX"]);
        let err = RespCommand::from_value(value).unwrap_err();

        match err {
            ProtocolError::UnknownCommand { command } => {
                assert_eq!(command, "GETX");
            }
            _ => panic!("Expected UnknownCommand error"),
        }
    }

    // Roundtrip and Integration Tests

    #[test]
    fn test_command_clone() {
        let cmd = RespCommand::Get {
            key: Bytes::from("key"),
        };
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test]
    fn test_command_equality_get() {
        let cmd1 = RespCommand::Get {
            key: Bytes::from("key"),
        };
        let cmd2 = RespCommand::Get {
            key: Bytes::from("key"),
        };
        assert_eq!(cmd1, cmd2);
    }

    #[test]
    fn test_command_equality_set() {
        let cmd1 = RespCommand::Set {
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        };
        let cmd2 = RespCommand::Set {
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        };
        assert_eq!(cmd1, cmd2);
    }

    #[test]
    fn test_command_inequality_different_keys() {
        let cmd1 = RespCommand::Get {
            key: Bytes::from("key1"),
        };
        let cmd2 = RespCommand::Get {
            key: Bytes::from("key2"),
        };
        assert_ne!(cmd1, cmd2);
    }

    #[test]
    fn test_command_inequality_different_types() {
        let cmd1 = RespCommand::Get {
            key: Bytes::from("key"),
        };
        let cmd2 = RespCommand::Ping { message: None };
        assert_ne!(cmd1, cmd2);
    }

    #[test]
    fn test_command_debug_format() {
        let cmd = RespCommand::Get {
            key: Bytes::from("mykey"),
        };
        let debug = format!("{cmd:?}");
        assert!(debug.contains("Get"));
        assert!(debug.contains("key"));
    }

    #[test]
    fn test_zero_copy_bytes() {
        let value = bulk_array(&["GET", "mykey"]);
        let cmd = RespCommand::from_value(value).unwrap();

        match cmd {
            RespCommand::Get { key } => {
                // Cloning the key should be cheap (zero-copy)
                let key_clone = key.clone();
                assert_eq!(key.as_ptr(), key_clone.as_ptr());
            }
            _ => panic!("Expected GET command"),
        }
    }
}
