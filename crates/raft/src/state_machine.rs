//! State machine for the Raft consensus implementation.
//!
//! The state machine maintains the key-value store state and tracks the last applied
//! log index. It provides basic operations for reading and querying the state.

use seshat_protocol::Operation;
use std::collections::HashMap;

/// State machine that maintains key-value store state.
///
/// The state machine stores data as raw bytes and tracks which log index
/// was last applied. It provides read-only operations for querying state.
///
/// # Examples
///
/// ```
/// use seshat_raft::StateMachine;
///
/// let sm = StateMachine::new();
/// assert_eq!(sm.last_applied(), 0);
/// assert_eq!(sm.get(b"key"), None);
/// assert!(!sm.exists(b"key"));
/// ```
pub struct StateMachine {
    /// The key-value data store
    data: HashMap<Vec<u8>, Vec<u8>>,
    /// The last applied log index
    last_applied: u64,
}

impl StateMachine {
    /// Creates a new empty state machine.
    ///
    /// The state machine is initialized with an empty data store and
    /// last_applied set to 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    ///
    /// let sm = StateMachine::new();
    /// assert_eq!(sm.last_applied(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            last_applied: 0,
        }
    }

    /// Retrieves a value for the given key.
    ///
    /// Returns a clone of the value if the key exists, or None if the key
    /// is not present in the state machine.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    ///
    /// let sm = StateMachine::new();
    /// assert_eq!(sm.get(b"nonexistent"), None);
    /// ```
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }

    /// Checks if a key exists in the state machine.
    ///
    /// Returns true if the key exists, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    ///
    /// let sm = StateMachine::new();
    /// assert!(!sm.exists(b"nonexistent"));
    /// ```
    pub fn exists(&self, key: &[u8]) -> bool {
        self.data.contains_key(key)
    }

    /// Returns the last applied log index.
    ///
    /// This value indicates which log entry was most recently applied to the
    /// state machine. A value of 0 indicates no entries have been applied yet.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    ///
    /// let sm = StateMachine::new();
    /// assert_eq!(sm.last_applied(), 0);
    /// ```
    pub fn last_applied(&self) -> u64 {
        self.last_applied
    }

    /// Apply a log entry to the state machine.
    ///
    /// This method deserializes the operation from the provided data bytes,
    /// checks for idempotency (ensures the index hasn't already been applied),
    /// executes the operation on the internal HashMap, and updates the
    /// last_applied index.
    ///
    /// # Arguments
    ///
    /// * `index` - The log index being applied (must be > last_applied)
    /// * `data` - The serialized operation bytes
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - The operation result bytes
    /// * `Err(Box<dyn std::error::Error>)` - If the operation fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The index has already been applied (idempotency violation)
    /// - The index is out of order (lower than last_applied)
    /// - Deserialization fails
    /// - Operation execution fails
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    /// use seshat_protocol::Operation;
    ///
    /// let mut sm = StateMachine::new();
    /// let op = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let data = op.serialize().unwrap();
    /// let result = sm.apply(1, &data).unwrap();
    /// assert_eq!(result, b"OK");
    /// assert_eq!(sm.last_applied(), 1);
    /// assert_eq!(sm.get(b"foo"), Some(b"bar".to_vec()));
    /// ```
    pub fn apply(
        &mut self,
        index: u64,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Step 1: Idempotency check - reject if index <= last_applied
        if index <= self.last_applied {
            return Err(format!(
                "Entry already applied or out of order: index {} <= last_applied {}",
                index, self.last_applied
            )
            .into());
        }

        // Step 2: Deserialize the operation from bytes
        let operation = Operation::deserialize(data)?;

        // Step 3: Execute the operation on the state HashMap
        let result = operation.apply(&mut self.data)?;

        // Step 4: Update last_applied after successful execution
        self.last_applied = index;

        // Step 5: Return the operation result bytes
        Ok(result)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify it starts empty and with last_applied = 0
        assert_eq!(sm.last_applied(), 0);
        assert_eq!(sm.data.len(), 0);
    }

    #[test]
    fn test_get_empty() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify get returns None on empty state
        assert_eq!(sm.get(b"any_key"), None);
    }

    #[test]
    fn test_exists_empty() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify exists returns false on empty state
        assert!(!sm.exists(b"any_key"));
    }

    #[test]
    fn test_last_applied_initial() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify last_applied returns 0 initially
        assert_eq!(sm.last_applied(), 0);
    }

    #[test]
    fn test_get_nonexistent_key() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Test various nonexistent keys
        assert_eq!(sm.get(b""), None);
        assert_eq!(sm.get(b"nonexistent"), None);
        assert_eq!(sm.get(b"another_missing_key"), None);
    }

    #[test]
    fn test_exists_nonexistent_key() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Test various nonexistent keys
        assert!(!sm.exists(b""));
        assert!(!sm.exists(b"nonexistent"));
        assert!(!sm.exists(b"another_missing_key"));
    }

    #[test]
    fn test_get_with_empty_key() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify get with empty key returns None
        assert_eq!(sm.get(b""), None);
    }

    #[test]
    fn test_exists_with_empty_key() {
        // Create a new state machine
        let sm = StateMachine::new();

        // Verify exists with empty key returns false
        assert!(!sm.exists(b""));
    }

    #[test]
    fn test_default_trait() {
        // Verify Default trait creates a valid state machine
        let sm = StateMachine::default();
        assert_eq!(sm.last_applied(), 0);
        assert_eq!(sm.data.len(), 0);
    }

    // ========== NEW TESTS FOR apply() METHOD ==========

    #[test]
    fn test_apply_set_operation() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Create a Set operation
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let data = op.serialize().expect("Serialization should succeed");

        // Apply the operation
        let result = sm.apply(1, &data).expect("Apply should succeed");

        // Verify result is "OK"
        assert_eq!(result, b"OK");

        // Verify state is updated
        assert_eq!(sm.get(b"foo"), Some(b"bar".to_vec()));
        assert_eq!(sm.last_applied(), 1);
    }

    #[test]
    fn test_apply_del_operation_exists() {
        // Create a state machine with existing data
        let mut sm = StateMachine::new();
        let set_op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let set_data = set_op.serialize().expect("Serialization should succeed");
        sm.apply(1, &set_data).expect("Apply should succeed");

        // Create a Del operation
        let del_op = Operation::Del {
            key: b"foo".to_vec(),
        };
        let del_data = del_op.serialize().expect("Serialization should succeed");

        // Apply the delete operation
        let result = sm.apply(2, &del_data).expect("Apply should succeed");

        // Verify result is "1" (key existed and was deleted)
        assert_eq!(result, b"1");

        // Verify key is removed
        assert_eq!(sm.get(b"foo"), None);
        assert_eq!(sm.last_applied(), 2);
    }

    #[test]
    fn test_apply_del_operation_not_exists() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Create a Del operation for a nonexistent key
        let op = Operation::Del {
            key: b"nonexistent".to_vec(),
        };
        let data = op.serialize().expect("Serialization should succeed");

        // Apply the delete operation
        let result = sm.apply(1, &data).expect("Apply should succeed");

        // Verify result is "0" (key didn't exist)
        assert_eq!(result, b"0");
        assert_eq!(sm.last_applied(), 1);
    }

    #[test]
    fn test_operation_ordering() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Set a key to "first"
        let op1 = Operation::Set {
            key: b"key".to_vec(),
            value: b"first".to_vec(),
        };
        let data1 = op1.serialize().expect("Serialization should succeed");
        sm.apply(1, &data1).expect("Apply should succeed");
        assert_eq!(sm.get(b"key"), Some(b"first".to_vec()));

        // Set the same key to "second"
        let op2 = Operation::Set {
            key: b"key".to_vec(),
            value: b"second".to_vec(),
        };
        let data2 = op2.serialize().expect("Serialization should succeed");
        sm.apply(2, &data2).expect("Apply should succeed");
        assert_eq!(sm.get(b"key"), Some(b"second".to_vec()));

        // Set the same key to "third"
        let op3 = Operation::Set {
            key: b"key".to_vec(),
            value: b"third".to_vec(),
        };
        let data3 = op3.serialize().expect("Serialization should succeed");
        sm.apply(3, &data3).expect("Apply should succeed");
        assert_eq!(sm.get(b"key"), Some(b"third".to_vec()));
        assert_eq!(sm.last_applied(), 3);
    }

    #[test]
    fn test_idempotency_check() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Apply operation at index 1
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let data = op.serialize().expect("Serialization should succeed");
        sm.apply(1, &data).expect("First apply should succeed");

        // Try to apply at index 1 again (duplicate)
        let result = sm.apply(1, &data);
        assert!(result.is_err(), "Duplicate index should fail");
        assert!(result.unwrap_err().to_string().contains("already applied"));
    }

    #[test]
    fn test_out_of_order_rejected() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Apply operation at index 5
        let op1 = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let data1 = op1.serialize().expect("Serialization should succeed");
        sm.apply(5, &data1).expect("Apply should succeed");
        assert_eq!(sm.last_applied(), 5);

        // Try to apply at index 3 (out of order - lower than last_applied)
        let op2 = Operation::Set {
            key: b"baz".to_vec(),
            value: b"qux".to_vec(),
        };
        let data2 = op2.serialize().expect("Serialization should succeed");
        let result = sm.apply(3, &data2);
        assert!(result.is_err(), "Out of order index should fail");
        assert!(result.unwrap_err().to_string().contains("out of order"));
    }

    #[test]
    fn test_apply_multiple_operations() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Apply a sequence of operations
        let ops = vec![
            (
                1,
                Operation::Set {
                    key: b"key1".to_vec(),
                    value: b"value1".to_vec(),
                },
            ),
            (
                2,
                Operation::Set {
                    key: b"key2".to_vec(),
                    value: b"value2".to_vec(),
                },
            ),
            (
                3,
                Operation::Set {
                    key: b"key3".to_vec(),
                    value: b"value3".to_vec(),
                },
            ),
            (
                4,
                Operation::Del {
                    key: b"key2".to_vec(),
                },
            ),
        ];

        for (index, op) in ops {
            let data = op.serialize().expect("Serialization should succeed");
            sm.apply(index, &data).expect("Apply should succeed");
        }

        // Verify final state
        assert_eq!(sm.get(b"key1"), Some(b"value1".to_vec()));
        assert_eq!(sm.get(b"key2"), None); // Deleted
        assert_eq!(sm.get(b"key3"), Some(b"value3".to_vec()));
        assert_eq!(sm.last_applied(), 4);
    }

    #[test]
    fn test_apply_with_invalid_data() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Try to apply with corrupted bytes
        let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = sm.apply(1, &invalid_data);

        // Should fail with deserialization error
        assert!(result.is_err(), "Invalid data should fail");
    }

    #[test]
    fn test_apply_empty_key() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Create a Set operation with empty key
        let op = Operation::Set {
            key: vec![],
            value: b"value".to_vec(),
        };
        let data = op.serialize().expect("Serialization should succeed");

        // Apply the operation
        let result = sm.apply(1, &data).expect("Apply should succeed");

        // Verify result
        assert_eq!(result, b"OK");
        assert_eq!(sm.get(b""), Some(b"value".to_vec()));
        assert_eq!(sm.last_applied(), 1);
    }

    #[test]
    fn test_apply_large_value() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Create a Set operation with large value (10KB)
        let large_value = vec![0xAB; 10 * 1024];
        let op = Operation::Set {
            key: b"large_key".to_vec(),
            value: large_value.clone(),
        };
        let data = op.serialize().expect("Serialization should succeed");

        // Apply the operation
        let result = sm.apply(1, &data).expect("Apply should succeed");

        // Verify result
        assert_eq!(result, b"OK");
        assert_eq!(sm.get(b"large_key"), Some(large_value));
        assert_eq!(sm.last_applied(), 1);
    }
}
