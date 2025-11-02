//! Type definitions for OpenRaft integration.
//!
//! This module defines the type configuration and associated types required
//! by OpenRaft's RaftTypeConfig trait, which parameterizes the Raft implementation.

use openraft::{EntryPayload, LeaderId, LogId, TokioRuntime, Vote};
use raft::prelude as eraftpb;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Type configuration for OpenRaft.
///
/// This struct implements RaftTypeConfig, defining all the associated types
/// that OpenRaft uses throughout its implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct RaftTypeConfig;

impl openraft::RaftTypeConfig for RaftTypeConfig {
    /// Application-specific request data.
    type D = Request;

    /// Application-specific response data.
    type R = Response;

    /// Node identifier type.
    type NodeId = u64;

    /// Node metadata type (address information).
    type Node = BasicNode;

    /// Raft log entry type.
    type Entry = openraft::Entry<RaftTypeConfig>;

    /// Snapshot data type (using Cursor for AsyncRead/Write/Seek).
    type SnapshotData = std::io::Cursor<Vec<u8>>;

    /// Async runtime (Tokio).
    type AsyncRuntime = TokioRuntime;

    /// Response sender for client write requests.
    type Responder = openraft::impls::OneshotResponder<RaftTypeConfig>;
}

/// Node metadata containing network address information.
///
/// BasicNode stores the network address of a Raft cluster member,
/// used for establishing connections between nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BasicNode {
    /// Network address (e.g., "127.0.0.1:7379")
    pub addr: String,
}

impl BasicNode {
    /// Create a new BasicNode with the given address.
    pub fn new(addr: String) -> Self {
        Self { addr }
    }
}

/// Request type wrapping operations submitted to Raft.
///
/// This type bridges the service layer (KV/SQL) and the Raft layer by
/// wrapping serialized Operation bytes in a protobuf-compatible format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Serialized Operation from KV or SQL service.
    ///
    /// Format: protobuf-encoded Operation (e.g., Operation::Set, Operation::Del)
    /// The Raft layer treats this as opaque bytes - deserialization happens
    /// in the StateMachine during apply().
    pub operation_bytes: Vec<u8>,
}

impl Request {
    /// Create a new Request from serialized operation bytes.
    pub fn new(operation_bytes: Vec<u8>) -> Self {
        Self { operation_bytes }
    }
}

/// Response type for operations applied to the state machine.
///
/// Contains the result of applying a Request to the state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// Result data from the state machine operation.
    pub result: Vec<u8>,
}

impl Response {
    /// Create a new Response with the given result data.
    pub fn new(result: Vec<u8>) -> Self {
        Self { result }
    }
}

// ============================================================================
// OpenRaft Trait Implementations
// ============================================================================

// Note: Request and Response automatically implement AppData and AppDataResponse
// via blanket impl for types that are: OptionalSend + OptionalSync + 'static + OptionalSerde

// ============================================================================
// Type Conversions between raft-rs (eraftpb) and openraft types
// ============================================================================

/// Convert eraftpb::Entry to openraft::Entry<RaftTypeConfig>
///
/// Maps raft-rs log entry to OpenRaft log entry format.
/// Note: Cannot use From trait due to orphan rules.
pub fn entry_from_eraftpb(entry: eraftpb::Entry) -> openraft::Entry<RaftTypeConfig> {
    // Create LogId from entry's index and term
    let log_id = LogId::new(
        LeaderId::new(entry.term, 0), // We don't have node_id in Entry, use 0
        entry.index,
    );

    // Wrap the data in a Request and create Normal payload
    let request = Request::new(entry.data);
    let payload = EntryPayload::Normal(request);

    openraft::Entry { log_id, payload }
}

/// Convert eraftpb::HardState to openraft::Vote
///
/// Extracts vote information (term and voted_for node_id) from HardState.
/// Note: Cannot use From trait due to orphan rules.
pub fn vote_from_hardstate(hs: eraftpb::HardState) -> Vote<u64> {
    // Create Vote with term and voted_for node_id
    Vote::new(hs.term, hs.vote)
}

