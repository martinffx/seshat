//! Common type aliases used throughout Seshat.
//!
//! This module defines fundamental type aliases for Raft consensus
//! and cluster management. Using type aliases provides semantic clarity
//! and makes it easier to change underlying types in the future if needed.

/// Unique identifier for a node in the cluster.
///
/// Each node in the Seshat cluster has a unique `NodeId` assigned during
/// cluster formation. Node IDs must be greater than 0 and are used throughout
/// the system for:
/// - Raft consensus voting and leadership
/// - Cluster membership tracking
/// - Shard replica assignment
///
/// # Examples
///
/// ```
/// use seshat_common::NodeId;
///
/// let node_id: NodeId = 1;
/// assert!(node_id > 0);
/// ```
pub type NodeId = u64;

/// Raft term number.
///
/// In Raft consensus, time is divided into terms of arbitrary length.
/// Terms are numbered with consecutive integers and act as a logical clock.
/// Each term begins with an election, and at most one leader can be elected
/// per term.
///
/// Terms are used to:
/// - Detect stale information (lower term numbers)
/// - Ensure safety during leader elections
/// - Maintain consistency across log replication
///
/// # Examples
///
/// ```
/// use seshat_common::Term;
///
/// let current_term: Term = 5;
/// let next_term: Term = current_term + 1;
/// assert_eq!(next_term, 6);
/// ```
pub type Term = u64;

/// Index into the Raft log.
///
/// Each entry in the Raft log is identified by a unique `LogIndex`.
/// Log indices start at 1 (not 0) and increase monotonically.
/// The log index combined with the term uniquely identifies a log entry.
///
/// Log indices are used for:
/// - Tracking which entries have been committed
/// - Identifying the last applied entry
/// - Log compaction and snapshot coordination
///
/// # Examples
///
/// ```
/// use seshat_common::LogIndex;
///
/// let last_applied: LogIndex = 100;
/// let commit_index: LogIndex = 120;
/// assert!(commit_index >= last_applied);
/// ```
pub type LogIndex = u64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_basic_operations() {
        // NodeId can be created and compared
        let node1: NodeId = 1;
        let node2: NodeId = 2;
        let node1_copy: NodeId = 1;

        assert_eq!(node1, node1_copy);
        assert_ne!(node1, node2);
        assert!(node2 > node1);
    }

    #[test]
    fn test_node_id_arithmetic() {
        // NodeId supports basic arithmetic (though rarely used)
        let node_id: NodeId = 5;
        let next_id = node_id + 1;
        assert_eq!(next_id, 6);
    }

    #[test]
    fn test_term_ordering() {
        // Terms can be compared to detect stale information
        let old_term: Term = 3;
        let current_term: Term = 5;
        let future_term: Term = 7;

        assert!(old_term < current_term);
        assert!(current_term < future_term);
        assert!(old_term < future_term);
    }

    #[test]
    fn test_term_increment() {
        // Terms increment during elections
        let mut term: Term = 1;
        term += 1;
        assert_eq!(term, 2);

        term += 1;
        assert_eq!(term, 3);
    }

    #[test]
    fn test_log_index_sequence() {
        // Log indices form a monotonic sequence
        let indices: Vec<LogIndex> = vec![1, 2, 3, 4, 5];

        for i in 1..indices.len() {
            assert!(indices[i] > indices[i - 1]);
            assert_eq!(indices[i], indices[i - 1] + 1);
        }
    }

    #[test]
    fn test_log_index_range_check() {
        // Common pattern: checking if an index is within committed range
        let last_applied: LogIndex = 100;
        let commit_index: LogIndex = 120;
        let test_index: LogIndex = 110;

        assert!(test_index >= last_applied);
        assert!(test_index <= commit_index);
    }

    #[test]
    fn test_types_are_distinct_semantically() {
        // While all three types are u64, they represent different concepts
        let node: NodeId = 1;
        let term: Term = 1;
        let index: LogIndex = 1;

        // They have the same value but different semantic meanings
        assert_eq!(node, 1);
        assert_eq!(term, 1);
        assert_eq!(index, 1);
    }

    #[test]
    fn test_type_aliases_are_copy() {
        // All types should be Copy since they're u64
        let node1: NodeId = 5;
        let node2 = node1; // Copy, not move
        assert_eq!(node1, node2);

        let term1: Term = 3;
        let term2 = term1;
        assert_eq!(term1, term2);

        let index1: LogIndex = 100;
        let index2 = index1;
        assert_eq!(index1, index2);
    }

    #[test]
    fn test_zero_values() {
        // Test edge case: zero values (though NodeId should be > 0 in practice)
        let zero_node: NodeId = 0;
        let zero_term: Term = 0;
        let zero_index: LogIndex = 0;

        assert_eq!(zero_node, 0);
        assert_eq!(zero_term, 0);
        assert_eq!(zero_index, 0);
    }

    #[test]
    fn test_max_values() {
        // Test that types can hold maximum u64 values
        let max_node: NodeId = u64::MAX;
        let max_term: Term = u64::MAX;
        let max_index: LogIndex = u64::MAX;

        assert_eq!(max_node, u64::MAX);
        assert_eq!(max_term, u64::MAX);
        assert_eq!(max_index, u64::MAX);
    }
}
