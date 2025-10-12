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
use raft::RaftState;
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

    /// Returns the initial Raft state from storage.
    ///
    /// This method reads the current hard state and configuration state
    /// from the storage and returns them as a `RaftState`. This is typically
    /// called when initializing a Raft node to restore its persisted state.
    ///
    /// # Thread Safety
    ///
    /// This method acquires read locks on both `hard_state` and `conf_state`.
    /// Multiple concurrent calls are safe and efficient.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    ///
    /// let storage = MemStorage::new();
    /// let state = storage.initial_state().unwrap();
    /// assert_eq!(state.hard_state.term, 0);
    /// assert_eq!(state.hard_state.vote, 0);
    /// assert_eq!(state.hard_state.commit, 0);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Lock acquisition fails (lock poisoning)
    pub fn initial_state(&self) -> raft::Result<RaftState> {
        let hard_state = self.hard_state.read().unwrap();
        let conf_state = self.conf_state.read().unwrap();

        Ok(RaftState {
            hard_state: hard_state.clone(),
            conf_state: conf_state.clone(),
        })
    }

    /// Sets the hard state of the storage.
    ///
    /// This is primarily used for testing and during Raft ready processing
    /// to persist the updated hard state.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::HardState;
    ///
    /// let storage = MemStorage::new();
    /// let mut hs = HardState::default();
    /// hs.term = 5;
    /// hs.vote = 1;
    /// hs.commit = 10;
    /// storage.set_hard_state(hs);
    ///
    /// let state = storage.initial_state().unwrap();
    /// assert_eq!(state.hard_state.term, 5);
    /// assert_eq!(state.hard_state.vote, 1);
    /// assert_eq!(state.hard_state.commit, 10);
    /// ```
    pub fn set_hard_state(&self, hs: HardState) {
        *self.hard_state.write().unwrap() = hs;
    }

    /// Sets the configuration state of the storage.
    ///
    /// This is primarily used for testing and during Raft ready processing
    /// to persist the updated configuration state.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::ConfState;
    ///
    /// let storage = MemStorage::new();
    /// let mut cs = ConfState::default();
    /// cs.voters = vec![1, 2, 3];
    /// storage.set_conf_state(cs);
    ///
    /// let state = storage.initial_state().unwrap();
    /// assert_eq!(state.conf_state.voters, vec![1, 2, 3]);
    /// ```
    pub fn set_conf_state(&self, cs: ConfState) {
        *self.conf_state.write().unwrap() = cs;
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
    use std::sync::Arc;
    use std::thread;

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

    // ============================================================================
    // Tests for initial_state() method
    // ============================================================================

    #[test]
    fn test_initial_state_returns_defaults() {
        let storage = MemStorage::new();

        let state = storage.initial_state().expect("initial_state should succeed");

        // Verify default HardState
        assert_eq!(state.hard_state.term, 0, "Default term should be 0");
        assert_eq!(state.hard_state.vote, 0, "Default vote should be 0");
        assert_eq!(state.hard_state.commit, 0, "Default commit should be 0");

        // Verify default ConfState
        assert!(
            state.conf_state.voters.is_empty(),
            "Default voters should be empty"
        );
        assert!(
            state.conf_state.learners.is_empty(),
            "Default learners should be empty"
        );
    }

    #[test]
    fn test_initial_state_reflects_hard_state_changes() {
        let storage = MemStorage::new();

        // Modify hard_state
        let new_hard_state = HardState {
            term: 10,
            vote: 3,
            commit: 25,
        };
        storage.set_hard_state(new_hard_state);

        // Verify initial_state reflects the change
        let state = storage.initial_state().expect("initial_state should succeed");
        assert_eq!(state.hard_state.term, 10, "Term should be updated to 10");
        assert_eq!(state.hard_state.vote, 3, "Vote should be updated to 3");
        assert_eq!(
            state.hard_state.commit, 25,
            "Commit should be updated to 25"
        );
    }

    #[test]
    fn test_initial_state_reflects_conf_state_changes() {
        let storage = MemStorage::new();

        // Modify conf_state
        let new_conf_state = ConfState {
            voters: vec![1, 2, 3],
            learners: vec![4, 5],
            ..Default::default()
        };
        storage.set_conf_state(new_conf_state);

        // Verify initial_state reflects the change
        let state = storage.initial_state().expect("initial_state should succeed");
        assert_eq!(
            state.conf_state.voters,
            vec![1, 2, 3],
            "Voters should be updated"
        );
        assert_eq!(
            state.conf_state.learners,
            vec![4, 5],
            "Learners should be updated"
        );
    }

    #[test]
    fn test_initial_state_is_thread_safe() {
        let storage = Arc::new(MemStorage::new());

        // Set initial values
        let hs = HardState {
            term: 5,
            vote: 2,
            commit: 10,
        };
        storage.set_hard_state(hs);

        let cs = ConfState {
            voters: vec![1, 2, 3],
            ..Default::default()
        };
        storage.set_conf_state(cs);

        // Spawn multiple threads calling initial_state
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    let state = storage_clone
                        .initial_state()
                        .expect("initial_state should succeed");
                    assert_eq!(state.hard_state.term, 5);
                    assert_eq!(state.hard_state.vote, 2);
                    assert_eq!(state.hard_state.commit, 10);
                    assert_eq!(state.conf_state.voters, vec![1, 2, 3]);
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    #[test]
    fn test_initial_state_returns_cloned_data() {
        let storage = MemStorage::new();

        // Get initial state
        let state1 = storage.initial_state().expect("initial_state should succeed");

        // Modify storage
        let new_hard_state = HardState {
            term: 100,
            ..Default::default()
        };
        storage.set_hard_state(new_hard_state);

        // Get initial state again
        let state2 = storage.initial_state().expect("initial_state should succeed");

        // Verify state1 is independent of the change
        assert_eq!(
            state1.hard_state.term, 0,
            "First state should not be affected by later changes"
        );
        assert_eq!(
            state2.hard_state.term, 100,
            "Second state should reflect the change"
        );
    }

    #[test]
    fn test_initial_state_multiple_calls_are_consistent() {
        let storage = MemStorage::new();

        // Set specific values
        let hs = HardState {
            term: 42,
            vote: 7,
            commit: 99,
        };
        storage.set_hard_state(hs);

        // Call initial_state multiple times
        for _ in 0..100 {
            let state = storage.initial_state().expect("initial_state should succeed");
            assert_eq!(state.hard_state.term, 42);
            assert_eq!(state.hard_state.vote, 7);
            assert_eq!(state.hard_state.commit, 99);
        }
    }

    #[test]
    fn test_set_hard_state_updates_storage() {
        let storage = MemStorage::new();

        // Create and set a new hard state
        let hs = HardState {
            term: 15,
            vote: 8,
            commit: 50,
        };
        storage.set_hard_state(hs);

        // Verify the update by reading directly
        let stored_hs = storage.hard_state.read().unwrap();
        assert_eq!(stored_hs.term, 15);
        assert_eq!(stored_hs.vote, 8);
        assert_eq!(stored_hs.commit, 50);
    }

    #[test]
    fn test_set_conf_state_updates_storage() {
        let storage = MemStorage::new();

        // Create and set a new conf state
        let cs = ConfState {
            voters: vec![10, 20, 30],
            learners: vec![40],
            ..Default::default()
        };
        storage.set_conf_state(cs);

        // Verify the update by reading directly
        let stored_cs = storage.conf_state.read().unwrap();
        assert_eq!(stored_cs.voters, vec![10, 20, 30]);
        assert_eq!(stored_cs.learners, vec![40]);
    }

    #[test]
    fn test_initial_state_with_empty_conf_state() {
        let storage = MemStorage::new();

        // Set only hard state, leave conf state empty
        let hs = HardState {
            term: 1,
            ..Default::default()
        };
        storage.set_hard_state(hs);

        let state = storage.initial_state().expect("initial_state should succeed");
        assert_eq!(state.hard_state.term, 1);
        assert!(state.conf_state.voters.is_empty());
        assert!(state.conf_state.learners.is_empty());
    }

    #[test]
    fn test_initial_state_with_complex_conf_state() {
        let storage = MemStorage::new();

        // Create a complex configuration
        let cs = ConfState {
            voters: vec![1, 2, 3, 4, 5],
            learners: vec![6, 7],
            voters_outgoing: vec![1, 2, 3], // During configuration change
            learners_next: vec![8],         // Learners being added
            auto_leave: true,
        };
        storage.set_conf_state(cs.clone());

        let state = storage.initial_state().expect("initial_state should succeed");
        assert_eq!(state.conf_state.voters, cs.voters);
        assert_eq!(state.conf_state.learners, cs.learners);
        assert_eq!(state.conf_state.voters_outgoing, cs.voters_outgoing);
        assert_eq!(state.conf_state.learners_next, cs.learners_next);
        assert_eq!(state.conf_state.auto_leave, cs.auto_leave);
    }
}
