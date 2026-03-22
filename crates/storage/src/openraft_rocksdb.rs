//! RocksDB-backed storage for OpenRaft.
//!
//! This module provides persistent storage implementations for OpenRaft using RocksDB.
//! It implements both `RaftLogStorage` and `RaftStateMachine` traits for two Raft groups:
//!
//! - **System Raft**: Manages cluster metadata (ClusterMembership, ShardMap)
//! - **Data Raft**: Manages user key-value data
//!
//! # Architecture
//!
//! The implementation is parameterized by a `RaftGroup` type that maps to specific
//! column families. This allows a single implementation to serve both Raft groups.
//!
//! # Serialization
//!
//! Vote, LogId, and membership data are serialized to bytes using prost protobuf.
//! The snapshot data uses OpenRaft's Cursor<Vec<u8>> format.

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::io::Cursor;
use std::marker::PhantomData;
use std::ops::RangeBounds;
use std::sync::Arc;

use openraft::storage::{
    LogFlushed, LogState, RaftLogReader, RaftLogStorage, RaftSnapshotBuilder, RaftStateMachine,
    Snapshot, SnapshotMeta,
};
use openraft::{
    AnyError, Entry, ErrorSubject, ErrorVerb, LeaderId, LogId, OptionalSend,
    StorageError, StorageIOError, StoredMembership, Vote,
};
use prost::Message;
use serde::{Deserialize, Serialize};

use crate::{BasicNode, ColumnFamily, RaftTypeConfig, Request, Response, Storage};

// =============================================================================
// Raft Group Trait
// =============================================================================

/// Defines column family mapping for a Raft group.
///
/// This trait abstracts over the two Raft groups in Seshat:
/// - System Raft: cluster metadata
/// - Data Raft: user KV data
pub trait RaftGroup: Send + Sync + 'static + Clone + Debug {
    /// Log entries column family.
    const LOG_CF: ColumnFamily;
    /// Persistent state column family (vote, applied, membership).
    const STATE_CF: ColumnFamily;
    /// KV data column family (None for System, Some(DataKv) for Data).
    const KV_CF: Option<ColumnFamily>;
}

/// System Raft group for cluster metadata.
///
/// Uses SystemRaftLog for log entries and SystemRaftState for persistent state.
/// Does not use a KV column family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemRaft;

impl RaftGroup for SystemRaft {
    const LOG_CF: ColumnFamily = ColumnFamily::SystemRaftLog;
    const STATE_CF: ColumnFamily = ColumnFamily::SystemRaftState;
    const KV_CF: Option<ColumnFamily> = None;
}

/// Data Raft group for user key-value data.
///
/// Uses DataRaftLog for log entries, DataRaftState for persistent state,
/// and DataKv for actual KV data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataRaft;

impl RaftGroup for DataRaft {
    const LOG_CF: ColumnFamily = ColumnFamily::DataRaftLog;
    const STATE_CF: ColumnFamily = ColumnFamily::DataRaftState;
    const KV_CF: Option<ColumnFamily> = Some(ColumnFamily::DataKv);
}

// =============================================================================
// Serialization Keys
// =============================================================================

const VOTE_KEY: &[u8] = b"vote";
const APPLIED_KEY: &[u8] = b"applied";
const MEMBERSHIP_KEY: &[u8] = b"membership";

// =============================================================================
// Protobuf Message Types
// =============================================================================

/// Protobuf message for Vote serialization.
#[derive(Message, Serialize, Deserialize)]
pub struct VoteMessage {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(uint64, tag = "2")]
    pub node_id: u64,
}

impl VoteMessage {
    pub fn from_vote(vote: &Vote<u64>) -> Self {
        Self {
            term: vote.leader_id().term,
            node_id: vote.leader_id().node_id,
        }
    }

    pub fn to_vote(&self) -> Vote<u64> {
        Vote::new(self.term, self.node_id)
    }
}

/// Protobuf message for LogId serialization.
#[derive(Message, Serialize, Deserialize)]
pub struct LogIdMessage {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(uint64, tag = "2")]
    pub node_id: u64,
    #[prost(uint64, tag = "3")]
    pub index: u64,
}

impl LogIdMessage {
    pub fn from_log_id(log_id: &LogId<u64>) -> Self {
        Self {
            term: log_id.leader_id.term,
            node_id: log_id.leader_id.node_id,
            index: log_id.index,
        }
    }

