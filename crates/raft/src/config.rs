//! Configuration types for Raft consensus.
//!
//! This module defines the configuration structures used to initialize and
//! configure Raft nodes and clusters.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Configuration for a single Raft node.
///
/// # Examples
///
/// ```
/// use seshat_raft::NodeConfig;
/// use std::path::PathBuf;
///
/// let config = NodeConfig {
///     id: 1,
///     client_addr: "0.0.0.0:6379".to_string(),
///     internal_addr: "0.0.0.0:7379".to_string(),
///     data_dir: PathBuf::from("/var/lib/seshat/node1"),
///     advertise_addr: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier. Must be > 0.
    pub id: u64,

    /// Address for client connections (Redis protocol).
    /// Example: "0.0.0.0:6379"
    pub client_addr: String,

    /// Address for internal Raft communication (gRPC).
    /// Example: "0.0.0.0:7379"
    pub internal_addr: String,

    /// Directory for persisting data.
    pub data_dir: PathBuf,

    /// Advertise address for other nodes to connect.
    /// Auto-detected if None.
    pub advertise_addr: Option<String>,
}

impl NodeConfig {
    /// Validates the node configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `id` is 0
    /// - `client_addr` is invalid
    /// - `internal_addr` is invalid
    /// - `data_dir` is not writable
    pub fn validate(&self) -> Result<(), String> {
        if self.id == 0 {
            return Err("node_id must be > 0".to_string());
        }

        // Basic address validation (non-empty)
        if self.client_addr.is_empty() {
            return Err("client_addr cannot be empty".to_string());
        }

        if self.internal_addr.is_empty() {
            return Err("internal_addr cannot be empty".to_string());
        }

        // Validate addresses contain port separator
        if !self.client_addr.contains(':') {
            return Err("client_addr must contain port (e.g., '0.0.0.0:6379')".to_string());
        }

        if !self.internal_addr.contains(':') {
            return Err("internal_addr must contain port (e.g., '0.0.0.0:7379')".to_string());
        }

        Ok(())
    }
}

/// Configuration for an initial cluster member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialMember {
    /// Node ID of the cluster member.
    pub id: u64,

    /// Internal address of the cluster member.
    /// Example: "kvstore-1:7379"
    pub addr: String,
}

/// Configuration for the Raft cluster.
///
/// # Examples
///
/// ```
/// use seshat_raft::{ClusterConfig, InitialMember};
///
/// let config = ClusterConfig {
///     bootstrap: true,
///     initial_members: vec![
///         InitialMember { id: 1, addr: "node1:7379".to_string() },
///         InitialMember { id: 2, addr: "node2:7379".to_string() },
///         InitialMember { id: 3, addr: "node3:7379".to_string() },
///     ],
///     replication_factor: 3,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Whether this node should bootstrap a new cluster.
    pub bootstrap: bool,

    /// Initial cluster members for bootstrapping.
    pub initial_members: Vec<InitialMember>,

    /// Number of replicas (must be 3 for Phase 1).
    pub replication_factor: usize,
}

impl ClusterConfig {
    /// Validates the cluster configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `initial_members` has fewer than 3 members
    /// - `initial_members` contains duplicate IDs
    /// - `node_id` is not in `initial_members`
    /// - `replication_factor` is not 3 (Phase 1 constraint)
    pub fn validate(&self, node_id: u64) -> Result<(), String> {
        // Check minimum cluster size
        if self.initial_members.len() < 3 {
            return Err(format!(
                "cluster must have at least 3 members, got {}",
                self.initial_members.len()
            ));
        }

        // Check for duplicate IDs
        let mut seen_ids = HashSet::new();
        for member in &self.initial_members {
            if !seen_ids.insert(member.id) {
                return Err(format!("duplicate node ID found: {}", member.id));
            }
        }

        // Check that node_id is in initial_members
        if !self.initial_members.iter().any(|m| m.id == node_id) {
            return Err(format!("node_id {node_id} not in initial_members"));
        }

        // Check replication factor (Phase 1 constraint)
        if self.replication_factor != 3 {
            return Err("replication_factor must be 3 for Phase 1".to_string());
        }

        Ok(())
    }
}

