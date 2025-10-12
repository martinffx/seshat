//! In-memory storage implementation for Raft consensus.
//!
//! This module provides `MemStorage`, an in-memory implementation suitable for
//! testing and development. For production use, a persistent storage backend
//! (e.g., RocksDB) should be used instead.
//!
//! # Thread Safety
//!
//! All fields are wrapped in `RwLock` to provide thread-safe concurrent access.
//! Multiple readers can access the data simultaneously, but writers have exclusive access.

use raft::eraftpb::{ConfState, Entry, HardState, Snapshot};
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
/// use seshat_raft::MemStorage;
///
/// let storage = MemStorage::new();
/// // Storage is ready to use with default values
/// ```
#[derive(Debug)]
#[allow(dead_code)] // Fields will be used when Storage trait is implemented
pub struct MemStorage {
    /// Persistent state that must survive crashes.
    ///
    /// Contains the current term, the candidate that received the vote
    /// in the current term, and the highest log entry known to be committed.
    hard_state: RwLock<HardState>,

    /// Current cluster membership configuration.
    ///
    /// Tracks which nodes are voters, learners, and which nodes are
    /// being added or removed from the cluster.
    conf_state: RwLock<ConfState>,

    /// Log entries for state machine replication.
    ///
    /// Entries are indexed starting at 1. The vector may not start at index 1
    /// after log compaction (snapshot creation).
    entries: RwLock<Vec<Entry>>,

    /// Current snapshot for log compaction.
    ///
    /// Represents the state machine state at a particular point in time,
    /// allowing truncation of old log entries.
    snapshot: RwLock<Snapshot>,
}

impl MemStorage {
    /// Creates a new `MemStorage` with default values.
    ///
    /// All fields are initialized to their default states:
    /// - Empty hard state (term=0, vote=0, commit=0)
    /// - Empty configuration state
    /// - Empty log entries
    /// - Empty snapshot
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    ///
    /// let storage = MemStorage::new();
    /// // Storage is now ready to use
    /// ```
    pub fn new() -> Self {
        Self {
            hard_state: RwLock::new(HardState::default()),
            conf_state: RwLock::new(ConfState::default()),
            entries: RwLock::new(Vec::new()),
            snapshot: RwLock::new(Snapshot::default()),
        }
    }
}

impl Default for MemStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_storage_new_creates_successfully() {
        let storage = MemStorage::new();

        // Verify storage was created without panicking
        // We can't directly access the fields since they're private,
        // but we can verify the storage exists
        let _debug_output = format!("{storage:?}");
    }

    #[test]
    fn test_mem_storage_default_creates_successfully() {
        let storage = MemStorage::default();

        // Verify default() works the same as new()
        let _debug_output = format!("{storage:?}");
    }

    #[test]
    fn test_mem_storage_has_default_hard_state() {
        let storage = MemStorage::new();

        // Access hard_state to verify it's initialized
        let hard_state = storage.hard_state.read().unwrap();
        assert_eq!(hard_state.term, 0, "Initial term should be 0");
        assert_eq!(hard_state.vote, 0, "Initial vote should be 0");
        assert_eq!(hard_state.commit, 0, "Initial commit should be 0");
    }

    #[test]
    fn test_mem_storage_has_default_conf_state() {
        let storage = MemStorage::new();

        // Access conf_state to verify it's initialized
        let conf_state = storage.conf_state.read().unwrap();
        assert!(
            conf_state.voters.is_empty(),
            "Initial voters should be empty"
        );
        assert!(
            conf_state.learners.is_empty(),
            "Initial learners should be empty"
        );
    }

    #[test]
    fn test_mem_storage_has_empty_entries() {
        let storage = MemStorage::new();

        // Access entries to verify it's an empty vector
        let entries = storage.entries.read().unwrap();
        assert!(entries.is_empty(), "Initial entries should be empty");
        assert_eq!(entries.len(), 0, "Initial entries length should be 0");
    }

    #[test]
    fn test_mem_storage_has_default_snapshot() {
        let storage = MemStorage::new();

        // Access snapshot to verify it's initialized
        let snapshot = storage.snapshot.read().unwrap();
        assert!(
            snapshot.data.is_empty(),
            "Initial snapshot data should be empty"
        );
    }

    #[test]
    fn test_mem_storage_fields_are_thread_safe() {
        let storage = MemStorage::new();

        // Verify we can get read locks on all fields
        let _hard_state = storage.hard_state.read().unwrap();
        let _conf_state = storage.conf_state.read().unwrap();
        let _entries = storage.entries.read().unwrap();
        let _snapshot = storage.snapshot.read().unwrap();

        // All locks should be released when the guards go out of scope
    }

    #[test]
    fn test_mem_storage_multiple_readers() {
        let storage = MemStorage::new();

        // Verify multiple readers can access simultaneously
        let _lock1 = storage.hard_state.read().unwrap();
        let _lock2 = storage.hard_state.read().unwrap();
        let _lock3 = storage.hard_state.read().unwrap();

        // All read locks should coexist
    }

    #[test]
    fn test_mem_storage_write_lock() {
        let storage = MemStorage::new();

        // Verify we can get write locks
        {
            let mut hard_state = storage.hard_state.write().unwrap();
            hard_state.term = 1;
        }

        // Verify the write persisted
        let hard_state = storage.hard_state.read().unwrap();
        assert_eq!(hard_state.term, 1);
    }

    #[test]
    fn test_mem_storage_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MemStorage>();
    }

    #[test]
    fn test_mem_storage_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<MemStorage>();
    }

    #[test]
    fn test_mem_storage_can_be_used_across_threads() {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemStorage::new());
        let storage_clone = Arc::clone(&storage);

        let handle = thread::spawn(move || {
            let hard_state = storage_clone.hard_state.read().unwrap();
            assert_eq!(hard_state.term, 0);
        });

        handle.join().unwrap();
    }

    #[test]
    fn test_mem_storage_independent_instances() {
        let storage1 = MemStorage::new();
        let storage2 = MemStorage::new();

        // Modify storage1
        {
            let mut hard_state = storage1.hard_state.write().unwrap();
            hard_state.term = 5;
        }

        // Verify storage2 is unaffected
        let hard_state2 = storage2.hard_state.read().unwrap();
        assert_eq!(hard_state2.term, 0);
    }
}