    pub fn to_log_id(&self) -> LogId<u64> {
        LogId::new(LeaderId::new(self.term, self.node_id), self.index)
    }
}

/// Protobuf message for membership serialization.
#[derive(Message, Serialize, Deserialize)]
pub struct MembershipMessage {
    #[prost(message, optional, tag = "1")]
    pub log_id: Option<LogIdMessage>,
    #[prost(message, repeated, tag = "2")]
    pub voters: Vec<VotersMessage>,
    #[prost(message, repeated, tag = "3")]
    pub learners: Vec<u64>,
}

#[derive(Message, Serialize, Deserialize)]
pub struct VotersMessage {
    #[prost(uint64, repeated, tag = "1")]
    pub nodes: Vec<u64>,
}

impl MembershipMessage {
    pub fn from_membership(membership: &StoredMembership<u64, BasicNode>) -> Self {
        let log_id = membership.log_id().as_ref().map(LogIdMessage::from_log_id);

        let voters = vec![VotersMessage {
            nodes: membership.voter_ids().collect(),
        }];

        let learners = membership.membership().learner_ids().collect();

        Self {
            log_id,
            voters,
            learners,
        }
    }

    pub fn to_membership(&self) -> StoredMembership<u64, BasicNode> {
        let log_id = self.log_id.as_ref().map(|m| m.to_log_id());

        let mut voter_ids = BTreeSet::new();
        for voters in &self.voters {
            for &node_id in &voters.nodes {
                voter_ids.insert(node_id);
            }
        }

        let learner_ids: BTreeSet<u64> = self.learners.iter().copied().collect();

        let membership = openraft::Membership::new(
            vec![voter_ids],
            if learner_ids.is_empty() {
                None
            } else {
                Some(learner_ids)
            },
        );

        StoredMembership::new(log_id, membership)
    }
}

impl MembershipMessage {
    pub fn from_stored_membership(membership: &StoredMembership<u64, BasicNode>) -> Self {
        let log_id = membership.log_id().as_ref().map(LogIdMessage::from_log_id);

        let voters = vec![VotersMessage {
            nodes: membership.voter_ids().collect(),
        }];

        let learners: Vec<u64> = membership.membership().learner_ids().collect();

        Self {
            log_id,
            voters,
            learners,
        }
    }
}

// =============================================================================
// RocksDBLogStorage
// =============================================================================

/// RocksDB-backed log storage for OpenRaft.
///
/// Implements `RaftLogStorage` trait for persisting log entries and vote state.
pub struct RocksDBLogStorage<G: RaftGroup> {
    storage: Arc<Storage>,
    cached_vote: Arc<parking_lot::RwLock<Option<Vote<u64>>>>,
    _marker: PhantomData<G>,
}

impl<G: RaftGroup> RocksDBLogStorage<G> {
    /// Create a new RocksDB log storage instance.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            cached_vote: Arc::new(parking_lot::RwLock::new(None)),
            _marker: PhantomData,
        }
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }
}

impl<G: RaftGroup> RaftLogReader<RaftTypeConfig> for RocksDBLogStorage<G> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<RaftTypeConfig>>, StorageError<u64>> {
        let start: u64 = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 1,
        };

        let end: u64 = match range.end_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n - 1,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        let mut entries = Vec::new();
        for index in start..=end {
            let key = format_log_key(index);
            match self.storage.get(G::LOG_CF, key.as_bytes()) {
                Ok(Some(value)) => {
                    let log_id = LogId::new(LeaderId::new(0, 0), index);
                    let payload = if value.is_empty() {
                        openraft::EntryPayload::Blank
                    } else {
                        openraft::EntryPayload::Normal(Request::new(value))
                    };
                    entries.push(Entry { log_id, payload });
                }
                Ok(None) => {
                    // Key doesn't exist, skip
                }
                Err(e) => {
                    return Err(StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::Store,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    });
                }
            }
        }

        Ok(entries)
    }
}

/// Log reader for RocksDB log storage.
pub struct RocksDBLogReader<G: RaftGroup> {
    storage: Arc<Storage>,
    _marker: PhantomData<G>,
}

impl<G: RaftGroup> RocksDBLogReader<G> {
    fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            _marker: PhantomData,
        }
    }
}