/// Raft timing and resource configuration.
///
/// # Examples
///
/// ```
/// use seshat_raft::RaftConfig;
///
/// // Use default values
/// let config = RaftConfig::default();
///
/// // Or customize
/// let config = RaftConfig {
///     heartbeat_interval_ms: 100,
///     election_timeout_min_ms: 500,
///     election_timeout_max_ms: 1000,
///     snapshot_interval_entries: 10_000,
///     snapshot_interval_bytes: 100 * 1024 * 1024,
///     max_log_size_bytes: 500 * 1024 * 1024,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Interval between heartbeats in milliseconds.
    /// Default: 100ms
    pub heartbeat_interval_ms: u64,

    /// Minimum election timeout in milliseconds.
    /// Default: 500ms
    pub election_timeout_min_ms: u64,

    /// Maximum election timeout in milliseconds.
    /// Default: 1000ms
    pub election_timeout_max_ms: u64,

    /// Number of log entries before triggering snapshot.
    /// Default: 10,000
    pub snapshot_interval_entries: u64,

    /// Bytes in log before triggering snapshot.
    /// Default: 100MB
    pub snapshot_interval_bytes: u64,

    /// Maximum log size in bytes before compaction.
    /// Default: 500MB
    pub max_log_size_bytes: u64,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 100,
            election_timeout_min_ms: 500,
            election_timeout_max_ms: 1000,
            snapshot_interval_entries: 10_000,
            snapshot_interval_bytes: 100 * 1024 * 1024,
            max_log_size_bytes: 500 * 1024 * 1024,
        }
    }
}

