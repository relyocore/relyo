use relyo_core::{now_ms, Checkpoint, MerkleTree};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};

use crate::graph::DagGraph;
use crate::state::LedgerState;
use crate::storage::DagStorage;

/// A record of a checkpoint stored persistently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint: Checkpoint,
    pub created_at: u64,
}

/// Manages periodic checkpoint creation for fast state sync.
pub struct CheckpointManager {
    /// Interval in number of transactions between checkpoints.
    pub tx_interval: u64,
    /// Last checkpoint epoch.
    last_epoch: AtomicU64,
    /// Transactions since last checkpoint.
    tx_since_checkpoint: AtomicU64,
}

impl CheckpointManager {
    pub fn new(tx_interval: u64) -> Self {
        CheckpointManager {
            tx_interval,
            last_epoch: AtomicU64::new(0),
            tx_since_checkpoint: AtomicU64::new(0),
        }
    }

    /// Notify the manager that a transaction was processed.
    /// Returns true if a checkpoint should be created.
    pub fn on_transaction(&self) -> bool {
        let count = self.tx_since_checkpoint.fetch_add(1, Ordering::Relaxed) + 1;
        count >= self.tx_interval
    }

    /// Create a checkpoint from the current DAG and state.
    pub fn create_checkpoint(
        &self,
        dag: &DagGraph,
        state: &LedgerState,
    ) -> CheckpointRecord {
        let epoch = self.last_epoch.fetch_add(1, Ordering::Relaxed) + 1;
        self.tx_since_checkpoint.store(0, Ordering::Relaxed);

        // Compute state hash
        let state_hash = state.state_hash();

        // Build Merkle tree from all transaction hashes
        let all_txs = dag.all_transactions();
        let tx_hashes: Vec<Vec<u8>> = all_txs.iter().map(|n| n.hash.as_bytes().to_vec()).collect();
        let tx_hash_refs: Vec<&[u8]> = tx_hashes.iter().map(|h| h.as_slice()).collect();
        let merkle = MerkleTree::from_leaves(&tx_hash_refs);

        let checkpoint = Checkpoint {
            epoch,
            dag_depth: dag.max_depth(),
            merkle_root: merkle.root(),
            state_hash,
            timestamp: now_ms(),
            transaction_count: dag.len(),
            validator_signatures: Vec::new(), // Filled by consensus later
        };

        info!(
            "checkpoint created: epoch={}, depth={}, txs={}",
            epoch, checkpoint.dag_depth, checkpoint.transaction_count
        );

        CheckpointRecord {
            checkpoint,
            created_at: now_ms(),
        }
    }

    /// Save a checkpoint to persistent storage.
    pub fn save_checkpoint(
        &self,
        storage: &DagStorage,
        record: &CheckpointRecord,
    ) -> relyo_core::Result<()> {
        let data = bincode::serialize(record)
            .map_err(|e| relyo_core::RelyoError::CheckpointError(e.to_string()))?;
        storage.put_checkpoint(record.checkpoint.epoch, &data)?;
        debug!(
            "checkpoint saved: epoch={}",
            record.checkpoint.epoch
        );
        Ok(())
    }

    /// Load a checkpoint from storage.
    pub fn load_checkpoint(
        storage: &DagStorage,
        epoch: u64,
    ) -> relyo_core::Result<Option<CheckpointRecord>> {
        match storage.get_checkpoint(epoch)? {
            Some(data) => {
                let record: CheckpointRecord = bincode::deserialize(&data)
                    .map_err(|e| relyo_core::RelyoError::CheckpointError(e.to_string()))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Current epoch number.
    pub fn current_epoch(&self) -> u64 {
        self.last_epoch.load(Ordering::Relaxed)
    }

    /// Transactions processed since last checkpoint.
    pub fn tx_since_last(&self) -> u64 {
        self.tx_since_checkpoint.load(Ordering::Relaxed)
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new(10_000) // Checkpoint every 10,000 transactions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_interval() {
        let mgr = CheckpointManager::new(5);
        assert!(!mgr.on_transaction());
        assert!(!mgr.on_transaction());
        assert!(!mgr.on_transaction());
        assert!(!mgr.on_transaction());
        assert!(mgr.on_transaction()); // 5th tx triggers checkpoint
    }

    #[test]
    fn test_epoch_counter() {
        let mgr = CheckpointManager::new(1000);
        assert_eq!(mgr.current_epoch(), 0);
    }

    #[test]
    fn test_tx_since_last() {
        let mgr = CheckpointManager::new(100);
        assert_eq!(mgr.tx_since_last(), 0);
        mgr.on_transaction();
        mgr.on_transaction();
        assert_eq!(mgr.tx_since_last(), 2);
    }
}
