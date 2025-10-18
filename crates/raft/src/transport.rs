//! gRPC transport layer for Raft messages
//!
//! This module provides the network transport for sending Raft messages between nodes.
//! It uses gRPC with our own protobuf definitions (latest tonic 0.14 / prost 0.14) and
//! converts between our messages and raft-rs's `eraftpb::Message` types.
//!
//! # Architecture
//!
//! Each node runs:
//! - **1 gRPC Server**: Receives messages from all peers
//! - **N-1 gRPC Clients**: Sends messages to each peer
//!
//! # Example
//!
//! ```rust,no_run
//! use seshat_raft::transport::{TransportServer, TransportClient};
//! use tokio::sync::mpsc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create channel for incoming messages
//! let (tx, mut rx) = mpsc::channel(100);
//!
//! // Start server
//! let server = TransportServer::new(tx);
//! tokio::spawn(async move {
//!     tonic::transport::Server::builder()
//!         .add_service(server.into_service())
//!         .serve("0.0.0.0:7379".parse().unwrap())
//!         .await
//! });
//!
//! // Create client to peer
//! let mut client = TransportClient::connect("http://peer:7379").await?;
//! # Ok(())
//! # }
//! ```

use raft::eraftpb;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

// Include the generated protobuf code
// This uses prost 0.13 (latest)
pub mod proto {
    tonic::include_proto!("transport");
}

pub use proto::{
    raft_transport_client::RaftTransportClient, raft_transport_server::RaftTransport,
    raft_transport_server::RaftTransportServer,
};

