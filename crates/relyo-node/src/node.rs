use relyo_consensus::engine::ConsensusEngine;
use relyo_core::{NodeId, Transaction, TransactionHash, now_ms};
use relyo_dag::{ConflictDetector, DagGraph, LedgerState, Mempool, TipSelector};
use relyo_explorer::server::{ExplorerConfig, ExplorerServer};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::config::NodeConfig;
use crate::genesis;
use crate::rate_limiter::RateLimiter;

/// A full Relyo node that participates in the network.
pub struct RelyoNode {
    pub config: NodeConfig,
    pub dag: Arc<DagGraph>,
    pub mempool: Arc<Mempool>,
    pub consensus: Arc<ConsensusEngine>,
    pub rate_limiter: Arc<RateLimiter>,
    pub tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    node_id: NodeId,
    started_at: u64,
}

impl RelyoNode {
    /// Initialize a new node with the given configuration.
    pub fn new(config: NodeConfig, tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>) -> anyhow::Result<Self> {
        let node_id = NodeId::new(&config.node_name);
        let state = Arc::new(LedgerState::new());
        let tips = Arc::new(TipSelector::new());
        let conflicts = Arc::new(ConflictDetector::new());
        let dag = Arc::new(DagGraph::new(state, tips, conflicts));
        let mempool = Arc::new(Mempool::new(config.mempool_max_size));

        let consensus = Arc::new(ConsensusEngine::new(
            dag.clone(),
            config.consensus.clone(),
            node_id.clone(),
        ));

        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_tps));

        info!(
            "node '{}' initialized (mempool_max={}, rate_limit={}tps)",
            config.node_name, config.mempool_max_size, config.rate_limit_tps
        );

        Ok(RelyoNode {
            config,
            dag,
            mempool,
            consensus,
            rate_limiter,
            tx_sender,
            node_id,
            started_at: now_ms(),
        })
    }

    /// Initialize the genesis state.
    /// Uses a deterministic seed so all nodes produce identical genesis state.
    pub fn init_genesis(&self) {
        let genesis_seed = relyo_core::crypto::sha3_256(b"relyo-genesis-v1");
        let genesis_secret = relyo_core::crypto::SecretKey::from_bytes(genesis_seed);
        let genesis_kp = relyo_core::crypto::KeyPair::from_secret(&genesis_secret);
        let hashes = genesis::create_genesis(&self.dag, &genesis_kp);
        info!("genesis initialized with {} allocations", hashes.len());
    }

    /// Submit a transaction to the node for processing.
    pub fn submit_transaction(&self, tx: Transaction) -> relyo_core::Result<TransactionHash> {
        // Rate limiting.
        if !self.rate_limiter.check(&tx.sender) {
            return Err(relyo_core::RelyoError::RateLimitExceeded(
                tx.sender.to_string(),
            ));
        }

        let total_circulating = self.dag.state().total_circulating();
        if total_circulating + tx.amount > relyo_core::constants::TOTAL_SUPPLY {
            return Err(relyo_core::RelyoError::SupplyCapExceeded(
                format!("supply cap exceeded: {} + {} > {}", total_circulating, tx.amount, relyo_core::constants::TOTAL_SUPPLY)
            ));
        }

        // Validate and insert into DAG.
        let tx_clone = tx.clone();
        let hash = self.dag.insert(tx)?;

        // Broadcast to the network.
        let _ = self.tx_sender.send(tx_clone);

        info!("transaction submitted: {}", hash);
        Ok(hash)
    }

    /// Process a batch of transactions from the mempool.
    pub fn process_mempool_batch(&self, batch_size: usize) -> Vec<TransactionHash> {
        let batch = self.mempool.drain_batch(batch_size);
        let mut hashes = Vec::with_capacity(batch.len());

        for tx in batch {
            match self.dag.insert(tx) {
                Ok(hash) => {
                    hashes.push(hash);
                }
                Err(e) => {
                    warn!("failed to insert mempool tx: {}", e);
                }
            }
        }

        if !hashes.is_empty() {
            debug!("processed {} mempool transactions", hashes.len());
        }

        hashes
    }

    /// Get the node's uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        (now_ms() - self.started_at) / 1000
    }

    /// Get the node ID.
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Get the number of transactions in the DAG.
    pub fn dag_size(&self) -> u64 {
        self.dag.len()
    }

    /// Get the number of pending mempool transactions.
    pub fn mempool_size(&self) -> usize {
        self.mempool.len()
    }

    /// Start the explorer API server (non-blocking, spawns a task).
    pub fn start_explorer(&self) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.explorer_enabled {
            return None;
        }

        let explorer_config = ExplorerConfig {
            bind_addr: self.config.explorer_bind,
        };

        let server = ExplorerServer::new(explorer_config, self.dag.clone());

        let handle = tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("explorer server error: {}", e);
            }
        });

        Some(handle)
    }

    /// Start the JSON-RPC server (non-blocking, spawns a task).
    /// Requires `self` wrapped in `Arc`.
    pub fn start_rpc(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.rpc_enabled {
            return None;
        }

        let bind = self.config.rpc_bind;
        let node = Arc::clone(self);

        let handle = tokio::spawn(async move {
            if let Err(e) = crate::rpc::start_rpc_server(bind, node).await {
                error!("RPC server error: {}", e);
            }
        });

        Some(handle)
    }
}


