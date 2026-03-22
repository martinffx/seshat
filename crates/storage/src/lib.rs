//! Storage implementations for Seshat distributed KV store.
//!
//! This crate provides:
//! - OpenRaft type configuration and core types
//! - Operation definitions for state machine commands
//! - In-memory storage backend (OpenRaftMemStorage)
//! - State machine for applying operations
//! - RocksDB storage backend (future)
//! - Comprehensive error handling for storage operations

// OpenRaft types and configuration
pub mod openraft_mem;
pub mod operations;
pub mod state_machine;
pub mod types;

// RocksDB storage types and error handling
pub mod batch;
pub mod column_family;
pub mod error;
pub mod iterator;
pub mod options;
pub mod storage;

// Re-export commonly used types
pub use openraft_mem::{
    OpenRaftMemLog, OpenRaftMemLogReader, OpenRaftMemSnapshotBuilder, OpenRaftMemStateMachine,
};
pub use operations::{Operation, OperationError, OperationResult};
pub use state_machine::StateMachine;
pub use types::{BasicNode, RaftTypeConfig, Request, Response};

// Re-export RocksDB storage types
pub use batch::WriteBatch;
pub use column_family::ColumnFamily;
pub use error::{Result, StorageError};
pub use iterator::{Direction, IteratorMode, StorageIterator};
pub use options::{CFOptions, StorageOptions};
pub use storage::Storage;

// ============================================================================
// Legacy raft-rs storage (will be removed after OpenRaft migration)
// ============================================================================

// In-memory storage implementation for Raft consensus.
//
// This module provides `MemStorage`, an in-memory implementation suitable for
// testing and development. For production use, a persistent storage backend
// (e.g., RocksDB) should be used instead.
//
// # Protobuf Version Bridging
//
// This module uses `prost_old` (prost 0.11) to maintain compatibility with `raft-rs`,
// which depends on prost 0.11. Our transport layer uses the latest prost 0.14 for
// gRPC communication with tonic 0.14. The bridging happens in the transport layer
// via binary serialization/deserialization.
//
// - `prost_old` (0.11): Used here for raft-rs `eraftpb` types (Entry, HardState, etc.)
// - `prost` (0.14): Used in transport layer for gRPC wire protocol
//
// # Thread Safety
//
// All fields are wrapped in `RwLock` to provide thread-safe concurrent access.
// Multiple readers can access the data simultaneously, but writers have exclusive access.
//
// ## Lock Poisoning Philosophy
//
// This implementation uses `.expect()` instead of `.unwrap()` for lock acquisition
// to provide clear error messages when lock poisoning occurs. Lock poisoning indicates
// that a thread panicked while holding the lock, leaving the data in a potentially
// inconsistent state.
//
// **For Phase 1 (MemStorage)**: Lock poisoning is considered a serious bug that should
// cause the application to panic immediately with a descriptive message. This approach
// is acceptable because:
// 1. MemStorage is used for testing and single-node scenarios
// 2. Lock poisoning indicates a critical bug in the concurrent access logic
// 3. Continuing with poisoned state would lead to data corruption
//
// **For Future Production Storage (RocksDB)**: Lock poisoning should be handled gracefully
// by returning a proper error through the Raft error system, allowing the node to
// potentially recover or fail safely without cascading panics.
//
// The `.expect()` messages clearly identify which lock failed, making debugging easier
// during development and testing.

use prost_old::Message;
use raft::eraftpb::{ConfState, Entry, HardState, Snapshot};
use raft::{RaftState, StorageError as RaftStorageError};
use std::sync::RwLock;

/// In-memory storage for Raft state.
///
/// `MemStorage` stores all Raft consensus state in memory:
/// - `hard_state`: Persistent voting state (term, vote, commit)
/// - `conf_state`: Cluster membership configuration
/// - `entries`: Log entries for replication
/// - `snapshot`: Snapshot data for log compaction
///
/// # Examples
///
/// ```
/// use seshat_storage::MemStorage;
///
/// let storage = MemStorage::new();
/// // Storage is ready to use with default values
/// ```
#[derive(Debug)]
pub struct MemStorage {
    /// Persistent state that must survive crashes.
    ///
    /// Contains the current term, the candidate that received the vote
    /// in the current term, and the highest log entry known to be committed.
    hard_state: RwLock<HardState>,

    /// Current cluster membership configuration.
    ///
    /// Tracks which nodes are voters, learners, and which are being added/removed.
    /// This is part of the replicated state machine configuration.
    conf_state: RwLock<ConfState>,

