use dashmap::DashMap;
use relyo_core::{crypto::sha3_256, Address, RelyoError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnstakeRequest {
    pub amount: u64,
    pub unlock_time_ms: u64,
    pub unlock_epoch: u64,
}

/// Thread-safe ledger state tracking balances and nonces for all addresses.
pub struct LedgerState {
    balances: DashMap<Address, AtomicU64>,
    nonces: DashMap<Address, AtomicU64>,
    stakes: DashMap<Address, AtomicU64>,
    unstake_requests: DashMap<Address, Vec<UnstakeRequest>>,
}

/// A snapshot of the entire ledger state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub balances: BTreeMap<String, u64>,
    pub nonces: BTreeMap<String, u64>,
    pub stakes: BTreeMap<String, u64>,
    pub unstake_requests: BTreeMap<String, Vec<UnstakeRequest>>,
    pub state_hash: [u8; 32],
    pub timestamp: u64,
}

impl LedgerState {
    pub fn new() -> Self {
        LedgerState {
            balances: DashMap::new(),
            nonces: DashMap::new(),
            stakes: DashMap::new(),
            unstake_requests: DashMap::new(),
        }
    }

    pub fn balance(&self, addr: &Address) -> u64 {
        self.balances
            .get(addr)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub fn nonce(&self, addr: &Address) -> u64 {
        self.nonces
            .get(addr)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub fn credit(&self, addr: &Address, amount: u64) {
        self.balances
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(amount, Ordering::AcqRel);
    }

    pub fn debit(&self, addr: &Address, amount: u64) -> Result<()> {
        let entry = self
            .balances
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0));

        loop {
            let current = entry.load(Ordering::Acquire);
            if current < amount {
                return Err(RelyoError::InsufficientBalance {
                    have: current,
                    need: amount,
                });
            }
            if entry
                .compare_exchange(current, current - amount, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    pub fn increment_nonce(&self, addr: &Address) {
        self.nonces
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_balance(&self, addr: &Address, amount: u64) {
        self.balances
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .store(amount, Ordering::Release);
    }

    pub fn set_nonce(&self, addr: &Address, nonce: u64) {
        self.nonces
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .store(nonce, Ordering::Release);
    }

    pub fn stake_balance(&self, addr: &Address) -> u64 {
        self.stakes
            .get(addr)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub fn add_stake(&self, addr: &Address, amount: u64) {
        self.stakes
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(amount, Ordering::AcqRel);
    }

    pub fn set_stake(&self, addr: &Address, amount: u64) {
        self.stakes
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .store(amount, Ordering::Release);
    }

    pub fn remove_stake(&self, addr: &Address, amount: u64) -> Result<()> {
        let entry = self
            .stakes
            .entry(addr.clone())
            .or_insert_with(|| AtomicU64::new(0));

        loop {
            let current = entry.load(Ordering::Acquire);
            if current < amount {
                return Err(RelyoError::InsufficientBalance {
                    have: current,
                    need: amount,
                });
            }
            if entry
                .compare_exchange(current, current - amount, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    pub fn unbond_requests(&self, addr: &Address) -> Vec<UnstakeRequest> {
        self.unstake_requests
            .get(addr)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Request unstaking (starts the lock period).
    /// Rule 13: Lock period = minimum 30 real days OR 10 epochs — whichever comes first.
    pub fn request_unstake(
        &self,
        addr: &Address,
        amount: u64,
        current_time_ms: u64,
        current_epoch: u64,
    ) -> Result<()> {
        self.remove_stake(addr, amount)?;

        let mut reqs = self
            .unstake_requests
            .entry(addr.clone())
            .or_insert_with(Vec::new);
        
        reqs.push(UnstakeRequest {
            amount,
            unlock_time_ms: current_time_ms + 30 * 24 * 60 * 60 * 1000,
            unlock_epoch: current_epoch + 10,
        });

        Ok(())
    }

    /// Process unlocked unstaking requests (credits balances).
    pub fn process_unstakes(
        &self,
        addr: &Address,
        current_time_ms: u64,
        current_epoch: u64,
    ) -> u64 {
        let mut unlocked_amount = 0;
        if let Some(mut reqs) = self.unstake_requests.get_mut(addr) {
            reqs.retain(|req| {
                // Rule 13 check: Whichever comes first
                let is_unlocked = current_time_ms >= req.unlock_time_ms || current_epoch >= req.unlock_epoch;
                if is_unlocked {
                    unlocked_amount += req.amount;
                    false // Remove from requests
                } else {
                    true // Keep in requests
                }
            });
        }

        if unlocked_amount > 0 {
            self.credit(addr, unlocked_amount);
        }
        unlocked_amount
    }

    pub fn address_count(&self) -> usize {
        self.balances
            .iter()
            .filter(|e| e.value().load(Ordering::Relaxed) > 0)
            .count()
    }

    pub fn total_circulating(&self) -> u64 {
        let balances_sum: u64 = self
            .balances
            .iter()
            .map(|e| e.value().load(Ordering::Relaxed))
            .sum();

        let stakes_sum: u64 = self
            .stakes
            .iter()
            .map(|e| e.value().load(Ordering::Relaxed))
            .sum();

        let unstakes_sum: u64 = self
            .unstake_requests
            .iter()
            .map(|e| e.value().iter().map(|req| req.amount).sum::<u64>())
            .sum();

        balances_sum + stakes_sum + unstakes_sum
    }

    /// Compute a deterministic hash of the entire state.
    /// Sorts all address:balance pairs alphabetically for determinism.
    pub fn state_hash(&self) -> [u8; 32] {
        let mut entries: Vec<(String, u64)> = self
            .balances
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut data = Vec::new();
        for (addr, balance) in &entries {
            data.extend_from_slice(addr.as_bytes());
            data.extend_from_slice(&balance.to_le_bytes());
        }
        // Include nonces
        let mut nonce_entries: Vec<(String, u64)> = self
            .nonces
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();
        nonce_entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (addr, nonce) in &nonce_entries {
            data.extend_from_slice(addr.as_bytes());
            data.extend_from_slice(&nonce.to_le_bytes());
        }

        // Include stakes
        let mut stake_entries: Vec<(String, u64)> = self
            .stakes
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();
        stake_entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (addr, stake) in &stake_entries {
            data.extend_from_slice(addr.as_bytes());
            data.extend_from_slice(&stake.to_le_bytes());
        }

        // Include unstake_requests
        let mut unstake_entries: Vec<(String, Vec<UnstakeRequest>)> = self
            .unstake_requests
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().clone(),
                )
            })
            .collect();
        unstake_entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (addr, reqs) in &unstake_entries {
            data.extend_from_slice(addr.as_bytes());
            for req in reqs {
                data.extend_from_slice(&req.amount.to_le_bytes());
                data.extend_from_slice(&req.unlock_time_ms.to_le_bytes());
                data.extend_from_slice(&req.unlock_epoch.to_le_bytes());
            }
        }

        sha3_256(&data)
    }

    /// Create a snapshot of the current state.
    pub fn create_snapshot(&self) -> StateSnapshot {
        let balances: BTreeMap<String, u64> = self
            .balances
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();

        let nonces: BTreeMap<String, u64> = self
            .nonces
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();

        let stakes: BTreeMap<String, u64> = self
            .stakes
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().load(Ordering::Relaxed),
                )
            })
            .collect();

        let unstake_requests: BTreeMap<String, Vec<UnstakeRequest>> = self
            .unstake_requests
            .iter()
            .map(|e| {
                (
                    e.key().as_str().to_string(),
                    e.value().clone(),
                )
            })
            .collect();

        let state_hash = self.state_hash();

        StateSnapshot {
            balances,
            nonces,
            stakes,
            unstake_requests,
            state_hash,
            timestamp: relyo_core::now_ms(),
        }
    }

    /// Restore the state from a snapshot.
    pub fn restore_from_snapshot(&self, snapshot: &StateSnapshot) {
        self.balances.clear();
        self.nonces.clear();
        self.stakes.clear();
        self.unstake_requests.clear();

        for (addr_str, balance) in &snapshot.balances {
            if let Ok(addr) = addr_str.parse::<Address>() {
                self.set_balance(&addr, *balance);
            }
        }

        for (addr_str, nonce) in &snapshot.nonces {
            if let Ok(addr) = addr_str.parse::<Address>() {
                self.set_nonce(&addr, *nonce);
            }
        }

        for (addr_str, stake) in &snapshot.stakes {
            if let Ok(addr) = addr_str.parse::<Address>() {
                self.set_stake(&addr, *stake);
            }
        }

        for (addr_str, reqs) in &snapshot.unstake_requests {
            if let Ok(addr) = addr_str.parse::<Address>() {
                self.unstake_requests.insert(addr, reqs.clone());
            }
        }
    }
}

impl Default for LedgerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::crypto::KeyPair;

    #[test]
    fn test_credit_debit() {
        let state = LedgerState::new();
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);

        state.credit(&addr, 1000);
        assert_eq!(state.balance(&addr), 1000);

        state.debit(&addr, 400).unwrap();
        assert_eq!(state.balance(&addr), 600);

        assert!(state.debit(&addr, 700).is_err());
    }

    #[test]
    fn test_nonce() {
        let state = LedgerState::new();
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);

        assert_eq!(state.nonce(&addr), 0);
        state.increment_nonce(&addr);
        assert_eq!(state.nonce(&addr), 1);
    }

    #[test]
    fn test_state_hash_deterministic() {
        let state = LedgerState::new();
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        state.credit(&addr, 1000);

        let h1 = state.state_hash();
        let h2 = state.state_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let state = LedgerState::new();
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let addr1 = Address::from_public_key(&kp1.public_key);
        let addr2 = Address::from_public_key(&kp2.public_key);

        state.credit(&addr1, 1000);
        state.credit(&addr2, 2000);
        state.increment_nonce(&addr1);

        let snapshot = state.create_snapshot();
        let original_hash = state.state_hash();

        // Restore to new state
        let state2 = LedgerState::new();
        state2.restore_from_snapshot(&snapshot);
        assert_eq!(state2.state_hash(), original_hash);
    }

    #[test]
    fn test_total_circulating() {
        let state = LedgerState::new();
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let addr1 = Address::from_public_key(&kp1.public_key);
        let addr2 = Address::from_public_key(&kp2.public_key);

        state.credit(&addr1, 500);
        state.credit(&addr2, 300);
        assert_eq!(state.total_circulating(), 800);
    }
}
