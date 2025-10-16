//! State machine for the Raft consensus implementation.
//!
//! The state machine maintains the key-value store state and tracks the last applied
//! log index. It provides basic operations for reading and querying the state.

use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Creates a snapshot of the current state machine.
    ///
    /// This method serializes the entire state machine (data and last_applied)
    /// into a byte vector using bincode. The snapshot can be used for log
    /// compaction or transferring state to new Raft nodes.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - The serialized snapshot bytes
    /// * `Err(Box<dyn std::error::Error>)` - If serialization fails
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
    /// sm.apply(1, &data).unwrap();
    ///
    /// let snapshot = sm.snapshot().unwrap();
    /// assert!(!snapshot.is_empty());
    /// ```
    pub fn snapshot(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        bincode::serialize(self).map_err(|e| e.into())
    }

    /// Restores the state machine from a snapshot.
    ///
    /// This method deserializes a snapshot and replaces the current state
    /// machine data and last_applied index with the snapshot contents.
    /// Any existing state is completely overwritten.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The serialized snapshot bytes
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If restoration succeeds
    /// * `Err(Box<dyn std::error::Error>)` - If deserialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::StateMachine;
    /// use seshat_protocol::Operation;
    ///
    /// let mut sm1 = StateMachine::new();
    /// let op = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let data = op.serialize().unwrap();
    /// sm1.apply(1, &data).unwrap();
    ///
    /// let snapshot = sm1.snapshot().unwrap();
    ///
    /// let mut sm2 = StateMachine::new();
    /// sm2.restore(&snapshot).unwrap();
    /// assert_eq!(sm2.get(b"foo"), Some(b"bar".to_vec()));
    /// assert_eq!(sm2.last_applied(), 1);
    /// ```
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let restored: StateMachine = bincode::deserialize(snapshot)?;
        self.data = restored.data;
        self.last_applied = restored.last_applied;
        Ok(())
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

    // ========== NEW TESTS FOR snapshot() AND restore() METHODS ==========

    #[test]
    fn test_snapshot_empty() {
        // Create an empty state machine
        let sm = StateMachine::new();

        // Create a snapshot
        let snapshot = sm.snapshot().expect("Snapshot should succeed");

        // Verify snapshot is not empty (contains at least metadata)
        assert!(!snapshot.is_empty(), "Snapshot should not be empty");
    }

    #[test]
    fn test_snapshot_with_data() {
        // Create a state machine with some data
        let mut sm = StateMachine::new();
        let op = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let data = op.serialize().expect("Serialization should succeed");
        sm.apply(1, &data).expect("Apply should succeed");

        // Create a snapshot
        let snapshot = sm.snapshot().expect("Snapshot should succeed");

        // Verify snapshot is not empty
        assert!(!snapshot.is_empty(), "Snapshot should contain data");
    }

    #[test]
    fn test_restore_from_snapshot() {
        // Create a state machine with data
        let mut sm1 = StateMachine::new();
        let op1 = Operation::Set {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };
        let data1 = op1.serialize().expect("Serialization should succeed");
        sm1.apply(1, &data1).expect("Apply should succeed");

        let op2 = Operation::Set {
            key: b"key2".to_vec(),
            value: b"value2".to_vec(),
        };
        let data2 = op2.serialize().expect("Serialization should succeed");
        sm1.apply(2, &data2).expect("Apply should succeed");

        // Create a snapshot
        let snapshot = sm1.snapshot().expect("Snapshot should succeed");

        // Create a new state machine and restore from snapshot
        let mut sm2 = StateMachine::new();
        sm2.restore(&snapshot).expect("Restore should succeed");

        // Verify the state was restored correctly
        assert_eq!(sm2.get(b"key1"), Some(b"value1".to_vec()));
        assert_eq!(sm2.get(b"key2"), Some(b"value2".to_vec()));
        assert_eq!(sm2.last_applied(), 2);
    }

    #[test]
    fn test_snapshot_restore_roundtrip() {
        // Create a state machine with multiple operations
        let mut sm1 = StateMachine::new();
        let ops = [
            Operation::Set {
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            },
            Operation::Set {
                key: b"b".to_vec(),
                value: b"2".to_vec(),
            },
            Operation::Set {
                key: b"c".to_vec(),
                value: b"3".to_vec(),
            },
        ];

        for (i, op) in ops.iter().enumerate() {
            let data = op.serialize().expect("Serialization should succeed");
            sm1.apply((i + 1) as u64, &data)
                .expect("Apply should succeed");
        }

        // Create snapshot
        let snapshot = sm1.snapshot().expect("Snapshot should succeed");

        // Restore to new state machine
        let mut sm2 = StateMachine::new();
        sm2.restore(&snapshot).expect("Restore should succeed");

        // Verify all data matches
        assert_eq!(sm2.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(sm2.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(sm2.get(b"c"), Some(b"3".to_vec()));
        assert_eq!(sm2.last_applied(), 3);
        assert_eq!(sm2.data.len(), 3);
    }

    #[test]
    fn test_restore_empty_snapshot() {
        // Create an empty state machine and snapshot it
        let sm1 = StateMachine::new();
        let snapshot = sm1.snapshot().expect("Snapshot should succeed");

        // Restore to new state machine
        let mut sm2 = StateMachine::new();
        sm2.restore(&snapshot).expect("Restore should succeed");

        // Verify state is empty
        assert_eq!(sm2.last_applied(), 0);
        assert_eq!(sm2.data.len(), 0);
    }

    #[test]
    fn test_restore_overwrites_existing_state() {
        // Create a state machine with some data
        let mut sm1 = StateMachine::new();
        let op1 = Operation::Set {
            key: b"old_key".to_vec(),
            value: b"old_value".to_vec(),
        };
        let data1 = op1.serialize().expect("Serialization should succeed");
        sm1.apply(1, &data1).expect("Apply should succeed");

        // Create another state machine with different data
        let mut sm2 = StateMachine::new();
        let op2 = Operation::Set {
            key: b"new_key".to_vec(),
            value: b"new_value".to_vec(),
        };
        let data2 = op2.serialize().expect("Serialization should succeed");
        sm2.apply(5, &data2).expect("Apply should succeed");

        // Create snapshot from sm2
        let snapshot = sm2.snapshot().expect("Snapshot should succeed");

        // Restore sm1 from sm2's snapshot
        sm1.restore(&snapshot).expect("Restore should succeed");

        // Verify sm1 now has sm2's state
        assert_eq!(sm1.get(b"old_key"), None); // Old data gone
        assert_eq!(sm1.get(b"new_key"), Some(b"new_value".to_vec())); // New data present
        assert_eq!(sm1.last_applied(), 5);
    }

    #[test]
    fn test_restore_with_invalid_data() {
        // Create a state machine
        let mut sm = StateMachine::new();

        // Try to restore from corrupted snapshot data
        let invalid_snapshot = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = sm.restore(&invalid_snapshot);

        // Should fail with deserialization error
        assert!(result.is_err(), "Invalid snapshot should fail to restore");
    }

    #[test]
    fn test_snapshot_large_state() {
        // Create a state machine with many keys
        let mut sm = StateMachine::new();
        for i in 0..100 {
            let key = format!("key{i}").into_bytes();
            let value = format!("value{i}").into_bytes();
            let op = Operation::Set { key, value };
            let data = op.serialize().expect("Serialization should succeed");
            sm.apply(i + 1, &data).expect("Apply should succeed");
        }

        // Create snapshot
        let snapshot = sm.snapshot().expect("Snapshot should succeed");

        // Restore to new state machine
        let mut sm2 = StateMachine::new();
        sm2.restore(&snapshot).expect("Restore should succeed");

        // Verify all 100 keys are present
        for i in 0..100 {
            let key = format!("key{i}").into_bytes();
            let expected_value = format!("value{i}").into_bytes();
            assert_eq!(sm2.get(&key), Some(expected_value));
        }
        assert_eq!(sm2.last_applied(), 100);
        assert_eq!(sm2.data.len(), 100);
    }
}