/// Errors that can occur in the transport layer
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("gRPC transport error: {0}")]
    GrpcTransport(#[from] tonic::transport::Error),

    #[error("gRPC status error: {0}")]
    GrpcStatus(#[source] Box<tonic::Status>),

    #[error("Failed to send message to channel")]
    ChannelSend,

    #[error("Message conversion error: {0}")]
    Conversion(String),
}

impl From<tonic::Status> for TransportError {
    fn from(status: tonic::Status) -> Self {
        TransportError::GrpcStatus(Box::new(status))
    }
}

/// Convert our proto `RaftMessage` to raft-rs's `eraftpb::Message`
///
/// This bridges the gap between our latest prost 0.14 types and raft-rs's prost 0.11 types.
pub fn to_eraftpb(msg: proto::RaftMessage) -> Result<eraftpb::Message, TransportError> {
    // Serialize our message using prost 0.14
    let bytes = {
        use prost::Message as ProstMessage14;
        msg.encode_to_vec()
    };

    // Deserialize into raft-rs message using prost 0.11
    {
        use prost_old::Message as ProstMessage11;
        eraftpb::Message::decode(&bytes[..]).map_err(|e| TransportError::Conversion(e.to_string()))
    }
}

/// Convert raft-rs's `eraftpb::Message` to our proto `RaftMessage`
///
/// This bridges the gap between raft-rs's prost 0.11 types and our latest prost 0.14 types.
pub fn from_eraftpb(msg: eraftpb::Message) -> Result<proto::RaftMessage, TransportError> {
    // Serialize raft-rs message using prost 0.11
    let bytes = {
        use prost_old::Message as ProstMessage11;
        msg.encode_to_vec()
    };

    // Deserialize into our message using prost 0.14
    {
        use prost::Message as ProstMessage14;
        proto::RaftMessage::decode(&bytes[..])
            .map_err(|e| TransportError::Conversion(e.to_string()))
    }
}

/// gRPC server that receives Raft messages from peers
///
/// The server immediately enqueues messages to a channel and returns success.
/// The actual processing happens in the event loop.
pub struct TransportServer {
    /// Channel sender for incoming messages
    msg_tx: mpsc::Sender<eraftpb::Message>,
}

impl TransportServer {
    /// Create a new transport server
    ///
    /// # Arguments
    /// * `msg_tx` - Channel sender for enqueuing incoming messages
    pub fn new(msg_tx: mpsc::Sender<eraftpb::Message>) -> Self {
        Self { msg_tx }
    }

    /// Convert into a gRPC service
    pub fn into_service(self) -> RaftTransportServer<Self> {
        RaftTransportServer::new(self)
    }
}

#[tonic::async_trait]
impl RaftTransport for TransportServer {
    async fn send_message(
        &self,
        request: Request<proto::RaftMessage>,
    ) -> Result<Response<proto::SendMessageResponse>, Status> {
        let wire_msg = request.into_inner();

        // Convert from our proto to eraftpb
        let raft_msg = to_eraftpb(wire_msg)
            .map_err(|e| Status::invalid_argument(format!("Failed to convert message: {e}")))?;

        // Enqueue for processing (non-blocking)
        self.msg_tx
            .try_send(raft_msg)
            .map_err(|_| Status::resource_exhausted("Message queue full"))?;

        Ok(Response::new(proto::SendMessageResponse {
            success: true,
            error: String::new(),
        }))
    }
}

/// gRPC client for sending messages to a peer
pub struct TransportClient {
    client: RaftTransportClient<tonic::transport::Channel>,
    peer_addr: String,
}

impl TransportClient {
    /// Connect to a peer
    ///
    /// # Arguments
    /// * `addr` - Peer address (e.g., "http://localhost:7379")
    pub async fn connect(addr: impl Into<String>) -> Result<Self, TransportError> {
        let peer_addr = addr.into();
        let client = RaftTransportClient::connect(peer_addr.clone()).await?;

        Ok(Self { client, peer_addr })
    }

    /// Send a Raft message to the peer
    pub async fn send(&mut self, msg: eraftpb::Message) -> Result<(), TransportError> {
        // Convert from eraftpb to our proto
        let wire_msg = from_eraftpb(msg)?;

        // Send via gRPC
        let response = self.client.send_message(Request::new(wire_msg)).await?;

        let result = response.into_inner();
        if !result.success {
            return Err(TransportError::Conversion(result.error));
        }

        Ok(())
    }

    /// Get the peer address
    pub fn peer_addr(&self) -> &str {
        &self.peer_addr
    }
}

/// Pool of clients for sending messages to multiple peers
pub struct TransportClientPool {
    clients: HashMap<u64, TransportClient>,
    peer_addrs: HashMap<u64, String>,
}

impl TransportClientPool {
    /// Create a new empty client pool
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            peer_addrs: HashMap::new(),
        }
    }

    /// Register a peer address
    ///
    /// # Arguments
    /// * `peer_id` - Peer node ID
    /// * `addr` - Peer address (e.g., "http://localhost:7379")
    pub fn add_peer(&mut self, peer_id: u64, addr: String) {
        self.peer_addrs.insert(peer_id, addr);
    }

    /// Send a message to a peer
    ///
    /// Lazily connects to the peer on first send.
    pub async fn send_to_peer(
        &mut self,
        peer_id: u64,
        msg: eraftpb::Message,
    ) -> Result<(), TransportError> {
        // Get or create client for this peer
        if !self.clients.contains_key(&peer_id) {
            let addr = self
                .peer_addrs
                .get(&peer_id)
                .ok_or_else(|| TransportError::Conversion(format!("Unknown peer ID: {peer_id}")))?
                .clone();

            let client = TransportClient::connect(addr).await?;
            self.clients.insert(peer_id, client);
        }

        // Send message
        let client = self.clients.get_mut(&peer_id).unwrap();
        client.send(msg).await
    }

    /// Remove a peer from the pool
    pub fn remove_peer(&mut self, peer_id: u64) {
        self.clients.remove(&peer_id);
        self.peer_addrs.remove(&peer_id);
    }
}

impl Default for TransportClientPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_pool_add_peer() {
        let mut pool = TransportClientPool::new();
        pool.add_peer(1, "http://localhost:7001".to_string());
        pool.add_peer(2, "http://localhost:7002".to_string());

        assert_eq!(pool.peer_addrs.len(), 2);
    }

    #[test]
    fn test_client_pool_remove_peer() {
        let mut pool = TransportClientPool::new();
        pool.add_peer(1, "http://localhost:7001".to_string());
        pool.add_peer(2, "http://localhost:7002".to_string());

        pool.remove_peer(1);
        assert_eq!(pool.peer_addrs.len(), 1);
        assert!(!pool.peer_addrs.contains_key(&1));
    }
}
