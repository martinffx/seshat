//! Integration tests for OpenRaft cluster operations
//!
//! Phase 6: Integration & Cleanup
//!
//! These tests verify end-to-end cluster behavior with OpenRaft,
//! including single-node and multi-node scenarios.

use seshat_kv::{Operation, RaftNode};

// ============================================================================
// Single Node Cluster Tests
// ============================================================================

#[tokio::test]
async fn test_single_node_cluster_initialization() {
    // Single node should initialize successfully and become leader
    let node = RaftNode::new(1, vec![]).await;
    assert!(
        node.is_ok(),
        "Single node cluster should initialize: {:?}",
        node.err()
    );

    let node = node.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Verify it is leader
    let is_leader = node.is_leader().await.unwrap();
    assert!(is_leader, "Single node should become leader");

    let leader_id = node.get_leader_id().await.unwrap();
    assert_eq!(leader_id, Some(1), "Leader should be node 1");
}

#[tokio::test]
async fn test_single_node_basic_operations() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Test Set operation
    let set_op = Operation::Set {
        key: b"integration_key".to_vec(),
        value: b"integration_value".to_vec(),
    };
    let result = node.propose(set_op.serialize().unwrap()).await;
    assert!(result.is_ok(), "Set operation should succeed");
    assert_eq!(result.unwrap(), b"OK");

    // Test Del operation
    let del_op = Operation::Del {
        key: b"integration_key".to_vec(),
    };
    let result = node.propose(del_op.serialize().unwrap()).await;
    assert!(result.is_ok(), "Del operation should succeed");
    assert_eq!(result.unwrap(), b"1");

    // Test Del on nonexistent key
    let del_op2 = Operation::Del {
        key: b"nonexistent".to_vec(),
    };
    let result = node.propose(del_op2.serialize().unwrap()).await;
    assert!(result.is_ok(), "Del operation should succeed");
    assert_eq!(result.unwrap(), b"0");
}

#[tokio::test]
async fn test_single_node_operation_sequence() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Perform a sequence of operations
    for i in 0..10 {
        let key = format!("key_{i}").into_bytes();
        let value = format!("value_{i}").into_bytes();

        let set_op = Operation::Set {
            key: key.clone(),
            value,
        };
        let result = node.propose(set_op.serialize().unwrap()).await;
        assert!(result.is_ok(), "Set operation {i} should succeed");
        assert_eq!(result.unwrap(), b"OK");
    }

    // Delete all keys
    for i in 0..10 {
        let key = format!("key_{i}").into_bytes();
        let del_op = Operation::Del { key };
        let result = node.propose(del_op.serialize().unwrap()).await;
        assert!(result.is_ok(), "Del operation {i} should succeed");
        assert_eq!(result.unwrap(), b"1");
    }
}

#[tokio::test]
async fn test_single_node_concurrent_operations() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Submit multiple operations concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let node_clone = node.clone();
        let handle = tokio::spawn(async move {
            let key = format!("concurrent_key_{i}").into_bytes();
            let value = format!("concurrent_value_{i}").into_bytes();

            let set_op = Operation::Set { key, value };
            node_clone.propose(set_op.serialize().unwrap()).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent operation {i} should succeed");
        assert_eq!(result.unwrap(), b"OK");
    }
}

#[tokio::test]
async fn test_single_node_metrics_tracking() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Get initial metrics
    let metrics_before = node.get_metrics().await.unwrap();
    assert!(metrics_before.contains("id=1"));

    // Perform operations
    for i in 0..5 {
        let set_op = Operation::Set {
            key: format!("key_{i}").into_bytes(),
            value: b"value".to_vec(),
        };
        node.propose(set_op.serialize().unwrap()).await.unwrap();
    }

    // Get metrics after operations
    let metrics_after = node.get_metrics().await.unwrap();
    assert!(metrics_after.contains("id=1"));
    assert!(metrics_after.contains("leader=Some(1)"));

    // Verify last_applied increased
    assert!(
        !metrics_after.contains("last_applied=None"),
        "Should have applied logs: {metrics_after}"
    );
}

// ============================================================================
// Multi-Node Cluster Tests (with stub network)
// ============================================================================
//
// Note: These tests use StubNetwork which doesn't actually replicate.
// They verify that the RaftNode API works correctly with multi-node
// initialization, but actual replication requires gRPC transport.

#[tokio::test]
async fn test_multi_node_initialization() {
    // Create 3-node cluster setup (stub network, no actual replication)
    let node1 = RaftNode::new(1, vec![2, 3]).await;
    let node2 = RaftNode::new(2, vec![1, 3]).await;
    let node3 = RaftNode::new(3, vec![1, 2]).await;

    assert!(node1.is_ok(), "Node 1 should initialize");
    assert!(node2.is_ok(), "Node 2 should initialize");
    assert!(node3.is_ok(), "Node 3 should initialize");
}

#[tokio::test]
async fn test_multi_node_metrics() {
    // Create multi-node setup
    let node1 = RaftNode::new(1, vec![2, 3]).await.unwrap();
    let node2 = RaftNode::new(2, vec![1, 3]).await.unwrap();

    // Brief wait for initialization (nodes won't elect leader with StubNetwork)
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Both nodes should be able to report metrics
    let metrics1 = node1.get_metrics().await;
    let metrics2 = node2.get_metrics().await;

    assert!(metrics1.is_ok(), "Node 1 metrics should be accessible");
    assert!(metrics2.is_ok(), "Node 2 metrics should be accessible");

    let m1 = metrics1.unwrap();
    let m2 = metrics2.unwrap();

    assert!(m1.contains("id=1"), "Node 1 metrics should contain id=1");
    assert!(m2.contains("id=2"), "Node 2 metrics should contain id=2");
}

