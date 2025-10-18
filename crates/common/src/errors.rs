//! Error types for Seshat distributed key-value store.
//!
//! This module defines the common error types used across all Seshat crates.
//! Uses `thiserror` for ergonomic error handling.

use thiserror::Error;

/// Common error type for Seshat operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Operation attempted on a non-leader node.
    #[error("not leader{}", match .leader_id {
        Some(id) => format!(": current leader is node {id}"),
        None => String::new(),
    })]
    NotLeader {
        /// The current leader node ID, if known.
        leader_id: Option<u64>,
    },

    /// Quorum cannot be achieved for the operation.
    #[error("no quorum: cluster cannot achieve quorum")]
    NoQuorum,

    /// Raft consensus error.
    #[error("raft error: {0}")]
    Raft(String),

    /// Storage layer error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    ConfigError(String),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Convenience type alias for Result with Seshat Error.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_leader_error_without_leader_id() {
        let err = Error::NotLeader { leader_id: None };
        assert_eq!(err.to_string(), "not leader");
    }

    #[test]
    fn test_not_leader_error_with_leader_id() {
        let err = Error::NotLeader {
            leader_id: Some(42),
        };
        assert_eq!(err.to_string(), "not leader: current leader is node 42");
    }

    #[test]
    fn test_not_leader_with_multiple_leader_ids() {
        let err1 = Error::NotLeader { leader_id: Some(1) };
        let err2 = Error::NotLeader { leader_id: Some(2) };
        let err3 = Error::NotLeader {
            leader_id: Some(999),
        };

        assert_eq!(err1.to_string(), "not leader: current leader is node 1");
        assert_eq!(err2.to_string(), "not leader: current leader is node 2");
        assert_eq!(err3.to_string(), "not leader: current leader is node 999");
    }

    #[test]
    fn test_no_quorum_error() {
        let err = Error::NoQuorum;
        assert_eq!(err.to_string(), "no quorum: cluster cannot achieve quorum");
    }

    #[test]
    fn test_raft_error() {
        let err = Error::Raft("leader election failed".to_string());
        assert_eq!(err.to_string(), "raft error: leader election failed");
    }

    #[test]
    fn test_raft_error_empty_string() {
        let err = Error::Raft(String::new());
        assert_eq!(err.to_string(), "raft error: ");
    }

    #[test]
    fn test_storage_error() {
        let err = Error::Storage("failed to write to disk".to_string());
        assert_eq!(err.to_string(), "storage error: failed to write to disk");
    }

    #[test]
    fn test_storage_error_with_detail() {
        let err = Error::Storage("RocksDB write failed: IO error".to_string());
        assert_eq!(
            err.to_string(),
            "storage error: RocksDB write failed: IO error"
        );
    }

    #[test]
    fn test_config_error() {
        let err = Error::ConfigError("invalid port number".to_string());
        assert_eq!(err.to_string(), "configuration error: invalid port number");
    }

    #[test]
    fn test_config_error_various_messages() {
        let err1 = Error::ConfigError("missing required field".to_string());
        let err2 = Error::ConfigError("invalid format".to_string());

        assert_eq!(
            err1.to_string(),
            "configuration error: missing required field"
        );
        assert_eq!(err2.to_string(), "configuration error: invalid format");
    }

    #[test]
    fn test_serialization_error() {
        let err = Error::Serialization("failed to decode bincode".to_string());
        assert_eq!(
            err.to_string(),
            "serialization error: failed to decode bincode"
        );
    }

    #[test]
    fn test_error_is_debug() {
        let err = Error::NoQuorum;
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("NoQuorum"));
    }

    #[test]
    fn test_error_debug_includes_fields() {
        let err = Error::NotLeader {
            leader_id: Some(42),
        };
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("NotLeader"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Error>();
        assert_sync::<Error>();
    }

    #[test]
    fn test_result_type_alias_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert_eq!(val, 42);
        }
    }

    #[test]
    fn test_result_type_alias_err() {
        let result: Result<i32> = Err(Error::NoQuorum);
        assert!(result.is_err());
    }

    #[test]
    fn test_result_type_alias_with_various_types() {
        let result_string: Result<String> = Ok("test".to_string());
        let result_vec: Result<Vec<u8>> = Ok(vec![1, 2, 3]);
        let result_unit: Result<()> = Ok(());

        assert!(result_string.is_ok());
        assert!(result_vec.is_ok());
        assert!(result_unit.is_ok());
    }

    #[test]
    fn test_error_can_be_propagated() {
        fn inner() -> Result<()> {
            Err(Error::NoQuorum)
        }

        fn outer() -> Result<()> {
            inner()?;
            Ok(())
        }

        let result = outer();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NoQuorum));
    }

    #[test]
    fn test_error_propagation_with_different_types() {
        fn inner() -> Result<i32> {
            Err(Error::Storage("disk full".to_string()))
        }

        fn outer() -> Result<String> {
            let value = inner()?;
            Ok(value.to_string())
        }

        let result = outer();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Storage(_)));
    }

    #[test]
    fn test_error_pattern_matching() {
        let err = Error::NotLeader {
            leader_id: Some(42),
        };

        match err {
            Error::NotLeader { leader_id } => {
                assert_eq!(leader_id, Some(42));
            }
            _ => panic!("Expected NotLeader error"),
        }
    }

    #[test]
    fn test_all_error_variants_are_displayable() {
        let errors = vec![
            Error::NotLeader { leader_id: None },
            Error::NotLeader { leader_id: Some(1) },
            Error::NoQuorum,
            Error::Raft("test".to_string()),
            Error::Storage("test".to_string()),
            Error::ConfigError("test".to_string()),
            Error::Serialization("test".to_string()),
        ];

        for err in errors {
            let display = err.to_string();
            assert!(!display.is_empty());
        }
    }
}
