//! Raft node integration for KV service.
//!
//! This module provides the RaftNode wrapper that integrates OpenRaft
//! with the KV service layer, handling client write operations and
//! leader state queries.

use openraft::error::{InstallSnapshotError, RPCError, RaftError};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{Config, Raft};
use seshat_storage::{BasicNode, OpenRaftMemLog, OpenRaftMemStateMachine, RaftTypeConfig, Request};
use std::collections::BTreeSet;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur during Raft node operations.
#[derive(Debug, Error)]
pub enum RaftNodeError {
    #[error("Not implemented")]
    NotImplemented,

    #[error("OpenRaft error: {0}")]
    OpenRaft(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Raft node wrapper for the KV service.
///
/// This struct wraps OpenRaft's Raft instance and provides a simplified
/// interface for the KV service layer to propose operations and query
/// leader state.
#[derive(Clone)]
pub struct RaftNode {
    /// Node identifier
    node_id: u64,

    /// The underlying OpenRaft Raft instance
    raft: Arc<Raft<RaftTypeConfig>>,

    /// Log storage (for direct read access if needed in future)
    #[allow(dead_code)]
    log_storage: OpenRaftMemLog,

    /// State machine (for direct read access if needed in future)
    #[allow(dead_code)]
    state_machine: OpenRaftMemStateMachine,
}

impl RaftNode {
    /// Create a new RaftNode instance.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The unique identifier for this Raft node
    /// * `peers` - List of peer node IDs in the cluster (empty for bootstrap node)
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the new `RaftNode` or an error.
    ///
    /// # Errors
    ///
    /// - `InvalidConfig` - If OpenRaft configuration validation fails
    /// - `OpenRaft` - If Raft instance creation fails
    pub async fn new(node_id: u64, peers: Vec<u64>) -> Result<Self, RaftNodeError> {
        // 1. Create OpenRaft storage components
        let log_storage = OpenRaftMemLog::new();
        let state_machine = OpenRaftMemStateMachine::new();

        // 2. Create Raft configuration
        let config = Arc::new(
            Config {
                heartbeat_interval: 500,
                election_timeout_min: 1500,
                election_timeout_max: 3000,
                ..Default::default()
            }
            .validate()
            .map_err(|e| RaftNodeError::InvalidConfig(e.to_string()))?,
        );

        // 3. Create network stub factory (gRPC transport planned for Phase 2+)
        // StubNetwork allows single-node testing but doesn't actually replicate
        let network = StubNetworkFactory::new(node_id);

        // 4. Initialize Raft instance
        let raft = Raft::new(
            node_id,
            config,
            network,
            log_storage.clone(),
            state_machine.clone(),
        )
        .await
        .map_err(|e| RaftNodeError::OpenRaft(e.to_string()))?;

        // 5. Initialize cluster membership if bootstrap node (empty peers = single node)
        if peers.is_empty() {
            raft.initialize(BTreeSet::from([node_id]))
                .await
                .map_err(|e| RaftNodeError::OpenRaft(e.to_string()))?;
        }

        Ok(Self {
            node_id,
            raft: Arc::new(raft),
            log_storage,
            state_machine,
        })
    }

    /// Propose a new operation to the Raft cluster.
    ///
    /// This method submits a client write request to the Raft cluster.
    /// The operation will be replicated to a majority of nodes before
    /// being applied to the state machine.
    ///
    /// # Arguments
    ///
    /// * `operation_bytes` - Serialized operation data
    ///
    /// # Returns
    ///
    /// Returns the response bytes from the state machine on success,
    /// or an error if the proposal fails.
    ///
    /// # Errors
    ///
    /// - `OpenRaft` - If the operation fails or node is not leader
    pub async fn propose(&self, operation_bytes: Vec<u8>) -> Result<Vec<u8>, RaftNodeError> {
        // Create Request from operation bytes
        let request = Request::new(operation_bytes);

        // Submit write request to Raft
        let client_write_response = self
            .raft
            .client_write(request)
            .await
            .map_err(|e| RaftNodeError::OpenRaft(format!("client_write failed: {e}")))?;

        // Extract response data
        Ok(client_write_response.data.result)
    }

    /// Check if this node is the current leader.
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if this node is the leader, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// - `OpenRaft` - If metrics cannot be retrieved
    pub async fn is_leader(&self) -> Result<bool, RaftNodeError> {
        let metrics = self.raft.metrics().borrow().clone();

        // Check if current_leader matches our node_id
        Ok(metrics.current_leader == Some(self.node_id))
    }

    /// Get the current leader's node ID.
    ///
    /// # Returns
    ///
    /// Returns the leader's node ID if known, or `None` if no leader is elected.
    ///
    /// # Errors
    ///
    /// - `OpenRaft` - If metrics cannot be retrieved
    pub async fn get_leader_id(&self) -> Result<Option<u64>, RaftNodeError> {
        let metrics = self.raft.metrics().borrow().clone();

        Ok(metrics.current_leader)
    }

    /// Get Raft metrics as a formatted string.
    ///
    /// Returns cluster state information including leader, term, and log indices.
    ///
    /// # Returns
    ///
    /// Formatted string containing key metrics.
    ///
    /// # Errors
    ///
    /// - `OpenRaft` - If metrics cannot be retrieved
    pub async fn get_metrics(&self) -> Result<String, RaftNodeError> {
        let metrics = self.raft.metrics().borrow().clone();

        Ok(format!(
            "id={} leader={:?} term={} last_applied={:?} last_log={:?}",
            self.node_id,
            metrics.current_leader,
            metrics.current_term,
            metrics.last_applied,
            metrics.last_log_index
        ))
    }

    /// Wait for this node to become leader (test helper).
    ///
    /// Polls metrics until leader election completes or timeout.
    /// This is primarily useful for testing to avoid sleep-based timing.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum wait time in milliseconds
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if node becomes leader, error otherwise.
    ///
    /// # Errors
    ///
    /// - `OpenRaft` - If timeout expires or node cannot become leader
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use seshat_kv::RaftNode;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = RaftNode::new(1, vec![]).await?;
    /// node.wait_for_leader(500).await?;
    /// assert!(node.is_leader().await?);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for_leader(&self, timeout_ms: u64) -> Result<(), RaftNodeError> {
        use tokio::time::{sleep, Duration};

        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if self.is_leader().await? {
                return Ok(());
            }
            sleep(Duration::from_millis(10)).await;
        }

        Err(RaftNodeError::OpenRaft(format!(
            "Leader election timeout after {timeout_ms}ms"
        )))
    }
}