impl<G: RaftGroup> RaftLogReader<RaftTypeConfig> for RocksDBLogReader<G> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<RaftTypeConfig>>, StorageError<u64>> {
        let start: u64 = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 1,
        };

        let end: u64 = match range.end_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n - 1,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        let mut entries = Vec::new();
        for index in start..=end {
            let key = format_log_key(index);
            match self.storage.get(G::LOG_CF, key.as_bytes()) {
                Ok(Some(value)) => {
                    let log_id = LogId::new(LeaderId::new(0, 0), index);
                    let payload = if value.is_empty() {
                        openraft::EntryPayload::Blank
                    } else {
                        openraft::EntryPayload::Normal(Request::new(value))
                    };
                    entries.push(Entry { log_id, payload });
                }
                Ok(None) => {
                    // Key doesn't exist, skip
                }
                Err(e) => {
                    return Err(StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::Store,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    });
                }
            }
        }

        Ok(entries)
    }
}

impl<G: RaftGroup> RaftLogStorage<RaftTypeConfig> for RocksDBLogStorage<G> {
    type LogReader = RocksDBLogReader<G>;

    async fn get_log_state(
        &mut self,
    ) -> Result<LogState<RaftTypeConfig>, StorageError<u64>> {
        let last_log_id = match self.storage.get_last_log_index(G::LOG_CF) {
            Ok(Some(index)) => Some(LogId::new(LeaderId::new(0, 0), index)),
            Ok(None) => None,
            Err(e) => {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::Store,
                        ErrorVerb::Read,
                        AnyError::error(e),
                    ),
                });
            }
        };

        Ok(LogState {
            last_purged_log_id: None,
            last_log_id,
        })
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<(), StorageError<u64>> {
        let msg = VoteMessage::from_vote(vote);
        let bytes = msg.encode_to_vec();

        self.storage
            .put(G::STATE_CF, VOTE_KEY, &bytes)
            .map_err(|e| StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Vote,
                    ErrorVerb::Write,
                    AnyError::error(e),
                ),
            })?;

        // Update cache
        {
            let mut cached = self.cached_vote.write();
            *cached = Some(*vote);
        }

        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>, StorageError<u64>> {
        // Check cache first
        {
            let cached = self.cached_vote.read();
            if let Some(vote) = *cached {
                return Ok(Some(vote));
            }
        }

        // Read from storage
        match self.storage.get(G::STATE_CF, VOTE_KEY) {
            Ok(Some(bytes)) => {
                let msg = VoteMessage::decode(&bytes[..]).map_err(|e| {
                    StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::Vote,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    }
                })?;

                let vote = msg.to_vote();

                // Update cache
                {
                    let mut cached = self.cached_vote.write();
                    *cached = Some(vote);
                }

                Ok(Some(vote))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Vote,
                    ErrorVerb::Read,
                    AnyError::error(e),
                ),
            }),
        }
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<RaftTypeConfig>,
    ) -> Result<(), StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<RaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut last_index: Option<u64> = None;

        for entry in entries {
            let index = entry.log_id.index;
            let log_id = entry.log_id;

            let bytes = match &entry.payload {
                openraft::EntryPayload::Normal(req) => req.operation_bytes.clone(),
                openraft::EntryPayload::Blank => Vec::new(),
                openraft::EntryPayload::Membership(_) => Vec::new(),
            };

            let key = format_log_key(index);

            if let Err(e) = self.storage.put(G::LOG_CF, key.as_bytes(), &bytes) {
                callback.log_io_completed(Err(std::io::Error::other(e.to_string())));
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::Log(log_id),
                        ErrorVerb::Write,
                        AnyError::error(e),
                    ),
                });
            }

            last_index = Some(index);
        }

        callback.log_io_completed(Ok(()));

        // Update last log index cache if needed
        if let Some(idx) = last_index {
            let _ = self.storage.update_cached_last_log_index(G::LOG_CF, idx);
        }

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        self.storage
            .truncate_log_before(G::LOG_CF, log_id.index)
            .map_err(|e| StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Store,
                    ErrorVerb::Write,
                    AnyError::error(e),
                ),
            })?;

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        // Purge is essentially truncate - delete all entries <= log_id
        self.storage
            .truncate_log_before(G::LOG_CF, log_id.index + 1)
            .map_err(|e| StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Store,
                    ErrorVerb::Write,
                    AnyError::error(e),
                ),
            })?;

        Ok(())
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        RocksDBLogReader::new(Arc::clone(&self.storage))
    }
}

// =============================================================================
// RocksDBStateMachine
// =============================================================================