impl RaftConfig {
    /// Validates the Raft configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `election_timeout_min_ms` < `heartbeat_interval_ms * 2`
    /// - `election_timeout_max_ms` <= `election_timeout_min_ms`
    pub fn validate(&self) -> Result<(), String> {
        // Election timeout must be at least 2x heartbeat interval
        if self.election_timeout_min_ms < self.heartbeat_interval_ms * 2 {
            return Err(format!(
                "election_timeout_min_ms ({}) must be at least 2x heartbeat_interval_ms ({})",
                self.election_timeout_min_ms,
                self.heartbeat_interval_ms * 2
            ));
        }

        // Max timeout must be greater than min timeout
        if self.election_timeout_max_ms <= self.election_timeout_min_ms {
            return Err(format!(
                "election_timeout_max_ms ({}) must be > election_timeout_min_ms ({})",
                self.election_timeout_max_ms, self.election_timeout_min_ms
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_config_validation() {
        // Valid configuration
        let valid_config = NodeConfig {
            id: 1,
            client_addr: "0.0.0.0:6379".to_string(),
            internal_addr: "0.0.0.0:7379".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: None,
        };
        assert!(valid_config.validate().is_ok());

        // Invalid: node_id = 0
        let invalid_config = NodeConfig {
            id: 0,
            client_addr: "0.0.0.0:6379".to_string(),
            internal_addr: "0.0.0.0:7379".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: None,
        };
        assert!(invalid_config.validate().is_err());
        assert!(invalid_config
            .validate()
            .unwrap_err()
            .contains("node_id must be > 0"));
    }

    #[test]
    fn test_cluster_config_validation() {
        let members = vec![
            InitialMember {
                id: 1,
                addr: "node1:7379".to_string(),
            },
            InitialMember {
                id: 2,
                addr: "node2:7379".to_string(),
            },
            InitialMember {
                id: 3,
                addr: "node3:7379".to_string(),
            },
        ];

        // Valid configuration
        let valid_config = ClusterConfig {
            bootstrap: true,
            initial_members: members.clone(),
            replication_factor: 3,
        };
        assert!(valid_config.validate(1).is_ok());

        // Invalid: fewer than 3 members
        let invalid_config = ClusterConfig {
            bootstrap: true,
            initial_members: vec![
                InitialMember {
                    id: 1,
                    addr: "node1:7379".to_string(),
                },
                InitialMember {
                    id: 2,
                    addr: "node2:7379".to_string(),
                },
            ],
            replication_factor: 3,
        };
        assert!(invalid_config.validate(1).is_err());
        assert!(invalid_config
            .validate(1)
            .unwrap_err()
            .contains("at least 3 members"));

        // Invalid: duplicate IDs
        let invalid_config = ClusterConfig {
            bootstrap: true,
            initial_members: vec![
                InitialMember {
                    id: 1,
                    addr: "node1:7379".to_string(),
                },
                InitialMember {
                    id: 1,
                    addr: "node2:7379".to_string(),
                },
                InitialMember {
                    id: 3,
                    addr: "node3:7379".to_string(),
                },
            ],
            replication_factor: 3,
        };
        assert!(invalid_config.validate(1).is_err());
        assert!(invalid_config
            .validate(1)
            .unwrap_err()
            .contains("duplicate"));

        // Invalid: node_id not in members
        assert!(valid_config.validate(99).is_err());
        assert!(valid_config
            .validate(99)
            .unwrap_err()
            .contains("not in initial_members"));

        // Invalid: wrong replication factor
        let invalid_config = ClusterConfig {
            bootstrap: true,
            initial_members: members,
            replication_factor: 5,
        };
        assert!(invalid_config.validate(1).is_err());
        assert!(invalid_config
            .validate(1)
            .unwrap_err()
            .contains("replication_factor must be 3"));
    }

    #[test]
    fn test_raft_config_default() {
        let config = RaftConfig::default();
        assert_eq!(config.heartbeat_interval_ms, 100);
        assert_eq!(config.election_timeout_min_ms, 500);
        assert_eq!(config.election_timeout_max_ms, 1000);
        assert_eq!(config.snapshot_interval_entries, 10_000);
        assert_eq!(config.snapshot_interval_bytes, 100 * 1024 * 1024);
        assert_eq!(config.max_log_size_bytes, 500 * 1024 * 1024);
    }

    #[test]
    fn test_raft_config_validation() {
        // Valid configuration
        let valid_config = RaftConfig::default();
        assert!(valid_config.validate().is_ok());

        // Invalid: election_timeout_min too small
        let invalid_config = RaftConfig {
            heartbeat_interval_ms: 100,
            election_timeout_min_ms: 150,
            election_timeout_max_ms: 1000,
            snapshot_interval_entries: 10_000,
            snapshot_interval_bytes: 100 * 1024 * 1024,
            max_log_size_bytes: 500 * 1024 * 1024,
        };
        assert!(invalid_config.validate().is_err());
        assert!(invalid_config
            .validate()
            .unwrap_err()
            .contains("election_timeout_min_ms"));

        // Invalid: election_timeout_max <= election_timeout_min
        let invalid_config = RaftConfig {
            heartbeat_interval_ms: 100,
            election_timeout_min_ms: 500,
            election_timeout_max_ms: 500,
            snapshot_interval_entries: 10_000,
            snapshot_interval_bytes: 100 * 1024 * 1024,
            max_log_size_bytes: 500 * 1024 * 1024,
        };
        assert!(invalid_config.validate().is_err());
        assert!(invalid_config
            .validate()
            .unwrap_err()
            .contains("election_timeout_max_ms"));
    }

    #[test]
    fn test_serde_roundtrip_node_config() {
        let config = NodeConfig {
            id: 1,
            client_addr: "0.0.0.0:6379".to_string(),
            internal_addr: "0.0.0.0:7379".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: Some("public.example.com:7379".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NodeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.id, deserialized.id);
        assert_eq!(config.client_addr, deserialized.client_addr);
        assert_eq!(config.internal_addr, deserialized.internal_addr);
        assert_eq!(config.data_dir, deserialized.data_dir);
        assert_eq!(config.advertise_addr, deserialized.advertise_addr);
    }

    #[test]
    fn test_serde_roundtrip_cluster_config() {
        let config = ClusterConfig {
            bootstrap: true,
            initial_members: vec![
                InitialMember {
                    id: 1,
                    addr: "node1:7379".to_string(),
                },
                InitialMember {
                    id: 2,
                    addr: "node2:7379".to_string(),
                },
                InitialMember {
                    id: 3,
                    addr: "node3:7379".to_string(),
                },
            ],
            replication_factor: 3,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ClusterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.bootstrap, deserialized.bootstrap);
        assert_eq!(
            config.initial_members.len(),
            deserialized.initial_members.len()
        );
        assert_eq!(config.replication_factor, deserialized.replication_factor);
    }

    #[test]
    fn test_serde_roundtrip_raft_config() {
        let config = RaftConfig::default();

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RaftConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            config.heartbeat_interval_ms,
            deserialized.heartbeat_interval_ms
        );
        assert_eq!(
            config.election_timeout_min_ms,
            deserialized.election_timeout_min_ms
        );
        assert_eq!(
            config.election_timeout_max_ms,
            deserialized.election_timeout_max_ms
        );
        assert_eq!(
            config.snapshot_interval_entries,
            deserialized.snapshot_interval_entries
        );
        assert_eq!(
            config.snapshot_interval_bytes,
            deserialized.snapshot_interval_bytes
        );
        assert_eq!(config.max_log_size_bytes, deserialized.max_log_size_bytes);
    }

    #[test]
    fn test_node_config_empty_addresses() {
        let config = NodeConfig {
            id: 1,
            client_addr: "".to_string(),
            internal_addr: "0.0.0.0:7379".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: None,
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("client_addr"));

        let config = NodeConfig {
            id: 1,
            client_addr: "0.0.0.0:6379".to_string(),
            internal_addr: "".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: None,
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("internal_addr"));
    }

    #[test]
    fn test_node_config_invalid_address_format() {
        let config = NodeConfig {
            id: 1,
            client_addr: "0.0.0.0".to_string(), // Missing port
            internal_addr: "0.0.0.0:7379".to_string(),
            data_dir: PathBuf::from("/tmp/seshat/node1"),
            advertise_addr: None,
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("must contain port"));
    }

    #[test]
    fn test_initial_member_serialization() {
        let member = InitialMember {
            id: 1,
            addr: "node1:7379".to_string(),
        };
        let json = serde_json::to_string(&member).unwrap();
        let deserialized: InitialMember = serde_json::from_str(&json).unwrap();
        assert_eq!(member.id, deserialized.id);
        assert_eq!(member.addr, deserialized.addr);
    }
}
