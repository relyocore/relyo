use crate::constants::*;
use serde::{Deserialize, Serialize};

/// Token configuration for the Relyo (RLY) currency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Human-readable currency name.
    pub name: &'static str,
    /// Ticker symbol.
    pub ticker: &'static str,
    /// Total supply in smallest unit (1 RLY = 10^8 base units).
    pub total_supply: u64,
    /// Decimal places.
    pub decimals: u8,
    /// Base transaction fee in smallest unit.
    pub base_fee: u64,
    /// Fraction of fee to validators (basis points, 10000 = 100%).
    pub validator_fee_share_bps: u16,
    /// Daily reward emission in smallest unit.
    pub daily_reward_emission: u64,
    /// Reward distribution timeline in years.
    pub reward_timeline_years: u32,
}


/// Canonical Relyo token configuration.
pub const RELYO_CONFIG: TokenConfig = TokenConfig {
    name: "Relyo",
    ticker: "RLY",
    // 25 billion RLY × 10^8 base units
    total_supply: 25_000_000_000 * RLY_UNIT,
    decimals: RLY_DECIMALS,
    // 0.01 RLY
    base_fee: RLY_UNIT / 100,
    validator_fee_share_bps: 10_000,
    // ~187,500 RLY/day in base units
    daily_reward_emission: 187_500 * RLY_UNIT,
    reward_timeline_years: 256,
};

/// Distribution percentages (stored as parts per thousand for precision).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    /// 100% — Node rewards.
    pub node_rewards_permille: u16,
}

pub const RELYO_DISTRIBUTION: Distribution = Distribution {
    node_rewards_permille: 1000,
};

/// Convert a human-readable RLY amount to the smallest base unit.
/// Uses rounding to mitigate IEEE 754 floating-point precision errors.
pub fn rly_to_base(rly: f64) -> u64 {
    if rly <= 0.0 {
        return 0;
    }
    // Round to nearest integer to avoid truncation errors (e.g., 0.1 * 1e8 = 9999999.999...)
    (rly * RLY_UNIT as f64 + 0.5) as u64
}

/// Convert a whole number of RLY to base units without any floating point.
/// Preferred over rly_to_base() for programmatic use.
pub fn rly_whole_to_base(rly: u64) -> u64 {
    rly.saturating_mul(RLY_UNIT)
}

/// Convert base units back to human-readable RLY.
pub fn base_to_rly(base: u64) -> f64 {
    base as f64 / RLY_UNIT as f64
}

/// Format a base-unit amount as a human-readable string with ticker.
pub fn format_rly(base: u64) -> String {
    let whole = base / RLY_UNIT;
    let frac = base % RLY_UNIT;
    if frac == 0 {
        format!("{} RLY", whole)
    } else {
        let s = format!("{}.{:08}", whole, frac);
        format!(
            "{} RLY",
            s.trim_end_matches('0').trim_end_matches('.')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversions() {
        assert_eq!(rly_to_base(1.0), RLY_UNIT);
        assert_eq!(rly_to_base(0.0001), RLY_UNIT / 10_000);
        assert_eq!(rly_to_base(0.0), 0);
        assert_eq!(rly_to_base(-1.0), 0);
        assert!((base_to_rly(RLY_UNIT) - 1.0).abs() < f64::EPSILON);
        assert_eq!(rly_whole_to_base(1), RLY_UNIT);
        assert_eq!(rly_whole_to_base(25_000_000_000), RELYO_CONFIG.total_supply);
    }

    #[test]
    fn test_distribution_sums_to_1000() {
        let d = &RELYO_DISTRIBUTION;
        let total = d.node_rewards_permille;
        assert_eq!(total, 1000);
    }

    #[test]
    fn test_fee_shares() {
        let c = &RELYO_CONFIG;
        assert_eq!(c.validator_fee_share_bps, 10_000);
    }
}
