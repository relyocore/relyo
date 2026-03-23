use relyo_core::{NodeId, NodeScore};
use relyo_core::token::RELYO_CONFIG;
use std::collections::HashMap;

/// Calculates node rewards based on the weighted performance score model.
///
/// Each node is scored on:
/// - 40% uptime (hours / 24)
/// - 40% validation participation
/// - 20% bandwidth contribution
///
/// Rewards are then distributed proportionally from the daily pool.
pub struct RewardCalculator {
    daily_pool: u64,
}

impl RewardCalculator {
    pub fn new() -> Self {
        RewardCalculator {
            daily_pool: RELYO_CONFIG.daily_reward_emission,
        }
    }

    pub fn with_pool(daily_pool: u64) -> Self {
        RewardCalculator { daily_pool }
    }

    /// Calculate rewards for all nodes in a given epoch.
    ///
    /// Returns a map of NodeId → reward amount in base units.
    pub fn calculate(
        &self,
        nodes: &HashMap<NodeId, NodeScore>,
        total_network_validations: u64,
        total_network_bandwidth: u64,
    ) -> HashMap<NodeId, u64> {
        if nodes.is_empty() {
            return HashMap::new();
        }

        // Compute weighted scores.
        let scores: HashMap<&NodeId, f64> = nodes
            .iter()
            .map(|(id, s)| {
                let score = s.compute(total_network_validations, total_network_bandwidth);
                (id, score)
            })
            .collect();

        let total_score: f64 = scores.values().sum();

        if total_score <= 0.0 {
            return HashMap::new();
        }

        // Distribute daily pool proportionally.
        let mut rewards = HashMap::new();
        let mut distributed = 0u64;

        let node_ids: Vec<_> = scores.keys().cloned().collect();
        for (i, id) in node_ids.iter().enumerate() {
            let score = scores[id];
            let reward = if i == node_ids.len() - 1 {
                // Last node gets remainder to avoid rounding loss.
                self.daily_pool - distributed
            } else {
                ((score / total_score) * self.daily_pool as f64) as u64
            };
            distributed += reward;
            rewards.insert((*id).clone(), reward);
        }

        rewards
    }
}

impl Default for RewardCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reward_distribution() {
        let calc = RewardCalculator::with_pool(1_000_000);

        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId::new("node-1"),
            NodeScore {
                uptime_hours: 24.0,
                validated_txs: 500,
                relayed_bytes: 1_000_000,
            },
        );
        nodes.insert(
            NodeId::new("node-2"),
            NodeScore {
                uptime_hours: 12.0,
                validated_txs: 300,
                relayed_bytes: 500_000,
            },
        );

        let rewards = calc.calculate(&nodes, 1000, 2_000_000);

        // Both nodes get something.
        assert!(rewards[&NodeId::new("node-1")] > 0);
        assert!(rewards[&NodeId::new("node-2")] > 0);

        // Total distributed equals pool.
        let total: u64 = rewards.values().sum();
        assert_eq!(total, 1_000_000);

        // Node 1 (better scores) gets more.
        assert!(rewards[&NodeId::new("node-1")] > rewards[&NodeId::new("node-2")]);
    }

    #[test]
    fn test_empty_nodes() {
        let calc = RewardCalculator::with_pool(1_000_000);
        let rewards = calc.calculate(&HashMap::new(), 0, 0);
        assert!(rewards.is_empty());
    }
}
