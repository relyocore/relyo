use dashmap::DashMap;
use relyo_core::Address;
use std::time::Instant;

/// Simple sliding-window rate limiter per address.
///
/// Prevents spam by limiting how many transactions an address can
/// submit per second.
pub struct RateLimiter {
    /// Per-address: list of timestamps of recent submissions.
    windows: DashMap<Address, Vec<Instant>>,
    /// Maximum transactions per second per address.
    max_tps: u32,
}

impl RateLimiter {
    pub fn new(max_tps: u32) -> Self {
        RateLimiter {
            windows: DashMap::new(),
            max_tps,
        }
    }

    /// Check if the address is allowed to submit a transaction.
    /// Returns true if allowed, false if rate-limited.
    pub fn check(&self, addr: &Address) -> bool {
        let now = Instant::now();
        let one_sec_ago = now - std::time::Duration::from_secs(1);

        let mut entry = self
            .windows
            .entry(addr.clone())
            .or_default();

        // Remove timestamps older than 1 second.
        entry.retain(|&t| t > one_sec_ago);

        if entry.len() >= self.max_tps as usize {
            return false;
        }

        entry.push(now);
        true
    }

    /// Clear stale entries (call periodically).
    pub fn cleanup(&self) {
        let one_sec_ago = Instant::now() - std::time::Duration::from_secs(1);
        self.windows.retain(|_, v: &mut Vec<Instant>| {
            v.retain(|&t| t > one_sec_ago);
            !v.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::crypto::KeyPair;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(3);
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);

        assert!(limiter.check(&addr));
        assert!(limiter.check(&addr));
        assert!(limiter.check(&addr));
        assert!(!limiter.check(&addr)); // 4th in 1 sec → blocked
    }
}