    /// Append-only log of committed entries.
    ///
    /// Entries are indexed starting from 1. Index 0 is reserved.
    /// Each entry contains the term when it was created, its index,
    /// the type of entry, and the data payload.
    entries: RwLock<Vec<Entry>>,

    /// Most recent snapshot of the state machine.
    ///
    /// Used for log compaction. When the log grows too large,
    /// create a snapshot and truncate old log entries.
    snapshot: RwLock<Snapshot>,
}

impl Default for MemStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemStorage {
    /// Creates a new empty in-memory storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::MemStorage;
    ///
    /// let storage = MemStorage::new();
    /// ```
    pub fn new() -> Self {
        Self {
            hard_state: RwLock::new(HardState::default()),
            conf_state: RwLock::new(ConfState::default()),
            entries: RwLock::new(Vec::new()),
            snapshot: RwLock::new(Snapshot::default()),
        }
    }

    /// Returns the current hard state.
    ///
    /// Hard state contains the current term, vote, and commit index.
    /// This is the persistent state that must survive node crashes.
    pub fn hard_state(&self) -> HardState {
        self.hard_state
            .read()
            .expect("MemStorage: hard_state lock poisoned")
            .clone()
    }

    /// Returns the current configuration state.
    ///
    /// Configuration state tracks cluster membership.
    pub fn conf_state(&self) -> ConfState {
        self.conf_state
            .read()
            .expect("MemStorage: conf_state lock poisoned")
            .clone()
    }

    /// Serializes the hard state to bytes using protobuf.
    ///
    /// # Returns
    /// Serialized hard state as a byte vector, or `None` if the hard state is default/empty.
    pub fn hard_state_bytes(&self) -> Option<Vec<u8>> {
        let hs = self.hard_state();

        // Don't serialize if it's the default (empty) state
        if hs == HardState::default() {
            return None;
        }

        Some(hs.encode_to_vec())
    }

    /// Deserializes hard state from protobuf bytes.
    ///
    /// # Arguments
    /// * `bytes` - Protobuf-encoded hard state
    ///
    /// # Errors
    /// Returns error if deserialization fails.
    pub fn set_hard_state_from_bytes(
        &self,
        bytes: &[u8],
    ) -> std::result::Result<(), prost_old::DecodeError> {
        let hs = HardState::decode(bytes)?;
        self.set_hard_state(hs);
        Ok(())
    }

    /// Sets the hard state.
    ///
    /// # Arguments
    /// * `hs` - New hard state to store
    pub fn set_hard_state(&self, hs: HardState) {
        let mut hard_state = self
            .hard_state
            .write()
            .expect("MemStorage: hard_state lock poisoned");
        *hard_state = hs;
    }

    /// Serializes the configuration state to bytes using protobuf.
    ///
    /// # Returns
    /// Serialized configuration state as a byte vector, or `None` if the conf state is default/empty.
    pub fn conf_state_bytes(&self) -> Option<Vec<u8>> {
        let cs = self.conf_state();

        // Don't serialize if it's the default (empty) state
        if cs == ConfState::default() {
            return None;
        }

        Some(cs.encode_to_vec())
    }

    /// Deserializes configuration state from protobuf bytes.
    ///
    /// # Arguments
    /// * `bytes` - Protobuf-encoded configuration state
    ///
    /// # Errors
    /// Returns error if deserialization fails.
    pub fn set_conf_state_from_bytes(
        &self,
        bytes: &[u8],
    ) -> std::result::Result<(), prost_old::DecodeError> {
        let cs = ConfState::decode(bytes)?;
        self.set_conf_state(cs);
        Ok(())
    }

    /// Sets the configuration state.
    ///
    /// # Arguments
    /// * `cs` - New configuration state to store
    pub fn set_conf_state(&self, cs: ConfState) {
        let mut conf_state = self
            .conf_state
            .write()
            .expect("MemStorage: conf_state lock poisoned");
        *conf_state = cs;
    }

    /// Returns all log entries.
    ///
    /// # Returns
    /// A cloned vector of all entries in the log.
    pub fn entries(&self) -> Vec<Entry> {
        self.entries
            .read()
            .expect("MemStorage: entries lock poisoned")
            .clone()
    }

    /// Appends entries to the log.
    ///
    /// # Arguments
    /// * `ents` - Entries to append
    pub fn append(&self, ents: &[Entry]) {
        let mut entries = self
            .entries
            .write()
            .expect("MemStorage: entries lock poisoned");
        entries.extend_from_slice(ents);
    }

