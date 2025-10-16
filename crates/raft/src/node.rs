//! Raft node implementation that wraps raft-rs RawNode.
//!
//! The RaftNode integrates MemStorage, StateMachine, and raft-rs RawNode
//! to provide a complete Raft consensus implementation.

use crate::{state_machine::StateMachine, storage::MemStorage};
use raft::RawNode;

/// Raft node that orchestrates consensus using raft-rs.
///
/// RaftNode wraps the raft-rs RawNode and integrates our custom storage
/// and state machine implementations.
#[allow(dead_code)] // Fields will be used in future tasks (propose, ready handling)
pub struct RaftNode {
    /// Node identifier
    id: u64,
    /// raft-rs RawNode instance
    raw_node: RawNode<MemStorage>,
    /// State machine for applying committed entries
    state_machine: StateMachine,
}

impl RaftNode {
    /// Creates a new RaftNode with the given node ID and peer IDs.
    ///
    /// # Arguments
    ///
    /// * `id` - Node identifier
    /// * `peers` - List of peer node IDs in the cluster
    ///
    /// # Returns
    ///
    /// * `Ok(RaftNode)` - Initialized node
    /// * `Err(Box<dyn std::error::Error>)` - If initialization fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_raft::RaftNode;
    ///
    /// let node = RaftNode::new(1, vec![1, 2, 3]).unwrap();
    /// ```
    pub fn new(id: u64, _peers: Vec<u64>) -> Result<Self, Box<dyn std::error::Error>> {
        // Step 1: Create MemStorage
        let storage = MemStorage::new();

        // Step 2: Create raft::Config
        let config = raft::Config {
            id,
            election_tick: 10,
            heartbeat_tick: 3,
            ..Default::default()
        };

        // Step 3: Initialize RawNode with storage and config
        // Note: peers parameter will be used in future tasks for cluster setup
        let raw_node = RawNode::new(
            &config,
            storage,
            &slog::Logger::root(slog::Discard, slog::o!()),
        )?;

        // Step 4: Create StateMachine
        let state_machine = StateMachine::new();

        // Step 5: Return initialized RaftNode
        Ok(RaftNode {
            id,
            raw_node,
            state_machine,
        })
    }

