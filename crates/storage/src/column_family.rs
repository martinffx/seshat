//! Column Family definitions for RocksDB storage.
//!
//! This module defines the 6 column families used in Seshat's storage layer,
//! providing type-safe access and metadata about each column family.

/// Type-safe enum for RocksDB column families.
///
/// Seshat uses 6 column families to organize data:
///
/// **System Group (shard_id = 0)**:
/// - `SystemRaftLog` - Raft log entries for system shard
/// - `SystemRaftState` - Raft hard state (term, vote, commit) - requires fsync
/// - `SystemData` - Cluster metadata (membership, shard map)
///
/// **Data Shards (shard_id > 0)**:
/// - `DataRaftLog` - Raft log entries for data shards
/// - `DataRaftState` - Raft hard state for data shards - requires fsync
/// - `DataKv` - User key-value data
///
/// # Design Rationale
///
/// Using an enum instead of strings prevents typos at compile time and enables
/// column-family-specific behavior (e.g., fsync requirements, optimization profiles).
///
/// # Examples
///
/// ```
/// use seshat_storage::ColumnFamily;
///
/// let cf = ColumnFamily::SystemRaftLog;
/// assert_eq!(cf.as_str(), "system_raft_log");
/// assert!(cf.is_log_cf());
/// assert!(!cf.requires_fsync());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnFamily {
    /// System group Raft log entries.
    ///
    /// Contains Raft log entries for the system shard (shard_id = 0).
    /// System shard manages cluster membership and shard mapping.
    SystemRaftLog,

    /// System group Raft hard state.
    ///
    /// Contains Raft hard state (term, vote, commit index) for system shard.
    /// **Requires fsync** on every write for durability guarantees.
    SystemRaftState,

    /// System group cluster metadata.
    ///
    /// Contains cluster metadata like membership configuration and shard map.
    /// This is the state machine data for the system shard.
    SystemData,

    /// Data shard Raft log entries.
    ///
    /// Contains Raft log entries for all data shards (shard_id > 0).
    /// Each data shard handles a portion of the user key space.
    DataRaftLog,

    /// Data shard Raft hard state.
    ///
    /// Contains Raft hard state (term, vote, commit index) for data shards.
    /// **Requires fsync** on every write for durability guarantees.
    DataRaftState,

    /// User key-value data.
    ///
    /// Contains the actual user data stored in the key-value store.
    /// This is the state machine data for data shards.
    DataKv,
}

impl ColumnFamily {
    /// Returns the RocksDB column family name as a string.
    ///
    /// Column family names use snake_case convention for RocksDB compatibility.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::ColumnFamily;
    ///
    /// assert_eq!(ColumnFamily::SystemRaftLog.as_str(), "system_raft_log");
    /// assert_eq!(ColumnFamily::DataKv.as_str(), "data_kv");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            ColumnFamily::SystemRaftLog => "system_raft_log",
            ColumnFamily::SystemRaftState => "system_raft_state",
            ColumnFamily::SystemData => "system_data",
            ColumnFamily::DataRaftLog => "data_raft_log",
            ColumnFamily::DataRaftState => "data_raft_state",
            ColumnFamily::DataKv => "data_kv",
        }
    }

    /// Returns an array containing all column family variants.
    ///
    /// This is useful for initializing RocksDB with all required column families.
    /// The order is consistent across calls.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::ColumnFamily;
    ///
    /// let all_cfs = ColumnFamily::all();
    /// assert_eq!(all_cfs.len(), 6);
    /// ```
    pub fn all() -> [ColumnFamily; 6] {
        [
            ColumnFamily::SystemRaftLog,
            ColumnFamily::SystemRaftState,
            ColumnFamily::SystemData,
            ColumnFamily::DataRaftLog,
            ColumnFamily::DataRaftState,
            ColumnFamily::DataKv,
        ]
    }

    /// Returns true if writes to this column family require immediate fsync.
    ///
    /// Only Raft hard state column families (`*_raft_state`) require fsync
    /// to ensure durability guarantees. Other column families use async WAL.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::ColumnFamily;
    ///
    /// assert!(ColumnFamily::SystemRaftState.requires_fsync());
    /// assert!(ColumnFamily::DataRaftState.requires_fsync());
    /// assert!(!ColumnFamily::SystemRaftLog.requires_fsync());
    /// assert!(!ColumnFamily::DataKv.requires_fsync());
    /// ```
    pub fn requires_fsync(&self) -> bool {
        matches!(
            self,
            ColumnFamily::SystemRaftState | ColumnFamily::DataRaftState
        )
    }

    /// Returns true if this is a Raft log column family.
    ///
    /// Raft log column families (`*_raft_log`) store sequential log entries
    /// and support log-specific operations like append and truncate.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_storage::ColumnFamily;
    ///
    /// assert!(ColumnFamily::SystemRaftLog.is_log_cf());
    /// assert!(ColumnFamily::DataRaftLog.is_log_cf());
    /// assert!(!ColumnFamily::SystemRaftState.is_log_cf());
    /// assert!(!ColumnFamily::DataKv.is_log_cf());
    /// ```
    pub fn is_log_cf(&self) -> bool {
        matches!(
            self,
            ColumnFamily::SystemRaftLog | ColumnFamily::DataRaftLog
        )
    }

    // Note: default_options() will be implemented in ROCKS-003 when CFOptions is created
}
