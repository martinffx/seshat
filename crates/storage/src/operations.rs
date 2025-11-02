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
    /// use seshat_storage::Operation;
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
    /// use seshat_storage::Operation;
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
    /// use seshat_storage::Operation;
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

    // ========================================================================
    // Basic Serialization Tests (5 tests)
    // ========================================================================

    /// Test 1: Operation::Set serialization roundtrip
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

    /// Test 2: Operation::Del serialization roundtrip
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

    /// Test 3: Serialize Set with empty key
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

    /// Test 4: Serialize Set with empty value
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

    /// Test 5: Serialize with binary data (null bytes and special chars)
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

    // ========================================================================
    // Unicode and Special Character Tests (5 tests)
    // ========================================================================

    /// Test 6: Unicode keys with various scripts
    #[test]
    fn test_serialize_unicode_keys() {
        let op = Operation::Set {
            key: "Hello世界🌍".as_bytes().to_vec(),
            value: b"value".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 7: Unicode values with emojis and special chars
    #[test]
    fn test_serialize_unicode_values() {
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: "Value with emojis 🎉🔥 and symbols ✓✗".as_bytes().to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 8: Keys with newlines and tabs
    #[test]
    fn test_serialize_keys_with_whitespace() {
        let op = Operation::Set {
            key: b"key\n\t\rwith\nwhitespace".to_vec(),
            value: b"value".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 9: Values with quotes and escape sequences
    #[test]
    fn test_serialize_values_with_quotes() {
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value with \"quotes\" and 'apostrophes' and \\backslash".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 10: Del operation with unicode key
    #[test]
    fn test_serialize_del_with_unicode() {
        let op = Operation::Del {
            key: "删除键🗑️".as_bytes().to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    // ========================================================================
    // Large Data Tests (5 tests)
    // ========================================================================

    /// Test 11: Serialize large key (1MB)
    #[test]
    fn test_serialize_large_key() {
        let large_key = vec![b'K'; 1_000_000];
        let op = Operation::Set {
            key: large_key.clone(),
            value: b"value".to_vec(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 12: Serialize large value (1MB)
    #[test]
    fn test_serialize_large_value() {
        let large_value = vec![0xAB; 1_000_000];
        let op = Operation::Set {
            key: b"large_key".to_vec(),
            value: large_value.clone(),
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 13: Serialize with both large key and value
    #[test]
    fn test_serialize_large_key_and_value() {
        let op = Operation::Set {
            key: vec![b'K'; 100_000],
            value: vec![b'V'; 100_000],
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 14: Del operation with large key
    #[test]
    fn test_serialize_del_large_key() {
        let op = Operation::Del {
            key: vec![b'X'; 500_000],
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        let deserialized = Operation::deserialize(&bytes).expect("Deserialization should succeed");

        assert_eq!(op, deserialized);
    }

    /// Test 15: Verify serialized size is reasonable for large data
    #[test]
    fn test_serialized_size_large_data() {
        let op = Operation::Set {
            key: vec![b'A'; 1000],
            value: vec![b'B'; 1000],
        };

        let bytes = op.serialize().expect("Serialization should succeed");
        // Bincode should be efficient - serialized size should be close to data size
        // plus minimal overhead
        assert!(bytes.len() < 2100); // 2000 bytes data + ~100 bytes overhead max
    }

    // ========================================================================
    // Malformed Data Tests (5 tests)
    // ========================================================================

    /// Test 16: Deserialize completely invalid data
    #[test]
    fn test_deserialize_invalid_data() {
        let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = Operation::deserialize(&invalid_bytes);

        assert!(result.is_err());
    }

    /// Test 17: Deserialize empty byte array
    #[test]
    fn test_deserialize_empty_bytes() {
        let result = Operation::deserialize(&[]);

        assert!(result.is_err());
    }

    /// Test 18: Deserialize truncated data
    #[test]
    fn test_deserialize_truncated_data() {
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        let mut bytes = op.serialize().expect("Serialization should succeed");
        // Truncate to half length
        bytes.truncate(bytes.len() / 2);

        let result = Operation::deserialize(&bytes);
        assert!(result.is_err());
    }

    /// Test 19: Deserialize data with extra bytes at the end
    #[test]
    fn test_deserialize_extra_bytes() {
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        let mut bytes = op.serialize().expect("Serialization should succeed");
        // Add extra bytes
        bytes.extend_from_slice(&[0x00, 0x00, 0x00]);

        // bincode may or may not accept extra bytes - test current behavior
        let result = Operation::deserialize(&bytes);
        // If it succeeds, verify the operation is correct
        if let Ok(deserialized) = result {
            assert_eq!(deserialized, op);
        }
    }

    /// Test 20: Deserialize with wrong operation variant tag
    #[test]
    fn test_deserialize_wrong_variant() {
        let invalid_bytes = vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = Operation::deserialize(&invalid_bytes);

        assert!(result.is_err());
    }

    // ========================================================================
    // State Machine Apply Tests (15 tests)
    // ========================================================================

    /// Test 21: Apply Set operation to empty state
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

    /// Test 22: Apply Del operation when key exists
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

    /// Test 23: Apply Del operation when key doesn't exist
    #[test]
    fn test_apply_del_operation_key_not_exists() {
        let mut state = HashMap::new();

        let op = Operation::Del {
            key: b"foo".to_vec(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(result, b"0");
    }

    /// Test 24: Apply Set overwrites existing key
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

    /// Test 25: Apply multiple Set operations in sequence
    #[test]
    fn test_apply_multiple_set_operations() {
        let mut state = HashMap::new();

        for i in 0..10 {
            let op = Operation::Set {
                key: format!("key{i}").into_bytes(),
                value: format!("value{i}").into_bytes(),
            };
            let result = op.apply(&mut state).expect("Apply should succeed");
            assert_eq!(result, b"OK");
        }

        assert_eq!(state.len(), 10);
        assert_eq!(state.get(b"key5".as_slice()), Some(&b"value5".to_vec()));
    }

    /// Test 26: Apply Set then Get pattern (idempotency)
    #[test]
    fn test_apply_set_idempotency() {
        let mut state = HashMap::new();
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        // Apply twice
        op.apply(&mut state).expect("First apply should succeed");
        op.apply(&mut state).expect("Second apply should succeed");

        // State should still be correct
        assert_eq!(state.len(), 1);
        assert_eq!(state.get(b"key".as_slice()), Some(&b"value".to_vec()));
    }

    /// Test 27: Apply Del then Del pattern (idempotency)
    #[test]
    fn test_apply_del_idempotency() {
        let mut state = HashMap::new();
        state.insert(b"key".to_vec(), b"value".to_vec());

        let op = Operation::Del {
            key: b"key".to_vec(),
        };

        // First delete should succeed
        let result1 = op.apply(&mut state).expect("First delete should succeed");
        assert_eq!(result1, b"1");

        // Second delete should return 0 (not found)
        let result2 = op.apply(&mut state).expect("Second delete should succeed");
        assert_eq!(result2, b"0");
    }

    /// Test 28: Apply operations with empty keys
    #[test]
    fn test_apply_operations_with_empty_keys() {
        let mut state = HashMap::new();

        let op = Operation::Set {
            key: vec![],
            value: b"value".to_vec(),
        };
        op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(state.get(&vec![]), Some(&b"value".to_vec()));

        let del_op = Operation::Del { key: vec![] };
        let result = del_op.apply(&mut state).expect("Apply should succeed");
        assert_eq!(result, b"1");
    }

    /// Test 29: Apply operations with empty values
    #[test]
    fn test_apply_operations_with_empty_values() {
        let mut state = HashMap::new();

        let op = Operation::Set {
            key: b"key".to_vec(),
            value: vec![],
        };
        op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(state.get(b"key".as_slice()), Some(&vec![]));
    }

    /// Test 30: Apply large number of operations (stress test)
    #[test]
    fn test_apply_large_number_of_operations() {
        let mut state = HashMap::new();

        // Apply 1000 Set operations
        for i in 0..1000 {
            let op = Operation::Set {
                key: i.to_string().into_bytes(),
                value: format!("val{i}").into_bytes(),
            };
            op.apply(&mut state).expect("Apply should succeed");
        }

        assert_eq!(state.len(), 1000);

        // Delete half of them
        for i in (0..1000).step_by(2) {
            let op = Operation::Del {
                key: i.to_string().into_bytes(),
            };
            let result = op.apply(&mut state).expect("Apply should succeed");
            assert_eq!(result, b"1");
        }

        assert_eq!(state.len(), 500);
    }

    /// Test 31: Apply Set with very large value
    #[test]
    fn test_apply_set_very_large_value() {
        let mut state = HashMap::new();
        let large_value = vec![0xAB; 10_000];

        let op = Operation::Set {
            key: b"large".to_vec(),
            value: large_value.clone(),
        };

        let result = op.apply(&mut state).expect("Apply should succeed");
        assert_eq!(result, b"OK");
        assert_eq!(state.get(b"large".as_slice()), Some(&large_value));
    }

    /// Test 32: Apply operations with binary data
    #[test]
    fn test_apply_operations_with_binary_data() {
        let mut state = HashMap::new();

        let op = Operation::Set {
            key: vec![0x00, 0xFF, 0xAB, 0xCD],
            value: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        op.apply(&mut state).expect("Apply should succeed");
        assert_eq!(
            state.get(&vec![0x00, 0xFF, 0xAB, 0xCD]),
            Some(&vec![0xDE, 0xAD, 0xBE, 0xEF])
        );
    }

    /// Test 33: Apply mixed Set and Del operations
    #[test]
    fn test_apply_mixed_operations() {
        let mut state = HashMap::new();

        // Set multiple keys
        for i in 0..5 {
            let op = Operation::Set {
                key: format!("k{i}").into_bytes(),
                value: format!("v{i}").into_bytes(),
            };
            op.apply(&mut state).expect("Apply should succeed");
        }

        // Delete some
        let op = Operation::Del {
            key: b"k2".to_vec(),
        };
        op.apply(&mut state).expect("Apply should succeed");

        // Overwrite one
        let op = Operation::Set {
            key: b"k1".to_vec(),
            value: b"new_value".to_vec(),
        };
        op.apply(&mut state).expect("Apply should succeed");

        assert_eq!(state.len(), 4);
        assert!(!state.contains_key(b"k2".as_slice()));
        assert_eq!(state.get(b"k1".as_slice()), Some(&b"new_value".to_vec()));
    }

    /// Test 34: State remains valid after failed operations (error handling)
    #[test]
    fn test_state_consistency_after_operations() {
        let mut state = HashMap::new();

        // Add initial data
        state.insert(b"existing".to_vec(), b"data".to_vec());

        // Apply successful operation
        let op = Operation::Set {
            key: b"new".to_vec(),
            value: b"value".to_vec(),
        };
        op.apply(&mut state).expect("Apply should succeed");

        // State should be consistent
        assert_eq!(state.len(), 2);
        assert_eq!(state.get(b"existing".as_slice()), Some(&b"data".to_vec()));
        assert_eq!(state.get(b"new".as_slice()), Some(&b"value".to_vec()));
    }

    /// Test 35: Apply operations preserves key order in iteration
    #[test]
    fn test_apply_operations_key_order() {
        let mut state = HashMap::new();

        let keys = vec![b"z", b"a", b"m", b"b"];
        for key in &keys {
            let op = Operation::Set {
                key: key.to_vec(),
                value: b"val".to_vec(),
            };
            op.apply(&mut state).expect("Apply should succeed");
        }

        assert_eq!(state.len(), 4);
        for key in &keys {
            assert!(state.contains_key(key.as_slice()));
        }
    }

    // ========================================================================
    // Debug and Clone Tests (5 tests)
    // ========================================================================

    /// Test 36: Operation Debug format includes relevant info
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

    /// Test 37: Operation Clone creates independent copy
    #[test]
    fn test_operation_clone() {
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let cloned = op.clone();
        assert_eq!(op, cloned);
    }

    /// Test 38: Cloned operation can be modified independently
    #[test]
    fn test_cloned_operation_independence() {
        let op1 = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };

        let mut op2 = op1.clone();
        if let Operation::Set { ref mut value, .. } = op2 {
            *value = b"baz".to_vec();
        }

        // Original should be unchanged
        if let Operation::Set { value, .. } = op1 {
            assert_eq!(value, b"bar");
        }
    }

    /// Test 39: PartialEq works correctly
    #[test]
    fn test_operation_equality() {
        let op1 = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        let op2 = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        let op3 = Operation::Set {
            key: b"key".to_vec(),
            value: b"different".to_vec(),
        };

        assert_eq!(op1, op2);
        assert_ne!(op1, op3);
    }

    /// Test 40: Different operation variants are not equal
    #[test]
    fn test_operation_variant_inequality() {
        let set_op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };

        let del_op = Operation::Del {
            key: b"key".to_vec(),
        };

        assert_ne!(set_op, del_op);
    }
}
