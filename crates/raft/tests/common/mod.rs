//! Common test utilities for Raft integration tests.
//!
//! This module provides helper functions for creating test clusters,
//! running event loops, and waiting for specific conditions.

use seshat_raft::RaftNode;
use std::time::{Duration, Instant};

/// Runs the event loop (tick + handle_ready) until a condition is met or timeout occurs.
///
/// # Arguments
///
/// * `node` - The RaftNode to run the event loop on
/// * `condition` - Function that returns true when the desired state is reached
/// * `timeout` - Maximum time to wait for the condition
///
/// # Returns
///
/// * `true` - Condition was met within timeout
/// * `false` - Timeout occurred before condition was met
///
/// # Examples
///
/// ```no_run
/// use seshat_raft::RaftNode;
/// use std::time::Duration;
///
/// let mut node = RaftNode::new(1, vec![1]).unwrap();
///
/// // Run until node becomes leader or 5 seconds pass
/// let became_leader = run_until(
///     &mut node,
///     |n| n.is_leader(),
///     Duration::from_secs(5),
/// );
/// ```
pub fn run_until<F>(node: &mut RaftNode, condition: F, timeout: Duration) -> bool
where
    F: Fn(&RaftNode) -> bool,
{
    let start = Instant::now();

    while !condition(node) {
        if start.elapsed() >= timeout {
            return false;
        }

        // Tick to advance Raft logical clock
        node.tick().expect("Tick failed");

        // Process any ready state
        node.handle_ready().expect("Handle ready failed");

        // Small sleep to avoid tight loop
        std::thread::sleep(Duration::from_millis(10));
    }

    true
}

/// Creates a single-node cluster for testing.
///
/// # Arguments
///
/// * `id` - Node identifier
///
/// # Returns
///
/// * `RaftNode` - Initialized single-node cluster
///
/// # Panics
///
/// Panics if node creation fails
///
/// # Examples
///
/// ```no_run
/// let mut node = create_single_node_cluster(1);
/// ```
pub fn create_single_node_cluster(id: u64) -> RaftNode {
    RaftNode::new(id, vec![id]).expect("Failed to create single-node cluster")
}
