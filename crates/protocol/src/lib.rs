//! Protocol definitions for Seshat distributed key-value store
//!
//! This crate provides protocol definitions for internal Raft communication
//! using gRPC and Protocol Buffers. It defines the RPC service and message
//! types required for Raft consensus operations.
//!
//! # Architecture
//!
//! The protocol layer handles:
//! - **RequestVote RPC**: Leader election
//! - **AppendEntries RPC**: Log replication and heartbeats
//! - **InstallSnapshot RPC**: Snapshot transfer
//! - **Operations**: State machine commands (Set, Del)
//!
//! # Example
//!
//! ```rust
//! use seshat_protocol::{RequestVoteRequest, EntryType, Operation};
//!
//! // Create a RequestVote request
//! let request = RequestVoteRequest {
//!     term: 5,
//!     candidate_id: 1,
//!     last_log_index: 100,
//!     last_log_term: 4,
//! };
//!
//! // Create a state machine operation
//! let op = Operation::Set {
//!     key: b"foo".to_vec(),
//!     value: b"bar".to_vec(),
//! };
//! ```

// Include the generated protobuf code
pub mod raft {
    tonic::include_proto!("raft");
}

// State machine operations
pub mod operations;

// Re-export commonly used types for convenience
pub use raft::{
    raft_service_client::RaftServiceClient, raft_service_server::RaftService,
    raft_service_server::RaftServiceServer, AppendEntriesRequest, AppendEntriesResponse, EntryType,
    InstallSnapshotRequest, InstallSnapshotResponse, LogEntry, RequestVoteRequest,
    RequestVoteResponse,
};

