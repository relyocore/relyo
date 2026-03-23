use serde::{Deserialize, Serialize};

/// Unique identifier for a node in the network.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        NodeId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Performance scores for a node, used to compute reward share.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScore {
    pub uptime_hours: f64,
    pub validated_txs: u64,
    pub relayed_bytes: u64,
}

impl NodeScore {
    /// Weighted score: 40% uptime + 40% validation + 20% bandwidth.
    pub fn compute(
        &self,
        total_network_validations: u64,
        total_network_bandwidth: u64,
    ) -> f64 {
        let uptime = (self.uptime_hours / 24.0).min(1.0);
        let validation = if total_network_validations > 0 {
            self.validated_txs as f64 / total_network_validations as f64
        } else {
            0.0
        };
        let bandwidth = if total_network_bandwidth > 0 {
            self.relayed_bytes as f64 / total_network_bandwidth as f64
        } else {
            0.0
        };

        0.40 * uptime + 0.40 * validation + 0.20 * bandwidth
    }
}

/// Network-level statistics snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_transactions: u64,
    pub confirmed_transactions: u64,
    pub pending_transactions: u64,
    pub rejected_transactions: u64,
    pub active_nodes: u64,
    pub transactions_per_second: f64,
    pub average_confirmation_ms: f64,
    pub total_staked: u64,
    pub total_circulating: u64,
    pub mempool_size: u64,
    pub dag_depth: u64,
}

/// Validator information for staking system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub node_id: NodeId,
    pub stake_amount: u64,
    pub is_active: bool,
    pub slash_count: u32,
    pub registered_at: u64,
    pub last_heartbeat: u64,
    pub accumulated_rewards: u64,
}

/// Staking configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingConfig {
    pub min_stake: u64,
    pub lock_period_epochs: u64,
    pub lock_period_days: u64,
    pub slash_penalty_bps: u64,
    pub max_validators: usize,
    pub unbonding_epochs: u64,
}

impl Default for StakingConfig {
    fn default() -> Self {
        Self {
            min_stake: 10_000 * 100_000_000, // 10,000 RLY in base units
            lock_period_epochs: 10,  // Reduced from 100 to 10
            lock_period_days: 30,    // 30 Real Time Days Floor
            slash_penalty_bps: 1000, // 10%
            max_validators: 1000,
            unbonding_epochs: 5,     // Reduced from 50 to 5
        }
    }
}

/// A checkpoint capturing a snapshot of the network state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub epoch: u64,
    pub dag_depth: u64,
    pub merkle_root: [u8; 32],
    pub state_hash: [u8; 32],
    pub timestamp: u64,
    pub transaction_count: u64,
    pub validator_signatures: Vec<(NodeId, Vec<u8>)>,
}

/// Peer reputation score for network health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScore {
    pub reputation: i64,
    pub good_responses: u64,
    pub bad_responses: u64,
    pub invalid_messages: u64,
    pub latency_ms_avg: u64,
    pub bytes_served: u64,
    pub banned_until: Option<u64>,
}

impl Default for PeerScore {
    fn default() -> Self {
        Self {
            reputation: 100,
            good_responses: 0,
            bad_responses: 0,
            invalid_messages: 0,
            latency_ms_avg: 0,
            bytes_served: 0,
            banned_until: None,
        }
    }
}

/// Dynamic fee market state tracking mempool congestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeMarket {
    pub base_fee: u64,
    pub fee_multiplier_bps: u64,
    pub min_fee: u64,
    pub max_fee: u64,
    pub congestion_level: f64,
}

impl FeeMarket {
    /// Calculate the effective fee given current congestion.
    pub fn effective_fee(&self) -> u64 {
        let fee = (self.base_fee as f64 * self.fee_multiplier_bps as f64 / 10_000.0) as u64;
        fee.max(self.min_fee).min(self.max_fee)
    }

