use dashmap::DashMap;
use relyo_core::{Address, Transaction, TransactionHash};
use std::collections::HashSet;
use tracing::warn;

/// Detects and tracks conflicting transactions in the DAG.
///
/// Conflicts occur when:
/// 1. Two transactions from the same sender have the same nonce (nonce collision)
/// 2. Two transactions attempt to spend the same balance concurrently (double spend)
/// 3. A transaction references an invalid or non-existent parent
pub struct ConflictDetector {
    /// Set of (sender_str, nonce) pairs already committed to the DAG.
    committed_nonces: DashMap<(String, u64), TransactionHash>,
    /// Active conflict sets: maps conflict_id -> set of conflicting tx hashes.
    conflicts: DashMap<u64, HashSet<TransactionHash>>,
    /// Reverse map: tx_hash -> conflict_id.
    tx_to_conflict: DashMap<TransactionHash, u64>,
    /// Next conflict ID.
    next_conflict_id: std::sync::atomic::AtomicU64,
}

/// Result of conflict checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResult {
    /// No conflict detected.
    NoConflict,
    /// Nonce conflict: same sender already used this nonce.
    NonceConflict {
        existing_hash: TransactionHash,
        sender: Address,
        nonce: u64,
    },
    /// Double spend: spending more than available.
    DoubleSpend {
        sender: Address,
        attempted_total: u64,
        available: u64,
    },
}

impl ConflictDetector {
    pub fn new() -> Self {
        ConflictDetector {
            committed_nonces: DashMap::new(),
            conflicts: DashMap::new(),
            tx_to_conflict: DashMap::new(),
            next_conflict_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Check a transaction for conflicts against committed state.
    pub fn check_transaction(
        &self,
        tx: &Transaction,
        available_balance: u64,
        pending_spend: u64,
    ) -> ConflictResult {
        let key = (tx.sender.as_str().to_string(), tx.nonce);

        // Check nonce collision
        if let Some(existing) = self.committed_nonces.get(&key) {
            return ConflictResult::NonceConflict {
                existing_hash: existing.value().clone(),
                sender: tx.sender.clone(),
                nonce: tx.nonce,
            };
        }

        // Check double spend
        let total_needed = pending_spend.saturating_add(tx.amount).saturating_add(tx.fee);
        if total_needed > available_balance {
            return ConflictResult::DoubleSpend {
                sender: tx.sender.clone(),
                attempted_total: total_needed,
                available: available_balance,
            };
        }

        ConflictResult::NoConflict
    }

    /// Commit a transaction to the nonce tracker.
    pub fn commit_transaction(&self, tx: &Transaction) {
        let hash = tx.hash();
        let key = (tx.sender.as_str().to_string(), tx.nonce);
        self.committed_nonces.insert(key, hash);
    }

    /// Register a conflict between two transactions.
    pub fn register_conflict(&self, hash_a: TransactionHash, hash_b: TransactionHash) {
        let conflict_id = self
            .next_conflict_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut set = HashSet::new();
        set.insert(hash_a.clone());
        set.insert(hash_b.clone());

        self.conflicts.insert(conflict_id, set);
        self.tx_to_conflict.insert(hash_a.clone(), conflict_id);
        self.tx_to_conflict.insert(hash_b, conflict_id);

        warn!("conflict registered: id={}", conflict_id);
    }

    /// Check if a transaction is part of any conflict.
    pub fn is_conflicting(&self, hash: &TransactionHash) -> bool {
        self.tx_to_conflict.contains_key(hash)
    }

    /// Get all transactions in the same conflict set.
    pub fn get_conflict_set(&self, hash: &TransactionHash) -> Option<HashSet<TransactionHash>> {
        self.tx_to_conflict
            .get(hash)
            .and_then(|cid| self.conflicts.get(cid.value()).map(|s| s.clone()))
    }

    /// Resolve a conflict by accepting one transaction and rejecting others.
    pub fn resolve_conflict(&self, accepted: &TransactionHash) -> Vec<TransactionHash> {
        let cid = self.tx_to_conflict.get(accepted).map(|c| *c.value());
        if let Some(cid) = cid {
            if let Some((_, set)) = self.conflicts.remove(&cid) {
                let rejected: Vec<_> = set.into_iter().filter(|h| h != accepted).collect();
                self.tx_to_conflict.remove(accepted);
                for h in &rejected {
                    self.tx_to_conflict.remove(h);
                }
                return rejected;
            }
        }
        Vec::new()
    }

    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    pub fn revert_transaction(&self, tx: &Transaction) {
        let key = (tx.sender.as_str().to_string(), tx.nonce);
        self.committed_nonces.remove(&key);
    }

    /// Calculate total pending spend for a sender.
    pub fn pending_spend_for_sender(&self, sender: &Address, pending_txs: &[Transaction]) -> u64 {
        pending_txs
            .iter()
            .filter(|tx| &tx.sender == sender)
            .map(|tx| tx.amount.saturating_add(tx.fee))
            .fold(0u64, |acc, x| acc.saturating_add(x))
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::{crypto::KeyPair, token::RELYO_CONFIG, transaction::TransactionBuilder, Address};

    fn make_tx(kp: &KeyPair, nonce: u64) -> Transaction {
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);
        TransactionBuilder::new(addr, recv, 100, RELYO_CONFIG.base_fee, nonce).sign(kp)
    }

    #[test]
    fn test_no_conflict() {
        let det = ConflictDetector::new();
        let kp = KeyPair::generate();
        let tx = make_tx(&kp, 1);
        assert_eq!(det.check_transaction(&tx, 10_000_000, 0), ConflictResult::NoConflict);
    }

    #[test]
    fn test_nonce_conflict() {
        let det = ConflictDetector::new();
        let kp = KeyPair::generate();
        let tx1 = make_tx(&kp, 1);
        det.commit_transaction(&tx1);
        let tx2 = make_tx(&kp, 1);
        assert!(matches!(det.check_transaction(&tx2, 10_000_000, 0), ConflictResult::NonceConflict { .. }));
    }

    #[test]
    fn test_double_spend() {
        let det = ConflictDetector::new();
        let kp = KeyPair::generate();
        let tx = make_tx(&kp, 1);
        assert!(matches!(det.check_transaction(&tx, 50, 0), ConflictResult::DoubleSpend { .. }));
    }

    #[test]
    fn test_conflict_resolution() {
        let det = ConflictDetector::new();
        let h1 = TransactionHash([1u8; 32]);
        let h2 = TransactionHash([2u8; 32]);
        det.register_conflict(h1.clone(), h2.clone());
        assert!(det.is_conflicting(&h1));
        assert_eq!(det.conflict_count(), 1);
        let rejected = det.resolve_conflict(&h1);
        assert_eq!(rejected, vec![h2]);
        assert_eq!(det.conflict_count(), 0);
    }
}