/// Stub network factory for OpenRaft.
///
/// This is a placeholder until gRPC transport is implemented in future phases.
/// Creates StubNetworkConnection instances that don't actually send network messages.
///
/// # Testing Behavior
///
/// The stub network always succeeds with operations, which simplifies single-node
/// testing but means multi-node consensus cannot be tested. For realistic failure
/// scenarios (network partitions, timeouts, vote rejections), a real gRPC transport
/// implementation is required.
///
/// Future enhancement: Add configurable failure modes for testing error paths:
/// - `FailureMode::AlwaysSucceed` (current behavior)
/// - `FailureMode::AlwaysFail` (simulate network partition)
/// - `FailureMode::RandomFailure(probability)` (intermittent failures)
/// - `FailureMode::Timeout` (simulate slow network)
#[derive(Clone)]
struct StubNetworkFactory {
    #[allow(dead_code)]
    node_id: u64,
}

impl StubNetworkFactory {
    fn new(node_id: u64) -> Self {
        Self { node_id }
    }
}

/// Stub network connection for a single peer.
///
/// Currently always succeeds with operations. Future versions could accept
/// a FailureMode parameter to simulate network failures for testing.
struct StubNetworkConnection {}

impl RaftNetworkFactory<RaftTypeConfig> for StubNetworkFactory {
    type Network = StubNetworkConnection;

    async fn new_client(&mut self, _target: u64, _node: &BasicNode) -> Self::Network {
        StubNetworkConnection {}
    }
}