    /// Update congestion based on mempool size relative to capacity.
    pub fn update_congestion(&mut self, mempool_size: usize, mempool_capacity: usize) {
        self.congestion_level = if mempool_capacity > 0 {
            mempool_size as f64 / mempool_capacity as f64
        } else {
            0.0
        };
        // Exponential fee adjustment based on congestion
        if self.congestion_level > 0.8 {
            self.fee_multiplier_bps = (self.fee_multiplier_bps * 11 / 10).min(100_000);
        } else if self.congestion_level < 0.2 {
            self.fee_multiplier_bps = (self.fee_multiplier_bps * 9 / 10).max(10_000);
        }
    }
}

impl Default for FeeMarket {
    fn default() -> Self {
        use crate::token::RELYO_CONFIG;
        Self {
            base_fee: RELYO_CONFIG.base_fee,
            fee_multiplier_bps: 10_000, // 1x = no multiplier
            min_fee: RELYO_CONFIG.base_fee,
            max_fee: RELYO_CONFIG.base_fee * 1000,
            congestion_level: 0.0,
        }
    }
}

/// Slashing offense types with severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlashingOffense {
    DoubleVote,
    ExtendedDowntime,
    InvalidBlock,
    CensorTransaction,
}

impl SlashingOffense {
    /// Penalty in basis points of the validator's stake.
    pub fn penalty_bps(&self) -> u64 {
        match self {
            SlashingOffense::DoubleVote => 5000,          // 50%
            SlashingOffense::ExtendedDowntime => 500,       // 5%
            SlashingOffense::InvalidBlock => 3000,          // 30%
            SlashingOffense::CensorTransaction => 1000,     // 10%
        }
    }
}

/// Bandwidth tracking per peer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BandwidthStats {
    pub bytes_sent: AtomicU64Wrapper,
    pub bytes_received: AtomicU64Wrapper,
    pub messages_sent: AtomicU64Wrapper,
    pub messages_received: AtomicU64Wrapper,
}

/// Wrapper around u64 for serde compatibility (AtomicU64 doesn't implement Serialize).
#[derive(Debug, Clone, Default)]
pub struct AtomicU64Wrapper(pub u64);

impl Serialize for AtomicU64Wrapper {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for AtomicU64Wrapper {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        u64::deserialize(deserializer).map(AtomicU64Wrapper)
    }
}

/// Epoch number for consensus rounds.
pub type Epoch = u64;

/// Timestamp in milliseconds since UNIX epoch.
pub type Timestamp = u64;

/// Get the current timestamp in milliseconds.
pub fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_score_compute() {
        let score = NodeScore {
            uptime_hours: 24.0,
            validated_txs: 100,
            relayed_bytes: 1_000_000,
        };
        let s = score.compute(1000, 10_000_000);
        assert!(s > 0.0 && s <= 1.0);
    }

    #[test]
    fn test_fee_market_congestion() {
        let mut market = FeeMarket::default();
        let original_multiplier = market.fee_multiplier_bps;

        // High congestion should increase multiplier
        market.update_congestion(90, 100);
        assert!(market.fee_multiplier_bps > original_multiplier);

        // Low congestion should decrease multiplier
        let high_multiplier = market.fee_multiplier_bps;
        market.update_congestion(10, 100);
        assert!(market.fee_multiplier_bps < high_multiplier);
    }

    #[test]
    fn test_slashing_penalties() {
        assert_eq!(SlashingOffense::DoubleVote.penalty_bps(), 5000);
        assert_eq!(SlashingOffense::ExtendedDowntime.penalty_bps(), 500);
        assert!(SlashingOffense::DoubleVote.penalty_bps() > SlashingOffense::ExtendedDowntime.penalty_bps());
    }

    #[test]
    fn test_staking_config_defaults() {
        let config = StakingConfig::default();
        assert_eq!(config.min_stake, 10_000 * 100_000_000);
        assert_eq!(config.max_validators, 1000);
    }

    #[test]
    fn test_peer_score_default() {
        let score = PeerScore::default();
        assert_eq!(score.reputation, 100);
        assert!(score.banned_until.is_none());
    }
}
