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
//!
//! ## Lock Poisoning Philosophy
//!
//! This implementation uses `.expect()` instead of `.unwrap()` for lock acquisition
//! to provide clear error messages when lock poisoning occurs. Lock poisoning indicates
//! that a thread panicked while holding the lock, leaving the data in a potentially
//! inconsistent state.
//!
//! **For Phase 1 (MemStorage)**: Lock poisoning is considered a serious bug that should
//! cause the application to panic immediately with a descriptive message. This approach
//! is acceptable because:
//! 1. MemStorage is used for testing and single-node scenarios
//! 2. Lock poisoning indicates a critical bug in the concurrent access logic
//! 3. Continuing with poisoned state would lead to data corruption
//!
//! **For Future Production Storage (RocksDB)**: Lock poisoning should be handled gracefully
//! by returning a proper error through the Raft error system, allowing the node to
//! potentially recover or fail safely without cascading panics.
//!
//! The `.expect()` messages clearly identify which lock failed, making debugging easier
//! during development and testing.

use prost_old::Message;
use raft::eraftpb::{ConfState, Entry, HardState, Snapshot};
use raft::{RaftState, StorageError};
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
        let hard_state = self
            .hard_state
            .read()
            .expect("Hard state lock poisoned - indicates bug in concurrent access");
        let conf_state = self
            .conf_state
            .read()
            .expect("Conf state lock poisoned - indicates bug in concurrent access");

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
        *self
            .hard_state
            .write()
            .expect("Hard state lock poisoned - indicates bug in concurrent access") = hs;
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
        *self
            .conf_state
            .write()
            .expect("Conf state lock poisoned - indicates bug in concurrent access") = cs;
    }

    /// Returns a range of log entries.
    ///
    /// Returns log entries in the range `[low, high)`, limiting the total size
    /// to `max_size` bytes if specified.
    ///
    /// # Arguments
    ///
    /// * `low` - The inclusive lower bound of the range (first index to return)
    /// * `high` - The exclusive upper bound of the range (one past the last index)
    /// * `max_size` - Optional maximum total size in bytes of returned entries
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(Vec<Entry>)` - The requested entries (may be empty if low == high)
    /// - `Err(StorageError::Compacted)` - If `low` is less than `first_index()`
    /// - `Err(StorageError::Unavailable)` - If `high` is greater than `last_index() + 1`
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::Entry;
    ///
    /// let storage = MemStorage::new();
    /// // With empty storage, requesting any range returns empty or error
    /// let result = storage.entries(1, 1, None);
    /// assert!(result.is_ok());
    /// assert_eq!(result.unwrap().len(), 0);
    /// ```
    pub fn entries(&self, low: u64, high: u64, max_size: Option<u64>) -> raft::Result<Vec<Entry>> {
        // Handle empty range first
        if low >= high {
            return Ok(Vec::new());
        }

        // Acquire all locks once for consistent state (fixes TOCTOU race)
        let snapshot = self
            .snapshot
            .read()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");
        let entries = self
            .entries
            .read()
            .expect("Entries lock poisoned - indicates bug in concurrent access");

        // Calculate first and last indices from locked state
        let first = if snapshot.get_metadata().index > 0 {
            snapshot.get_metadata().index + 1
        } else if !entries.is_empty() {
            entries[0].index
        } else {
            1
        };

        let last = if let Some(last_entry) = entries.last() {
            last_entry.index
        } else {
            snapshot.get_metadata().index
        };

        // Check if low is before first available entry (compacted)
        if low < first {
            return Err(raft::Error::Store(StorageError::Compacted));
        }

        // Check if high is beyond available entries
        // Note: high can be last_index + 1 (to request all entries up to and including last_index)
        if high > last + 1 {
            return Err(raft::Error::Store(StorageError::Unavailable));
        }

        // Handle empty log
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate slice bounds
        // entries vector may not start at index 1 after compaction
        let offset = entries[0].index;

        // Convert logical indices to vector indices
        let start_idx = (low.saturating_sub(offset)) as usize;
        let end_idx = (high.saturating_sub(offset)) as usize;

        // Ensure we don't go out of bounds
        let start_idx = start_idx.min(entries.len());
        let end_idx = end_idx.min(entries.len());

        // If start >= end, return empty
        if start_idx >= end_idx {
            return Ok(Vec::new());
        }

        // Get the slice
        let mut result = Vec::new();
        let mut total_size: u64 = 0;

        for entry in &entries[start_idx..end_idx] {
            // Calculate entry size using prost's encoded_len
            let entry_size = entry.encoded_len() as u64;

            // If we have a size limit and we've already added at least one entry
            // and adding this entry would exceed the limit, stop
            if let Some(max) = max_size {
                if !result.is_empty() && total_size + entry_size > max {
                    break;
                }
            }

            result.push(entry.clone());
            total_size += entry_size;
        }

        // Always return at least one entry if any are available
        // (even if it exceeds max_size)
        if result.is_empty() && start_idx < end_idx {
            result.push(entries[start_idx].clone());
        }

        Ok(result)
    }

    /// Returns the term of the entry at the given index.
    ///
    /// # Arguments
    ///
    /// * `index` - The log index to query
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(term)` - The term of the entry at the given index
    /// - `Err(StorageError::Compacted)` - If the index has been compacted
    /// - `Err(StorageError::Unavailable)` - If the index is not yet available
    ///
    /// # Special Cases
    ///
    /// - `term(0)` always returns `0` (by Raft convention)
    /// - If `index == snapshot.metadata.index`, returns `snapshot.metadata.term`
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::Entry;
    ///
    /// let storage = MemStorage::new();
    ///
    /// // Index 0 always returns term 0
    /// assert_eq!(storage.term(0).unwrap(), 0);
    ///
    /// // Add entries and query their terms
    /// let entries = vec![
    ///     Entry { index: 1, term: 1, ..Default::default() },
    ///     Entry { index: 2, term: 2, ..Default::default() },
    /// ];
    /// storage.append(&entries);
    /// assert_eq!(storage.term(1).unwrap(), 1);
    /// assert_eq!(storage.term(2).unwrap(), 2);
    /// ```
    pub fn term(&self, index: u64) -> raft::Result<u64> {
        // Special case: index 0 always has term 0
        if index == 0 {
            return Ok(0);
        }

        // Acquire locks once for consistent state
        let snapshot = self
            .snapshot
            .read()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");
        let entries = self
            .entries
            .read()
            .expect("Entries lock poisoned - indicates bug in concurrent access");

        // Calculate bounds from locked state
        let first = if snapshot.get_metadata().index > 0 {
            snapshot.get_metadata().index + 1
        } else if !entries.is_empty() {
            entries[0].index
        } else {
            1
        };

        let last = if let Some(last_entry) = entries.last() {
            last_entry.index
        } else {
            snapshot.get_metadata().index
        };

        // Check if this is exactly the snapshot index
        if index == snapshot.get_metadata().index {
            return Ok(snapshot.get_metadata().term);
        }

        // Check if index is before first available entry (compacted)
        if index < first {
            return Err(raft::Error::Store(StorageError::Compacted));
        }

        // Check if index is beyond available entries
        if index > last {
            return Err(raft::Error::Store(StorageError::Unavailable));
        }

        // Handle empty log (shouldn't happen given bounds checks, but be safe)
        if entries.is_empty() {
            return Err(raft::Error::Store(StorageError::Unavailable));
        }

        // Calculate offset
        let offset = entries[0].index;
        let vec_index = (index - offset) as usize;

        // Bounds check
        if vec_index >= entries.len() {
            return Err(raft::Error::Store(StorageError::Unavailable));
        }

        Ok(entries[vec_index].term)
    }

    /// Returns the first index in the log.
    ///
    /// This is the index of the first entry available in the log. After log compaction,
    /// this may be greater than 1 (the first entry that was ever appended).
    ///
    /// # Returns
    ///
    /// - If there's a snapshot, returns `snapshot.metadata.index + 1`
    /// - Otherwise, returns 1 (the default first index)
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    ///
    /// let storage = MemStorage::new();
    /// assert_eq!(storage.first_index().unwrap(), 1);
    /// ```
    pub fn first_index(&self) -> raft::Result<u64> {
        let snapshot = self
            .snapshot
            .read()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");
        let entries = self
            .entries
            .read()
            .expect("Entries lock poisoned - indicates bug in concurrent access");

        if snapshot.get_metadata().index > 0 {
            Ok(snapshot.get_metadata().index + 1)
        } else if !entries.is_empty() {
            Ok(entries[0].index)
        } else {
            Ok(1)
        }
    }

    /// Returns the last index in the log.
    ///
    /// This is the index of the last entry available in the log.
    ///
    /// # Returns
    ///
    /// - If there are entries, returns the index of the last entry
    /// - If there's a snapshot but no entries, returns the snapshot index
    /// - Otherwise, returns 0 (empty log)
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    ///
    /// let storage = MemStorage::new();
    /// assert_eq!(storage.last_index().unwrap(), 0);
    /// ```
    pub fn last_index(&self) -> raft::Result<u64> {
        let entries = self
            .entries
            .read()
            .expect("Entries lock poisoned - indicates bug in concurrent access");
        let snapshot = self
            .snapshot
            .read()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");

        if let Some(last) = entries.last() {
            Ok(last.index)
        } else {
            Ok(snapshot.get_metadata().index)
        }
    }

    /// Returns the current snapshot.
    ///
    /// In Phase 1, this is simplified to always return the stored snapshot
    /// regardless of the `request_index` parameter. In later phases, this
    /// would check if the snapshot is ready for the given index.
    ///
    /// # Arguments
    ///
    /// * `request_index` - The index for which a snapshot is requested (unused in Phase 1)
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(Snapshot)` - A clone of the current snapshot
    ///
    /// # Phase 1 Simplification
    ///
    /// This implementation ignores `request_index` and always returns the current
    /// snapshot. Future phases may return `StorageError::SnapshotTemporarilyUnavailable`
    /// if a snapshot is being created for a specific index.
    ///
    /// # Thread Safety
    ///
    /// This method acquires a read lock on the snapshot field. Multiple concurrent
    /// calls are safe and efficient.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::Snapshot;
    ///
    /// let storage = MemStorage::new();
    ///
    /// // Empty storage returns default snapshot
    /// let snapshot = storage.snapshot(0).unwrap();
    /// assert_eq!(snapshot.get_metadata().index, 0);
    /// assert_eq!(snapshot.get_metadata().term, 0);
    /// assert!(snapshot.data.is_empty());
    /// ```
    pub fn snapshot(&self, _request_index: u64) -> raft::Result<Snapshot> {
        // Phase 1: Simplified implementation
        // Just return the current snapshot, ignoring request_index
        let snapshot = self
            .snapshot
            .read()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");
        Ok(snapshot.clone())
    }

    /// Appends entries to the log.
    ///
    /// This is a helper method for testing. In production use, entries are
    /// typically appended through the Raft ready processing.
    ///
    /// # Arguments
    ///
    /// * `ents` - Slice of entries to append
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::Entry;
    ///
    /// let storage = MemStorage::new();
    /// let entries = vec![
    ///     Entry { index: 1, term: 1, ..Default::default() },
    ///     Entry { index: 2, term: 1, ..Default::default() },
    /// ];
    /// storage.append(&entries);
    /// ```
    pub fn append(&self, ents: &[Entry]) {
        let mut entries = self
            .entries
            .write()
            .expect("Entries lock poisoned - indicates bug in concurrent access");
        entries.extend_from_slice(ents);
    }

    /// Applies a snapshot to the storage.
    ///
    /// This method replaces the entire storage state with the given snapshot.
    /// All log entries covered by the snapshot (entries with index <= snapshot.metadata.index)
    /// are removed. The hard state and configuration state are updated from the snapshot metadata.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The snapshot to apply
    ///
    /// # Thread Safety
    ///
    /// This method acquires write locks on all storage fields. It is safe to call
    /// concurrently with other methods, but write operations are serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::{Snapshot, ConfState};
    ///
    /// let storage = MemStorage::new();
    ///
    /// // Create a snapshot
    /// let mut snapshot = Snapshot::default();
    /// snapshot.mut_metadata().index = 10;
    /// snapshot.mut_metadata().term = 3;
    /// snapshot.mut_metadata().conf_state = Some(ConfState {
    ///     voters: vec![1, 2, 3],
    ///     ..Default::default()
    /// });
    /// snapshot.data = vec![1, 2, 3, 4, 5];
    ///
    /// // Apply snapshot
    /// storage.apply_snapshot(snapshot.clone()).unwrap();
    ///
    /// // Verify snapshot was applied
    /// let retrieved = storage.snapshot(0).unwrap();
    /// assert_eq!(retrieved.get_metadata().index, 10);
    /// assert_eq!(retrieved.get_metadata().term, 3);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Lock acquisition fails (lock poisoning)
    pub fn apply_snapshot(&self, snapshot: Snapshot) -> raft::Result<()> {
        // Get snapshot index and term for updating hard_state
        let snap_index = snapshot.get_metadata().index;
        let snap_term = snapshot.get_metadata().term;

        // Acquire write locks in consistent order to prevent deadlocks
        // Lock ordering: snapshot → entries → hard_state → conf_state (documented to prevent deadlocks)
        let mut storage_snapshot = self
            .snapshot
            .write()
            .expect("Snapshot lock poisoned - indicates bug in concurrent access");
        let mut entries = self
            .entries
            .write()
            .expect("Entries lock poisoned - indicates bug in concurrent access");
        let mut hard_state = self
            .hard_state
            .write()
            .expect("Hard state lock poisoned - indicates bug in concurrent access");
        let mut conf_state = self
            .conf_state
            .write()
            .expect("Conf state lock poisoned - indicates bug in concurrent access");

        // Replace snapshot
        *storage_snapshot = snapshot.clone();

        // Remove entries covered by the snapshot
        // Keep only entries with index > snapshot.metadata.index
        entries.retain(|entry| entry.index > snap_index);

        // Update hard_state commit to at least snapshot index
        if hard_state.commit < snap_index {
            hard_state.commit = snap_index;
        }
        // Update term if snapshot term is higher
        if hard_state.term < snap_term {
            hard_state.term = snap_term;
        }

        // Update conf_state from snapshot metadata
        if let Some(ref cs) = snapshot.get_metadata().conf_state {
            *conf_state = cs.clone();
        }

        Ok(())
    }

    /// Appends entries to the log with proper conflict resolution.
    ///
    /// This method implements the Raft log append logic with truncation of conflicting
    /// entries. If an incoming entry has the same index as an existing entry but a
    /// different term, all entries from that point onwards are removed before appending
    /// the new entries.
    ///
    /// # Arguments
    ///
    /// * `entries` - Slice of entries to append
    ///
    /// # Thread Safety
    ///
    /// This method acquires a write lock on the entries field. Multiple concurrent
    /// calls are serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_raft::MemStorage;
    /// use raft::eraftpb::Entry;
    ///
    /// let storage = MemStorage::new();
    ///
    /// // Append initial entries
    /// let entries1 = vec![
    ///     Entry { index: 1, term: 1, ..Default::default() },
    ///     Entry { index: 2, term: 1, ..Default::default() },
    ///     Entry { index: 3, term: 1, ..Default::default() },
    /// ];
    /// storage.wl_append_entries(&entries1).unwrap();
    /// assert_eq!(storage.last_index().unwrap(), 3);
    ///
    /// // Append conflicting entries (will truncate from index 2)
    /// let entries2 = vec![
    ///     Entry { index: 2, term: 2, ..Default::default() },
    ///     Entry { index: 3, term: 2, ..Default::default() },
    /// ];
    /// storage.wl_append_entries(&entries2).unwrap();
    /// assert_eq!(storage.last_index().unwrap(), 3);
    /// assert_eq!(storage.term(2).unwrap(), 2);
    /// assert_eq!(storage.term(3).unwrap(), 2);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Lock acquisition fails (lock poisoning)
    pub fn wl_append_entries(&self, entries: &[Entry]) -> raft::Result<()> {
        // Empty entries slice is valid - just return
        if entries.is_empty() {
            return Ok(());
        }

        // Acquire write lock on entries
        let mut storage_entries = self
            .entries
            .write()
            .expect("Entries lock poisoned - indicates bug in concurrent access");

        // If storage is empty, just append all entries
        if storage_entries.is_empty() {
            storage_entries.extend_from_slice(entries);
            return Ok(());
        }

        // Find the first conflicting entry
        let first_new_index = entries[0].index;
        let storage_offset = storage_entries[0].index;

        // If new entries start after our log, just append
        // Note: storage_entries is guaranteed non-empty by check above
        if first_new_index
            > storage_entries
                .last()
                .expect("Storage entries non-empty - checked above")
                .index
        {
            storage_entries.extend_from_slice(entries);
            return Ok(());
        }

        // If new entries start before our log, we need to handle overlap
        if first_new_index < storage_offset {
            // New entries start before our log - this shouldn't happen normally
            // but we'll handle it by clearing everything and appending
            storage_entries.clear();
            storage_entries.extend_from_slice(entries);
            return Ok(());
        }

        // Find conflict point
        for (i, entry) in entries.iter().enumerate() {
            let storage_idx = (entry.index - storage_offset) as usize;

            // If this entry is beyond our current log, append remaining entries
            if storage_idx >= storage_entries.len() {
                storage_entries.extend_from_slice(&entries[i..]);
                return Ok(());
            }

            // Check for conflict
            if storage_entries[storage_idx].term != entry.term {
                // Found conflict - truncate from this point and append new entries
                storage_entries.truncate(storage_idx);
                storage_entries.extend_from_slice(&entries[i..]);
                return Ok(());
            }

            // Terms match - this entry is already in the log, continue checking
        }

        Ok(())
    }
}