    /// Returns the current snapshot.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .read()
            .expect("MemStorage: snapshot lock poisoned")
            .clone()
    }

    /// Sets the snapshot.
    ///
    /// # Arguments
    /// * `snapshot` - New snapshot to store
    pub fn set_snapshot(&self, snapshot: Snapshot) {
        let mut snap = self
            .snapshot
            .write()
            .expect("MemStorage: snapshot lock poisoned");
        *snap = snapshot;
    }
}

impl raft::Storage for MemStorage {
    fn initial_state(&self) -> raft::Result<RaftState> {
        let hard_state = self
            .hard_state
            .read()
            .expect("MemStorage: hard_state lock poisoned")
            .clone();

        let conf_state = self
            .conf_state
            .read()
            .expect("MemStorage: conf_state lock poisoned")
            .clone();

        Ok(RaftState {
            hard_state,
            conf_state,
        })
    }

    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
        _context: raft::GetEntriesContext,
    ) -> raft::Result<Vec<Entry>> {
        let entries = self
            .entries
            .read()
            .expect("MemStorage: entries lock poisoned");

        if entries.is_empty() {
            return Err(raft::Error::Store(RaftStorageError::Unavailable));
        }

        let offset = entries[0].index;

        if low < offset {
            return Err(raft::Error::Store(RaftStorageError::Compacted));
        }

        if high > entries.last().unwrap().index + 1 {
            return Err(raft::Error::Store(RaftStorageError::Unavailable));
        }

        let lo = (low - offset) as usize;
        let hi = (high - offset) as usize;

        let mut result = entries[lo..hi].to_vec();

        if let Some(max) = max_size.into() {
            let mut size = 0u64;
            result.retain(|e| {
                size += e.encoded_len() as u64;
                size <= max
            });
        }

        Ok(result)
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        let entries = self
            .entries
            .read()
            .expect("MemStorage: entries lock poisoned");

        if entries.is_empty() {
            return Ok(0);
        }

        let offset = entries[0].index;

        if idx < offset {
            return Err(raft::Error::Store(RaftStorageError::Compacted));
        }

        if idx > entries.last().unwrap().index {
            return Err(raft::Error::Store(RaftStorageError::Unavailable));
        }

        let pos = (idx - offset) as usize;
        Ok(entries[pos].term)
    }

    fn first_index(&self) -> raft::Result<u64> {
        let entries = self
            .entries
            .read()
            .expect("MemStorage: entries lock poisoned");

        if let Some(first) = entries.first() {
            Ok(first.index)
        } else {
            // If no entries, check snapshot
            let snapshot = self
                .snapshot
                .read()
                .expect("MemStorage: snapshot lock poisoned");
            Ok(snapshot.get_metadata().index + 1)
        }
    }

    fn last_index(&self) -> raft::Result<u64> {
        let entries = self
            .entries
            .read()
            .expect("MemStorage: entries lock poisoned");

        if let Some(last) = entries.last() {
            Ok(last.index)
        } else {
            // If no entries, return snapshot index
            let snapshot = self
                .snapshot
                .read()
                .expect("MemStorage: snapshot lock poisoned");
            Ok(snapshot.get_metadata().index)
        }
    }

    fn snapshot(&self, _request_index: u64, _to: u64) -> raft::Result<Snapshot> {
        Ok(self
            .snapshot
            .read()
            .expect("MemStorage: snapshot lock poisoned")
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_storage_is_empty() {
        let storage = MemStorage::new();
        assert_eq!(storage.hard_state(), HardState::default());
        assert_eq!(storage.conf_state(), ConfState::default());
        assert_eq!(storage.entries().len(), 0);
    }

    #[test]
    fn test_hard_state_serialization() {
        let storage = MemStorage::new();

        let hs = HardState {
            term: 5,
            vote: 1,
            commit: 10,
        };

        storage.set_hard_state(hs.clone());

        let bytes = storage.hard_state_bytes().unwrap();
        assert!(!bytes.is_empty());

        let storage2 = MemStorage::new();
        storage2.set_hard_state_from_bytes(&bytes).unwrap();

        assert_eq!(storage2.hard_state(), hs);
    }

    #[test]
    fn test_conf_state_serialization() {
        let storage = MemStorage::new();

        let cs = ConfState {
            voters: vec![1, 2, 3],
            ..Default::default()
        };

        storage.set_conf_state(cs.clone());

        let bytes = storage.conf_state_bytes().unwrap();
        assert!(!bytes.is_empty());

        let storage2 = MemStorage::new();
        storage2.set_conf_state_from_bytes(&bytes).unwrap();

        assert_eq!(storage2.conf_state(), cs);
    }
}
