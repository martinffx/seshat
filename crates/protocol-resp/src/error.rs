//! RESP protocol error types
//!
//! This module defines all error variants that can occur during RESP protocol
//! parsing, encoding, and command processing.

use thiserror::Error;

/// Protocol error types for RESP parsing and command handling
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Unknown RESP type marker byte
    ///
    /// This error occurs when the parser encounters a byte that doesn't match
    /// any known RESP type marker (+, -, :, $, *, _, #, etc.).
    #[error("Invalid type marker: {0:#x}")]
    InvalidTypeMarker(u8),

    /// Malformed length parsing
    ///
    /// This error occurs when parsing length prefixes for bulk strings, arrays,
    /// or other length-prefixed types fails (e.g., non-numeric characters or negative
    /// lengths where not allowed).
    #[error("Invalid length")]
    InvalidLength,

    /// Bulk string exceeds maximum allowed size
    ///
    /// This error occurs when a bulk string length exceeds the configured maximum
    /// (default 512 MB) to prevent memory exhaustion attacks.
    #[error("Bulk string too large: {size} bytes (max: {max})")]
    BulkStringTooLarge {
        /// The size of the bulk string being parsed
        size: usize,
        /// The maximum allowed size
        max: usize,
    },

    /// Array exceeds maximum allowed elements
    ///
    /// This error occurs when an array element count exceeds the configured maximum
    /// (default 1M elements) to prevent memory exhaustion attacks.
    #[error("Array too large: {size} elements (max: {max})")]
    ArrayTooLarge {
        /// The number of elements in the array
        size: usize,
        /// The maximum allowed elements
        max: usize,
    },

    /// Nesting depth exceeds maximum allowed
    ///
    /// This error occurs when nested structures (arrays, maps, sets) exceed the
    /// configured maximum depth (default 32 levels) to prevent stack overflow.
    #[error("Nesting too deep (max: 32 levels)")]
    NestingTooDeep,

    /// Expected array for command parsing
    ///
    /// This error occurs when attempting to parse a command from a non-array
    /// RespValue. Commands must be represented as arrays.
    #[error("Expected array for command")]
    ExpectedArray,

    /// No command provided
    ///
    /// This error occurs when parsing an empty array as a command.
    /// Commands must have at least a command name.
    #[error("Empty command")]
    EmptyCommand,

    /// Invalid command name format
    ///
    /// This error occurs when the command name (first element of command array)
    /// is not a valid string type (SimpleString or BulkString).
    #[error("Invalid command name")]
    InvalidCommandName,

    /// Invalid key format
    ///
    /// This error occurs when a key parameter is not a valid string type.
    #[error("Invalid key")]
    InvalidKey,

    /// Invalid value format
    ///
    /// This error occurs when a value parameter is not a valid string type.
    #[error("Invalid value")]
    InvalidValue,

    /// Wrong number of arguments for command
    ///
    /// This error occurs when a command receives an incorrect number of arguments.
    /// Provides details about the command, expected count, and actual count.
    #[error("Wrong number of arguments for '{command}': expected {expected}, got {got}")]
    WrongArity {
        /// The command name that received wrong arity
        command: &'static str,
        /// The expected number of arguments (including command name)
        expected: usize,
        /// The actual number of arguments received
        got: usize,
    },

    /// Unknown command
    ///
    /// This error occurs when the command name doesn't match any supported command.
    #[error("Unknown command: {command}")]
    UnknownCommand {
        /// The unknown command name
        command: String,
    },

    /// I/O error
    ///
    /// This error wraps std::io::Error for I/O operations during parsing or encoding.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// UTF-8 decoding error
    ///
    /// This error wraps std::str::Utf8Error when byte sequences cannot be decoded
    /// as valid UTF-8 strings.
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

