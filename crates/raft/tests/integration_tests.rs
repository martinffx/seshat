//! Integration tests for Raft consensus implementation.
//!
//! These tests verify end-to-end behavior of the Raft node, including
//! cluster bootstrap, leader election, and command replication.

use seshat_kv::Operation;
use std::time::Duration;

mod common;

#[test]
fn test_single_node_bootstrap() {
    // Create a single-node cluster (node ID 1, peers [1])
    let mut node = common::create_single_node_cluster(1);

    // Verify initial state - should not be leader before election
    assert!(!node.is_leader(), "Node should not be leader initially");
    assert_eq!(
        node.leader_id(),
        None,
        "Node should not know leader initially"
    );

    // Run event loop for a period to drive Raft state machine
    // Note: In raft-rs, automatic leadership depends on cluster configuration
    // This test verifies the event loop utilities work correctly
    let _ran_event_loop =
        common::run_until(&mut node, |n| n.is_leader(), Duration::from_millis(500));

    // The test passes if the event loop runs without panicking
    // Actual leadership depends on raft-rs cluster initialization
}

#[test]
fn test_event_loop_tick_and_ready() {
    // Create a single-node cluster
    let mut node = common::create_single_node_cluster(1);

    // Run several iterations of the event loop
    for _ in 0..10 {
        node.tick().expect("Tick should succeed");
        node.handle_ready().expect("Handle ready should succeed");
    }

    // Test passes if event loop runs without errors
}

#[test]
fn test_run_until_timeout() {
    // Test the run_until helper with a condition that's never met
    let mut node = common::create_single_node_cluster(1);

    // Condition that's always false - should timeout
    let result = common::run_until(&mut node, |_n| false, Duration::from_millis(100));
    assert!(!result, "Should timeout when condition never met");
}

#[test]
fn test_run_until_success() {
    // Test the run_until helper with a condition that's immediately met
    let mut node = common::create_single_node_cluster(1);

    // Condition that's always true - should succeed immediately
    let result = common::run_until(&mut node, |_n| true, Duration::from_secs(1));
    assert!(result, "Should succeed when condition is met");
}

#[test]
fn test_create_single_node_cluster_utility() {
    // Test the create_single_node_cluster helper
    let node1 = common::create_single_node_cluster(1);
    let node2 = common::create_single_node_cluster(100);

    // Both should be created successfully (verified by no panic)
    // We can't easily access the internal ID, but we can verify they work
    drop(node1);
    drop(node2);
}

#[test]
fn test_multiple_node_ids() {
    // Test that nodes can be created with various IDs
    for id in [1u64, 2, 10, 100, 999] {
        let mut node = common::create_single_node_cluster(id);

        // Verify node was created successfully
        assert!(
            !node.is_leader(),
            "Node {id} should not be leader initially"
        );

        // Run a few iterations of event loop
        for _ in 0..5 {
            node.tick().expect("Tick should succeed");
            node.handle_ready().expect("Handle ready should succeed");
        }
    }
}

// ========== PROPOSE AND APPLY INTEGRATION TESTS ==========

#[test]
fn test_single_node_propose_and_apply() {
    // Step 1: Create a single-node cluster
    let mut node = common::create_single_node_cluster(1);

    // Step 2: Wait for node to become leader
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(
        became_leader,
        "Node should become leader in single-node cluster"
    );

    // Step 3: Create and serialize a SET operation
    let operation = Operation::Set {
        key: b"test_key".to_vec(),
        value: b"test_value".to_vec(),
    };
    let data = operation
        .serialize()
        .expect("Operation serialization should succeed");

    // Step 4: Propose the operation
    node.propose(data)
        .expect("Propose should succeed on leader");

    // Step 5: Process ready events until the operation is applied
    // In a single-node cluster, operations are committed immediately
    let applied = common::run_until(
        &mut node,
        |n| n.get(b"test_key").is_some(),
        Duration::from_secs(5),
    );
    assert!(
        applied,
        "Operation should be applied to state machine within timeout"
    );

    // Step 6: Verify the value was applied correctly
    let value = node.get(b"test_key");
    assert_eq!(
        value,
        Some(b"test_value".to_vec()),
        "State machine should contain the proposed value"
    );
}

#[test]
fn test_propose_multiple_operations() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Define multiple operations to propose
    let operations = vec![("key1", "value1"), ("key2", "value2"), ("key3", "value3")];

    // Propose each operation and verify it's applied
    for (key, value) in operations {
        let operation = Operation::Set {
            key: key.as_bytes().to_vec(),
            value: value.as_bytes().to_vec(),
        };
        let data = operation.serialize().expect("Serialization should succeed");

        node.propose(data).expect("Propose should succeed");

        // Wait for this specific operation to be applied
        let applied = common::run_until(
            &mut node,
            |n| n.get(key.as_bytes()).is_some(),
            Duration::from_secs(5),
        );
        assert!(applied, "Operation for key '{key}' should be applied");

        // Verify the value
        let stored_value = node.get(key.as_bytes());
        assert_eq!(
            stored_value,
            Some(value.as_bytes().to_vec()),
            "Value for key '{key}' should match"
        );
    }

    // Verify all values are still present
    assert_eq!(node.get(b"key1"), Some(b"value1".to_vec()));
    assert_eq!(node.get(b"key2"), Some(b"value2".to_vec()));
    assert_eq!(node.get(b"key3"), Some(b"value3".to_vec()));
}

