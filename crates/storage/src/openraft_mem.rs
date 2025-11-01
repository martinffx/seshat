//! In-memory storage implementation for OpenRaft.
//!
//! This module implements OpenRaft's storage-v2 traits using in-memory data structures.
//! It separates log storage (RaftLogStorage) from state machine (RaftStateMachine).

use openraft::storage::{LogState, RaftLogReader, RaftLogStorage, RaftStateMachine};
use openraft::{
    AnyError, Entry, ErrorSubject, ErrorVerb, LogId, OptionalSend, RaftLogId, SnapshotMeta,
    StorageError, StorageIOError, Vote,
};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::sync::{Arc, RwLock};

use crate::{BasicNode, Response, StateMachine};

/// In-memory log storage for OpenRaft.
///
/// Implements RaftLogStorage for storing Raft log entries and votes.
#[derive(Debug, Clone)]
pub struct OpenRaftMemLog {
    /// Raft log entries indexed by log index
    log: Arc<RwLock<BTreeMap<u64, Entry<crate::RaftTypeConfig>>>>,

    /// Current vote state
    vote: Arc<RwLock<Option<Vote<u64>>>>,
}

impl OpenRaftMemLog {
    /// Create new empty log storage.
    pub fn new() -> Self {
        Self {
            log: Arc::new(RwLock::new(BTreeMap::new())),
            vote: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for OpenRaftMemLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Log reader for in-memory storage.
#[derive(Debug, Clone)]
pub struct OpenRaftMemLogReader {
    log: Arc<RwLock<BTreeMap<u64, Entry<crate::RaftTypeConfig>>>>,
}

impl RaftLogReader<crate::RaftTypeConfig> for OpenRaftMemLogReader {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<crate::RaftTypeConfig>>, StorageError<u64>> {
        let log = self.log.read().unwrap();
        let entries: Vec<_> = log.range(range).map(|(_, entry)| entry.clone()).collect();
        Ok(entries)
    }
}

// OpenRaftMemLog must also implement RaftLogReader
impl RaftLogReader<crate::RaftTypeConfig> for OpenRaftMemLog {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<crate::RaftTypeConfig>>, StorageError<u64>> {
        let log = self.log.read().unwrap();
        let entries: Vec<_> = log.range(range).map(|(_, entry)| entry.clone()).collect();
        Ok(entries)
    }
}

impl RaftLogStorage<crate::RaftTypeConfig> for OpenRaftMemLog {
    type LogReader = OpenRaftMemLogReader;

    async fn get_log_state(
        &mut self,
    ) -> Result<LogState<crate::RaftTypeConfig>, StorageError<u64>> {
        let log = self.log.read().unwrap();
        let last_purged_log_id = None;
        let last_log_id = log.iter().last().map(|(_, entry)| *entry.get_log_id());

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<(), StorageError<u64>> {
        *self.vote.write().unwrap() = Some(*vote);
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>, StorageError<u64>> {
        Ok(*self.vote.read().unwrap())
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: openraft::storage::LogFlushed<crate::RaftTypeConfig>,
    ) -> Result<(), StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<crate::RaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut log = self.log.write().unwrap();
        for entry in entries {
            log.insert(entry.get_log_id().index, entry);
        }
        callback.log_io_completed(Ok(()));
        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let mut log = self.log.write().unwrap();
        log.retain(|idx, _| *idx <= log_id.index);
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let mut log = self.log.write().unwrap();
        log.retain(|idx, _| *idx > log_id.index);
        Ok(())
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        OpenRaftMemLogReader {
            log: Arc::clone(&self.log),
        }
    }
}

/// In-memory state machine for OpenRaft.
///
/// Implements RaftStateMachine for applying log entries and managing snapshots.
#[derive(Debug, Clone)]
pub struct OpenRaftMemStateMachine {
    /// The actual state machine that applies operations
    sm: Arc<RwLock<StateMachine>>,

    /// Current snapshot
    snapshot: Arc<RwLock<Option<openraft::Snapshot<crate::RaftTypeConfig>>>>,
}

impl OpenRaftMemStateMachine {
    /// Create new empty state machine.
    pub fn new() -> Self {
        Self {
            sm: Arc::new(RwLock::new(StateMachine::new())),
            snapshot: Arc::new(RwLock::new(None)),
        }
    }

    /// Get reference to state machine for direct read access.
    pub fn state_machine(&self) -> Arc<RwLock<StateMachine>> {
        Arc::clone(&self.sm)
    }
}

impl Default for OpenRaftMemStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot builder for in-memory storage.
#[derive(Debug, Clone)]
pub struct OpenRaftMemSnapshotBuilder {
    sm: Arc<RwLock<StateMachine>>,
}

impl openraft::storage::RaftSnapshotBuilder<crate::RaftTypeConfig> for OpenRaftMemSnapshotBuilder {
    async fn build_snapshot(
        &mut self,
    ) -> Result<openraft::Snapshot<crate::RaftTypeConfig>, StorageError<u64>> {
        let sm = self.sm.read().unwrap();

        // Create snapshot data from state machine
        let snapshot_data = sm.snapshot().map_err(|e| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::Snapshot(None),
                ErrorVerb::Read,
                AnyError::error(e),
            ),
        })?;

        let last_applied = sm.last_applied();
        let last_log_id = if last_applied > 0 {
            Some(LogId::new(
                openraft::CommittedLeaderId::new(0, 0),
                last_applied,
            ))
        } else {
            None
        };

        // For now, use default membership (empty cluster)
        let last_membership = openraft::StoredMembership::default();

        let snapshot = openraft::Snapshot {
            meta: SnapshotMeta {
                last_log_id,
                last_membership,
                snapshot_id: format!("snapshot-{last_applied}"),
            },
            snapshot: Box::new(Cursor::new(snapshot_data)),
        };

        Ok(snapshot)
    }
}

impl RaftStateMachine<crate::RaftTypeConfig> for OpenRaftMemStateMachine {
    type SnapshotBuilder = OpenRaftMemSnapshotBuilder;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<u64>>,
            openraft::StoredMembership<u64, BasicNode>,
        ),
        StorageError<u64>,
    > {
        let sm = self.sm.read().unwrap();
        let last_applied = sm.last_applied();

        let log_id = if last_applied > 0 {
            Some(LogId::new(
                openraft::CommittedLeaderId::new(0, 0),
                last_applied,
            ))
        } else {
            None
        };

        // For now, return default membership (empty cluster)
        let membership = openraft::StoredMembership::default();

        Ok((log_id, membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Response>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<crate::RaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut responses = Vec::new();
        let mut sm = self.sm.write().unwrap();

        for entry in entries {
            // Extract request and apply
            let request = match &entry.payload {
                openraft::EntryPayload::Normal(ref req) => req,
                openraft::EntryPayload::Blank => {
                    responses.push(Response::new(vec![]));
                    continue;
                }
                openraft::EntryPayload::Membership(_) => {
                    // Membership changes don't produce application-level responses
                    responses.push(Response::new(vec![]));
                    continue;
                }
            };

            // Apply to state machine (handles idempotency)
            let result = sm
                .apply(entry.get_log_id().index, &request.operation_bytes)
                .map_err(|e| StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Write,
                        AnyError::error(e),
                    ),
                })?;

            responses.push(Response::new(result));
        }

        Ok(responses)
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<u64>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        _meta: &SnapshotMeta<u64, BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        let snapshot_data = snapshot.into_inner();

        let mut sm = self.sm.write().unwrap();
        sm.restore(&snapshot_data).map_err(|e| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::Snapshot(None),
                ErrorVerb::Write,
                AnyError::error(e),
            ),
        })?;

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<openraft::Snapshot<crate::RaftTypeConfig>>, StorageError<u64>> {
        Ok(self.snapshot.read().unwrap().clone())
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        OpenRaftMemSnapshotBuilder {
            sm: Arc::clone(&self.sm),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Operation, Request};
    use openraft::storage::RaftSnapshotBuilder;

    #[tokio::test]
    async fn test_log_storage_read_and_get_state() {
        let mut storage = OpenRaftMemLog::new();

        // Initially empty
        let log_state = storage.get_log_state().await.unwrap();
        assert!(log_state.last_log_id.is_none());
        assert!(log_state.last_purged_log_id.is_none());

        // Manually insert entry for testing (since append requires callback)
        let entry = Entry {
            log_id: LogId::new(openraft::CommittedLeaderId::new(1, 1), 1),
            payload: openraft::EntryPayload::Blank,
        };
        {
            let mut log = storage.log.write().unwrap();
            log.insert(1, entry.clone());
        }

        // Get log state
        let log_state = storage.get_log_state().await.unwrap();
        assert_eq!(log_state.last_log_id.unwrap().index, 1);

        // Read entries
        let mut reader = storage.get_log_reader().await;
        let entries = reader.try_get_log_entries(1..=1).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].get_log_id().index, 1);
    }

    #[tokio::test]
    async fn test_vote_operations() {
        let mut storage = OpenRaftMemLog::new();

        // Initially no vote
        let vote = storage.read_vote().await.unwrap();
        assert!(vote.is_none());

        // Save vote
        let new_vote = Vote::new(5, 1);
        storage.save_vote(&new_vote).await.unwrap();

        // Read vote back
        let read_vote = storage.read_vote().await.unwrap();
        assert_eq!(read_vote, Some(new_vote));
    }

    #[tokio::test]
    async fn test_state_machine_apply() {
        let mut sm = OpenRaftMemStateMachine::new();

        let op = Operation::Set {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };
        let request = Request::new(op.serialize().unwrap());

        let entry = Entry {
            log_id: LogId::new(openraft::CommittedLeaderId::new(1, 1), 1),
            payload: openraft::EntryPayload::Normal(request),
        };

        let responses = sm.apply(vec![entry]).await.unwrap();

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].result, b"OK");

        let state_machine = sm.state_machine();
        let sm_guard = state_machine.read().unwrap();
        assert_eq!(sm_guard.get(b"key1"), Some(b"value1".to_vec()));
        assert_eq!(sm_guard.last_applied(), 1);
    }

    #[tokio::test]
    async fn test_snapshot_roundtrip() {
        let mut sm = OpenRaftMemStateMachine::new();

        // Apply some data
        let ops = vec![
            Operation::Set {
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            },
            Operation::Set {
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            },
        ];

        for (i, op) in ops.into_iter().enumerate() {
            let entry = Entry {
                log_id: LogId::new(openraft::CommittedLeaderId::new(1, 1), (i + 1) as u64),
                payload: openraft::EntryPayload::Normal(Request::new(op.serialize().unwrap())),
            };
            sm.apply(vec![entry]).await.unwrap();
        }

        // Build snapshot
        let mut builder = sm.get_snapshot_builder().await;
        let snapshot = builder.build_snapshot().await.unwrap();

        // Install to new state machine
        let mut sm2 = OpenRaftMemStateMachine::new();
        sm2.install_snapshot(&snapshot.meta, snapshot.snapshot)
            .await
            .unwrap();

        // Verify state
        let state_machine = sm2.state_machine();
        let sm_guard = state_machine.read().unwrap();
        assert_eq!(sm_guard.get(b"k1"), Some(b"v1".to_vec()));
        assert_eq!(sm_guard.get(b"k2"), Some(b"v2".to_vec()));
        assert_eq!(sm_guard.last_applied(), 2);
    }
}