/// Convenient Result type alias for protocol operations
pub type Result<T> = std::result::Result<T, ProtocolError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_type_marker_display() {
        let err = ProtocolError::InvalidTypeMarker(0x3f);
        assert_eq!(err.to_string(), "Invalid type marker: 0x3f");
    }

    #[test]
    fn test_invalid_type_marker_hex_format() {
        let err = ProtocolError::InvalidTypeMarker(0xff);
        assert_eq!(err.to_string(), "Invalid type marker: 0xff");
    }

    #[test]
    fn test_invalid_length_display() {
        let err = ProtocolError::InvalidLength;
        assert_eq!(err.to_string(), "Invalid length");
    }

    #[test]
    fn test_bulk_string_too_large_display() {
        let err = ProtocolError::BulkStringTooLarge {
            size: 1024,
            max: 512,
        };
        assert_eq!(
            err.to_string(),
            "Bulk string too large: 1024 bytes (max: 512)"
        );
    }

    #[test]
    fn test_array_too_large_display() {
        let err = ProtocolError::ArrayTooLarge {
            size: 2000000,
            max: 1000000,
        };
        assert_eq!(
            err.to_string(),
            "Array too large: 2000000 elements (max: 1000000)"
        );
    }

    #[test]
    fn test_nesting_too_deep_display() {
        let err = ProtocolError::NestingTooDeep;
        assert_eq!(err.to_string(), "Nesting too deep (max: 32 levels)");
    }

    #[test]
    fn test_expected_array_display() {
        let err = ProtocolError::ExpectedArray;
        assert_eq!(err.to_string(), "Expected array for command");
    }

    #[test]
    fn test_empty_command_display() {
        let err = ProtocolError::EmptyCommand;
        assert_eq!(err.to_string(), "Empty command");
    }

    #[test]
    fn test_invalid_command_name_display() {
        let err = ProtocolError::InvalidCommandName;
        assert_eq!(err.to_string(), "Invalid command name");
    }

    #[test]
    fn test_invalid_key_display() {
        let err = ProtocolError::InvalidKey;
        assert_eq!(err.to_string(), "Invalid key");
    }

    #[test]
    fn test_invalid_value_display() {
        let err = ProtocolError::InvalidValue;
        assert_eq!(err.to_string(), "Invalid value");
    }

    #[test]
    fn test_wrong_arity_display() {
        let err = ProtocolError::WrongArity {
            command: "GET",
            expected: 2,
            got: 3,
        };
        assert_eq!(
            err.to_string(),
            "Wrong number of arguments for 'GET': expected 2, got 3"
        );
    }

    #[test]
    fn test_wrong_arity_display_set() {
        let err = ProtocolError::WrongArity {
            command: "SET",
            expected: 3,
            got: 2,
        };
        assert_eq!(
            err.to_string(),
            "Wrong number of arguments for 'SET': expected 3, got 2"
        );
    }

    #[test]
    fn test_unknown_command_display() {
        let err = ProtocolError::UnknownCommand {
            command: "UNKNOWN".to_string(),
        };
        assert_eq!(err.to_string(), "Unknown command: UNKNOWN");
    }

    #[test]
    fn test_io_error_from_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let err: ProtocolError = io_err.into();
        assert!(matches!(err, ProtocolError::Io(_)));
        assert!(err.to_string().contains("pipe broken"));
    }

    #[test]
    fn test_utf8_error_from_conversion() {
        let invalid_utf8 = vec![0xff, 0xfe, 0xfd];
        let utf8_result = std::str::from_utf8(&invalid_utf8);
        assert!(utf8_result.is_err());

        let utf8_err = utf8_result.unwrap_err();
        let err: ProtocolError = utf8_err.into();
        assert!(matches!(err, ProtocolError::Utf8(_)));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ProtocolError>();
        assert_sync::<ProtocolError>();
    }

    #[test]
    fn test_result_type_alias() {
        let ok_result: Result<i32> = Ok(42);
        assert!(ok_result.is_ok());
        assert_eq!(ok_result.ok(), Some(42));

        let err_result: Result<i32> = Err(ProtocolError::EmptyCommand);
        assert!(err_result.is_err());
    }
}