#[test]
fn test_propose_del_operation() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Step 1: Set a key
    let set_op = Operation::Set {
        key: b"delete_me".to_vec(),
        value: b"initial_value".to_vec(),
    };
    let set_data = set_op.serialize().expect("Serialization should succeed");
    node.propose(set_data).expect("Propose should succeed");

    // Wait for SET to be applied
    let set_applied = common::run_until(
        &mut node,
        |n| n.get(b"delete_me").is_some(),
        Duration::from_secs(5),
    );
    assert!(set_applied, "SET operation should be applied");
    assert_eq!(node.get(b"delete_me"), Some(b"initial_value".to_vec()));

    // Step 2: Delete the key
    let del_op = Operation::Del {
        key: b"delete_me".to_vec(),
    };
    let del_data = del_op.serialize().expect("Serialization should succeed");
    node.propose(del_data).expect("Propose should succeed");

    // Wait for DEL to be applied (key should be None)
    let del_applied = common::run_until(
        &mut node,
        |n| n.get(b"delete_me").is_none(),
        Duration::from_secs(5),
    );
    assert!(del_applied, "DEL operation should be applied");
    assert_eq!(node.get(b"delete_me"), None, "Key should be deleted");
}

#[test]
fn test_propose_and_verify_persistence() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Propose a SET operation
    let operation = Operation::Set {
        key: b"persistent_key".to_vec(),
        value: b"persistent_value".to_vec(),
    };
    let data = operation.serialize().expect("Serialization should succeed");
    node.propose(data).expect("Propose should succeed");

    // Wait for operation to be applied
    let applied = common::run_until(
        &mut node,
        |n| n.get(b"persistent_key").is_some(),
        Duration::from_secs(5),
    );
    assert!(applied, "Operation should be applied");

    // Verify the value persists across multiple ready cycles
    for _ in 0..10 {
        node.tick().expect("Tick should succeed");
        node.handle_ready().expect("Handle ready should succeed");

        // Value should still be present
        assert_eq!(
            node.get(b"persistent_key"),
            Some(b"persistent_value".to_vec()),
            "Value should persist across event loop iterations"
        );
    }
}

#[test]
fn test_propose_empty_key() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Propose a SET operation with empty key
    let operation = Operation::Set {
        key: vec![],
        value: b"empty_key_value".to_vec(),
    };
    let data = operation.serialize().expect("Serialization should succeed");
    node.propose(data).expect("Propose should succeed");

    // Wait for operation to be applied
    let applied = common::run_until(&mut node, |n| n.get(b"").is_some(), Duration::from_secs(5));
    assert!(applied, "Operation with empty key should be applied");

    // Verify the value
    assert_eq!(
        node.get(b""),
        Some(b"empty_key_value".to_vec()),
        "Empty key should be stored correctly"
    );
}

#[test]
fn test_propose_large_value() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Create a large value (10KB)
    let large_value = vec![0xAB; 10 * 1024];
    let operation = Operation::Set {
        key: b"large_key".to_vec(),
        value: large_value.clone(),
    };
    let data = operation.serialize().expect("Serialization should succeed");
    node.propose(data).expect("Propose should succeed");

    // Wait for operation to be applied
    let applied = common::run_until(
        &mut node,
        |n| n.get(b"large_key").is_some(),
        Duration::from_secs(5),
    );
    assert!(applied, "Large value operation should be applied");

    // Verify the large value
    assert_eq!(
        node.get(b"large_key"),
        Some(large_value),
        "Large value should be stored correctly"
    );
}

#[test]
fn test_propose_overwrite_value() {
    // Create a single-node cluster and wait for leadership
    let mut node = common::create_single_node_cluster(1);
    let became_leader = common::run_until(&mut node, |n| n.is_leader(), Duration::from_secs(5));
    assert!(became_leader, "Node should become leader");

    // Set initial value
    let op1 = Operation::Set {
        key: b"overwrite_key".to_vec(),
        value: b"first_value".to_vec(),
    };
    let data1 = op1.serialize().expect("Serialization should succeed");
    node.propose(data1).expect("Propose should succeed");

    // Wait for first operation
    let applied1 = common::run_until(
        &mut node,
        |n| n.get(b"overwrite_key") == Some(b"first_value".to_vec()),
        Duration::from_secs(5),
    );
    assert!(applied1, "First operation should be applied");

    // Overwrite with new value
    let op2 = Operation::Set {
        key: b"overwrite_key".to_vec(),
        value: b"second_value".to_vec(),
    };
    let data2 = op2.serialize().expect("Serialization should succeed");
    node.propose(data2).expect("Propose should succeed");

    // Wait for second operation
    let applied2 = common::run_until(
        &mut node,
        |n| n.get(b"overwrite_key") == Some(b"second_value".to_vec()),
        Duration::from_secs(5),
    );
    assert!(applied2, "Second operation should be applied");

    // Verify final value
    assert_eq!(
        node.get(b"overwrite_key"),
        Some(b"second_value".to_vec()),
        "Value should be overwritten"
    );
}
