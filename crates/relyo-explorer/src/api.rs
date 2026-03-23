use relyo_core::token::{base_to_rly, RELYO_CONFIG};
use serde::{Deserialize, Serialize};

/// Explorer API response types.

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub hash: String,
    pub sender: String,
    pub receiver: String,
    pub amount: f64,
    pub amount_base: u64,
    pub fee: f64,
    pub fee_base: u64,
    pub timestamp: u64,
    pub nonce: u64,
    pub parent_1: String,
    pub parent_2: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressResponse {
    pub address: String,
    pub balance: f64,
    pub balance_base: u64,
    pub nonce: u64,
    pub transaction_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkStatsResponse {
    pub total_transactions: u64,
    pub confirmed_transactions: u64,
    pub pending_transactions: u64,
    pub active_nodes: u64,
    pub tps: f64,
    pub avg_confirmation_ms: f64,
    pub dag_tips: usize,
    pub total_supply: f64,
    pub circulating_supply: f64,
    pub ticker: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DagVisualization {
    pub nodes: Vec<DagVizNode>,
    pub edges: Vec<DagVizEdge>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DagVizNode {
    pub id: String,
    pub status: String,
    pub timestamp: u64,
    pub weight: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DagVizEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenInfoResponse {
    pub name: &'static str,
    pub ticker: &'static str,
    pub total_supply: f64,
    pub decimals: u8,
    pub base_fee: f64,
    pub daily_reward_emission: f64,
    pub reward_timeline_years: u32,
    pub distribution: DistributionResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DistributionResponse {
    pub node_rewards_pct: f64,
}

impl TokenInfoResponse {
    pub fn from_config() -> Self {
        TokenInfoResponse {
            name: RELYO_CONFIG.name,
            ticker: RELYO_CONFIG.ticker,
            total_supply: base_to_rly(RELYO_CONFIG.total_supply),
            decimals: RELYO_CONFIG.decimals,
            base_fee: base_to_rly(RELYO_CONFIG.base_fee),
            daily_reward_emission: base_to_rly(RELYO_CONFIG.daily_reward_emission),
            reward_timeline_years: RELYO_CONFIG.reward_timeline_years,
            distribution: DistributionResponse {
                node_rewards_pct: 100.0,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}