#[tokio::test]
async fn test_multi_node_leader_election() {
    // Note: With StubNetwork, actual leader election won't work across nodes
    // This test verifies that nodes can check leadership status independently

    let node1 = RaftNode::new(1, vec![2, 3]).await.unwrap();
    let node2 = RaftNode::new(2, vec![1, 3]).await.unwrap();

    // Brief wait for initialization
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Each node can check if it's leader
    let is_leader1 = node1.is_leader().await;
    let is_leader2 = node2.is_leader().await;

    assert!(
        is_leader1.is_ok(),
        "Node 1 should be able to check leadership"
    );
    assert!(
        is_leader2.is_ok(),
        "Node 2 should be able to check leadership"
    );

    // With StubNetwork, nodes can't communicate, so they won't establish leadership
    // Both should report not being leader (or timeout trying to elect)
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_propose_on_uninitialized_follower() {
    // Node with peers that hasn't established leadership
    let node = RaftNode::new(2, vec![1, 3]).await.unwrap();

    // Try to propose immediately (before leader election)
    let op = Operation::Set {
        key: b"test".to_vec(),
        value: b"value".to_vec(),
    };

    let result = node.propose(op.serialize().unwrap()).await;

    // Should fail because there's no leader (StubNetwork can't elect)
    assert!(result.is_err(), "Propose should fail without leader");
}

#[tokio::test]
async fn test_invalid_operation_bytes() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Try to propose invalid bytes (can't deserialize to Operation)
    let result = node.propose(vec![0xFF, 0xFF, 0xFF]).await;

    // Should fail during state machine apply (deserialization error)
    assert!(result.is_err(), "Invalid operation bytes should fail");
}

#[tokio::test]
async fn test_empty_operation_bytes() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Try to propose empty bytes
    let result = node.propose(vec![]).await;

    // Should fail (empty bytes can't deserialize to Operation)
    assert!(result.is_err(), "Empty operation bytes should fail");
}

// ============================================================================
// Stress Tests
// ============================================================================

#[tokio::test]
async fn test_high_volume_operations() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Submit 100 operations sequentially
    for i in 0..100 {
        let op = Operation::Set {
            key: format!("key_{i}").into_bytes(),
            value: format!("value_{i}").into_bytes(),
        };
        let result = node.propose(op.serialize().unwrap()).await;
        assert!(result.is_ok(), "Operation {i} should succeed");
    }

    // Verify metrics show all operations applied
    let metrics = node.get_metrics().await.unwrap();
    assert!(
        !metrics.contains("last_applied=None"),
        "Should have applied all operations"
    );
}

#[tokio::test]
async fn test_large_operation_payloads() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Test with large key and value (100KB each)
    let large_key = vec![b'K'; 100_000];
    let large_value = vec![b'V'; 100_000];

    let op = Operation::Set {
        key: large_key,
        value: large_value,
    };

    let result = node.propose(op.serialize().unwrap()).await;
    assert!(result.is_ok(), "Large operation should succeed");
    assert_eq!(result.unwrap(), b"OK");
}

#[tokio::test]
async fn test_mixed_operation_types() {
    let node = RaftNode::new(1, vec![]).await.unwrap();
    node.wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Mix Set and Del operations
    for i in 0..20 {
        if i % 2 == 0 {
            // Set operation
            let op = Operation::Set {
                key: format!("key_{i}").into_bytes(),
                value: b"value".to_vec(),
            };
            let result = node.propose(op.serialize().unwrap()).await;
            assert!(result.is_ok(), "Set operation {i} should succeed");
        } else {
            // Del operation
            let op = Operation::Del {
                key: format!("key_{}", i - 1).into_bytes(),
            };
            let result = node.propose(op.serialize().unwrap()).await;
            assert!(result.is_ok(), "Del operation {i} should succeed");
        }
    }
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[tokio::test]
async fn test_different_node_ids() {
    // Test various node IDs
    for node_id in [1, 10, 100, 1000] {
        let node = RaftNode::new(node_id, vec![]).await;
        assert!(node.is_ok(), "Node {node_id} should initialize");

        let node = node.unwrap();
        node.wait_for_leader(500)
            .await
            .expect("Node should become leader");

        let metrics = node.get_metrics().await.unwrap();
        assert!(
            metrics.contains(&format!("id={node_id}")),
            "Metrics should contain correct node ID: {metrics}"
        );
    }
}

#[tokio::test]
async fn test_clone_behavior() {
    let node1 = RaftNode::new(1, vec![]).await.unwrap();
    node1
        .wait_for_leader(500)
        .await
        .expect("Node should become leader");

    // Clone the node
    let node2 = node1.clone();

    // Both should work independently
    let op1 = Operation::Set {
        key: b"key1".to_vec(),
        value: b"value1".to_vec(),
    };
    let result1 = node1.propose(op1.serialize().unwrap()).await;
    assert!(result1.is_ok(), "Original node should work");

    let op2 = Operation::Set {
        key: b"key2".to_vec(),
        value: b"value2".to_vec(),
    };
    let result2 = node2.propose(op2.serialize().unwrap()).await;
    assert!(result2.is_ok(), "Cloned node should work");

    // Both should see the same state (same underlying Raft instance)
    let metrics1 = node1.get_metrics().await.unwrap();
    let metrics2 = node2.get_metrics().await.unwrap();

    // Both should show operations applied (they share state)
    assert!(
        !metrics1.contains("last_applied=None"),
        "Original node should show applied logs"
    );
    assert!(
        !metrics2.contains("last_applied=None"),
        "Cloned node should show applied logs"
    );
}
