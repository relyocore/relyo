use dashmap::DashMap;
use relyo_core::{
    Address, RelyoError, Result, Transaction, TransactionHash,
    TransactionStatus,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::debug;

use crate::conflict::ConflictDetector;
use crate::conflict::ConflictResult;
use crate::state::LedgerState;
use crate::tips::TipSelector;

/// An in-memory node of the DAG graph.
#[derive(Debug, Clone)]
pub struct DagNode {
    pub tx: Transaction,
    pub hash: TransactionHash,
    pub status: TransactionStatus,
    pub children: Vec<TransactionHash>,
    pub weight: u64,
    pub depth: u64,
}

/// The core DAG data structure — concurrent, in-memory directed acyclic graph.
pub struct DagGraph {
    nodes: DashMap<TransactionHash, DagNode>,
    sender_locks: DashMap<String, Arc<parking_lot::Mutex<()>>>,
    state: Arc<LedgerState>,
    tips: Arc<TipSelector>,
    conflicts: Arc<ConflictDetector>,
    total_count: AtomicU64,
    max_depth: AtomicU64,
}

impl DagGraph {
    pub fn new(
        state: Arc<LedgerState>,
        tips: Arc<TipSelector>,
        conflicts: Arc<ConflictDetector>,
    ) -> Self {
        DagGraph {
            nodes: DashMap::new(),
            sender_locks: DashMap::new(),
            state,
            tips,
            conflicts,
            total_count: AtomicU64::new(0),
            max_depth: AtomicU64::new(0),
        }
    }

    /// Insert a genesis transaction (no validation, credits receiver).
    pub fn insert_genesis(&self, tx: Transaction) -> Result<TransactionHash> {
        if !tx.is_genesis() {
            return Err(RelyoError::InvalidTransaction("not a genesis transaction".into()));
        }

        let hash = tx.hash();
        self.state.credit(&tx.receiver, tx.amount);

        let node = DagNode {
            tx,
            hash: hash.clone(),
            status: TransactionStatus::Confirmed,
            children: Vec::new(),
            weight: 1,
            depth: 0,
        };

        self.nodes.insert(hash.clone(), node);
        self.tips.add(hash.clone());
        self.total_count.fetch_add(1, Ordering::Relaxed);

        debug!("genesis transaction inserted: {}", hash);
        Ok(hash)
    }

    /// Insert a validated transaction into the DAG.
    pub fn insert(&self, tx: Transaction) -> Result<TransactionHash> {
        tx.validate()?;

        // Pre-check balance before allocating a lock to prevent memory/spam DoS
        let total_debit = tx.amount.checked_add(tx.fee)
            .ok_or_else(|| RelyoError::InvalidTransaction("amount overflow".into()))?;
        let pre_balance = self.state.balance(&tx.sender);
        if pre_balance < total_debit {
            return Err(RelyoError::InsufficientBalance {
                have: pre_balance,
                need: total_debit,
            });
        }

        // Serialize transactions per sender to prevent nonce/balance races.
        let sender_key = tx.sender.as_str().to_string();
        let sender_lock = self
            .sender_locks
            .entry(sender_key)
            .or_insert_with(|| Arc::new(parking_lot::Mutex::new(())))
            .clone();
        let _sender_guard = sender_lock.lock();

        let hash = tx.hash();

        if self.nodes.contains_key(&hash) {
            return Err(RelyoError::InvalidTransaction("duplicate transaction".into()));
        }

        // Parent validation
        if !tx.is_genesis() {
            if !self.nodes.contains_key(&tx.parent_1) {
                return Err(RelyoError::InvalidParent(tx.parent_1.to_hex()));
            }
            if !self.nodes.contains_key(&tx.parent_2) {
                return Err(RelyoError::InvalidParent(tx.parent_2.to_hex()));
            }
        }

        if self.state.total_circulating().saturating_add(tx.amount) > 25_000_000_000 * relyo_core::constants::RLY_UNIT {
            return Err(RelyoError::SupplyCapExceeded(format!("Transaction amount {} violates 25B max supply limit.", tx.amount)));
        }

        // Balance check
        let total_debit = tx.amount.checked_add(tx.fee)
            .ok_or_else(|| RelyoError::InvalidTransaction("amount overflow".into()))?;
        let sender_balance = self.state.balance(&tx.sender);
        if sender_balance < total_debit {
            return Err(RelyoError::InsufficientBalance {
                have: sender_balance,
                need: total_debit,
            });
        }

        match self.conflicts.check_transaction(&tx, sender_balance, 0) {
            ConflictResult::NoConflict => {}
            ConflictResult::NonceConflict { .. } => {
                return Err(RelyoError::DuplicateNonce(tx.nonce));
            }
            ConflictResult::DoubleSpend { .. } => {
                return Err(RelyoError::DoubleSpend(hash.to_hex()));
            }
        }

        // Nonce check
        let expected_nonce = self.state.nonce(&tx.sender) + 1;
        if tx.nonce != expected_nonce {
            return Err(RelyoError::DuplicateNonce(tx.nonce));
        }

        // Apply state changes
        self.state.debit(&tx.sender, total_debit)?;
        self.state.credit(&tx.receiver, tx.amount);
        self.state.increment_nonce(&tx.sender);
        self.conflicts.commit_transaction(&tx);

        // Calculate depth
        let depth = if tx.is_genesis() {
            0
        } else {
            let d1 = self.nodes.get(&tx.parent_1).map(|n| n.depth).unwrap_or(0);
            let d2 = self.nodes.get(&tx.parent_2).map(|n| n.depth).unwrap_or(0);
            d1.max(d2) + 1
        };

        // Update max depth
        loop {
            let current_max = self.max_depth.load(Ordering::Relaxed);
            if depth <= current_max {
                break;
            }
            if self.max_depth.compare_exchange(current_max, depth, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                break;
            }
        }

        // Update parent children and weights
        if !tx.is_genesis() {
            self.add_child(&tx.parent_1, &hash);
            self.add_child(&tx.parent_2, &hash);
            self.propagate_weight(&tx.parent_1);
            self.propagate_weight(&tx.parent_2);
        }

        // Update tips
        self.tips.remove(&tx.parent_1);
        self.tips.remove(&tx.parent_2);

        let node = DagNode {
            tx,
            hash: hash.clone(),
            status: TransactionStatus::Pending,
            children: Vec::new(),
            weight: 1,
            depth,
        };

        self.tips.add(hash.clone());
        self.nodes.insert(hash.clone(), node);
        self.total_count.fetch_add(1, Ordering::Relaxed);

        debug!("transaction inserted: {} (depth={})", hash, depth);
        Ok(hash)
    }

    pub fn set_status(&self, hash: &TransactionHash, status: TransactionStatus) {
        if let Some(mut node) = self.nodes.get_mut(hash) {
            node.status = status;
        }
    }

    pub fn get(&self, hash: &TransactionHash) -> Option<DagNode> {
        self.nodes.get(hash).map(|r| r.clone())
    }

    pub fn contains(&self, hash: &TransactionHash) -> bool {
        self.nodes.contains_key(hash)
    }

    /// Alias for contains — used by consensus voting to check if tx exists.
    pub fn has_transaction(&self, hash: &TransactionHash) -> bool {
        self.nodes.contains_key(hash)
    }

    pub fn len(&self) -> u64 {
        self.total_count.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn max_depth(&self) -> u64 {
        self.max_depth.load(Ordering::Relaxed)
    }

    pub fn balance(&self, addr: &Address) -> u64 {
        self.state.balance(addr)
    }

    pub fn nonce(&self, addr: &Address) -> u64 {
        self.state.nonce(addr)
    }

    pub fn tips(&self) -> &TipSelector {
        &self.tips
    }

    pub fn state(&self) -> &LedgerState {
        &self.state
    }

    pub fn conflicts(&self) -> &ConflictDetector {
        &self.conflicts
    }

    pub fn all_transactions(&self) -> Vec<DagNode> {
        self.nodes.iter().map(|r| r.value().clone()).collect()
    }

    /// Get confirmation depth: how many layers of descendants a tx has.
    pub fn confirmation_depth(&self, hash: &TransactionHash) -> u64 {
        let current_max = self.max_depth.load(Ordering::Relaxed);
        self.nodes
            .get(hash)
            .map(|n| current_max.saturating_sub(n.depth))
            .unwrap_or(0)
    }

    /// Get all ancestors of a transaction up to max_depth levels.
    pub fn get_ancestors(
        &self,
        hash: &TransactionHash,
        max_levels: u64,
    ) -> Vec<TransactionHash> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back((hash.clone(), 0u64));

        while let Some((current, level)) = queue.pop_front() {
            if level > max_levels {
                continue;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.nodes.get(&current) {
                if level > 0 {
                    result.push(current);
                }
                if !node.tx.parent_1.is_zero() {
                    queue.push_back((node.tx.parent_1.clone(), level + 1));
                }
                if !node.tx.parent_2.is_zero() {
                    queue.push_back((node.tx.parent_2.clone(), level + 1));
                }
            }
        }
        result
    }

    /// Get all descendants of a transaction up to max_levels.
    pub fn get_descendants(
        &self,
        hash: &TransactionHash,
        max_levels: u64,
    ) -> Vec<TransactionHash> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back((hash.clone(), 0u64));

        while let Some((current, level)) = queue.pop_front() {
            if level > max_levels {
                continue;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.nodes.get(&current) {
                if level > 0 {
                    result.push(current);
                }
                for child in &node.children {
                    queue.push_back((child.clone(), level + 1));
                }
            }
        }
        result
    }

    /// Prune deeply confirmed transactions from memory (keep only in storage).
    pub fn prune_confirmed(&self, min_confirmation_depth: u64) -> Vec<TransactionHash> {
        let current_max = self.max_depth.load(Ordering::Relaxed);
        let mut pruned = Vec::new();

        self.nodes.retain(|hash, node| {
            if node.status == TransactionStatus::Confirmed {
                let conf_depth = current_max.saturating_sub(node.depth);
                if conf_depth > min_confirmation_depth {
                    pruned.push(hash.clone());
                    return false; // remove from memory
                }
            }
            true // keep in memory
        });

        if !pruned.is_empty() {
            debug!("pruned {} deeply confirmed transactions", pruned.len());
        }
        pruned
    }

    // ─── internal helpers ────────────────────────────────────────────────

    fn add_child(&self, parent_hash: &TransactionHash, child_hash: &TransactionHash) {
        if let Some(mut parent) = self.nodes.get_mut(parent_hash) {
            parent.children.push(child_hash.clone());
        }
    }

    fn propagate_weight(&self, hash: &TransactionHash) {
        if let Some(mut node) = self.nodes.get_mut(hash) {
            node.weight += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::{crypto::KeyPair, crypto::Signature, TransactionType};

    fn setup() -> DagGraph {
        let state = Arc::new(LedgerState::new());
        let tips = Arc::new(TipSelector::new());
        let conflicts = Arc::new(ConflictDetector::new());
        DagGraph::new(state, tips, conflicts)
    }

    #[test]
    fn test_genesis_insert() {
        let dag = setup();
        let kp = KeyPair::generate();
        let gen_addr = Address::genesis();
        let recv = Address::from_public_key(&kp.public_key);

        let mut tx = Transaction {
            tx_type: TransactionType::Genesis,
            sender: gen_addr,
            receiver: recv.clone(),
            amount: 1_000_000,
            fee: 0,
            timestamp: relyo_core::now_ms(),
            nonce: 0,
            parent_1: TransactionHash::zero(),
            parent_2: TransactionHash::zero(),
            sender_pubkey: kp.public_key.clone(),
            signature: Signature::from_bytes([0; 64]),
            data: Vec::new(),
        };
        let msg = tx.signable_bytes();
        tx.signature = kp.sign(&msg);

        let hash = dag.insert_genesis(tx).unwrap();
        assert!(dag.contains(&hash));
        assert_eq!(dag.balance(&recv), 1_000_000);
        assert_eq!(dag.len(), 1);
        assert_eq!(dag.max_depth(), 0);
    }

    #[test]
    fn test_depth_tracking() {
        let dag = setup();
        // Depth should start at 0 for genesis
        assert_eq!(dag.max_depth(), 0);
    }
}
