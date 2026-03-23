use relyo_network::NetworkConfig;
use relyo_consensus::engine::ConsensusConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Full node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node identity name (human-readable).
    pub node_name: String,
    /// Data directory for storage.
    pub data_dir: PathBuf,
    /// Network configuration.
    pub network: NetworkConfig,
    /// Consensus configuration.
    pub consensus: ConsensusConfig,
    /// Explorer API bind address.
    pub explorer_bind: SocketAddr,
    /// Whether to enable the explorer API.
    pub explorer_enabled: bool,
    /// JSON-RPC server bind address.
    pub rpc_bind: SocketAddr,
    /// Whether to enable the JSON-RPC server.
    pub rpc_enabled: bool,
    /// Mempool max size.
    pub mempool_max_size: usize,
    /// Rate limiter: max transactions per second per address.
    pub rate_limit_tps: u32,
    /// Minimum PoW difficulty (leading zero bits) required for transactions.
    /// Whether to enable persistent storage (redb).
    pub persistence_enabled: bool,
    /// Log level.
    pub log_level: String,
    /// Log file path (empty = stdout only).
    pub log_file: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            node_name: "relyo-node".to_string(),
            data_dir: PathBuf::from("data"),
            network: NetworkConfig::default(),
            consensus: ConsensusConfig::default(),
            explorer_bind: SocketAddr::from(([0, 0, 0, 0], 8080)),
            explorer_enabled: true,
            rpc_bind: SocketAddr::from(([0, 0, 0, 0], 9090)),
            rpc_enabled: true,
            mempool_max_size: 100_000,
            rate_limit_tps: 100,
            persistence_enabled: true,
            log_level: "info".to_string(),
            log_file: String::new(),
        }
    }
}

impl NodeConfig {
    /// Load configuration from a TOML file, falling back to defaults.
    pub fn load(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        if path.as_ref().exists() {
            let content = std::fs::read_to_string(path)?;
            let config: NodeConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(NodeConfig::default())
        }
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