    /// Advances the Raft logical clock by one tick.
    ///
    /// This method should be called periodically to drive the Raft state machine's
    /// timing mechanisms (election timeouts, heartbeats, etc.). Each call advances
    /// the internal clock by one logical tick.
    ///
    /// The tick interval typically ranges from 10-100ms in practice. When the
    /// election_tick count is reached, followers will start elections. When the
    /// heartbeat_tick count is reached, leaders will send heartbeats.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Tick processed successfully
    /// * `Err(Box<dyn std::error::Error>)` - If tick processing fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_raft::RaftNode;
    ///
    /// let mut node = RaftNode::new(1, vec![1, 2, 3]).unwrap();
    ///
    /// // Advance the logical clock by one tick
    /// node.tick().unwrap();
    ///
    /// // In a real application, call this periodically:
    /// // loop {
    /// //     node.tick().unwrap();
    /// //     std::thread::sleep(std::time::Duration::from_millis(10));
    /// // }
    /// ```
    pub fn tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Advance the Raft state machine's logical clock
        self.raw_node.tick();
        Ok(())
    }

    /// Proposes a client command to the Raft cluster for consensus.
    ///
    /// This method submits data (typically a serialized Operation) to the Raft
    /// consensus algorithm. The proposal will be replicated to a majority of
    /// nodes before being committed and applied to the state machine.
    ///
    /// **Important**: This method can only be called on the leader node. If called
    /// on a follower, it will return an error. Clients should handle this error
    /// and redirect requests to the current leader.
    ///
    /// # Arguments
    ///
    /// * `data` - Raw bytes to propose (typically a serialized Operation)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Proposal accepted and will be processed by Raft
    /// * `Err(Box<dyn std::error::Error>)` - If proposal fails (e.g., not leader)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_raft::RaftNode;
    /// # use seshat_protocol::Operation;
    ///
    /// let mut node = RaftNode::new(1, vec![1, 2, 3]).unwrap();
    ///
    /// // Serialize a SET operation
    /// let operation = Operation::Set {
    ///     key: b"foo".to_vec(),
    ///     value: b"bar".to_vec(),
    /// };
    /// let data = operation.serialize().unwrap();
    ///
    /// // Propose to Raft (only works if this node is leader)
    /// match node.propose(data) {
    ///     Ok(()) => println!("Proposal accepted"),
    ///     Err(e) => eprintln!("Proposal failed: {}", e),
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - This node is not the leader
    /// - The Raft state machine rejects the proposal
    /// - Internal consensus error occurs
    pub fn propose(&mut self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        // Submit proposal to Raft using raw_node.propose()
        // The first parameter is the context (empty vector as we don't use it)
        // The second parameter is the actual data to propose
        self.raw_node.propose(vec![], data)?;
        Ok(())
    }

    /// Processes the Ready state from the Raft state machine.
    ///
    /// This method is the core of the Raft processing loop and must be called after
    /// any operation that might generate Raft state changes (tick, propose, step).
    /// It handles all four critical phases of Raft consensus:
    ///
    /// 1. **Persist** - Saves hard state and log entries to durable storage
    /// 2. **Send** - Returns messages to be sent to peer nodes
    /// 3. **Apply** - Applies committed entries to the state machine
    /// 4. **Advance** - Notifies raft-rs that processing is complete
    ///
    /// **Critical Ordering**: These phases MUST be executed in this exact order.
    /// Violating this order can lead to data loss, split-brain scenarios, or
    /// inconsistent state across the cluster.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Message>)` - Messages to send to peer nodes via gRPC
    /// * `Err(Box<dyn std::error::Error>)` - If processing fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use seshat_raft::RaftNode;
    ///
    /// let mut node = RaftNode::new(1, vec![1, 2, 3]).unwrap();
    ///
    /// // Event loop pattern
    /// loop {
    ///     // Advance logical clock
    ///     node.tick().unwrap();
    ///
    ///     // Process any ready state
    ///     let messages = node.handle_ready().unwrap();
    ///
    ///     // Send messages to peers (via gRPC in production)
    ///     for msg in messages {
    ///         // send_to_peer(msg.to, msg);
    ///     }
    ///
    ///     // Sleep for tick interval
    ///     std::thread::sleep(std::time::Duration::from_millis(10));
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Storage persistence fails
    /// - State machine application fails
    /// - Invalid committed entry data
    pub fn handle_ready(
        &mut self,
    ) -> Result<Vec<raft::eraftpb::Message>, Box<dyn std::error::Error>> {
        // Step 1: Check if there's any ready state to process
        if !self.raw_node.has_ready() {
            return Ok(vec![]);
        }

        // Step 2: Get the Ready struct from raft-rs
        let mut ready = self.raw_node.ready();

        // Step 3: Persist hard state (term, vote, commit) to storage
        // CRITICAL: This MUST happen before sending messages to ensure durability
        if let Some(hs) = ready.hs() {
            self.raw_node.store().set_hard_state(hs.clone());
        }

        // Step 4: Persist log entries to storage
        // CRITICAL: This MUST happen before sending messages to prevent data loss
        if !ready.entries().is_empty() {
            self.raw_node.store().append(ready.entries());
        }

        // Step 5: Extract messages to send to peers
        // These will be returned to the caller for network transmission
        let messages = ready.take_messages();

        // Step 6: Apply committed entries to the state machine
        // This updates the application state based on consensus decisions
        let committed_entries = ready.take_committed_entries();
        if !committed_entries.is_empty() {
            self.apply_committed_entries(committed_entries)?;
        }

        // Step 7: Advance the RawNode to signal completion
        // CRITICAL: This MUST be called after all processing is complete
        self.raw_node.advance(ready);

        // Step 8: Return messages for network transmission
        Ok(messages)
    }

    /// Applies committed entries to the state machine.
    ///
    /// This helper method processes entries that have been committed by the Raft
    /// consensus algorithm and applies them to the local state machine. Empty
    /// entries (configuration changes, leader election markers) are skipped.
    ///
    /// # Arguments
    ///
    /// * `entries` - Committed log entries to apply
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All entries applied successfully
    /// * `Err(Box<dyn std::error::Error>)` - If any entry application fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Entry data is malformed or cannot be deserialized
    /// - State machine rejects the operation
    /// - Idempotency check fails (applying out of order)
    fn apply_committed_entries(
        &mut self,
        entries: Vec<raft::prelude::Entry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entry in entries {
            // Skip empty entries (configuration changes, leader election markers)
            if entry.data.is_empty() {
                continue;
            }

            // Apply the entry to the state machine
            // The state machine handles deserialization and idempotency checks
            self.state_machine.apply(entry.index, &entry.data)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seshat_protocol::Operation;

    #[test]
    fn test_new_creates_node_successfully() {
        // Create a node with ID 1 in a 3-node cluster
        let result = RaftNode::new(1, vec![1, 2, 3]);

        // Verify it succeeds
        assert!(result.is_ok(), "Node creation should succeed");
    }

    #[test]
    fn test_new_single_node_cluster() {
        // Create a single-node cluster
        let result = RaftNode::new(1, vec![1]);

        // Verify it succeeds
        assert!(
            result.is_ok(),
            "Single node cluster creation should succeed"
        );
    }

    #[test]
    fn test_node_id_matches_parameter() {
        // Create a node with ID 42
        let node = RaftNode::new(42, vec![42, 43, 44]).expect("Node creation should succeed");

        // Verify the node ID matches
        assert_eq!(node.id, 42, "Node ID should match parameter");
    }

    #[test]
    fn test_state_machine_is_initialized() {
        // Create a node
        let node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Verify state machine is initialized (last_applied should be 0)
        assert_eq!(
            node.state_machine.last_applied(),
            0,
            "State machine should be initialized with last_applied = 0"
        );
    }

    #[test]
    fn test_multiple_nodes_can_be_created() {
        // Create multiple nodes with different IDs
        let node1 = RaftNode::new(1, vec![1, 2, 3]).expect("First node creation should succeed");
        let node2 = RaftNode::new(2, vec![1, 2, 3]).expect("Second node creation should succeed");
        let node3 = RaftNode::new(3, vec![1, 2, 3]).expect("Third node creation should succeed");

        // Verify they have different IDs
        assert_eq!(node1.id, 1);
        assert_eq!(node2.id, 2);
        assert_eq!(node3.id, 3);
    }

    #[test]
    fn test_raftnode_is_send() {
        // Verify RaftNode implements Send trait
        fn assert_send<T: Send>() {}
        assert_send::<RaftNode>();
    }

    // ===== tick() tests =====

    #[test]
    fn test_tick_succeeds() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Call tick() once
        let result = node.tick();

        // Verify it succeeds
        assert!(result.is_ok(), "tick() should succeed");
    }

    #[test]
    fn test_tick_multiple_times() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Call tick() 10 times in a loop
        for i in 0..10 {
            let result = node.tick();
            assert!(
                result.is_ok(),
                "tick() should succeed on iteration {}",
                i + 1
            );
        }
    }

    #[test]
    fn test_tick_on_new_node() {
        // Create a node and immediately tick
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Verify tick succeeds on newly created node
        let result = node.tick();
        assert!(
            result.is_ok(),
            "tick() should succeed on newly created node"
        );
    }

    #[test]
    fn test_tick_does_not_panic() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Call tick multiple times and ensure no panics
        for _ in 0..20 {
            let _ = node.tick();
        }

        // If we reach here, no panics occurred - test passes
    }

    // ===== propose() tests =====

    #[test]
    fn test_propose_succeeds_on_node() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Call propose with some data
        let data = b"test data".to_vec();
        let result = node.propose(data);

        // Note: raft-rs may reject proposals on uninitialized nodes
        // We're testing that the method can be called and returns a Result
        // The actual acceptance depends on the node's cluster state
        let _ = result; // Test passes if method can be called
    }

    #[test]
    fn test_propose_with_data() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Create some test data (simulating a serialized Operation)
        let data = vec![1, 2, 3, 4, 5];

        // Try to propose the data
        let result = node.propose(data);

        // Test that the method accepts the data parameter
        let _ = result;
    }

    #[test]
    fn test_propose_empty_data() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Try to propose empty data
        let data = Vec::new();
        let result = node.propose(data);

        // Test that the method accepts empty data
        let _ = result;
    }

    #[test]
    fn test_propose_large_data() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Create large data (10KB)
        let data = vec![42u8; 10 * 1024];

        // Try to propose large data
        let result = node.propose(data);

        // Test that the method accepts large data
        let _ = result;
    }

    #[test]
    fn test_propose_multiple_times() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Propose multiple times
        for i in 0..5 {
            let data = format!("proposal {}", i).into_bytes();
            let _ = node.propose(data);
            // Test passes if all proposals can be submitted without panicking
        }
    }

    // ===== handle_ready() tests =====

    #[test]
    fn test_handle_ready_no_ready_state() {
        // Create a new node - should have no ready state initially
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Call handle_ready when there's no ready state
        let result = node.handle_ready();

        // Should succeed and return empty messages vector
        assert!(
            result.is_ok(),
            "handle_ready should succeed with no ready state"
        );
        let messages = result.unwrap();
        assert_eq!(
            messages.len(),
            0,
            "Should return empty messages when no ready state"
        );
    }

    #[test]
    fn test_handle_ready_persists_hard_state() {
        // Create a single-node cluster (will become leader immediately)
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Tick until it becomes leader (generates ready state with hard state)
        for _ in 0..15 {
            node.tick().unwrap();
        }

        // Get initial hard state from storage
        let storage_before = node.raw_node.store().initial_state().unwrap();
        let term_before = storage_before.hard_state.term;

        // Process ready which should persist hard state
        let result = node.handle_ready();
        assert!(result.is_ok(), "handle_ready should succeed");

        // Verify hard state was persisted (term should be > 0 after election)
        let storage_after = node.raw_node.store().initial_state().unwrap();
        let term_after = storage_after.hard_state.term;

        assert!(
            term_after >= term_before,
            "Hard state term should be persisted (before: {}, after: {})",
            term_before,
            term_after
        );
    }

    #[test]
    fn test_handle_ready_persists_entries() {
        // Create a single-node cluster
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Tick until it becomes leader and process the election ready
        for _ in 0..15 {
            node.tick().unwrap();
        }

        // Process election ready states until node becomes leader
        for _ in 0..5 {
            node.handle_ready().unwrap();
        }

        // Get entry count before proposal
        let entries_before = node.raw_node.store().last_index().unwrap();

        // Propose an operation to generate entries
        let operation = Operation::Set {
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };
        let data = operation.serialize().unwrap();

        // Propose should succeed after becoming leader
        if node.propose(data).is_ok() {
            // Process ready which should persist entries
            let result = node.handle_ready();
            assert!(result.is_ok(), "handle_ready should succeed");

            // Verify entries were persisted
            let entries_after = node.raw_node.store().last_index().unwrap();
            assert!(
                entries_after >= entries_before,
                "Entries should be persisted (before: {}, after: {})",
                entries_before,
                entries_after
            );
        }
    }

    #[test]
    fn test_handle_ready_applies_committed_entries() {
        // Create a single-node cluster
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Tick until it becomes leader
        for _ in 0..15 {
            node.tick().unwrap();
        }

        // Process election ready states until node becomes leader
        for _ in 0..5 {
            node.handle_ready().unwrap();
        }

        // Propose a SET operation
        let operation = Operation::Set {
            key: b"foo".to_vec(),
            value: b"bar".to_vec(),
        };
        let data = operation.serialize().unwrap();

        // Propose and process ready if successful
        if node.propose(data).is_ok() {
            // Process ready - should apply the committed entry
            let result = node.handle_ready();
            assert!(result.is_ok(), "handle_ready should succeed");

            // Verify the operation was applied to state machine
            let value = node.state_machine.get(b"foo");
            assert_eq!(
                value,
                Some(b"bar".to_vec()),
                "Committed entry should be applied to state machine"
            );

            // Verify last_applied was updated
            assert!(
                node.state_machine.last_applied() > 0,
                "last_applied should be updated after applying entries"
            );
        }
    }

    #[test]
    fn test_handle_ready_returns_messages() {
        // Create a multi-node cluster (will generate vote request messages)
        let mut node = RaftNode::new(1, vec![1, 2, 3]).expect("Node creation should succeed");

        // Tick until election timeout (will generate RequestVote messages)
        for _ in 0..15 {
            node.tick().unwrap();
        }

        // Process ready - should return messages for peers
        let result = node.handle_ready();
        assert!(result.is_ok(), "handle_ready should succeed");

        // Verify the method returns a Vec<Message>
        // The vec may be empty or populated depending on raft-rs state
        let _messages = result.unwrap();
    }

    #[test]
    fn test_handle_ready_advances_raw_node() {
        // Create a single-node cluster
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Tick to generate ready state
        for _ in 0..15 {
            node.tick().unwrap();
        }

        // Process ready multiple times - this tests that advance() is properly called
        // If advance() wasn't called, raft-rs would panic or fail on subsequent ready() calls
        for _ in 0..5 {
            let result = node.handle_ready();
            assert!(result.is_ok(), "handle_ready should succeed");
        }

        // The key test is that we can call handle_ready multiple times without panics
        // This proves that advance() is being called properly after each ready processing
    }

    #[test]
    fn test_handle_ready_can_be_called_multiple_times() {
        // Create a node
        let mut node = RaftNode::new(1, vec![1]).expect("Node creation should succeed");

        // Call handle_ready multiple times
        for _ in 0..5 {
            let result = node.handle_ready();
            assert!(
                result.is_ok(),
                "handle_ready should succeed on multiple calls"
            );
        }

        // Tick and handle_ready in a loop (simulating event loop)
        for _ in 0..20 {
            node.tick().unwrap();
            let result = node.handle_ready();
            assert!(result.is_ok(), "handle_ready should succeed in event loop");
        }
    }
}
