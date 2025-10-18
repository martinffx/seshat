//! Raft consensus wrapper for Seshat distributed key-value store.
//!
//! This crate provides a Raft consensus implementation built on top of
//! `raft-rs`, with custom storage backends and gRPC transport integration.
//!
//! # Transport Layer
//!
//! The `transport` module provides gRPC-based networking for Raft messages:
//! - Uses latest tonic 0.14 / prost 0.14 for the wire protocol
//! - Automatically converts between our protobuf and raft-rs's `eraftpb` types
//! - Each node runs 1 server + N-1 clients (where N = cluster size)
//!
//! # Example
//!
//! ```rust,no_run
//! use seshat_raft::RaftNode;
//! use seshat_raft::transport::{TransportServer, TransportClientPool};
//! use tokio::sync::mpsc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create Raft node
//! let node = RaftNode::new(1, vec![1, 2, 3])?;
//!
//! // Setup transport
//! let (msg_tx, mut msg_rx) = mpsc::channel(100);
//! let server = TransportServer::new(msg_tx);
//!
//! // Start server
//! tokio::spawn(async move {
//!     tonic::transport::Server::builder()
//!         .add_service(server.into_service())
//!         .serve("0.0.0.0:7379".parse().unwrap())
//!         .await
//! });
//!
//! // Setup client pool
//! let mut clients = TransportClientPool::new();
//! clients.add_peer(2, "http://node2:7379".to_string());
//! clients.add_peer(3, "http://node3:7379".to_string());
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod node;
pub mod state_machine;
pub mod storage;
pub mod transport;

// Re-export main types for convenience
pub use config::{ClusterConfig, InitialMember, NodeConfig, RaftConfig};
pub use node::RaftNode;
pub use state_machine::StateMachine;
pub use storage::MemStorage;

// Re-export raft-rs message types
pub use raft::prelude::{Entry, Message, MessageType, Snapshot};