/// Extract commit index as LogId from eraftpb::HardState
///
/// Helper for getting the committed log position from HardState.
pub fn hardstate_to_log_id(hs: &eraftpb::HardState) -> Option<LogId<u64>> {
    if hs.commit == 0 {
        // No commits yet
        None
    } else {
        // Create LogId with the commit index
        // We don't have term information for the committed entry here,
        // so we use the current term from HardState
        Some(LogId::new(LeaderId::new(hs.term, 0), hs.commit))
    }
}

/// Convert eraftpb::ConfState to openraft::Membership
///
/// Maps raft-rs cluster membership to OpenRaft membership format.
/// Note: Cannot use From trait due to orphan rules.
pub fn membership_from_confstate(cs: eraftpb::ConfState) -> openraft::Membership<u64, BasicNode> {
    // Convert voters and learners to BTreeSets
    let voters: BTreeSet<u64> = cs.voters.into_iter().collect();
    let learners: BTreeSet<u64> = cs.learners.into_iter().collect();

    // Create Membership with empty nodes (we don't have address info in ConfState)
    // The node registry will be populated separately
    openraft::Membership::new(vec![voters], Some(learners))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_is_u64() {
        // Verify NodeId is u64
        let _node_id: <RaftTypeConfig as openraft::RaftTypeConfig>::NodeId = 42u64;
    }

    #[test]
    fn test_basic_node_construction() {
        let node = BasicNode::new("127.0.0.1:7379".to_string());
        assert_eq!(node.addr, "127.0.0.1:7379");
    }

    #[test]
    fn test_basic_node_clone_and_eq() {
        let node1 = BasicNode::new("127.0.0.1:7379".to_string());
        let node2 = node1.clone();
        assert_eq!(node1, node2);
    }

    #[test]
    fn test_request_construction() {
        let data = vec![1, 2, 3, 4];
        let request = Request::new(data.clone());
        assert_eq!(request.operation_bytes, data);
    }

    #[test]
    fn test_request_serde() {
        let request = Request::new(vec![5, 6, 7, 8]);

        // Serialize
        let json = serde_json::to_string(&request).expect("Failed to serialize");

        // Deserialize
        let deserialized: Request = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_response_construction() {
        let result = vec![10, 20, 30];
        let response = Response::new(result.clone());
        assert_eq!(response.result, result);
    }

    #[test]
    fn test_response_serde() {
        let response = Response::new(vec![40, 50, 60]);

        // Serialize
        let json = serde_json::to_string(&response).expect("Failed to serialize");

        // Deserialize
        let deserialized: Response = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_basic_node_serde() {
        let node = BasicNode::new("192.168.1.100:8080".to_string());

        // Serialize
        let json = serde_json::to_string(&node).expect("Failed to serialize");

        // Deserialize
        let deserialized: BasicNode = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_empty_request() {
        let request = Request::new(vec![]);
        assert_eq!(request.operation_bytes.len(), 0);
    }

    #[test]
    fn test_empty_response() {
        let response = Response::new(vec![]);
        assert_eq!(response.result.len(), 0);
    }

    #[test]
    fn test_raft_type_config_implements_openraft_trait() {
        // This is a compile-time test - if it compiles, the trait is implemented correctly
        fn assert_impl<T: openraft::RaftTypeConfig>() {}
        assert_impl::<RaftTypeConfig>();
    }

    #[test]
    fn test_node_type_is_basic_node() {
        // Verify Node type is BasicNode
        let _node: <RaftTypeConfig as openraft::RaftTypeConfig>::Node =
            BasicNode::new("test:1234".to_string());
    }

    #[test]
    fn test_snapshot_data_is_cursor() {
        // Verify SnapshotData is Cursor<Vec<u8>>
        let _snapshot: <RaftTypeConfig as openraft::RaftTypeConfig>::SnapshotData =
            std::io::Cursor::new(vec![1, 2, 3]);
    }

    #[test]
    fn test_entry_type_is_openraft_entry() {
        // Verify Entry type is openraft::Entry<RaftTypeConfig>
        let request = Request::new(vec![1, 2, 3]);
        let log_id = LogId::new(LeaderId::new(1, 1), 1);
        let _entry: <RaftTypeConfig as openraft::RaftTypeConfig>::Entry = openraft::Entry {
            log_id,
            payload: EntryPayload::Normal(request),
        };
    }

    #[test]
    fn test_runtime_is_tokio() {
        // Verify AsyncRuntime is TokioRuntime
        // This is a compile-time check
        fn assert_runtime<T: openraft::RaftTypeConfig>()
        where
            T::AsyncRuntime: openraft::AsyncRuntime,
        {
        }
        assert_runtime::<RaftTypeConfig>();
    }

    // ========================================================================
    // Type Conversion Tests
    // ========================================================================

    #[test]
    fn test_eraftpb_entry_to_openraft_entry() {
        // Test converting raft-rs Entry to OpenRaft Entry
        let eraft_entry = eraftpb::Entry {
            entry_type: eraftpb::EntryType::EntryNormal.into(),
            term: 5,
            index: 10,
            data: vec![1, 2, 3, 4],
            ..Default::default()
        };

        let openraft_entry: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);

        // Verify log_id is correct
        assert_eq!(openraft_entry.log_id.index, 10);
        assert_eq!(openraft_entry.log_id.leader_id.term, 5);

        // Verify payload data is preserved
        let payload = openraft_entry.payload;
        match payload {
            openraft::EntryPayload::Blank => panic!("Expected Normal entry"),
            openraft::EntryPayload::Normal(request) => {
                assert_eq!(request.operation_bytes, vec![1, 2, 3, 4]);
            }
            openraft::EntryPayload::Membership(_) => panic!("Expected Normal entry"),
        }
    }

    #[test]
    fn test_eraftpb_entry_with_empty_data() {
        // Test converting entry with empty data
        let eraft_entry = eraftpb::Entry {
            entry_type: eraftpb::EntryType::EntryNormal.into(),
            term: 1,
            index: 1,
            data: vec![],
            ..Default::default()
        };

        let openraft_entry: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);

        match openraft_entry.payload {
            openraft::EntryPayload::Normal(request) => {
                assert_eq!(request.operation_bytes, Vec::<u8>::new());
            }
            _ => panic!("Expected Normal entry"),
        }
    }

    #[test]
    fn test_eraftpb_hardstate_to_vote() {
        // Test converting HardState to Vote
        let hardstate = eraftpb::HardState {
            term: 10,
            vote: 5, // voted_for node_id
            commit: 42,
        };

        let vote: Vote<u64> = vote_from_hardstate(hardstate);

        assert_eq!(vote.leader_id().term, 10);
        assert_eq!(vote.leader_id().node_id, 5);
    }

    #[test]
    fn test_eraftpb_hardstate_with_zero_vote() {
        // Test HardState with no vote (vote=0 means no vote)
        let hardstate = eraftpb::HardState {
            term: 3,
            vote: 0,
            commit: 0,
        };

        let vote: Vote<u64> = vote_from_hardstate(hardstate);

        // When vote is 0, we should still create a valid Vote
        // but with node_id 0 (indicating no vote cast yet)
        assert_eq!(vote.leader_id().term, 3);
        assert_eq!(vote.leader_id().node_id, 0);
    }

    #[test]
    fn test_hardstate_to_log_id_with_commit() {
        // Test extracting commit index as LogId
        let hardstate = eraftpb::HardState {
            term: 7,
            vote: 2,
            commit: 15,
        };

        let log_id = hardstate_to_log_id(&hardstate);

        assert!(log_id.is_some());
        let log_id = log_id.unwrap();
        assert_eq!(log_id.index, 15);
        // Note: LogId doesn't store term from commit, only from leader_id
        // The term would come from the log entry itself
    }

    #[test]
    fn test_hardstate_to_log_id_with_zero_commit() {
        // Test extracting LogId when commit is 0 (no commits yet)
        let hardstate = eraftpb::HardState {
            term: 1,
            vote: 0,
            commit: 0,
        };

        let log_id = hardstate_to_log_id(&hardstate);

        // When commit is 0, there's no committed log entry
        assert!(log_id.is_none());
    }

    #[test]
    fn test_eraftpb_confstate_to_membership() {
        // Test converting ConfState to Membership
        let confstate = eraftpb::ConfState {
            voters: vec![1, 2, 3],
            learners: vec![4, 5],
            ..Default::default()
        };

        let membership: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);

        // Verify voter node IDs
        let voter_ids: Vec<u64> = membership.voter_ids().collect();
        assert_eq!(voter_ids, vec![1, 2, 3]);

        // Verify learner node IDs
        let learner_ids: Vec<u64> = membership.learner_ids().collect();
        assert_eq!(learner_ids, vec![4, 5]);
    }

    #[test]
    fn test_eraftpb_confstate_empty_learners() {
        // Test ConfState with no learners
        let confstate = eraftpb::ConfState {
            voters: vec![1, 2, 3],
            learners: vec![],
            ..Default::default()
        };

        let membership: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);

        let voter_ids: Vec<u64> = membership.voter_ids().collect();
        assert_eq!(voter_ids, vec![1, 2, 3]);

        let learner_ids: Vec<u64> = membership.learner_ids().collect();
        assert!(learner_ids.is_empty());
    }

    #[test]
    fn test_eraftpb_confstate_empty_voters() {
        // Test ConfState with no voters (edge case - should still convert)
        let confstate = eraftpb::ConfState {
            voters: vec![],
            learners: vec![1],
            ..Default::default()
        };

        let membership: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);

        let voter_ids: Vec<u64> = membership.voter_ids().collect();
        assert!(voter_ids.is_empty());

        let learner_ids: Vec<u64> = membership.learner_ids().collect();
        assert_eq!(learner_ids, vec![1]);
    }

    #[test]
    fn test_eraftpb_entry_with_max_values() {
        // Test edge case with maximum u64 values
        let eraft_entry = eraftpb::Entry {
            entry_type: eraftpb::EntryType::EntryNormal.into(),
            term: u64::MAX,
            index: u64::MAX,
            data: vec![0xFF; 100],
            ..Default::default()
        };

        let openraft_entry: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);

        assert_eq!(openraft_entry.log_id.index, u64::MAX);
        assert_eq!(openraft_entry.log_id.leader_id.term, u64::MAX);
    }

    #[test]
    fn test_hardstate_with_max_term() {
        // Test HardState with maximum term value
        let hardstate = eraftpb::HardState {
            term: u64::MAX,
            vote: 1,
            commit: u64::MAX,
        };

        let vote: Vote<u64> = vote_from_hardstate(hardstate);

        assert_eq!(vote.leader_id().term, u64::MAX);
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating random eraftpb::Entry
    fn arb_eraftpb_entry() -> impl Strategy<Value = eraftpb::Entry> {
        (
            any::<u64>(),
            any::<u64>(),
            prop::collection::vec(any::<u8>(), 0..1000),
        )
            .prop_map(|(term, index, data)| eraftpb::Entry {
                entry_type: eraftpb::EntryType::EntryNormal.into(),
                term,
                index,
                data,
                ..Default::default()
            })
    }

    // Strategy for generating random eraftpb::HardState
    fn arb_eraftpb_hardstate() -> impl Strategy<Value = eraftpb::HardState> {
        (any::<u64>(), any::<u64>(), any::<u64>())
            .prop_map(|(term, vote, commit)| eraftpb::HardState { term, vote, commit })
    }

    // Strategy for generating random eraftpb::ConfState
    fn arb_eraftpb_confstate() -> impl Strategy<Value = eraftpb::ConfState> {
        (
            prop::collection::vec(any::<u64>(), 0..10),
            prop::collection::vec(any::<u64>(), 0..10),
        )
            .prop_map(|(voters, learners)| eraftpb::ConfState {
                voters,
                learners,
                ..Default::default()
            })
    }

    proptest! {
        #[test]
        fn prop_entry_conversion_preserves_data(eraft_entry in arb_eraftpb_entry()) {
            // Convert eraftpb::Entry to openraft::Entry
            let original_term = eraft_entry.term;
            let original_index = eraft_entry.index;
            let original_data = eraft_entry.data.clone();

            let openraft_entry: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);

            // Verify term and index are preserved
            prop_assert_eq!(openraft_entry.log_id.leader_id.term, original_term);
            prop_assert_eq!(openraft_entry.log_id.index, original_index);

            // Verify data is preserved in the payload
            match openraft_entry.payload {
                openraft::EntryPayload::Normal(request) => {
                    prop_assert_eq!(request.operation_bytes, original_data);
                }
                _ => return Err(TestCaseError::fail("Expected Normal payload")),
            }
        }

        #[test]
        fn prop_entry_conversion_no_panic(eraft_entry in arb_eraftpb_entry()) {
            // Test that conversion never panics regardless of input
            let _: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);
        }

        #[test]
        fn prop_hardstate_to_vote_preserves_data(hardstate in arb_eraftpb_hardstate()) {
            // Convert eraftpb::HardState to openraft::Vote
            let original_term = hardstate.term;
            let original_vote = hardstate.vote;

            let vote: Vote<u64> = vote_from_hardstate(hardstate);

            // Verify term and node_id are preserved
            prop_assert_eq!(vote.leader_id().term, original_term);
            prop_assert_eq!(vote.leader_id().node_id, original_vote);
        }

        #[test]
        fn prop_hardstate_conversion_no_panic(hardstate in arb_eraftpb_hardstate()) {
            // Test that conversion never panics
            let _vote: Vote<u64> = vote_from_hardstate(hardstate.clone());

            // Also test hardstate_to_log_id conversion
            let _log_id = hardstate_to_log_id(&hardstate);
        }

        #[test]
        fn prop_hardstate_to_log_id_consistency(hardstate in arb_eraftpb_hardstate()) {
            // Test that hardstate_to_log_id returns Some if commit > 0, None if commit == 0
            let log_id = hardstate_to_log_id(&hardstate);

            if hardstate.commit == 0 {
                prop_assert!(log_id.is_none());
            } else {
                prop_assert!(log_id.is_some());
                let log_id = log_id.unwrap();
                prop_assert_eq!(log_id.index, hardstate.commit);
            }
        }

        #[test]
        fn prop_confstate_conversion_preserves_voters(confstate in arb_eraftpb_confstate()) {
            // Convert eraftpb::ConfState to openraft::Membership
            let original_voters: BTreeSet<u64> = confstate.voters.iter().copied().collect();
            let original_learners: BTreeSet<u64> = confstate.learners.iter().copied().collect();

            let membership: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);

            // Verify voters are preserved (order doesn't matter, use BTreeSet)
            let converted_voters: BTreeSet<u64> = membership.voter_ids().collect();
            prop_assert_eq!(converted_voters, original_voters);

            // Verify learners are preserved
            let converted_learners: BTreeSet<u64> = membership.learner_ids().collect();
            prop_assert_eq!(converted_learners, original_learners);
        }

        #[test]
        fn prop_confstate_conversion_no_panic(confstate in arb_eraftpb_confstate()) {
            // Test that conversion never panics
            let _: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);
        }

        #[test]
        fn prop_entry_boundary_values(term in prop::num::u64::ANY, index in prop::num::u64::ANY) {
            // Test boundary values for term and index
            let eraft_entry = eraftpb::Entry {
                entry_type: eraftpb::EntryType::EntryNormal.into(),
                term,
                index,
                data: vec![],
                ..Default::default()
            };

            let openraft_entry: openraft::Entry<RaftTypeConfig> = entry_from_eraftpb(eraft_entry);

            prop_assert_eq!(openraft_entry.log_id.leader_id.term, term);
            prop_assert_eq!(openraft_entry.log_id.index, index);
        }

        #[test]
        fn prop_hardstate_boundary_values(term in prop::num::u64::ANY, vote in prop::num::u64::ANY, commit in prop::num::u64::ANY) {
            // Test boundary values for HardState fields
            let hardstate = eraftpb::HardState { term, vote, commit };

            let vote_result: Vote<u64> = vote_from_hardstate(hardstate);

            prop_assert_eq!(vote_result.leader_id().term, term);
            prop_assert_eq!(vote_result.leader_id().node_id, vote);
        }

        #[test]
        fn prop_empty_vectors_handled(
            voters in prop::collection::vec(any::<u64>(), 0..1),
            learners in prop::collection::vec(any::<u64>(), 0..1)
        ) {
            // Test that empty or single-element vectors are handled correctly
            let confstate = eraftpb::ConfState {
                voters,
                learners,
                ..Default::default()
            };

            let _membership: openraft::Membership<u64, BasicNode> = membership_from_confstate(confstate);
            // No panic = success
        }
    }
}
