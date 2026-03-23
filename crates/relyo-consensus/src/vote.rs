use relyo_core::{NodeId, TransactionHash};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use dashmap::DashMap;

/// A vote cast by a node on a transaction's validity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// The node casting the vote.
    pub voter: NodeId,
    /// The transaction being voted on.
    pub tx_hash: TransactionHash,
    /// Whether the voter considers the transaction valid.
    pub accept: bool,
    /// Timestamp of the vote.
    pub timestamp: u64,
}

/// The outcome of a vote aggregation round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteOutcome {
    /// Not enough votes yet to decide.
    Pending,
    /// Majority accepted the transaction.
    Accepted,
    /// Majority rejected the transaction.
    Rejected,
}

/// Per-transaction vote tally.
#[derive(Debug, Default)]
struct VoteTally {
    accept: u64,
    reject: u64,
    accept_weight: u128,
    reject_weight: u128,
    voters: HashSet<NodeId>,
}

/// Aggregates votes from multiple nodes using probabilistic gossip sampling.
///
/// The consensus threshold is configurable, defaulting to a supermajority of
/// sampled peers. In the gossip model, each node samples `k` peers per round;
/// after enough rounds the probability of an incorrect decision becomes
/// negligible.
pub struct VoteAggregator {
    /// Votes per transaction.
    tallies: DashMap<TransactionHash, VoteTally>,
    /// Number of votes required before a decision is made.
    quorum: u64,
    /// Minimum aggregate voting power required before a decision is made.
    quorum_weight: u128,
    /// Fraction of accept votes required (basis points, e.g., 6667 = 66.67%).
    threshold_bps: u64,
}

impl VoteAggregator {
    /// Create a new aggregator.
    ///
    /// - `quorum`: number of votes required before deciding.
    /// - `threshold_bps`: accept ratio in basis points (10000 = 100%).
    pub fn new(quorum: u64, threshold_bps: u64) -> Self {
        Self::with_weighted_quorum(quorum, quorum as u128, threshold_bps)
    }

    /// Create an aggregator with both vote-count quorum and voting-power quorum.
    pub fn with_weighted_quorum(quorum: u64, quorum_weight: u128, threshold_bps: u64) -> Self {
        VoteAggregator {
            tallies: DashMap::new(),
            quorum,
            quorum_weight,
            threshold_bps,
        }
    }

    /// Record a vote. Returns the current outcome.
    pub fn record_vote(&self, vote: Vote) -> VoteOutcome {
        self.record_vote_weighted(vote, 1)
    }

    /// Record a vote with explicit voting power.
    pub fn record_vote_weighted(&self, vote: Vote, voting_power: u64) -> VoteOutcome {
        let mut tally = self
            .tallies
            .entry(vote.tx_hash.clone())
            .or_default();

        // Prevent double voting by the same node.
        if !tally.voters.insert(vote.voter.clone()) {
            return self.evaluate(&tally);
        }

        let voting_power = u128::from(voting_power.max(1));
        if vote.accept {
            tally.accept += 1;
            tally.accept_weight = tally.accept_weight.saturating_add(voting_power);
        } else {
            tally.reject += 1;
            tally.reject_weight = tally.reject_weight.saturating_add(voting_power);
        }

        self.evaluate(&tally)
    }

    /// Evaluate the current tally against quorum and threshold.
    fn evaluate(&self, tally: &VoteTally) -> VoteOutcome {
        let total_votes = tally.accept + tally.reject;
        if total_votes < self.quorum {
            return VoteOutcome::Pending;
        }

        let total_weight = tally.accept_weight.saturating_add(tally.reject_weight);
        if total_weight < self.quorum_weight {
            return VoteOutcome::Pending;
        }

        let accept_bps = if total_weight == 0 {
            0
        } else {
            (tally.accept_weight * 10_000 / total_weight) as u64
        };

        if accept_bps >= self.threshold_bps {
            VoteOutcome::Accepted
        } else {
            VoteOutcome::Rejected
        }
    }

    /// Get the current outcome for a transaction without recording a vote.
    pub fn outcome(&self, tx_hash: &TransactionHash) -> VoteOutcome {
        match self.tallies.get(tx_hash) {
            Some(tally) => self.evaluate(&tally),
            None => VoteOutcome::Pending,
        }
    }

    /// Get vote counts for a transaction.
    pub fn vote_counts(&self, tx_hash: &TransactionHash) -> (u64, u64) {
        match self.tallies.get(tx_hash) {
            Some(tally) => (tally.accept, tally.reject),
            None => (0, 0),
        }
    }

    /// Clean up finalized transaction tallies to free memory.
    pub fn finalize(&self, tx_hash: &TransactionHash) {
        self.tallies.remove(tx_hash);
    }

    /// Number of transactions currently being voted on.
    pub fn pending_count(&self) -> usize {
        self.tallies.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vote(node_id: u64, tx: &TransactionHash, accept: bool) -> Vote {
        Vote {
            voter: NodeId::new(format!("node-{}", node_id)),
            tx_hash: tx.clone(),
            accept,
            timestamp: relyo_core::now_ms(),
        }
    }

    #[test]
    fn test_accept_consensus() {
        // quorum = 5, threshold = 66.67%
        let agg = VoteAggregator::new(5, 6667);
        let tx = TransactionHash([42u8; 32]);

        // 4 accepts, 1 reject → pending until quorum
        for i in 0..4 {
            let outcome = agg.record_vote(make_vote(i, &tx, true));
            assert_eq!(outcome, VoteOutcome::Pending);
        }

        // 5th vote (reject) → quorum reached, 80% accept → accepted
        let outcome = agg.record_vote(make_vote(4, &tx, false));
        assert_eq!(outcome, VoteOutcome::Accepted);
    }

    #[test]
    fn test_reject_consensus() {
        let agg = VoteAggregator::new(5, 6667);
        let tx = TransactionHash([42u8; 32]);

        // 1 accept, 4 rejects
        agg.record_vote(make_vote(0, &tx, true));
        for i in 1..5 {
            agg.record_vote(make_vote(i, &tx, false));
        }

        let outcome = agg.outcome(&tx);
        assert_eq!(outcome, VoteOutcome::Rejected);
    }

    #[test]
    fn test_no_double_vote() {
        let agg = VoteAggregator::new(3, 6667);
        let tx = TransactionHash([42u8; 32]);

        // Same node votes twice — second vote ignored.
        agg.record_vote(make_vote(0, &tx, true));
        agg.record_vote(make_vote(0, &tx, false));

        let (accept, reject) = agg.vote_counts(&tx);
        assert_eq!(accept, 1);
        assert_eq!(reject, 0);
    }

    #[test]
    fn test_weighted_quorum() {
        let agg = VoteAggregator::with_weighted_quorum(2, 10, 6667);
        let tx = TransactionHash([7u8; 32]);

        // Two votes but only total power 6 -> still pending.
        let out1 = agg.record_vote_weighted(make_vote(1, &tx, true), 3);
        let out2 = agg.record_vote_weighted(make_vote(2, &tx, true), 3);
        assert_eq!(out1, VoteOutcome::Pending);
        assert_eq!(out2, VoteOutcome::Pending);

        // Third vote increases total power past quorum and accepts.
        let out3 = agg.record_vote_weighted(make_vote(3, &tx, true), 5);
        assert_eq!(out3, VoteOutcome::Accepted);
    }
}
