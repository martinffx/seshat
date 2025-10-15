//! Operation types for state machine commands
//!
//! This module defines the operations that can be applied to the key-value store
//! state machine. Operations are serialized using bincode for storage in the Raft log
//! and can be applied to a HashMap to modify the state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during operation processing
#[derive(Error, Debug)]
pub enum OperationError {
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),
}

/// Result type for operation methods
pub type OperationResult<T> = Result<T, OperationError>;

/// Operations that can be applied to the state machine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Set a key-value pair
    Set {
        /// The key to set
        key: Vec<u8>,
        /// The value to set
        value: Vec<u8>,
    },
    /// Delete a key
    Del {
        /// The key to delete
        key: Vec<u8>,
    },
}

impl Operation {
    /// Apply this operation to a state HashMap
    ///
    /// # Arguments
    ///
    /// * `state` - Mutable reference to the state HashMap
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - Response bytes ("OK" for Set, "1"/"0" for Del)
    /// * `Err(OperationError)` - If the operation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol::Operation;
    /// use std::collections::HashMap;
    ///
    /// let mut state = HashMap::new();
    /// let op = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let result = op.apply(&mut state).unwrap();
    /// assert_eq!(result, b"OK");
    /// assert_eq!(state.get(&b"foo".to_vec()), Some(&b"bar".to_vec()));
    /// ```
    pub fn apply(&self, state: &mut HashMap<Vec<u8>, Vec<u8>>) -> OperationResult<Vec<u8>> {
        match self {
            Operation::Set { key, value } => {
                state.insert(key.clone(), value.clone());
                Ok(b"OK".to_vec())
            }
            Operation::Del { key } => {
                if state.remove(key).is_some() {
                    Ok(b"1".to_vec())
                } else {
                    Ok(b"0".to_vec())
                }
            }
        }
    }

    /// Serialize this operation to bytes
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - The serialized operation
    /// * `Err(OperationError)` - If serialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol::Operation;
    ///
    /// let op = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let bytes = op.serialize().unwrap();
    /// assert!(!bytes.is_empty());
    /// ```
    pub fn serialize(&self) -> OperationResult<Vec<u8>> {
        bincode::serialize(self).map_err(OperationError::SerializationError)
    }

    /// Deserialize an operation from bytes
    ///
    /// # Arguments
    ///
    /// * `bytes` - The bytes to deserialize
    ///
    /// # Returns
    ///
    /// * `Ok(Operation)` - The deserialized operation
    /// * `Err(OperationError)` - If deserialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol::Operation;
    ///
    /// let op = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let bytes = op.serialize().unwrap();
    /// let deserialized = Operation::deserialize(&bytes).unwrap();
    /// assert_eq!(op, deserialized);
    /// ```
    pub fn deserialize(bytes: &[u8]) -> OperationResult<Operation> {
        bincode::deserialize(bytes).map_err(OperationError::SerializationError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: Operation::Set serialization roundtrip
    #[test]
    fn test_operation_set_serialization_roundtrip() {
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let serialized = op.serialize().expect("Serialization should succeed");
        let deserialized =
            Operation::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    // Test 2: Operation::Del serialization roundtrip
    #[test]
    fn test_operation_del_serialization_roundtrip() {
        let op = Operation::Del {
            key: b"foo".to_vec(),
        };

        let serialized = op.serialize().expect("Serialization should succeed");
        let deserialized =
            Operation::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    // Test 3: Apply Set operation
    #[test]
    fn test_apply_set_operation() {
        let mut state = HashMap::new();
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"OK");
        assert_eq!(state.get(b"foo".as_slice()), Some(&b"bar".to_vec()));
    }

    // Test 4: Apply Del operation (key exists)
    #[test]
    fn test_apply_del_operation_key_exists() {
        let mut state = HashMap::new();
        state.insert(b"foo".to_vec(), b"bar".to_vec());

        let op = Operation::Del {
            key: b"foo".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"1");
        assert!(!state.contains_key(b"foo".as_slice()));
    }

    // Test 5: Apply Del operation (key doesn't exist)
    #[test]
    fn test_apply_del_operation_key_not_exists() {
        let mut state = HashMap::new();

        let op = Operation::Del {
            key: b"foo".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"0");
    }

    // Additional comprehensive tests

    #[test]
    fn test_serialize_then_deserialize_set() {
        let op = Operation::Set {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_serialize_then_deserialize_del() {
        let op = Operation::Del {
            key: b"key1".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_apply_set_updates_state() {
        let mut state = HashMap::new();
        let op = Operation::Set {
            key: b"mykey".to_vec(),
            value: b"myvalue".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"OK");
        assert_eq!(state.len(), 1);
        assert_eq!(state.get(b"mykey".as_slice()), Some(&b"myvalue".to_vec()));
    }

    #[test]
    fn test_apply_set_overwrites_existing() {
        let mut state = HashMap::new();
        state.insert(b"key".to_vec(), b"old".to_vec());

        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"new".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"OK");
        assert_eq!(state.get(b"key".as_slice()), Some(&b"new".to_vec()));
    }

    #[test]
    fn test_apply_del_removes_key() {
        let mut state = HashMap::new();
        state.insert(b"key".to_vec(), b"value".to_vec());

        let op = Operation::Del {
            key: b"key".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"1");
        assert!(!state.contains_key(b"key".as_slice()));
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_serialize_with_empty_key() {
        let op = Operation::Set {
            key: vec![],
            value: b"value".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_serialize_with_empty_value() {
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: vec![],
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_serialize_with_binary_data() {
        let op = Operation::Set {
            key: vec![0x00, 0xFF, 0xAB],
            value: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_apply_multiple_operations() {
        let mut state = HashMap::new();

        // Set multiple keys
        let op1 = Operation::Set {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };
        let op2 = Operation::Set {
            key: b"key2".to_vec(),
            value: b"value2".to_vec(),
        };
        let op3 = Operation::Set {
            key: b"key3".to_vec(),
            value: b"value3".to_vec(),
        };

        op1.apply(&mut state).expect("Apply should succeed");
        op2.apply(&mut state).expect("Apply should succeed");
        op3.apply(&mut state).expect("Apply should succeed");

        assert_eq!(state.len(), 3);

        // Delete one key
        let op4 = Operation::Del {
            key: b"key2".to_vec(),
        };
        let result = op4.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"1");
        assert_eq!(state.len(), 2);
        assert!(!state.contains_key(b"key2".as_slice()));
    }

    #[test]
    fn test_serialize_deserialize_large_value() {
        let large_value = vec![0xAB; 10_000];
        let op = Operation::Set {
            key: b"large_key".to_vec(),
            value: large_value.clone(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_deserialize_invalid_data() {
        let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = Operation::deserialize(&invalid_bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_operation_debug_format() {
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let debug_str = format!("{op:?}");
        assert!(debug_str.contains("Set"));
        assert!(debug_str.contains("key"));
        assert!(debug_str.contains("value"));
    }

    #[test]
    fn test_operation_clone() {
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let cloned = op.clone();
        assert_eq!(op, cloned);
    }
}
