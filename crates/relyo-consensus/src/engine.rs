use relyo_core::{
    NodeId, TransactionHash, TransactionStatus, now_ms,
};
use relyo_dag::DagGraph;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::vote::{Vote, VoteAggregator, VoteOutcome};

/// Events emitted by the consensus engine.
#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    /// A transaction reached finality (confirmed or rejected).
    Finalized {
        tx_hash: TransactionHash,
        accepted: bool,
    },
}

use serde::{Deserialize, Serialize};

/// Configuration for the consensus engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsensusConfig {
    /// Number of peers to sample per gossip round.
    pub sample_size: usize,
    /// Number of votes required before deciding.
    pub quorum: u64,
    /// Accept threshold in basis points (6667 = 66.67%).
    pub threshold_bps: u64,
    /// Maximum rounds before forcing a decision.
    pub max_rounds: u32,
    /// Interval between gossip rounds in milliseconds.
    pub round_interval_ms: u64,
    /// Maximum age for votes; older votes are ignored.
    pub vote_max_age_ms: u64,
    /// Minimum validator voting power required for a vote to be counted.
    pub min_validator_voting_power: u64,
    /// Minimum total voting power required before finalization.
    pub min_total_voting_power: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        ConsensusConfig {
            sample_size: 20,
            quorum: 14,
            threshold_bps: 6667,
            max_rounds: 10,
            round_interval_ms: 100,
            vote_max_age_ms: 15_000,
            min_validator_voting_power: 1,
            min_total_voting_power: 14,
        }
    }
}

/// The consensus engine drives probabilistic gossip voting.
///
/// Flow:
/// 1. A new transaction is submitted for consensus.
/// 2. The engine gossips to `sample_size` peers asking for their vote.
/// 3. Votes are aggregated. Once quorum is reached the transaction is finalized.
/// 4. The DAG node status is updated and an event is emitted.
pub struct ConsensusEngine {
    dag: Arc<DagGraph>,
    aggregator: Arc<VoteAggregator>,
    config: ConsensusConfig,
    validators: DashMap<NodeId, u64>,
    total_voting_power: AtomicU64,
    /// Channel for publishing finality events.
    event_tx: mpsc::UnboundedSender<ConsensusEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::UnboundedReceiver<ConsensusEvent>>>,
    /// Local node's identifier.
    local_node: NodeId,
}

impl ConsensusEngine {
    pub fn new(
        dag: Arc<DagGraph>,
        config: ConsensusConfig,
        local_node: NodeId,
    ) -> Self {
        let aggregator = Arc::new(VoteAggregator::with_weighted_quorum(
            config.quorum,
            config.min_total_voting_power as u128,
            config.threshold_bps,
        ));
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let validators = DashMap::new();
        validators.insert(local_node.clone(), 1);

        ConsensusEngine {
            dag,
            aggregator,
            config,
            validators,
            total_voting_power: AtomicU64::new(1),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
            local_node,
        }
    }

    /// Take the event receiver (can only be called once).
    pub fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<ConsensusEvent>> {
        self.event_rx.lock().take()
    }

    /// Get a reference to the vote aggregator.
    pub fn aggregator(&self) -> &VoteAggregator {
        &self.aggregator
    }

    /// Cast this node's vote on a transaction.
    ///
    /// The node checks whether the transaction is valid in its local DAG view.
    pub fn local_vote(&self, tx_hash: &TransactionHash) -> Vote {
        let accept = self.dag.contains(tx_hash);

        Vote {
            voter: self.local_node.clone(),
            tx_hash: tx_hash.clone(),
            accept,
            timestamp: now_ms(),
        }
    }

