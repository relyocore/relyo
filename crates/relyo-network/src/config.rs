use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Weights for peer scoring events.
/// Each weight is applied when the corresponding event occurs,
/// adjusting the peer's reputation score accordingly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScoringConfig {
    /// Reputation awarded for relaying a valid transaction.
    pub valid_transaction_weight: i64,
    /// Reputation penalty for sending an invalid transaction.
    pub invalid_transaction_weight: i64,
    /// Reputation awarded for a timely response to a request.
    pub timely_response_weight: i64,
    /// Reputation penalty for a slow response.
    pub slow_response_weight: i64,
    /// Reputation penalty for sending an invalid/unparseable message.
    pub invalid_message_weight: i64,
    /// Reputation awarded for completing a successful sync session.
    pub good_sync_weight: i64,
    /// Reputation penalty for a protocol violation (wrong version, abuse, etc.).
    pub protocol_violation_weight: i64,
    /// The reputation threshold below which a peer gets banned.
    pub ban_threshold: i64,
    /// Initial reputation score assigned to newly connected peers.
    pub initial_reputation: i64,
}

impl Default for PeerScoringConfig {
    fn default() -> Self {
        PeerScoringConfig {
            valid_transaction_weight: 2,
            invalid_transaction_weight: -10,
            timely_response_weight: 1,
            slow_response_weight: -1,
            invalid_message_weight: -20,
            good_sync_weight: 5,
            protocol_violation_weight: -50,
            ban_threshold: -100,
            initial_reputation: 100,
        }
    }
}

/// Network configuration for a Relyo node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listen address (multiaddr format, e.g. "/ip4/0.0.0.0").
    pub listen_addr: String,
    /// External address to advertise to peers.
    pub external_addr: Option<String>,
    /// Bootstrap peer addresses in multiaddr format.
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of connected peers.
    pub max_peers: usize,
    /// Port for the QUIC transport.
    pub quic_port: u16,
    /// Port for the TCP transport.
    pub tcp_port: u16,
    /// Gossipsub heartbeat interval in milliseconds.
    pub gossip_heartbeat_ms: u64,
    /// Path to store the node's identity keypair.
    pub identity_path: PathBuf,
    /// Peer scoring configuration with weights for different behaviors.
    pub peer_scoring: PeerScoringConfig,
    /// Duration in seconds for which a banned peer remains banned.
    pub ban_duration_secs: u64,
    /// Number of transactions to request per sync batch.
    pub sync_batch_size: usize,
    /// Maximum number of concurrent sync sessions with different peers.
    pub max_concurrent_syncs: usize,
    /// Interval in seconds between heartbeat broadcasts.
    pub heartbeat_interval_secs: u64,
    /// Timeout in seconds for establishing a new connection.
    pub connection_timeout_secs: u64,
    /// Maximum number of inbound connections to accept.
    pub max_inbound_connections: usize,
    /// Maximum number of outbound connections to initiate.
    pub max_outbound_connections: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            listen_addr: "/ip4/0.0.0.0".to_string(),
            external_addr: None,
            bootstrap_peers: vec![
                "/dns/seed1.relyo.network/tcp/9741".to_string(),
                "/dns/seed2.relyo.network/tcp/9741".to_string(),
                "/dns/seed3.relyo.network/tcp/9741".to_string(),
            ],
            max_peers: 50,
            quic_port: 9740,
            tcp_port: 9741,
            gossip_heartbeat_ms: 700,
            identity_path: PathBuf::from("node_identity.key"),
            peer_scoring: PeerScoringConfig::default(),
            ban_duration_secs: 3600,
            sync_batch_size: 500,
            max_concurrent_syncs: 5,
            heartbeat_interval_secs: 30,
            connection_timeout_secs: 10,
            max_inbound_connections: 100,
            max_outbound_connections: 50,
        }
    }
}

impl NetworkConfig {
    /// Build the TCP multiaddr string.
    pub fn tcp_multiaddr(&self) -> String {
        format!("{}/tcp/{}", self.listen_addr, self.tcp_port)
    }

    /// Build the QUIC multiaddr string.
    pub fn quic_multiaddr(&self) -> String {
        format!("{}/udp/{}/quic-v1", self.listen_addr, self.quic_port)
    }

    /// Total maximum connections (inbound + outbound).
    pub fn max_total_connections(&self) -> usize {
        self.max_inbound_connections + self.max_outbound_connections
    }

    /// Validate the configuration, returning an error message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.tcp_port == self.quic_port {
            return Err("TCP and QUIC ports must be different".to_string());
        }
        if self.max_peers == 0 {
            return Err("max_peers must be greater than zero".to_string());
        }
        if self.sync_batch_size == 0 {
            return Err("sync_batch_size must be greater than zero".to_string());
        }
        if self.heartbeat_interval_secs == 0 {
            return Err("heartbeat_interval_secs must be greater than zero".to_string());
        }
        if self.connection_timeout_secs == 0 {
            return Err("connection_timeout_secs must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert_eq!(config.tcp_port, 9741);
        assert_eq!(config.quic_port, 9740);
        assert_eq!(config.ban_duration_secs, 3600);
        assert_eq!(config.sync_batch_size, 500);
        assert_eq!(config.max_concurrent_syncs, 5);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.connection_timeout_secs, 10);
        assert_eq!(config.max_inbound_connections, 100);
        assert_eq!(config.max_outbound_connections, 50);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_multiaddr_building() {
        let config = NetworkConfig::default();
        assert_eq!(config.tcp_multiaddr(), "/ip4/0.0.0.0/tcp/9741");
        assert_eq!(config.quic_multiaddr(), "/ip4/0.0.0.0/udp/9740/quic-v1");
    }

    #[test]
    fn test_max_total_connections() {
        let config = NetworkConfig::default();
        assert_eq!(config.max_total_connections(), 150);
    }

    #[test]
    fn test_peer_scoring_defaults() {
        let scoring = PeerScoringConfig::default();
        assert_eq!(scoring.valid_transaction_weight, 2);
        assert_eq!(scoring.invalid_transaction_weight, -10);
        assert_eq!(scoring.protocol_violation_weight, -50);
        assert_eq!(scoring.ban_threshold, -100);
        assert_eq!(scoring.initial_reputation, 100);
    }

    #[test]
    fn test_validation_same_ports() {
        let config = NetworkConfig {
            tcp_port: 9740,
            quic_port: 9740,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_max_peers() {
        let config = NetworkConfig {
            max_peers: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = NetworkConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NetworkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tcp_port, config.tcp_port);
        assert_eq!(deserialized.quic_port, config.quic_port);
        assert_eq!(deserialized.ban_duration_secs, config.ban_duration_secs);
    }
}