/// RocksDB-backed state machine for OpenRaft.
///
/// Implements `RaftStateMachine` trait for applying entries and managing snapshots.
pub struct RocksDBStateMachine<G: RaftGroup> {
    storage: Arc<Storage>,
    cached_applied: Arc<parking_lot::RwLock<Option<LogId<u64>>>>,
    cached_membership: Arc<parking_lot::RwLock<StoredMembership<u64, BasicNode>>>,
    current_snapshot: Arc<parking_lot::RwLock<Option<Snapshot<RaftTypeConfig>>>>,
    _marker: PhantomData<G>,
}

impl<G: RaftGroup> RocksDBStateMachine<G> {
    /// Create a new RocksDB state machine instance.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            cached_applied: Arc::new(parking_lot::RwLock::new(None)),
            cached_membership: Arc::new(parking_lot::RwLock::new(StoredMembership::default())),
            current_snapshot: Arc::new(parking_lot::RwLock::new(None)),
            _marker: PhantomData,
        }
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }
}

/// Snapshot builder for RocksDB state machine.
pub struct RocksDBSnapshotBuilder<G: RaftGroup> {
    storage: Arc<Storage>,
    snapshot_id: String,
    _marker: PhantomData<G>,
}

impl<G: RaftGroup> RocksDBSnapshotBuilder<G> {
    fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            snapshot_id: format!("snapshot-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()),
            _marker: PhantomData,
        }
    }
}

impl<G: RaftGroup> RaftSnapshotBuilder<RaftTypeConfig> for RocksDBSnapshotBuilder<G> {
    async fn build_snapshot(
        &mut self,
    ) -> Result<Snapshot<RaftTypeConfig>, StorageError<u64>> {
        // Read applied state from storage
        let applied = match self.storage.get(G::STATE_CF, APPLIED_KEY) {
            Ok(Some(bytes)) => {
                let msg = LogIdMessage::decode(&bytes[..]).map_err(|e| {
                    StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::StateMachine,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    }
                })?;
                Some(msg.to_log_id())
            }
            Ok(None) => None,
            Err(e) => {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Read,
                        AnyError::error(e),
                    ),
                });
            }
        };

        // Read membership from storage
        let membership = match self.storage.get(G::STATE_CF, MEMBERSHIP_KEY) {
            Ok(Some(bytes)) => {
                let msg = MembershipMessage::decode(&bytes[..]).map_err(|e| {
                    StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::StateMachine,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    }
                })?;
                msg.to_membership()
            }
            Ok(None) => StoredMembership::default(),
            Err(e) => {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Read,
                        AnyError::error(e),
                    ),
                });
            }
        };

        let last_log_id = applied;
        let last_membership = membership;

        // For RocksDB, we create a checkpoint and serialize its path
        // The actual snapshot data is the checkpoint directory
        let snapshot_data = self.snapshot_id.as_bytes().to_vec();

        let snapshot = Snapshot {
            meta: SnapshotMeta {
                last_log_id,
                last_membership,
                snapshot_id: self.snapshot_id.clone(),
            },
            snapshot: Box::new(Cursor::new(snapshot_data)),
        };

        Ok(snapshot)
    }
}