    /// Process an incoming vote from a peer.
    pub fn receive_vote(&self, vote: Vote) {
        let age_ms = now_ms().saturating_sub(vote.timestamp);
        if age_ms > self.config.vote_max_age_ms {
            warn!("ignoring stale vote for {} (age={}ms)", vote.tx_hash, age_ms);
            return;
        }

        let voting_power = match self.validators.get(&vote.voter) {
            Some(v) => *v.value(),
            None => {
                warn!("ignoring vote from unregistered validator {}", vote.voter);
                return;
            }
        };

        if voting_power < self.config.min_validator_voting_power {
            warn!(
                "ignoring vote from {} due to insufficient voting power {}",
                vote.voter, voting_power
            );
            return;
        }

        let tx_hash = vote.tx_hash.clone();
        let outcome = self.aggregator.record_vote_weighted(vote, voting_power);

        match outcome {
            VoteOutcome::Accepted => {
                self.dag.set_status(&tx_hash, TransactionStatus::Confirmed);
                self.aggregator.finalize(&tx_hash);
                let _ = self.event_tx.send(ConsensusEvent::Finalized {
                    tx_hash: tx_hash.clone(),
                    accepted: true,
                });
                info!("transaction confirmed: {}", tx_hash);
            }
            VoteOutcome::Rejected => {
                self.dag.set_status(&tx_hash, TransactionStatus::Rejected);
                self.aggregator.finalize(&tx_hash);
                let _ = self.event_tx.send(ConsensusEvent::Finalized {
                    tx_hash: tx_hash.clone(),
                    accepted: false,
                });
                warn!("transaction rejected: {}", tx_hash);
            }
            VoteOutcome::Pending => {
                self.dag.set_status(&tx_hash, TransactionStatus::Voting);
            }
        }
    }

    /// Get consensus config (for network protocol).
    pub fn config(&self) -> &ConsensusConfig {
        &self.config
    }

    /// Number of transactions currently in consensus voting.
    pub fn pending_votes(&self) -> usize {
        self.aggregator.pending_count()
    }

    /// Register or update a validator's voting power.
    pub fn register_validator(&self, node_id: NodeId, voting_power: u64) {
        let voting_power = voting_power.max(1);
        let previous = self
            .validators
            .insert(node_id, voting_power)
            .unwrap_or(0);
        self.total_voting_power.fetch_sub(previous, Ordering::AcqRel);
        self.total_voting_power
            .fetch_add(voting_power, Ordering::AcqRel);
    }

    /// Replace the current validator set atomically.
    pub fn set_validator_set(&self, validators: &[(NodeId, u64)]) {
        self.validators.clear();
        let mut total = 0u64;
        for (node_id, voting_power) in validators {
            let p = (*voting_power).max(1);
            total = total.saturating_add(p);
            self.validators.insert(node_id.clone(), p);
        }

        if !self.validators.contains_key(&self.local_node) {
            total = total.saturating_add(1);
            self.validators.insert(self.local_node.clone(), 1);
        }

        self.total_voting_power.store(total, Ordering::Release);
    }

    /// Number of active validators currently allowed to vote.
    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }

    /// Total voting power in the current validator set.
    pub fn total_voting_power(&self) -> u64 {
        self.total_voting_power.load(Ordering::Acquire)
    }

    /// Process a vote received from a remote peer via gossip.
    /// Constructs a Vote and feeds it through the normal receive_vote pipeline.
    pub fn process_remote_vote(
        &self,
        tx_hash: &TransactionHash,
        voter: &NodeId,
        accept: bool,
    ) {
        let vote = Vote {
            voter: voter.clone(),
            tx_hash: tx_hash.clone(),
            accept,
            timestamp: now_ms(),
        };
        self.receive_vote(vote);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_dag::{ConflictDetector, LedgerState, TipSelector};

    #[test]
    fn test_engine_creation() {
        let state = Arc::new(LedgerState::new());
        let tips = Arc::new(TipSelector::new());
        let conflicts = Arc::new(ConflictDetector::new());
        let dag = Arc::new(DagGraph::new(state, tips, conflicts));

        let engine = ConsensusEngine::new(
            dag,
            ConsensusConfig::default(),
            NodeId::new("test-node"),
        );

        assert_eq!(engine.pending_votes(), 0);
        assert_eq!(engine.validator_count(), 1);
        assert!(engine.total_voting_power() >= 1);
    }
}
