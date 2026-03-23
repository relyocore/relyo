//! Emission schedule with smooth exponential decay over 256 years.
//!
//! The total supply is emitted gradually using the formula:
//!   reward(epoch) = initial_reward * e^(-λ * epoch)
//!
//! where λ is chosen so that ~99.7% of supply is emitted within
//! [`TOTAL_EPOCHS`] epochs (each epoch = [`EPOCH_DEPTH`] DAG depths).
//!
//! This is smoother than Bitcoin's sharp halving — rewards decrease
//! continuously rather than dropping 50% every N blocks.


use relyo_core::constants::{LAMBDA, R0, RLY_UNIT};
use relyo_core::token::RELYO_CONFIG;

/// Number of DAG depths per emission epoch.
pub const EPOCH_DEPTH: u64 = 100_000;

/// Total number of epochs across 256 years.
/// Assuming ~10 tx/sec average = ~315M depths/year × 256 ≈ 80 billion depths.
/// With EPOCH_DEPTH=100_000 that's ~800_000 epochs over 256 years.
pub const TOTAL_EPOCHS: u64 = 800_000;

/// Initial reward per epoch (base units). Derived from total_supply * λ.
fn initial_reward() -> u64 {
    (R0 * RLY_UNIT as f64) as u64
}

/// Calculate the reward for a single epoch at the given epoch number.
///
/// Uses exponential decay: `R(e) = R0 * e^(-λ * e)`.
/// Returns 0 once the reward drops below 1 base unit.
pub fn reward_at_epoch(epoch: u64) -> u64 {
    let r0 = initial_reward() as f64;
    let decay = (-LAMBDA * epoch as f64).exp();
    let reward = (r0 * decay) as u64;
    if reward == 0 {
        0
    } else {
        reward
    }
}

/// Get the epoch number for a given DAG depth.
pub fn epoch_at_depth(depth: u64) -> u64 {
    depth / EPOCH_DEPTH
}

/// Calculate the reward at a given DAG depth.
pub fn reward_at_depth(depth: u64) -> u64 {
    reward_at_epoch(epoch_at_depth(depth))
}

/// Approximate total emission up to a given DAG depth.
///
/// Uses the closed-form integral of the exponential decay:
///   total ≈ R0/λ * (1 - e^(-λ * epoch))
///
/// Capped at total supply.
pub fn total_emitted_by_depth(depth: u64) -> u64 {
    let epoch = epoch_at_depth(depth);
    total_emitted_by_epoch(epoch)
}

/// Approximate total emission up to a given epoch.
pub fn total_emitted_by_epoch(epoch: u64) -> u64 {
    let r0 = initial_reward() as f64;
    // Integral of R0 * e^(-λt) from 0 to epoch = (R0/λ) * (1 - e^(-λ*epoch))
    let total = (r0 / LAMBDA) * (1.0 - (-LAMBDA * epoch as f64).exp());
    let total_u64 = total as u64;
    // Never exceed total supply
    total_u64.min(RELYO_CONFIG.total_supply)
}

/// Calculate the percentage of total supply emitted at a given epoch.
pub fn emission_progress(epoch: u64) -> f64 {
    let emitted = total_emitted_by_epoch(epoch) as f64;
    let total = RELYO_CONFIG.total_supply as f64;
    (emitted / total) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_reward_nonzero() {
        assert!(initial_reward() > 0);
    }

    #[test]
    fn test_reward_decreases_over_time() {
        let r0 = reward_at_epoch(0);
        let r1 = reward_at_epoch(1000);
        let r2 = reward_at_epoch(10_000);
        let r3 = reward_at_epoch(100_000);
        assert!(r0 > r1);
        assert!(r1 > r2);
        assert!(r2 > r3);
    }

    #[test]
    fn test_reward_at_depth_consistent() {
        let depth = 500_000;
        let epoch = epoch_at_depth(depth);
        assert_eq!(reward_at_depth(depth), reward_at_epoch(epoch));
    }

    #[test]
    fn test_total_emitted_never_exceeds_supply() {
        // Even at a very large epoch, total emitted should not exceed supply.
        let emitted = total_emitted_by_epoch(TOTAL_EPOCHS * 10);
        assert!(emitted <= RELYO_CONFIG.total_supply);
    }

    #[test]
    fn test_emission_progress_at_start_zero() {
        let p = emission_progress(0);
        assert!(p < 0.01);
    }

    #[test]
    fn test_emission_progress_increases() {
        let p1 = emission_progress(100_000);
        let p2 = emission_progress(400_000);
        assert!(p2 > p1);
    }

    #[test]
    fn test_half_supply_emitted_around_half_life() {
        // Half-life ≈ ln(2)/λ ≈ 92_000 epochs
        let half_life = (0.693 / LAMBDA) as u64;
        let progress = emission_progress(half_life);
        // Should be roughly 50% (within 5% tolerance)
        assert!(progress > 45.0 && progress < 55.0,
            "expected ~50% at half-life, got {:.1}%", progress);
    }

    #[test]
    fn test_epoch_depth_calc() {
        assert_eq!(epoch_at_depth(0), 0);
        assert_eq!(epoch_at_depth(99_999), 0);
        assert_eq!(epoch_at_depth(100_000), 1);
        assert_eq!(epoch_at_depth(250_000), 2);
    }
}