impl Default for MemStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl raft::Storage for MemStorage {
    fn initial_state(&self) -> raft::Result<RaftState> {
        self.initial_state()
    }

    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
        _context: raft::GetEntriesContext,
    ) -> raft::Result<Vec<Entry>> {
        self.entries(low, high, max_size.into())
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        self.term(idx)
    }

    fn first_index(&self) -> raft::Result<u64> {
        self.first_index()
    }

    fn last_index(&self) -> raft::Result<u64> {
        self.last_index()
    }

    fn snapshot(&self, request_index: u64, _to: u64) -> raft::Result<Snapshot> {
        self.snapshot(request_index)
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

        let state = storage
            .initial_state()
            .expect("initial_state should succeed");

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
        let state = storage
            .initial_state()
            .expect("initial_state should succeed");
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
        let state = storage
            .initial_state()
            .expect("initial_state should succeed");
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
        let state1 = storage
            .initial_state()
            .expect("initial_state should succeed");

        // Modify storage
        let new_hard_state = HardState {
            term: 100,
            ..Default::default()
        };
        storage.set_hard_state(new_hard_state);

        // Get initial state again
        let state2 = storage
            .initial_state()
            .expect("initial_state should succeed");

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
            let state = storage
                .initial_state()
                .expect("initial_state should succeed");
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

        let state = storage
            .initial_state()
            .expect("initial_state should succeed");
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

        let state = storage
            .initial_state()
            .expect("initial_state should succeed");
        assert_eq!(state.conf_state.voters, cs.voters);
        assert_eq!(state.conf_state.learners, cs.learners);
        assert_eq!(state.conf_state.voters_outgoing, cs.voters_outgoing);
        assert_eq!(state.conf_state.learners_next, cs.learners_next);
        assert_eq!(state.conf_state.auto_leave, cs.auto_leave);
    }

    // ============================================================================
    // Tests for entries() method
    // ============================================================================

    #[test]
    fn test_entries_empty_range_returns_empty_vec() {
        let storage = MemStorage::new();

        // Query with low == high should return empty vector
        let result = storage.entries(1, 1, None);
        assert!(result.is_ok(), "Empty range should succeed");
        assert_eq!(
            result.unwrap().len(),
            0,
            "Empty range should return no entries"
        );
    }

    #[test]
    fn test_entries_empty_range_on_populated_storage() {
        let storage = MemStorage::new();

        // Add some entries
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 1,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Query with low == high should still return empty
        let result = storage.entries(2, 2, None);
        assert!(result.is_ok(), "Empty range should succeed");
        assert_eq!(
            result.unwrap().len(),
            0,
            "Empty range should return no entries"
        );
    }

    #[test]
    fn test_entries_normal_range_returns_correct_entries() {
        let storage = MemStorage::new();

        // Add entries with indices 1, 2, 3, 4, 5
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                data: vec![1],
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                data: vec![2],
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                data: vec![3],
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                data: vec![4],
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                data: vec![5],
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Query range [2, 5) should return entries 2, 3, 4
        let result = storage.entries(2, 5, None);
        assert!(result.is_ok(), "Valid range should succeed");

        let returned = result.unwrap();
        assert_eq!(returned.len(), 3, "Should return 3 entries");
        assert_eq!(returned[0].index, 2, "First entry should have index 2");
        assert_eq!(returned[1].index, 3, "Second entry should have index 3");
        assert_eq!(returned[2].index, 4, "Third entry should have index 4");
        assert_eq!(returned[0].data, vec![2], "First entry data should match");
        assert_eq!(returned[1].data, vec![3], "Second entry data should match");
        assert_eq!(returned[2].data, vec![4], "Third entry data should match");
    }

    #[test]
    fn test_entries_single_entry_range() {
        let storage = MemStorage::new();

        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                data: vec![1],
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                data: vec![2],
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                data: vec![3],
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Query single entry [2, 3)
        let result = storage.entries(2, 3, None);
        assert!(result.is_ok(), "Single entry range should succeed");

        let returned = result.unwrap();
        assert_eq!(returned.len(), 1, "Should return 1 entry");
        assert_eq!(returned[0].index, 2, "Entry should have index 2");
        assert_eq!(returned[0].data, vec![2], "Entry data should match");
    }

    #[test]
    fn test_entries_full_range() {
        let storage = MemStorage::new();

        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Query all entries [1, 4)
        let result = storage.entries(1, 4, None);
        assert!(result.is_ok(), "Full range should succeed");

        let returned = result.unwrap();
        assert_eq!(returned.len(), 3, "Should return all 3 entries");
        assert_eq!(returned[0].index, 1);
        assert_eq!(returned[1].index, 2);
        assert_eq!(returned[2].index, 3);
    }

    #[test]
    fn test_entries_with_max_size_returns_partial_results() {
        let storage = MemStorage::new();

        // Create entries with specific sizes
        // Each entry has some overhead, so we'll use data to control size
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                data: vec![0; 100],
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                data: vec![0; 100],
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                data: vec![0; 100],
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                data: vec![0; 100],
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Request range [1, 5) with size limit that fits only first 2 entries
        // Each entry is roughly 100+ bytes, so max_size of 250 should get us 2 entries
        let result = storage.entries(1, 5, Some(250));
        assert!(result.is_ok(), "Size-limited query should succeed");

        let returned = result.unwrap();
        assert!(
            !returned.is_empty() && returned.len() < 4,
            "Should return partial results (got {} entries)",
            returned.len()
        );
        assert_eq!(returned[0].index, 1, "First entry should have index 1");
    }

    #[test]
    fn test_entries_with_max_size_returns_at_least_one_entry() {
        let storage = MemStorage::new();

        // Create entry larger than max_size
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                data: vec![0; 1000],
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                data: vec![0; 1000],
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Request with very small max_size - should still return at least first entry
        let result = storage.entries(1, 3, Some(10));
        assert!(result.is_ok(), "Should succeed even with small max_size");

        let returned = result.unwrap();
        assert_eq!(returned.len(), 1, "Should return at least one entry");
        assert_eq!(returned[0].index, 1, "Should return first entry");
    }

    #[test]
    fn test_entries_error_when_low_less_than_first_index() {
        let storage = MemStorage::new();

        // Create a snapshot at index 5
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 2;
        *storage.snapshot.write().unwrap() = snapshot;

        // Add entries starting from index 6
        let entries = vec![
            Entry {
                index: 6,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 7,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // first_index() should be 6 (snapshot.index + 1)
        // Requesting entries before that should fail
        let result = storage.entries(4, 7, None);
        assert!(result.is_err(), "Should error when low < first_index");

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Compacted) => {
                // Expected error
            }
            other => panic!("Expected StorageError::Compacted, got {other:?}"),
        }
    }

    #[test]
    fn test_entries_error_when_high_greater_than_last_index_plus_one() {
        let storage = MemStorage::new();

        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // last_index() is 3, so high can be at most 4 (last_index + 1)
        // Requesting high > 4 should fail
        let result = storage.entries(1, 5, None);
        assert!(result.is_err(), "Should error when high > last_index + 1");

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected error
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn test_entries_boundary_at_last_index_plus_one() {
        let storage = MemStorage::new();

        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // last_index() is 3, so high = 4 (last_index + 1) should be valid
        let result = storage.entries(1, 4, None);
        assert!(result.is_ok(), "high = last_index + 1 should be valid");

        let returned = result.unwrap();
        assert_eq!(returned.len(), 3, "Should return all entries");
    }

    #[test]
    fn test_entries_on_empty_storage() {
        let storage = MemStorage::new();

        // Empty storage: first_index = 1, last_index = 0
        // Valid range should be [1, 1) which returns empty
        let result = storage.entries(1, 1, None);
        assert!(
            result.is_ok(),
            "Empty range on empty storage should succeed"
        );
        assert_eq!(result.unwrap().len(), 0);

        // Any request with high > 1 should fail (unavailable)
        let result = storage.entries(1, 2, None);
        assert!(
            result.is_err(),
            "Should error when requesting unavailable entries"
        );

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn test_entries_thread_safe() {
        let storage = Arc::new(MemStorage::new());

        // Populate storage
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Spawn multiple threads reading concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    let result = storage_clone.entries(2, 4, None);
                    assert!(result.is_ok());
                    let returned = result.unwrap();
                    assert_eq!(returned.len(), 2);
                    assert_eq!(returned[0].index, 2);
                    assert_eq!(returned[1].index, 3);
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    // ============================================================================
    // Tests for term() method
    // ============================================================================

    #[test]
    fn test_term_index_zero_returns_zero() {
        let storage = MemStorage::new();

        // Index 0 should always return term 0
        let result = storage.term(0);
        assert!(result.is_ok(), "term(0) should succeed");
        assert_eq!(result.unwrap(), 0, "term(0) should return 0");
    }

    #[test]
    fn test_term_for_valid_indices_in_log() {
        let storage = MemStorage::new();

        // Add entries with different terms
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Test term for each entry
        assert_eq!(storage.term(1).unwrap(), 1, "Entry 1 should have term 1");
        assert_eq!(storage.term(2).unwrap(), 1, "Entry 2 should have term 1");
        assert_eq!(storage.term(3).unwrap(), 2, "Entry 3 should have term 2");
        assert_eq!(storage.term(4).unwrap(), 3, "Entry 4 should have term 3");
        assert_eq!(storage.term(5).unwrap(), 3, "Entry 5 should have term 3");
    }

    #[test]
    fn test_term_for_snapshot_index() {
        let storage = MemStorage::new();

        // Create a snapshot at index 5 with term 2
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 2;
        *storage.snapshot.write().unwrap() = snapshot;

        // Add entries starting from index 6
        let entries = vec![
            Entry {
                index: 6,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 7,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Query term for snapshot index should return snapshot term
        let result = storage.term(5);
        assert!(result.is_ok(), "term(snapshot_index) should succeed");
        assert_eq!(result.unwrap(), 2, "Should return snapshot term");
    }

    #[test]
    fn test_term_error_for_compacted_index() {
        let storage = MemStorage::new();

        // Create a snapshot at index 5
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 2;
        *storage.snapshot.write().unwrap() = snapshot;

        // Add entries starting from index 6
        let entries = vec![
            Entry {
                index: 6,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 7,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // first_index() should be 6 (snapshot.index + 1)
        // Requesting term for index before that should fail
        let result = storage.term(4);
        assert!(result.is_err(), "Should error for compacted index");

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Compacted) => {
                // Expected error
            }
            other => panic!("Expected StorageError::Compacted, got {other:?}"),
        }
    }

    #[test]
    fn test_term_error_for_unavailable_index() {
        let storage = MemStorage::new();

        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // last_index() is 3
        // Requesting term for index > 3 should fail
        let result = storage.term(4);
        assert!(result.is_err(), "Should error for unavailable index");

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected error
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn test_term_on_empty_storage() {
        let storage = MemStorage::new();

        // Index 0 should work
        assert_eq!(storage.term(0).unwrap(), 0, "term(0) should return 0");

        // Any positive index should fail with Unavailable
        let result = storage.term(1);
        assert!(result.is_err(), "Should error for index beyond empty log");

        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn test_term_thread_safety() {
        let storage = Arc::new(MemStorage::new());

        // Populate storage
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Spawn multiple threads reading terms concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    assert_eq!(storage_clone.term(0).unwrap(), 0);
                    assert_eq!(storage_clone.term(1).unwrap(), 1);
                    assert_eq!(storage_clone.term(2).unwrap(), 2);
                    assert_eq!(storage_clone.term(3).unwrap(), 2);
                    assert_eq!(storage_clone.term(4).unwrap(), 3);
                    assert_eq!(storage_clone.term(5).unwrap(), 3);
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    #[test]
    fn test_term_boundary_conditions() {
        let storage = MemStorage::new();

        // Add a single entry
        let entries = vec![Entry {
            index: 1,
            term: 5,
            ..Default::default()
        }];
        storage.append(&entries);

        // Test boundaries
        assert_eq!(storage.term(0).unwrap(), 0, "Index 0 returns 0");
        assert_eq!(storage.term(1).unwrap(), 5, "Index 1 returns correct term");

        // Index 2 should be unavailable
        let result = storage.term(2);
        assert!(result.is_err(), "Index beyond last should error");
        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn test_term_with_snapshot_but_no_entries() {
        let storage = MemStorage::new();

        // Create a snapshot at index 10 with term 5
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 5;
        *storage.snapshot.write().unwrap() = snapshot;

        // No entries added, only snapshot exists

        // Index 0 should work
        assert_eq!(storage.term(0).unwrap(), 0, "Index 0 returns 0");

        // Snapshot index should return snapshot term
        assert_eq!(
            storage.term(10).unwrap(),
            5,
            "Snapshot index returns snapshot term"
        );

        // Indices before snapshot should be compacted
        let result = storage.term(9);
        assert!(result.is_err(), "Index before snapshot should be compacted");
        match result.unwrap_err() {
            raft::Error::Store(StorageError::Compacted) => {
                // Expected
            }
            other => panic!("Expected StorageError::Compacted, got {other:?}"),
        }

        // Indices after snapshot should be unavailable
        let result = storage.term(11);
        assert!(
            result.is_err(),
            "Index after snapshot should be unavailable"
        );
        match result.unwrap_err() {
            raft::Error::Store(StorageError::Unavailable) => {
                // Expected
            }
            other => panic!("Expected StorageError::Unavailable, got {other:?}"),
        }
    }

    // ============================================================================
    // Tests for first_index() method
    // ============================================================================

    #[test]
    fn test_first_index_empty_log() {
        let storage = MemStorage::new();

        // Empty log should return 1 as the default first index
        let result = storage.first_index();
        assert!(result.is_ok(), "first_index should succeed on empty log");
        assert_eq!(result.unwrap(), 1, "Empty log should have first_index = 1");
    }

    #[test]
    fn test_first_index_after_append() {
        let storage = MemStorage::new();

        // Append entries starting at index 1
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        let result = storage.first_index();
        assert!(result.is_ok(), "first_index should succeed");
        assert_eq!(
            result.unwrap(),
            1,
            "first_index should be 1 when entries start at 1"
        );
    }

    #[test]
    fn test_first_index_with_snapshot() {
        let storage = MemStorage::new();

        // Create a snapshot at index 10
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snapshot;

        // No entries yet, first_index should be snapshot.index + 1
        let result = storage.first_index();
        assert!(result.is_ok(), "first_index should succeed with snapshot");
        assert_eq!(
            result.unwrap(),
            11,
            "first_index should be snapshot.index + 1"
        );
    }

    #[test]
    fn test_first_index_with_snapshot_and_entries() {
        let storage = MemStorage::new();

        // Create a snapshot at index 10
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snapshot;

        // Add entries starting from index 11
        let entries = vec![
            Entry {
                index: 11,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 12,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 13,
                term: 4,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // first_index should still be snapshot.index + 1
        let result = storage.first_index();
        assert!(result.is_ok(), "first_index should succeed");
        assert_eq!(
            result.unwrap(),
            11,
            "first_index should be snapshot.index + 1 even with entries"
        );
    }

    #[test]
    fn test_first_index_after_compaction() {
        let storage = MemStorage::new();

        // Simulate log compaction by:
        // 1. Creating a snapshot at index 50
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 50;
        snapshot.mut_metadata().term = 10;
        *storage.snapshot.write().unwrap() = snapshot;

        // 2. Adding new entries after the snapshot
        let entries = vec![
            Entry {
                index: 51,
                term: 10,
                ..Default::default()
            },
            Entry {
                index: 52,
                term: 11,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        let result = storage.first_index();
        assert!(
            result.is_ok(),
            "first_index should succeed after compaction"
        );
        assert_eq!(
            result.unwrap(),
            51,
            "first_index should be 51 after compaction at index 50"
        );
    }

    #[test]
    fn test_first_index_with_entries_not_starting_at_one() {
        let storage = MemStorage::new();

        // Directly append entries that don't start at index 1
        // (simulating entries after compaction)
        let entries = vec![
            Entry {
                index: 20,
                term: 5,
                ..Default::default()
            },
            Entry {
                index: 21,
                term: 5,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Without a snapshot, first_index should return the first entry's index
        let result = storage.first_index();
        assert!(result.is_ok(), "first_index should succeed");
        assert_eq!(
            result.unwrap(),
            20,
            "first_index should match first entry index"
        );
    }

    // ============================================================================
    // Tests for last_index() method
    // ============================================================================

    #[test]
    fn test_last_index_empty_log() {
        let storage = MemStorage::new();

        // Empty log should return 0 as the last index
        let result = storage.last_index();
        assert!(result.is_ok(), "last_index should succeed on empty log");
        assert_eq!(result.unwrap(), 0, "Empty log should have last_index = 0");
    }

    #[test]
    fn test_last_index_after_append() {
        let storage = MemStorage::new();

        // Append entries
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        let result = storage.last_index();
        assert!(result.is_ok(), "last_index should succeed");
        assert_eq!(
            result.unwrap(),
            3,
            "last_index should be the index of the last entry"
        );
    }

    #[test]
    fn test_last_index_snapshot_only() {
        let storage = MemStorage::new();

        // Create a snapshot at index 10, no entries
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snapshot;

        // With no entries, last_index should return snapshot.index
        let result = storage.last_index();
        assert!(
            result.is_ok(),
            "last_index should succeed with snapshot only"
        );
        assert_eq!(
            result.unwrap(),
            10,
            "last_index should be snapshot.index when no entries exist"
        );
    }

    #[test]
    fn test_last_index_with_snapshot_and_entries() {
        let storage = MemStorage::new();

        // Create a snapshot at index 10
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snapshot;

        // Add entries after the snapshot
        let entries = vec![
            Entry {
                index: 11,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 12,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 13,
                term: 4,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // last_index should return the last entry's index, not the snapshot
        let result = storage.last_index();
        assert!(result.is_ok(), "last_index should succeed");
        assert_eq!(
            result.unwrap(),
            13,
            "last_index should be the last entry index, not snapshot index"
        );
    }

    #[test]
    fn test_last_index_after_multiple_appends() {
        let storage = MemStorage::new();

        // First append
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ];
        storage.append(&entries1);

        assert_eq!(
            storage.last_index().unwrap(),
            2,
            "After first append, last_index should be 2"
        );

        // Second append
        let entries2 = vec![
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
        ];
        storage.append(&entries2);

        assert_eq!(
            storage.last_index().unwrap(),
            5,
            "After second append, last_index should be 5"
        );
    }

    #[test]
    fn test_last_index_single_entry() {
        let storage = MemStorage::new();

        // Append a single entry
        let entries = vec![Entry {
            index: 1,
            term: 1,
            ..Default::default()
        }];
        storage.append(&entries);

        let result = storage.last_index();
        assert!(
            result.is_ok(),
            "last_index should succeed with single entry"
        );
        assert_eq!(
            result.unwrap(),
            1,
            "last_index should be 1 for single entry"
        );
    }

    // ============================================================================
    // Tests for first_index() and last_index() invariants
    // ============================================================================

    #[test]
    fn test_first_last_index_invariant() {
        // Test the invariant: first_index <= last_index + 1
        // This should hold in all valid states

        let storage = MemStorage::new();

        // Case 1: Empty log
        let first = storage.first_index().unwrap();
        let last = storage.last_index().unwrap();
        assert!(
            first <= last + 1,
            "Empty log: first_index ({first}) <= last_index ({last}) + 1"
        );

        // Case 2: After appending entries
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        let first = storage.first_index().unwrap();
        let last = storage.last_index().unwrap();
        assert!(
            first <= last + 1,
            "With entries: first_index ({first}) <= last_index ({last}) + 1"
        );

        // Case 3: With snapshot (need to clear old entries to simulate proper compaction)
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snapshot;
        // Clear old entries that are covered by the snapshot
        storage.entries.write().unwrap().clear();

        let first = storage.first_index().unwrap();
        let last = storage.last_index().unwrap();
        assert!(
            first <= last + 1,
            "With snapshot: first_index ({first}) <= last_index ({last}) + 1"
        );

        // Case 4: With snapshot and new entries
        let entries = vec![
            Entry {
                index: 11,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 12,
                term: 4,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        let first = storage.first_index().unwrap();
        let last = storage.last_index().unwrap();
        assert!(
            first <= last + 1,
            "With snapshot and entries: first_index ({first}) <= last_index ({last}) + 1"
        );
    }

    #[test]
    fn test_first_last_index_boundaries() {
        let storage = MemStorage::new();

        // Empty log special case
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 0);
        // This is the one case where first > last, but first <= last + 1 still holds

        // Single entry
        storage.append(&[Entry {
            index: 1,
            term: 1,
            ..Default::default()
        }]);
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 1);

        // Multiple entries
        storage.append(&[
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 1,
                ..Default::default()
            },
        ]);
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 3);
    }

    #[test]
    fn test_first_last_index_thread_safety() {
        let storage = Arc::new(MemStorage::new());

        // Populate storage
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Spawn multiple threads reading first_index and last_index concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    let first = storage_clone.first_index().unwrap();
                    let last = storage_clone.last_index().unwrap();
                    assert_eq!(first, 1, "first_index should be 1");
                    assert_eq!(last, 3, "last_index should be 3");
                    assert!(
                        first <= last + 1,
                        "Invariant should hold: first_index <= last_index + 1"
                    );
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    #[test]
    fn test_first_last_index_consistency() {
        let storage = MemStorage::new();

        // Test that multiple consecutive calls return the same values
        for _ in 0..100 {
            let first1 = storage.first_index().unwrap();
            let last1 = storage.last_index().unwrap();
            let first2 = storage.first_index().unwrap();
            let last2 = storage.last_index().unwrap();

            assert_eq!(first1, first2, "Consecutive first_index calls should match");
            assert_eq!(last1, last2, "Consecutive last_index calls should match");
        }

        // Add entries and test again
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        for _ in 0..100 {
            let first1 = storage.first_index().unwrap();
            let last1 = storage.last_index().unwrap();
            let first2 = storage.first_index().unwrap();
            let last2 = storage.last_index().unwrap();

            assert_eq!(first1, first2, "Consecutive first_index calls should match");
            assert_eq!(last1, last2, "Consecutive last_index calls should match");
        }
    }

    #[test]
    fn test_first_last_index_with_large_snapshot() {
        let storage = MemStorage::new();

        // Create a snapshot at a large index
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 1_000_000;
        snapshot.mut_metadata().term = 100;
        *storage.snapshot.write().unwrap() = snapshot;

        let first = storage.first_index().unwrap();
        let last = storage.last_index().unwrap();

        assert_eq!(first, 1_000_001, "first_index should be snapshot.index + 1");
        assert_eq!(last, 1_000_000, "last_index should be snapshot.index");
        assert!(
            first <= last + 1,
            "Invariant should hold even with large indices"
        );
    }

    #[test]
    fn test_first_last_index_multiple_scenarios() {
        let storage = MemStorage::new();

        // Scenario 1: Empty
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 0);

        // Scenario 2: Add entries
        storage.append(&[
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ]);
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 2);

        // Scenario 3: Add more entries
        storage.append(&[
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
        ]);
        assert_eq!(storage.first_index().unwrap(), 1);
        assert_eq!(storage.last_index().unwrap(), 5);

        // Scenario 4: Add snapshot (simulate compaction)
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 3;
        snapshot.mut_metadata().term = 2;
        *storage.snapshot.write().unwrap() = snapshot;
        assert_eq!(storage.first_index().unwrap(), 4);
        assert_eq!(storage.last_index().unwrap(), 5);

        // Scenario 5: Add more entries after snapshot
        storage.append(&[
            Entry {
                index: 6,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 7,
                term: 4,
                ..Default::default()
            },
        ]);
        assert_eq!(storage.first_index().unwrap(), 4);
        assert_eq!(storage.last_index().unwrap(), 7);
    }

    // ============================================================================
    // Tests for snapshot() method
    // ============================================================================

    #[test]
    fn test_snapshot_returns_default_on_new_storage() {
        let storage = MemStorage::new();

        // Empty storage should return default snapshot
        let result = storage.snapshot(0);
        assert!(result.is_ok(), "snapshot() should succeed on new storage");

        let snapshot = result.unwrap();
        assert_eq!(
            snapshot.get_metadata().index,
            0,
            "Default snapshot should have index 0"
        );
        assert_eq!(
            snapshot.get_metadata().term,
            0,
            "Default snapshot should have term 0"
        );
        assert!(
            snapshot.data.is_empty(),
            "Default snapshot should have empty data"
        );
    }

    #[test]
    fn test_snapshot_returns_stored_snapshot() {
        let storage = MemStorage::new();

        // Create and store a snapshot
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 10;
        snap.mut_metadata().term = 3;
        snap.data = vec![1, 2, 3, 4, 5];
        *storage.snapshot.write().unwrap() = snap;

        // Retrieve snapshot
        let result = storage.snapshot(0);
        assert!(result.is_ok(), "snapshot() should succeed");

        let retrieved = result.unwrap();
        assert_eq!(
            retrieved.get_metadata().index,
            10,
            "Should return stored snapshot index"
        );
        assert_eq!(
            retrieved.get_metadata().term,
            3,
            "Should return stored snapshot term"
        );
        assert_eq!(
            retrieved.data,
            vec![1, 2, 3, 4, 5],
            "Should return stored snapshot data"
        );
    }

    #[test]
    fn test_snapshot_ignores_request_index_in_phase_1() {
        let storage = MemStorage::new();

        // Store a snapshot at index 10
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 10;
        snap.mut_metadata().term = 3;
        *storage.snapshot.write().unwrap() = snap;

        // Request snapshot with different request_index values
        // In Phase 1, all should return the same snapshot
        let snap0 = storage.snapshot(0).unwrap();
        let snap5 = storage.snapshot(5).unwrap();
        let snap10 = storage.snapshot(10).unwrap();
        let snap100 = storage.snapshot(100).unwrap();

        // All should be identical
        assert_eq!(snap0.get_metadata().index, 10);
        assert_eq!(snap5.get_metadata().index, 10);
        assert_eq!(snap10.get_metadata().index, 10);
        assert_eq!(snap100.get_metadata().index, 10);
    }

    #[test]
    fn test_snapshot_with_metadata() {
        let storage = MemStorage::new();

        // Create snapshot with complex metadata
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 42;
        snap.mut_metadata().term = 7;

        // Set configuration in metadata
        snap.mut_metadata().conf_state = Some(ConfState {
            voters: vec![1, 2, 3],
            learners: vec![4, 5],
            ..Default::default()
        });

        *storage.snapshot.write().unwrap() = snap;

        // Retrieve and verify
        let retrieved = storage.snapshot(0).unwrap();
        assert_eq!(retrieved.get_metadata().index, 42);
        assert_eq!(retrieved.get_metadata().term, 7);
        assert_eq!(
            retrieved.get_metadata().conf_state.as_ref().unwrap().voters,
            vec![1, 2, 3]
        );
        assert_eq!(
            retrieved
                .get_metadata()
                .conf_state
                .as_ref()
                .unwrap()
                .learners,
            vec![4, 5]
        );
    }

    #[test]
    fn test_snapshot_with_data() {
        let storage = MemStorage::new();

        // Create snapshot with substantial data
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 100;
        snap.mut_metadata().term = 10;
        snap.data = vec![0; 10_000]; // 10KB of data
        *storage.snapshot.write().unwrap() = snap;

        // Retrieve and verify
        let retrieved = storage.snapshot(0).unwrap();
        assert_eq!(retrieved.get_metadata().index, 100);
        assert_eq!(retrieved.get_metadata().term, 10);
        assert_eq!(retrieved.data.len(), 10_000);
        assert!(retrieved.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_snapshot_returns_cloned_data() {
        let storage = MemStorage::new();

        // Store initial snapshot
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 5;
        snap.mut_metadata().term = 2;
        snap.data = vec![1, 2, 3];
        *storage.snapshot.write().unwrap() = snap;

        // Get first snapshot
        let snap1 = storage.snapshot(0).unwrap();

        // Modify storage snapshot
        let mut new_snap = Snapshot::default();
        new_snap.mut_metadata().index = 10;
        new_snap.mut_metadata().term = 5;
        new_snap.data = vec![4, 5, 6];
        *storage.snapshot.write().unwrap() = new_snap;

        // Get second snapshot
        let snap2 = storage.snapshot(0).unwrap();

        // Verify snap1 is unaffected by later changes
        assert_eq!(
            snap1.get_metadata().index,
            5,
            "First snapshot should be unaffected"
        );
        assert_eq!(
            snap1.get_metadata().term,
            2,
            "First snapshot term should be unaffected"
        );
        assert_eq!(
            snap1.data,
            vec![1, 2, 3],
            "First snapshot data should be unaffected"
        );

        // Verify snap2 has new values
        assert_eq!(
            snap2.get_metadata().index,
            10,
            "Second snapshot should have new values"
        );
        assert_eq!(
            snap2.get_metadata().term,
            5,
            "Second snapshot should have new term"
        );
        assert_eq!(
            snap2.data,
            vec![4, 5, 6],
            "Second snapshot should have new data"
        );
    }

    #[test]
    fn test_snapshot_is_thread_safe() {
        let storage = Arc::new(MemStorage::new());

        // Store a snapshot
        let mut snap = Snapshot::default();
        snap.mut_metadata().index = 20;
        snap.mut_metadata().term = 4;
        snap.data = vec![10, 20, 30, 40, 50];
        *storage.snapshot.write().unwrap() = snap;

        // Spawn multiple threads reading snapshot concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    // Each thread reads the snapshot 100 times
                    for request_idx in 0..100 {
                        let result = storage_clone.snapshot(request_idx);
                        assert!(result.is_ok(), "snapshot() should succeed");

                        let snapshot = result.unwrap();
                        assert_eq!(
                            snapshot.get_metadata().index,
                            20,
                            "Snapshot index should be consistent"
                        );
                        assert_eq!(
                            snapshot.get_metadata().term,
                            4,
                            "Snapshot term should be consistent"
                        );
                        assert_eq!(
                            snapshot.data,
                            vec![10, 20, 30, 40, 50],
                            "Snapshot data should be consistent"
                        );
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    // ============================================================================
    // Tests for apply_snapshot() method
    // ============================================================================

    #[test]
    fn test_apply_snapshot_replaces_all_state() {
        let storage = MemStorage::new();

        // Add some initial entries
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.append(&entries);

        // Create a snapshot at index 5
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 3;
        snapshot.mut_metadata().conf_state = Some(ConfState {
            voters: vec![1, 2, 3],
            ..Default::default()
        });
        snapshot.data = vec![10, 20, 30];

        // Apply snapshot
        let result = storage.apply_snapshot(snapshot.clone());
        assert!(result.is_ok(), "apply_snapshot should succeed");

        // Verify snapshot was stored
        let stored_snap = storage.snapshot(0).unwrap();
        assert_eq!(stored_snap.get_metadata().index, 5);
        assert_eq!(stored_snap.get_metadata().term, 3);
        assert_eq!(stored_snap.data, vec![10, 20, 30]);

        // Verify entries covered by snapshot were removed
        let remaining_entries = storage.entries.read().unwrap();
        assert!(
            remaining_entries.is_empty(),
            "All entries should be removed as they are covered by snapshot"
        );
    }

    #[test]
    fn test_apply_snapshot_clears_entries_covered_by_snapshot() {
        let storage = MemStorage::new();

        // Add entries 1-10
        let entries: Vec<Entry> = (1..=10)
            .map(|i| Entry {
                index: i,
                term: 1,
                ..Default::default()
            })
            .collect();
        storage.append(&entries);

        // Apply snapshot at index 5
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 2;

        storage.apply_snapshot(snapshot).unwrap();

        // Only entries 6-10 should remain
        let remaining = storage.entries.read().unwrap();
        assert_eq!(
            remaining.len(),
            5,
            "Only entries after snapshot should remain"
        );
        assert_eq!(
            remaining[0].index, 6,
            "First remaining entry should be index 6"
        );
        assert_eq!(
            remaining[4].index, 10,
            "Last remaining entry should be index 10"
        );
    }

    #[test]
    fn test_apply_snapshot_updates_hard_state() {
        let storage = MemStorage::new();

        // Set initial hard state
        storage.set_hard_state(HardState {
            term: 1,
            vote: 1,
            commit: 2,
        });

        // Apply snapshot with higher term and commit
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 5;

        storage.apply_snapshot(snapshot).unwrap();

        // Verify hard state was updated
        let hard_state = storage.hard_state.read().unwrap();
        assert_eq!(
            hard_state.term, 5,
            "Term should be updated to snapshot term"
        );
        assert_eq!(
            hard_state.commit, 10,
            "Commit should be updated to snapshot index"
        );
    }

    #[test]
    fn test_apply_snapshot_preserves_higher_hard_state_values() {
        let storage = MemStorage::new();

        // Set high commit
        storage.set_hard_state(HardState {
            term: 10,
            vote: 1,
            commit: 20,
        });

        // Apply snapshot with lower values
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 3;

        storage.apply_snapshot(snapshot).unwrap();

        // Verify higher values were preserved
        let hard_state = storage.hard_state.read().unwrap();
        assert_eq!(hard_state.term, 10, "Higher term should be preserved");
        assert_eq!(hard_state.commit, 20, "Higher commit should be preserved");
    }

    #[test]
    fn test_apply_snapshot_updates_conf_state() {
        let storage = MemStorage::new();

        // Set initial conf state
        storage.set_conf_state(ConfState {
            voters: vec![1, 2],
            learners: vec![3],
            ..Default::default()
        });

        // Apply snapshot with different conf state
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 5;
        snapshot.mut_metadata().conf_state = Some(ConfState {
            voters: vec![4, 5, 6],
            learners: vec![7, 8],
            ..Default::default()
        });

        storage.apply_snapshot(snapshot).unwrap();

        // Verify conf state was updated
        let conf_state = storage.conf_state.read().unwrap();
        assert_eq!(
            conf_state.voters,
            vec![4, 5, 6],
            "Voters should be updated from snapshot"
        );
        assert_eq!(
            conf_state.learners,
            vec![7, 8],
            "Learners should be updated from snapshot"
        );
    }

    #[test]
    fn test_apply_snapshot_with_no_conf_state_in_metadata() {
        let storage = MemStorage::new();

        // Set initial conf state
        storage.set_conf_state(ConfState {
            voters: vec![1, 2, 3],
            ..Default::default()
        });

        // Apply snapshot without conf_state in metadata
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 10;
        snapshot.mut_metadata().term = 5;
        // Don't set conf_state

        storage.apply_snapshot(snapshot).unwrap();

        // Verify conf state was not changed
        let conf_state = storage.conf_state.read().unwrap();
        assert_eq!(
            conf_state.voters,
            vec![1, 2, 3],
            "Conf state should remain unchanged when snapshot has no conf_state"
        );
    }

    #[test]
    fn test_apply_snapshot_thread_safety() {
        let storage = Arc::new(MemStorage::new());

        // Add initial entries
        let entries: Vec<Entry> = (1..=20)
            .map(|i| Entry {
                index: i,
                term: 1,
                ..Default::default()
            })
            .collect();
        storage.append(&entries);

        // Create multiple snapshots
        let snapshots: Vec<Snapshot> = (1..=5)
            .map(|i| {
                let mut snap = Snapshot::default();
                snap.mut_metadata().index = i * 5;
                snap.mut_metadata().term = i;
                snap.data = vec![i as u8; 100];
                snap
            })
            .collect();

        // Apply snapshots concurrently (should be serialized by write locks)
        let handles: Vec<_> = snapshots
            .into_iter()
            .map(|snap| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    storage_clone.apply_snapshot(snap).unwrap();
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().expect("Thread should not panic");
        }

        // Verify final state is consistent (one of the snapshots was applied)
        let final_snap = storage.snapshot(0).unwrap();
        assert!(
            final_snap.get_metadata().index > 0,
            "A snapshot should have been applied"
        );

        // Verify entries are consistent with snapshot
        let entries = storage.entries.read().unwrap();
        if !entries.is_empty() {
            assert!(
                entries[0].index > final_snap.get_metadata().index,
                "Remaining entries should be after snapshot index"
            );
        }
    }

    #[test]
    fn test_apply_snapshot_empty_log() {
        let storage = MemStorage::new();

        // Apply snapshot on empty log
        let mut snapshot = Snapshot::default();
        snapshot.mut_metadata().index = 5;
        snapshot.mut_metadata().term = 2;
        snapshot.data = vec![1, 2, 3];

        let result = storage.apply_snapshot(snapshot.clone());
        assert!(result.is_ok(), "apply_snapshot should succeed on empty log");

        // Verify snapshot was stored
        let stored = storage.snapshot(0).unwrap();
        assert_eq!(stored.get_metadata().index, 5);
        assert_eq!(stored.get_metadata().term, 2);
        assert_eq!(stored.data, vec![1, 2, 3]);
    }

    // ============================================================================
    // Tests for wl_append_entries() method
    // ============================================================================

    #[test]
    fn test_wl_append_entries_to_empty_log() {
        let storage = MemStorage::new();

        // Append to empty log
        let entries = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];

        let result = storage.wl_append_entries(&entries);
        assert!(result.is_ok(), "wl_append_entries should succeed");

        // Verify entries were appended
        assert_eq!(storage.last_index().unwrap(), 3);
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 3);
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[2].index, 3);
    }

    #[test]
    fn test_wl_append_entries_after_existing_entries() {
        let storage = MemStorage::new();

        // Add initial entries
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Append more entries after existing ones
        let entries2 = vec![
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries2).unwrap();

        // Verify all entries are present
        assert_eq!(storage.last_index().unwrap(), 4);
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 4);
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[3].index, 4);
    }

    #[test]
    fn test_wl_append_entries_truncates_conflicting_entries() {
        let storage = MemStorage::new();

        // Add initial entries in term 1
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 1,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Append conflicting entries (term 2 starting at index 2)
        let entries2 = vec![
            Entry {
                index: 2,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries2).unwrap();

        // Verify old entries were truncated and new ones appended
        assert_eq!(storage.last_index().unwrap(), 3);
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 3);
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[0].term, 1); // First entry unchanged
        assert_eq!(stored[1].index, 2);
        assert_eq!(stored[1].term, 2); // Replaced with term 2
        assert_eq!(stored[2].index, 3);
        assert_eq!(stored[2].term, 2); // Replaced with term 2
    }

    #[test]
    fn test_wl_append_entries_no_conflict_when_terms_match() {
        let storage = MemStorage::new();

        // Add initial entries
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Append entries with matching terms (should not truncate)
        let entries2 = vec![
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries2).unwrap();

        // Verify no truncation occurred, new entry was appended
        assert_eq!(storage.last_index().unwrap(), 4);
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 4);
        assert_eq!(stored[0].term, 1);
        assert_eq!(stored[1].term, 1);
        assert_eq!(stored[2].term, 2);
        assert_eq!(stored[3].term, 2);
    }

    #[test]
    fn test_wl_append_entries_empty_slice() {
        let storage = MemStorage::new();

        // Add initial entries
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Append empty slice (should be no-op)
        let empty: Vec<Entry> = vec![];
        let result = storage.wl_append_entries(&empty);
        assert!(result.is_ok(), "Empty append should succeed");

        // Verify nothing changed
        assert_eq!(storage.last_index().unwrap(), 2);
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[test]
    fn test_wl_append_entries_before_existing_log() {
        let storage = MemStorage::new();

        // Add entries starting at index 10
        let entries1 = vec![
            Entry {
                index: 10,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 11,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Append entries starting at index 1 (before existing log)
        let entries2 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries2).unwrap();

        // Should replace entire log
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[1].index, 2);
    }

    #[test]
    fn test_wl_append_entries_thread_safety() {
        let storage = Arc::new(MemStorage::new());

        // Start with some initial entries using the helper method
        storage.append(&[
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 1,
                ..Default::default()
            },
        ]);

        // Spawn multiple threads all appending the same extension
        // This tests that concurrent writes are properly serialized by the write lock
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let storage_clone = Arc::clone(&storage);
                thread::spawn(move || {
                    // All threads try to append entries 4 and 5
                    let entries = vec![
                        Entry {
                            index: 4,
                            term: 2,
                            ..Default::default()
                        },
                        Entry {
                            index: 5,
                            term: 2,
                            ..Default::default()
                        },
                    ];
                    storage_clone.wl_append_entries(&entries).unwrap();
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().expect("Thread should not panic");
        }

        // Verify final state is consistent - should have entries 1-5, no corruption
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 5, "Should have exactly 5 entries");
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[3].index, 4);
        assert_eq!(stored[4].index, 5);
        assert_eq!(stored[3].term, 2);
        assert_eq!(stored[4].term, 2);

        // Verify indices are contiguous
        for i in 1..stored.len() {
            assert_eq!(
                stored[i].index,
                stored[i - 1].index + 1,
                "Indices should be contiguous"
            );
        }
    }

    #[test]
    fn test_wl_append_entries_complex_conflict_resolution() {
        let storage = MemStorage::new();

        // Build log: [1:1, 2:1, 3:1, 4:2, 5:2]
        let entries1 = vec![
            Entry {
                index: 1,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 2,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 3,
                term: 1,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 2,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 2,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries1).unwrap();

        // Conflict at index 3: [3:3, 4:3, 5:3, 6:3]
        let entries2 = vec![
            Entry {
                index: 3,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 4,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 5,
                term: 3,
                ..Default::default()
            },
            Entry {
                index: 6,
                term: 3,
                ..Default::default()
            },
        ];
        storage.wl_append_entries(&entries2).unwrap();

        // Should have: [1:1, 2:1, 3:3, 4:3, 5:3, 6:3]
        let stored = storage.entries.read().unwrap();
        assert_eq!(stored.len(), 6);
        assert_eq!(stored[0].index, 1);
        assert_eq!(stored[0].term, 1);
        assert_eq!(stored[1].index, 2);
        assert_eq!(stored[1].term, 1);
        assert_eq!(stored[2].index, 3);
        assert_eq!(stored[2].term, 3);
        assert_eq!(stored[3].index, 4);
        assert_eq!(stored[3].term, 3);
        assert_eq!(stored[4].index, 5);
        assert_eq!(stored[4].term, 3);
        assert_eq!(stored[5].index, 6);
        assert_eq!(stored[5].term, 3);
    }
}
