use dashmap::DashMap;
use relyo_core::{Address, Transaction, TransactionHash};
use std::collections::HashSet;
use std::cmp::Ordering as CmpOrdering;
use tracing::debug;

/// Entry in the priority mempool.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MempoolEntry {
    tx: Transaction,
    hash: TransactionHash,
    fee_weight: u64,      // fee per byte for priority ordering
    inserted_at: u64,
}

impl PartialEq for MempoolEntry {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for MempoolEntry {}

impl PartialOrd for MempoolEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for MempoolEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Higher fee_weight = higher priority
        self.fee_weight
            .cmp(&other.fee_weight)
            .then_with(|| other.inserted_at.cmp(&self.inserted_at)) // earlier = higher for ties
    }
}

/// Priority-based transaction mempool.
/// Transactions with higher fees per byte are processed first.
pub struct Mempool {
    txs: DashMap<TransactionHash, Transaction>,
    /// Pending txs per sender for conflict checking.
    by_sender: DashMap<Address, HashSet<TransactionHash>>,
    max_size: usize,
    total_fees: std::sync::atomic::AtomicU64,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Mempool {
            txs: DashMap::new(),
            by_sender: DashMap::new(),
            max_size,
            total_fees: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Add a transaction to the mempool.
    /// Returns false if mempool is full or tx already exists.
    pub fn insert(&self, tx: Transaction) -> bool {
        if self.txs.len() >= self.max_size {
            return false;
        }

        let hash = tx.hash();
        if self.txs.contains_key(&hash) {
            return false;
        }

        self.total_fees.fetch_add(tx.fee, std::sync::atomic::Ordering::Relaxed);

        // Track by sender
        self.by_sender
            .entry(tx.sender.clone())
            .or_default()
            .insert(hash.clone());

        self.txs.insert(hash, tx);

        debug!("mempool insert (size={})", self.txs.len());
        true
    }

    /// Remove a transaction from the mempool.
    pub fn remove(&self, hash: &TransactionHash) -> Option<Transaction> {
        if let Some((_, tx)) = self.txs.remove(hash) {
            self.total_fees
                .fetch_sub(tx.fee, std::sync::atomic::Ordering::Relaxed);

            // Remove from sender tracking
            if let Some(mut set) = self.by_sender.get_mut(&tx.sender) {
                set.remove(hash);
                if set.is_empty() {
                    drop(set);
                    self.by_sender.remove(&tx.sender);
                }
            }

            Some(tx)
        } else {
            None
        }
    }

    /// Drain a batch ordered by priority (highest fee/weight first).
    pub fn drain_by_priority(&self, count: usize) -> Vec<Transaction> {
        // Collect all entries with their fee weights
        let mut entries: Vec<(u64, TransactionHash)> = self
            .txs
            .iter()
            .map(|r| (r.value().weight(), r.key().clone()))
            .collect();

        // Sort by weight descending
        entries.sort_by(|a, b| b.0.cmp(&a.0));

        let mut batch = Vec::with_capacity(count);
        for (_, hash) in entries.into_iter().take(count) {
            if let Some(tx) = self.remove(&hash) {
                batch.push(tx);
            }
        }
        batch
    }

    /// Drain oldest transactions (FIFO).
    pub fn drain_batch(&self, count: usize) -> Vec<Transaction> {
        let mut entries: Vec<(u64, TransactionHash)> = self
            .txs
            .iter()
            .map(|r| (r.value().timestamp, r.key().clone()))
            .collect();

        entries.sort_by_key(|(ts, _)| *ts);

        let mut batch = Vec::with_capacity(count);
        for (_, hash) in entries.into_iter().take(count) {
            if let Some(tx) = self.remove(&hash) {
                batch.push(tx);
            }
        }
        batch
    }

    /// Get all pending transactions from a specific sender.
    pub fn get_pending_for_address(&self, addr: &Address) -> Vec<Transaction> {
        self.by_sender
            .get(addr)
            .map(|hashes| {
                hashes
                    .iter()
                    .filter_map(|h| self.txs.get(h).map(|r| r.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if mempool has a conflicting tx from the same sender with same nonce.
    pub fn has_conflict(&self, tx: &Transaction) -> bool {
        self.by_sender
            .get(&tx.sender)
            .map(|hashes| {
                hashes.iter().any(|h| {
                    self.txs
                        .get(h)
                        .map(|existing| existing.nonce == tx.nonce)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    pub fn contains(&self, hash: &TransactionHash) -> bool {
        self.txs.contains_key(hash)
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    pub fn get(&self, hash: &TransactionHash) -> Option<Transaction> {
        self.txs.get(hash).map(|r| r.clone())
    }

    /// Total fees of all transactions in the mempool.
    pub fn total_fees(&self) -> u64 {
        self.total_fees.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Approximate memory usage in bytes.
    pub fn size_bytes(&self) -> usize {
        // Rough estimate: ~500 bytes per transaction
        self.txs.len() * 500
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::{crypto::KeyPair, token::RELYO_CONFIG, transaction::TransactionBuilder, Address};

    fn make_tx_with_fee(fee: u64) -> Transaction {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);
        TransactionBuilder::new(addr, recv, 100, fee, 1).sign(&kp)
    }

    fn make_tx() -> Transaction {
        make_tx_with_fee(RELYO_CONFIG.base_fee)
    }

    #[test]
    fn test_insert_remove() {
        let pool = Mempool::new(100);
        let tx = make_tx();
        let hash = tx.hash();
        assert!(pool.insert(tx));
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&hash));
        pool.remove(&hash);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_drain_batch() {
        let pool = Mempool::new(100);
        for _ in 0..10 {
            pool.insert(make_tx());
        }
        let batch = pool.drain_batch(5);
        assert_eq!(batch.len(), 5);
        assert_eq!(pool.len(), 5);
    }

    #[test]
    fn test_max_size() {
        let pool = Mempool::new(2);
        assert!(pool.insert(make_tx()));
        assert!(pool.insert(make_tx()));
        assert!(!pool.insert(make_tx()));
    }

    #[test]
    fn test_priority_drain() {
        let pool = Mempool::new(100);
        let low = make_tx_with_fee(RELYO_CONFIG.base_fee);
        let high = make_tx_with_fee(RELYO_CONFIG.base_fee * 10);
        pool.insert(low);
        pool.insert(high);

        let batch = pool.drain_by_priority(1);
        assert_eq!(batch.len(), 1);
        // The high-fee tx should be returned first
        assert!(batch[0].fee > RELYO_CONFIG.base_fee);
    }

    #[test]
    fn test_total_fees() {
        let pool = Mempool::new(100);
        let tx1 = make_tx_with_fee(100);
        let tx2 = make_tx_with_fee(200);
        pool.insert(tx1);
        pool.insert(tx2);
        assert_eq!(pool.total_fees(), 300);
    }
}