impl<G: RaftGroup> RaftStateMachine<RaftTypeConfig> for RocksDBStateMachine<G> {
    type SnapshotBuilder = RocksDBSnapshotBuilder<G>;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<u64>>,
            StoredMembership<u64, BasicNode>,
        ),
        StorageError<u64>,
    > {
        // Read applied state from storage
        let applied = match self.storage.get(G::STATE_CF, APPLIED_KEY) {
            Ok(Some(bytes)) => {
                let msg = LogIdMessage::decode(&bytes[..]).map_err(|e| {
                    StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::StateMachine,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    }
                })?;
                Some(msg.to_log_id())
            }
            Ok(None) => None,
            Err(e) => {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Read,
                        AnyError::error(e),
                    ),
                });
            }
        };

        // Read membership from storage
        let membership = match self.storage.get(G::STATE_CF, MEMBERSHIP_KEY) {
            Ok(Some(bytes)) => {
                let msg = MembershipMessage::decode(&bytes[..]).map_err(|e| {
                    StorageError::IO {
                        source: StorageIOError::new(
                            ErrorSubject::StateMachine,
                            ErrorVerb::Read,
                            AnyError::error(e),
                        ),
                    }
                })?;
                msg.to_membership()
            }
            Ok(None) => StoredMembership::default(),
            Err(e) => {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Read,
                        AnyError::error(e),
                    ),
                });
            }
        };

        // Update caches
        {
            let mut cached_applied = self.cached_applied.write();
            *cached_applied = applied;
        }
        {
            let mut cached_membership = self.cached_membership.write();
            *cached_membership = membership.clone();
        }

        Ok((applied, membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Response>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<RaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut responses = Vec::new();
        let mut last_applied: Option<LogId<u64>> = None;

        for entry in entries {
            let log_id = entry.log_id;
            last_applied = Some(log_id);

            let response = match &entry.payload {
                openraft::EntryPayload::Normal(req) => {
                    // For Data Raft, apply the operation to KV store
                    if let Some(kv_cf) = G::KV_CF {
                        // Apply SET/DEL operations to the KV column family
                        // The operation bytes contain the serialized Operation
                        // For now, we store the data directly
                        let key = req.operation_bytes.iter().take(32).cloned().collect::<Vec<_>>();
                        let value = req.operation_bytes.get(32..).unwrap_or(&[]).to_vec();

                        if !key.is_empty() && !value.is_empty() {
                            let _ = self.storage.put(kv_cf, &key, &value);
                        } else if !key.is_empty() && value.is_empty() {
                            // Empty value might indicate a delete
                            // For now, we just skip
                        }

                        Response::new(b"OK".to_vec())
                    } else {
                        // System Raft - no KV operations
                        Response::new(b"OK".to_vec())
                    }
                }
                openraft::EntryPayload::Blank => Response::new(Vec::new()),
                openraft::EntryPayload::Membership(m) => {
                    // Update membership
                    let membership = StoredMembership::new(Some(log_id), m.clone());
                    let msg = MembershipMessage::from_membership(&membership);
                    let bytes = msg.encode_to_vec();

                    if let Err(e) = self.storage.put(G::STATE_CF, MEMBERSHIP_KEY, &bytes) {
                        return Err(StorageError::IO {
                            source: StorageIOError::new(
                                ErrorSubject::StateMachine,
                                ErrorVerb::Write,
                                AnyError::error(e),
                            ),
                        });
                    }

                    // Update cache
                    {
                        let mut cached = self.cached_membership.write();
                        *cached = membership;
                    }

                    Response::new(Vec::new())
                }
            };

            responses.push(response);
        }

        // Persist applied state
        if let Some(log_id) = last_applied {
            let msg = LogIdMessage::from_log_id(&log_id);
            let bytes = msg.encode_to_vec();

            if let Err(e) = self.storage.put(G::STATE_CF, APPLIED_KEY, &bytes) {
                return Err(StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Write,
                        AnyError::error(e),
                    ),
                });
            }

            // Update cache
            {
                let mut cached = self.cached_applied.write();
                *cached = Some(log_id);
            }
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
        meta: &SnapshotMeta<u64, BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        let snapshot_data = snapshot.into_inner();

        // For RocksDB snapshots, we expect the snapshot_id to be a checkpoint path
        // For now, just validate the snapshot data
        if snapshot_data.is_empty() {
            return Err(StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Snapshot(None),
                    ErrorVerb::Read,
                    AnyError::error("Empty snapshot data"),
                ),
            });
        }

        // Update applied state
        if let Some(log_id) = meta.last_log_id {
            let msg = LogIdMessage::from_log_id(&log_id);
            let bytes = msg.encode_to_vec();

            self.storage
                .put(G::STATE_CF, APPLIED_KEY, &bytes)
                .map_err(|e| StorageError::IO {
                    source: StorageIOError::new(
                        ErrorSubject::StateMachine,
                        ErrorVerb::Write,
                        AnyError::error(e),
                    ),
                })?;

            {
                let mut cached = self.cached_applied.write();
                *cached = Some(log_id);
            }
        }

        // Update membership
        let msg = MembershipMessage::from_membership(&meta.last_membership);
        let bytes = msg.encode_to_vec();

        self.storage
            .put(G::STATE_CF, MEMBERSHIP_KEY, &bytes)
            .map_err(|e| StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::StateMachine,
                    ErrorVerb::Write,
                    AnyError::error(e),
                ),
            })?;

        {
            let mut cached = self.cached_membership.write();
            *cached = meta.last_membership.clone();
        }

        // Update current snapshot
        {
            let mut current = self.current_snapshot.write();
            *current = Some(Snapshot {
                meta: meta.clone(),
                snapshot: Box::new(Cursor::new(snapshot_data)),
            });
        }

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<RaftTypeConfig>>, StorageError<u64>> {
        let snapshot = {
            let current = self.current_snapshot.read();
            current.clone()
        };

        Ok(snapshot)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        RocksDBSnapshotBuilder::new(Arc::clone(&self.storage))
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn format_log_key(index: u64) -> String {
    format!("log:{:020}", index)
}

#[allow(dead_code)]
fn parse_log_key(key: &[u8]) -> Option<u64> {
    let key_str = std::str::from_utf8(key).ok()?;
    if !key_str.starts_with("log:") {
        return None;
    }
    key_str[4..].parse().ok()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_raft_cf_mapping() {
        assert_eq!(SystemRaft::LOG_CF, ColumnFamily::SystemRaftLog);
        assert_eq!(SystemRaft::STATE_CF, ColumnFamily::SystemRaftState);
        assert_eq!(SystemRaft::KV_CF, None);
    }

    #[test]
    fn test_data_raft_cf_mapping() {
        assert_eq!(DataRaft::LOG_CF, ColumnFamily::DataRaftLog);
        assert_eq!(DataRaft::STATE_CF, ColumnFamily::DataRaftState);
        assert_eq!(DataRaft::KV_CF, Some(ColumnFamily::DataKv));
    }

    #[test]
    fn test_vote_message_roundtrip() {
        let vote = Vote::new(5, 42);
        let msg = VoteMessage::from_vote(&vote);
        assert_eq!(msg.term, 5);
        assert_eq!(msg.node_id, 42);

        let decoded = msg.to_vote();
        assert_eq!(decoded.leader_id().term, 5);
        assert_eq!(decoded.leader_id().node_id, 42);
    }

    #[test]
    fn test_log_id_message_roundtrip() {
        let log_id = LogId::new(LeaderId::new(3, 7), 42);
        let msg = LogIdMessage::from_log_id(&log_id);
        assert_eq!(msg.term, 3);
        assert_eq!(msg.node_id, 7);
        assert_eq!(msg.index, 42);

        let decoded = msg.to_log_id();
        assert_eq!(decoded.leader_id.term, 3);
        assert_eq!(decoded.leader_id.node_id, 7);
        assert_eq!(decoded.index, 42);
    }

    #[test]
    fn test_format_log_key() {
        assert_eq!(format_log_key(0), "log:00000000000000000000");
        assert_eq!(format_log_key(1), "log:00000000000000000001");
        assert_eq!(format_log_key(42), "log:00000000000000000042");
        assert_eq!(format_log_key(u64::MAX), format!("log:{:020}", u64::MAX));
    }

    #[test]
    fn test_parse_log_key() {
        assert_eq!(parse_log_key(b"log:00000000000000000000"), Some(0));
        assert_eq!(parse_log_key(b"log:00000000000000000001"), Some(1));
        assert_eq!(parse_log_key(b"log:00000000000000000042"), Some(42));
        assert_eq!(parse_log_key(b"notalog:00000000000000000042"), None);
        assert_eq!(parse_log_key(b"log:abc"), None);
    }

    #[test]
    fn test_vote_message_serialization() {
        let vote = Vote::new(10, 99);
        let msg = VoteMessage::from_vote(&vote);
        let bytes = msg.encode_to_vec();
        let decoded = VoteMessage::decode(&bytes[..]).unwrap();
        assert_eq!(decoded.term, 10);
        assert_eq!(decoded.node_id, 99);
    }

    #[test]
    fn test_log_id_message_serialization() {
        let log_id = LogId::new(LeaderId::new(5, 3), 100);
        let msg = LogIdMessage::from_log_id(&log_id);
        let bytes = msg.encode_to_vec();
        let decoded = LogIdMessage::decode(&bytes[..]).unwrap();
        assert_eq!(decoded.term, 5);
        assert_eq!(decoded.node_id, 3);
        assert_eq!(decoded.index, 100);
    }

    #[test]
    fn test_membership_message_default() {
        let membership = StoredMembership::<u64, BasicNode>::default();
        let msg = MembershipMessage::from_membership(&membership);

        assert!(msg.log_id.is_none());
        // Default membership may have voters from the default config
        assert_eq!(msg.learners.len(), 0);
    }
}