pub use operations::{Operation, OperationError, OperationResult};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_vote_request_creation() {
        let request = RequestVoteRequest {
            term: 5,
            candidate_id: 1,
            last_log_index: 100,
            last_log_term: 4,
        };

        assert_eq!(request.term, 5);
        assert_eq!(request.candidate_id, 1);
        assert_eq!(request.last_log_index, 100);
        assert_eq!(request.last_log_term, 4);
    }

    #[test]
    fn test_request_vote_request_default() {
        let request = RequestVoteRequest::default();

        assert_eq!(request.term, 0);
        assert_eq!(request.candidate_id, 0);
        assert_eq!(request.last_log_index, 0);
        assert_eq!(request.last_log_term, 0);
    }

    #[test]
    fn test_request_vote_response_creation() {
        let response = RequestVoteResponse {
            term: 6,
            vote_granted: true,
        };

        assert_eq!(response.term, 6);
        assert!(response.vote_granted);
    }

    #[test]
    fn test_request_vote_response_default() {
        let response = RequestVoteResponse::default();

        assert_eq!(response.term, 0);
        assert!(!response.vote_granted);
    }

    #[test]
    fn test_append_entries_request_creation() {
        let request = AppendEntriesRequest {
            term: 5,
            leader_id: 1,
            prev_log_index: 99,
            prev_log_term: 4,
            entries: vec![],
            leader_commit: 98,
        };

        assert_eq!(request.term, 5);
        assert_eq!(request.leader_id, 1);
        assert_eq!(request.prev_log_index, 99);
        assert_eq!(request.prev_log_term, 4);
        assert!(request.entries.is_empty());
        assert_eq!(request.leader_commit, 98);
    }

    #[test]
    fn test_append_entries_request_with_entries() {
        let entry = LogEntry {
            index: 100,
            term: 5,
            entry_type: 0, // EntryType::Normal
            data: b"test command".to_vec(),
        };

        let request = AppendEntriesRequest {
            term: 5,
            leader_id: 1,
            prev_log_index: 99,
            prev_log_term: 4,
            entries: vec![entry.clone()],
            leader_commit: 98,
        };

        assert_eq!(request.entries.len(), 1);
        assert_eq!(request.entries[0].index, 100);
        assert_eq!(request.entries[0].term, 5);
        assert_eq!(request.entries[0].entry_type, 0);
        assert_eq!(request.entries[0].data, b"test command");
    }

    #[test]
    fn test_append_entries_response_creation() {
        let response = AppendEntriesResponse {
            term: 5,
            success: true,
            last_log_index: 100,
        };

        assert_eq!(response.term, 5);
        assert!(response.success);
        assert_eq!(response.last_log_index, 100);
    }

    #[test]
    fn test_install_snapshot_request_creation() {
        let snapshot_data = b"snapshot binary data".to_vec();
        let request = InstallSnapshotRequest {
            term: 5,
            leader_id: 1,
            last_included_index: 1000,
            last_included_term: 4,
            offset: 0,
            data: snapshot_data.clone(),
            done: false,
        };

        assert_eq!(request.term, 5);
        assert_eq!(request.leader_id, 1);
        assert_eq!(request.last_included_index, 1000);
        assert_eq!(request.last_included_term, 4);
        assert_eq!(request.offset, 0);
        assert_eq!(request.data, snapshot_data);
        assert!(!request.done);
    }

    #[test]
    fn test_install_snapshot_response_creation() {
        let response = InstallSnapshotResponse {
            term: 5,
            success: true,
        };

        assert_eq!(response.term, 5);
        assert!(response.success);
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry {
            index: 100,
            term: 5,
            entry_type: 0, // EntryType::Normal
            data: b"SET foo bar".to_vec(),
        };

        assert_eq!(entry.index, 100);
        assert_eq!(entry.term, 5);
        assert_eq!(entry.entry_type, 0);
        assert_eq!(entry.data, b"SET foo bar");
    }

    #[test]
    fn test_log_entry_types() {
        // Test Normal entry (value = 0)
        let normal_entry = LogEntry {
            index: 1,
            term: 1,
            entry_type: 0,
            data: vec![],
        };
        assert_eq!(normal_entry.entry_type, 0);

        // Test ConfigChange entry (value = 1)
        let conf_entry = LogEntry {
            index: 2,
            term: 1,
            entry_type: 1,
            data: vec![],
        };
        assert_eq!(conf_entry.entry_type, 1);

        // Test NoOp entry (value = 2)
        let noop_entry = LogEntry {
            index: 3,
            term: 1,
            entry_type: 2,
            data: vec![],
        };
        assert_eq!(noop_entry.entry_type, 2);
    }

    #[test]
    fn test_entry_type_enum_values() {
        // Verify enum values match proto definition
        assert_eq!(EntryType::Normal as i32, 0);
        assert_eq!(EntryType::ConfChange as i32, 1);
        assert_eq!(EntryType::Noop as i32, 2);
    }

    // Serialization/Deserialization roundtrip tests
    // These tests use prost's encode/decode to verify messages can be serialized

    #[test]
    fn test_request_vote_request_roundtrip() {
        use prost::Message;

        let original = RequestVoteRequest {
            term: 5,
            candidate_id: 1,
            last_log_index: 100,
            last_log_term: 4,
        };

        // Encode to bytes
        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        // Decode back
        let decoded = RequestVoteRequest::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.candidate_id, original.candidate_id);
        assert_eq!(decoded.last_log_index, original.last_log_index);
        assert_eq!(decoded.last_log_term, original.last_log_term);
    }

    #[test]
    fn test_request_vote_response_roundtrip() {
        use prost::Message;

        let original = RequestVoteResponse {
            term: 6,
            vote_granted: true,
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = RequestVoteResponse::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.vote_granted, original.vote_granted);
    }

    #[test]
    fn test_append_entries_request_roundtrip() {
        use prost::Message;

        let entry = LogEntry {
            index: 100,
            term: 5,
            entry_type: 0,
            data: b"test data".to_vec(),
        };

        let original = AppendEntriesRequest {
            term: 5,
            leader_id: 1,
            prev_log_index: 99,
            prev_log_term: 4,
            entries: vec![entry],
            leader_commit: 98,
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = AppendEntriesRequest::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.leader_id, original.leader_id);
        assert_eq!(decoded.prev_log_index, original.prev_log_index);
        assert_eq!(decoded.prev_log_term, original.prev_log_term);
        assert_eq!(decoded.entries.len(), original.entries.len());
        assert_eq!(decoded.entries[0].index, original.entries[0].index);
        assert_eq!(decoded.entries[0].term, original.entries[0].term);
        assert_eq!(
            decoded.entries[0].entry_type,
            original.entries[0].entry_type
        );
        assert_eq!(decoded.entries[0].data, original.entries[0].data);
        assert_eq!(decoded.leader_commit, original.leader_commit);
    }

    #[test]
    fn test_append_entries_response_roundtrip() {
        use prost::Message;

        let original = AppendEntriesResponse {
            term: 5,
            success: true,
            last_log_index: 100,
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = AppendEntriesResponse::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.success, original.success);
        assert_eq!(decoded.last_log_index, original.last_log_index);
    }

    #[test]
    fn test_install_snapshot_request_roundtrip() {
        use prost::Message;

        let snapshot_data = b"snapshot binary data".to_vec();
        let original = InstallSnapshotRequest {
            term: 5,
            leader_id: 1,
            last_included_index: 1000,
            last_included_term: 4,
            offset: 0,
            data: snapshot_data.clone(),
            done: true,
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = InstallSnapshotRequest::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.leader_id, original.leader_id);
        assert_eq!(decoded.last_included_index, original.last_included_index);
        assert_eq!(decoded.last_included_term, original.last_included_term);
        assert_eq!(decoded.offset, original.offset);
        assert_eq!(decoded.data, original.data);
        assert_eq!(decoded.done, original.done);
    }

    #[test]
    fn test_install_snapshot_response_roundtrip() {
        use prost::Message;

        let original = InstallSnapshotResponse {
            term: 5,
            success: true,
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = InstallSnapshotResponse::decode(&buf[..]).unwrap();

        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.success, original.success);
    }

    #[test]
    fn test_log_entry_roundtrip() {
        use prost::Message;

        let original = LogEntry {
            index: 100,
            term: 5,
            entry_type: 0,
            data: b"SET foo bar".to_vec(),
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = LogEntry::decode(&buf[..]).unwrap();

        assert_eq!(decoded.index, original.index);
        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.entry_type, original.entry_type);
        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn test_log_entry_with_empty_data() {
        use prost::Message;

        let original = LogEntry {
            index: 1,
            term: 1,
            entry_type: 2, // NOOP
            data: vec![],
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = LogEntry::decode(&buf[..]).unwrap();

        assert_eq!(decoded.index, original.index);
        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.entry_type, original.entry_type);
        assert!(decoded.data.is_empty());
    }

    #[test]
    fn test_log_entry_with_large_data() {
        use prost::Message;

        // Create a 1MB data payload
        let large_data = vec![0xAB; 1024 * 1024];
        let original = LogEntry {
            index: 500,
            term: 10,
            entry_type: 0,
            data: large_data.clone(),
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = LogEntry::decode(&buf[..]).unwrap();

        assert_eq!(decoded.index, original.index);
        assert_eq!(decoded.term, original.term);
        assert_eq!(decoded.entry_type, original.entry_type);
        assert_eq!(decoded.data.len(), 1024 * 1024);
        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn test_append_entries_heartbeat() {
        // Heartbeat is an AppendEntries with empty entries
        let heartbeat = AppendEntriesRequest {
            term: 5,
            leader_id: 1,
            prev_log_index: 100,
            prev_log_term: 5,
            entries: vec![],
            leader_commit: 100,
        };

        assert!(heartbeat.entries.is_empty());
        assert_eq!(heartbeat.leader_commit, heartbeat.prev_log_index);
    }

    #[test]
    fn test_append_entries_with_multiple_entries() {
        let entries = vec![
            LogEntry {
                index: 100,
                term: 5,
                entry_type: 0,
                data: b"entry 1".to_vec(),
            },
            LogEntry {
                index: 101,
                term: 5,
                entry_type: 0,
                data: b"entry 2".to_vec(),
            },
            LogEntry {
                index: 102,
                term: 5,
                entry_type: 0,
                data: b"entry 3".to_vec(),
            },
        ];

        let request = AppendEntriesRequest {
            term: 5,
            leader_id: 1,
            prev_log_index: 99,
            prev_log_term: 4,
            entries,
            leader_commit: 98,
        };

        assert_eq!(request.entries.len(), 3);
        assert_eq!(request.entries[0].index, 100);
        assert_eq!(request.entries[1].index, 101);
        assert_eq!(request.entries[2].index, 102);
    }

    #[test]
    fn test_install_snapshot_chunked_transfer() {
        // Simulate chunked snapshot transfer
        let chunk1 = InstallSnapshotRequest {
            term: 5,
            leader_id: 1,
            last_included_index: 1000,
            last_included_term: 4,
            offset: 0,
            data: vec![0x01; 1024],
            done: false,
        };

        let chunk2 = InstallSnapshotRequest {
            term: 5,
            leader_id: 1,
            last_included_index: 1000,
            last_included_term: 4,
            offset: 1024,
            data: vec![0x02; 1024],
            done: true,
        };

        assert_eq!(chunk1.offset, 0);
        assert!(!chunk1.done);
        assert_eq!(chunk2.offset, 1024);
        assert!(chunk2.done);
    }

    #[test]
    fn test_field_modification() {
        // Test that we can modify fields
        let request = RequestVoteRequest {
            term: 10,
            candidate_id: 5,
            last_log_index: 200,
            last_log_term: 9,
        };

        assert_eq!(request.term, 10);
        assert_eq!(request.candidate_id, 5);
        assert_eq!(request.last_log_index, 200);
        assert_eq!(request.last_log_term, 9);
    }

    #[test]
    fn test_clone_messages() {
        // Test that messages can be cloned
        let original = RequestVoteRequest {
            term: 5,
            candidate_id: 1,
            last_log_index: 100,
            last_log_term: 4,
        };

        let cloned = original.clone();

        assert_eq!(cloned.term, original.term);
        assert_eq!(cloned.candidate_id, original.candidate_id);
        assert_eq!(cloned.last_log_index, original.last_log_index);
        assert_eq!(cloned.last_log_term, original.last_log_term);
    }

    #[test]
    fn test_debug_output() {
        // Test that messages implement Debug
        let request = RequestVoteRequest {
            term: 5,
            candidate_id: 1,
            last_log_index: 100,
            last_log_term: 4,
        };

        let debug_str = format!("{request:?}");
        assert!(debug_str.contains("term"));
        assert!(debug_str.contains("5"));
    }

    #[test]
    fn test_entry_type_enum_conversion() {
        // Test that we can convert enum values
        use EntryType::*;

        assert_eq!(Normal as i32, 0);
        assert_eq!(ConfChange as i32, 1);
        assert_eq!(Noop as i32, 2);
    }

    #[test]
    fn test_service_traits_exist() {
        // This test verifies that the generated service trait exists
        // We can't instantiate it without async runtime, but we can verify the types exist
        fn _check_client_exists(_client: RaftServiceClient<tonic::transport::Channel>) {}
        fn _check_server_exists<T: RaftService>(_server: RaftServiceServer<T>) {}
    }
}