impl RaftNetwork<RaftTypeConfig> for StubNetworkConnection {
    async fn append_entries(
        &mut self,
        _rpc: AppendEntriesRequest<RaftTypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        // Return success - stub doesn't actually replicate
        Ok(AppendEntriesResponse::Success)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<u64>,
        _option: RPCOption,
    ) -> Result<VoteResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        // Grant vote
        Ok(VoteResponse::new(rpc.vote, rpc.last_log_id, true))
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<RaftTypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<u64>,
        RPCError<u64, BasicNode, RaftError<u64, InstallSnapshotError>>,
    > {
        // Accept snapshot
        Ok(InstallSnapshotResponse { vote: rpc.vote })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_imports_compile() {
        // This test verifies that all OpenRaft types are accessible
        // and that the dependencies are correctly configured.

        // Type check: verify RaftTypeConfig is accessible
        let _type_check: Option<RaftTypeConfig> = None;

        // Type check: verify storage types are accessible
        let _log_storage: Option<OpenRaftMemLog> = None;
        let _state_machine: Option<OpenRaftMemStateMachine> = None;
    }

    #[tokio::test]
    async fn test_storage_types_accessible_from_kv_crate() {
        // Verify that storage crate types are accessible via seshat-storage
        use seshat_storage::{Operation, Response};

        // Create test instances to verify types compile
        let _op = Operation::Set {
            key: b"test".to_vec(),
            value: b"value".to_vec(),
        };

        let _req = Request::new(vec![1, 2, 3]);
        let _resp = Response::new(vec![4, 5, 6]);
    }

    #[tokio::test]
    async fn test_raft_node_error_types() {
        // Verify error types are defined correctly
        let err1 = RaftNodeError::NotImplemented;
        let err2 = RaftNodeError::OpenRaft("test".to_string());
        let err3 = RaftNodeError::Storage("test".to_string());
        let err4 = RaftNodeError::InvalidConfig("test".to_string());

        // Verify Display trait works
        assert_eq!(err1.to_string(), "Not implemented");
        assert!(err2.to_string().contains("OpenRaft error"));
        assert!(err3.to_string().contains("Storage error"));
        assert!(err4.to_string().contains("Invalid configuration"));
    }

    // ========================================================================
    // Task 5.2: RaftNode Initialization Tests
    // ========================================================================

    #[tokio::test]
    async fn test_raft_node_initialization_succeeds() {
        // Test creating RaftNode with valid config (bootstrap node)
        let result = RaftNode::new(1, vec![]).await;

        assert!(
            result.is_ok(),
            "RaftNode initialization should succeed: {:?}",
            result.err()
        );

        let node = result.unwrap();
        // Verify internal state is initialized
        assert!(Arc::strong_count(&node.raft) >= 1);
    }

    #[tokio::test]
    async fn test_raft_node_initialization_with_peers() {
        // Test creating RaftNode with peers (non-bootstrap node)
        let result = RaftNode::new(1, vec![2, 3]).await;

        assert!(
            result.is_ok(),
            "RaftNode initialization with peers should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_raft_node_requires_valid_node_id() {
        // Test that node_id is accepted (u64, all values valid)
        let result_zero = RaftNode::new(0, vec![]).await;
        let result_max = RaftNode::new(u64::MAX, vec![]).await;

        // Both should succeed - u64::MAX and 0 are valid node IDs
        assert!(result_zero.is_ok(), "node_id=0 should be valid");
        assert!(result_max.is_ok(), "node_id=u64::MAX should be valid");
    }

    #[tokio::test]
    async fn test_raft_node_initializes_storage() {
        // Test that storage is initialized correctly
        let node = RaftNode::new(1, vec![])
            .await
            .expect("RaftNode creation should succeed");

        // Verify storage components are accessible
        let _log = &node.log_storage;
        let _sm = &node.state_machine;

        // Storage should be initialized (empty but valid)
        // We can't directly inspect private fields, but we verified they exist
    }

    #[tokio::test]
    async fn test_raft_node_creates_raft_instance() {
        // Test that Raft instance is created
        let node = RaftNode::new(1, vec![])
            .await
            .expect("RaftNode creation should succeed");

        // Verify Raft instance is wrapped in Arc
        assert!(Arc::strong_count(&node.raft) >= 1);
    }

    #[tokio::test]
    async fn test_raft_node_bootstrap_single_node() {
        // Test bootstrap node (empty peers) initializes cluster
        let result = RaftNode::new(1, vec![]).await;

        assert!(
            result.is_ok(),
            "Bootstrap node initialization should succeed"
        );
    }

    #[tokio::test]
    async fn test_raft_node_multiple_instances() {
        // Test creating multiple RaftNode instances with different IDs
        let node1 = RaftNode::new(1, vec![])
            .await
            .expect("Node 1 creation should succeed");
        let node2 = RaftNode::new(2, vec![])
            .await
            .expect("Node 2 creation should succeed");

        // Verify both instances are independent
        assert!(Arc::strong_count(&node1.raft) >= 1);
        assert!(Arc::strong_count(&node2.raft) >= 1);
    }

    // ========================================================================
    // Task 5.3: propose() Implementation Tests
    // ========================================================================

    #[tokio::test]
    async fn test_propose_succeeds() {
        // Create single-node cluster (will become leader)
        let node = RaftNode::new(1, vec![]).await.unwrap();

        // Wait for leader election
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        // Create operation bytes (using bincode serialization like the state machine expects)
        use seshat_storage::Operation;
        let operation = Operation::Set {
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };
        let operation_bytes = operation.serialize().unwrap();

        // Propose should succeed
        let result = node.propose(operation_bytes).await;
        assert!(result.is_ok(), "Propose should succeed: {:?}", result.err());

        let response = result.unwrap();
        // Should return "OK" for successful Set operation
        assert_eq!(response, b"OK");
    }

    #[tokio::test]
    async fn test_propose_empty_operation() {
        // Create single-node cluster
        let node = RaftNode::new(1, vec![]).await.unwrap();

        // Wait for leader election
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        // Propose empty operation - should succeed but may fail during apply
        let result = node.propose(vec![]).await;

        // Empty bytes won't deserialize to a valid Operation, so this will fail
        // during state machine apply. This is expected behavior.
        assert!(result.is_err(), "Empty operation should fail during apply");
    }

    #[tokio::test]
    async fn test_propose_del_operation() {
        // Create single-node cluster
        let node = RaftNode::new(1, vec![]).await.unwrap();

        // Wait for leader election
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        // Create Del operation
        use seshat_storage::Operation;
        let operation = Operation::Del {
            key: b"nonexistent".to_vec(),
        };
        let operation_bytes = operation.serialize().unwrap();

        // Propose should succeed
        let result = node.propose(operation_bytes).await;
        assert!(
            result.is_ok(),
            "Del propose should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        // Should return "0" for deleting nonexistent key
        assert_eq!(response, b"0");
    }

    #[tokio::test]
    async fn test_propose_multiple_operations() {
        // Create single-node cluster
        let node = RaftNode::new(1, vec![]).await.unwrap();

        // Wait for leader election
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        use seshat_storage::Operation;

        // Set a key
        let set_op = Operation::Set {
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };
        let result = node.propose(set_op.serialize().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"OK");

        // Delete the same key
        let del_op = Operation::Del {
            key: b"key1".to_vec(),
        };
        let result = node.propose(del_op.serialize().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"1"); // Should return "1" (key existed)

        // Delete again (key no longer exists)
        let del_op2 = Operation::Del {
            key: b"key1".to_vec(),
        };
        let result = node.propose(del_op2.serialize().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"0"); // Should return "0" (key doesn't exist)
    }

    #[tokio::test]
    async fn test_propose_binary_data() {
        // Test proposing binary data (not just text)
        let node = RaftNode::new(1, vec![]).await.unwrap();
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        use seshat_storage::Operation;
        let operation = Operation::Set {
            key: vec![0x00, 0xFF, 0xAB],
            value: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let result = node.propose(operation.serialize().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"OK");
    }

    #[tokio::test]
    async fn test_propose_large_value() {
        // Test proposing large value
        let node = RaftNode::new(1, vec![]).await.unwrap();
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        use seshat_storage::Operation;
        let large_value = vec![0xAB; 10_000];
        let operation = Operation::Set {
            key: b"large_key".to_vec(),
            value: large_value,
        };

        let result = node.propose(operation.serialize().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"OK");
    }

    // ========================================================================
    // Task 5.4: API Methods Implementation Tests
    // ========================================================================

    #[tokio::test]
    async fn test_is_leader_returns_true_for_single_node() {
        // Single node cluster should become leader
        let node = RaftNode::new(1, vec![]).await.unwrap();

        // Wait for leader election
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        let result = node.is_leader().await;
        assert!(
            result.is_ok(),
            "is_leader should succeed: {:?}",
            result.err()
        );
        assert!(result.unwrap(), "Single node should be leader");
    }

    #[tokio::test]
    async fn test_is_leader_returns_false_for_non_leader() {
        // Node with peers but not initialized should not be leader
        let node = RaftNode::new(2, vec![1, 3]).await.unwrap();

        // Wait briefly (not long enough to become leader without peers)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let result = node.is_leader().await;
        assert!(result.is_ok(), "is_leader should succeed");
        // Node 2 with peers should not be leader yet
        assert!(!result.unwrap(), "Node with peers should not be leader yet");
    }

    #[tokio::test]
    async fn test_get_leader_id_returns_self_for_single_node() {
        let node = RaftNode::new(1, vec![]).await.unwrap();
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        let result = node.get_leader_id().await;
        assert!(
            result.is_ok(),
            "get_leader_id should succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), Some(1), "Leader should be node 1");
    }

    #[tokio::test]
    async fn test_get_leader_id_returns_none_before_election() {
        // Node with peers that hasn't connected should have no leader
        let node = RaftNode::new(3, vec![1, 2]).await.unwrap();

        // Check immediately (before leader election)
        let result = node.get_leader_id().await;
        assert!(result.is_ok(), "get_leader_id should succeed");
        // Should be None since no peers are reachable
        assert_eq!(result.unwrap(), None, "Should have no leader without peers");
    }

    #[tokio::test]
    async fn test_get_metrics_returns_valid_string() {
        let node = RaftNode::new(1, vec![]).await.unwrap();
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        let result = node.get_metrics().await;
        assert!(
            result.is_ok(),
            "get_metrics should succeed: {:?}",
            result.err()
        );

        let metrics = result.unwrap();
        assert!(!metrics.is_empty(), "Metrics should not be empty");
        assert!(metrics.contains("id=1"), "Metrics should contain node ID");
        assert!(
            metrics.contains("leader="),
            "Metrics should contain leader info"
        );
        assert!(metrics.contains("term="), "Metrics should contain term");
    }

    #[tokio::test]
    async fn test_get_metrics_shows_leader_info() {
        let node = RaftNode::new(1, vec![]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let metrics = node.get_metrics().await.unwrap();

        // Should show this node as leader
        assert!(
            metrics.contains("leader=Some(1)"),
            "Metrics should show node 1 as leader: {metrics}"
        );
    }

    #[tokio::test]
    async fn test_metrics_after_propose() {
        let node = RaftNode::new(1, vec![]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Propose an operation
        use seshat_storage::Operation;
        let op = Operation::Set {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        node.propose(op.serialize().unwrap()).await.unwrap();

        // Check metrics reflect the operation
        let metrics = node.get_metrics().await.unwrap();
        assert!(
            metrics.contains("last_applied"),
            "Metrics should show last_applied: {metrics}"
        );
        // After applying one operation, last_applied should be Some(LogId)
        assert!(
            !metrics.contains("last_applied=None"),
            "Should have applied log after operation: {metrics}"
        );
    }

    #[tokio::test]
    async fn test_metrics_format_contains_all_fields() {
        let node = RaftNode::new(1, vec![]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let metrics = node.get_metrics().await.unwrap();

        // Verify all expected fields are present
        assert!(metrics.contains("id="), "Should contain id field");
        assert!(metrics.contains("leader="), "Should contain leader field");
        assert!(metrics.contains("term="), "Should contain term field");
        assert!(
            metrics.contains("last_applied="),
            "Should contain last_applied field"
        );
        assert!(
            metrics.contains("last_log="),
            "Should contain last_log field"
        );
    }

    #[tokio::test]
    async fn test_is_leader_consistent_with_get_leader_id() {
        // Test that is_leader() and get_leader_id() are consistent
        let node = RaftNode::new(1, vec![]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let is_leader = node.is_leader().await.unwrap();
        let leader_id = node.get_leader_id().await.unwrap();

        if is_leader {
            assert_eq!(
                leader_id,
                Some(1),
                "If is_leader is true, leader_id should be self"
            );
        } else {
            assert_ne!(
                leader_id,
                Some(1),
                "If is_leader is false, leader_id should not be self"
            );
        }
    }

    #[tokio::test]
    async fn test_multiple_metrics_calls() {
        // Test that get_metrics() can be called multiple times
        let node = RaftNode::new(1, vec![]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let metrics1 = node.get_metrics().await.unwrap();
        let metrics2 = node.get_metrics().await.unwrap();

        // Both should succeed and return formatted strings
        assert!(!metrics1.is_empty());
        assert!(!metrics2.is_empty());
        assert!(metrics1.contains("id=1"));
        assert!(metrics2.contains("id=1"));
    }
}
