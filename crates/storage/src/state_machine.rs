//! Re-export StateMachine from seshat-raft.
//!
//! This allows storage implementations to use the StateMachine without
//! creating a circular dependency.

// For now, we'll define a minimal StateMachine here
// In the future, this could be moved to seshat-common

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// State machine for applying KV operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachine {
    data: HashMap<Vec<u8>, Vec<u8>>,
    last_applied: u64,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            last_applied: 0,
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }

    pub fn last_applied(&self) -> u64 {
        self.last_applied
    }

    pub fn apply(
        &mut self,
        index: u64,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if index <= self.last_applied {
            return Err(format!(
                "Entry already applied: index {} <= last_applied {}",
                index, self.last_applied
            )
            .into());
        }

        let operation = crate::Operation::deserialize(data)?;
        let result = operation.apply(&mut self.data)?;
        self.last_applied = index;
        Ok(result)
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        bincode::serialize(self).map_err(|e| e.into())
    }

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
