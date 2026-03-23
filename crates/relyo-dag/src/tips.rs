use dashmap::DashMap;
use parking_lot::RwLock;
use relyo_core::TransactionHash;
use std::collections::HashSet;
use std::time::Instant;

/// Manages DAG tips with weighted selection support.
/// Tips are transactions not yet referenced as parents by newer transactions.
pub struct TipSelector {
    tips: RwLock<HashSet<TransactionHash>>,
    /// Weights for weighted tip selection (cumulative weight from DAG).
    weights: DashMap<TransactionHash, u64>,
    /// Timestamps when tips were added (for age-based pruning).
    added_at: DashMap<TransactionHash, Instant>,
}

impl TipSelector {
    pub fn new() -> Self {
        TipSelector {
            tips: RwLock::new(HashSet::new()),
            weights: DashMap::new(),
            added_at: DashMap::new(),
        }
    }

    pub fn add(&self, hash: TransactionHash) {
        self.tips.write().insert(hash.clone());
        self.weights.insert(hash.clone(), 1);
        self.added_at.insert(hash, Instant::now());
    }

    pub fn add_weighted(&self, hash: TransactionHash, weight: u64) {
        self.tips.write().insert(hash.clone());
        self.weights.insert(hash.clone(), weight);
        self.added_at.insert(hash, Instant::now());
    }

    pub fn remove(&self, hash: &TransactionHash) {
        self.tips.write().remove(hash);
        self.weights.remove(hash);
        self.added_at.remove(hash);
    }

    /// Update the weight of a tip.
    pub fn update_weight(&self, hash: &TransactionHash, weight: u64) {
        if let Some(mut w) = self.weights.get_mut(hash) {
            *w = weight;
        }
    }

    /// Select two distinct tips using uniform random selection.
    pub fn select_two(&self) -> Option<(TransactionHash, TransactionHash)> {
        let tips = self.tips.read();
        let count = tips.len();
        if count < 2 {
            return None;
        }

        let tip_vec: Vec<_> = tips.iter().cloned().collect();
        drop(tips);

        use rand::Rng;
        let mut rng = rand::thread_rng();
        let i = rng.gen_range(0..tip_vec.len());
        let mut j = rng.gen_range(0..tip_vec.len());
        while j == i {
            j = rng.gen_range(0..tip_vec.len());
        }

        Some((tip_vec[i].clone(), tip_vec[j].clone()))
    }

    /// Weighted random selection: tips with higher cumulative weight
    /// have proportionally higher probability of being selected.
    pub fn select_weighted(&self) -> Option<(TransactionHash, TransactionHash)> {
        let tips = self.tips.read();
        let count = tips.len();
        if count < 2 {
            return None;
        }

        let tip_vec: Vec<_> = tips.iter().cloned().collect();
        drop(tips);

        // Build cumulative weight distribution
        let mut total_weight: u64 = 0;
        let weights: Vec<(TransactionHash, u64)> = tip_vec
            .iter()
            .map(|h| {
                let w = self.weights.get(h).map(|v| *v).unwrap_or(1);
                total_weight += w;
                (h.clone(), w)
            })
            .collect();

        if total_weight == 0 {
            return self.select_two(); // fallback to uniform
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();

        let mut pick = |exclude: Option<&TransactionHash>| -> TransactionHash {
            // Safety: bounded retry count prevents infinite loop when one tip dominates all weight.
            for _ in 0..100 {
                let target = rng.gen_range(0..total_weight);
                let mut cumulative = 0u64;
                for (hash, weight) in &weights {
                    cumulative += weight;
                    if cumulative > target {
                        if exclude.map(|e| e != hash).unwrap_or(true) {
                            return hash.clone();
                        }
                        break;
                    }
                }
            }
            // Fallback: return any tip that isn't the excluded one.
            for (hash, _) in &weights {
                if exclude.map(|e| e != hash).unwrap_or(true) {
                    return hash.clone();
                }
            }
            // Absolute fallback (should never reach here with count >= 2).
            weights[0].0.clone()
        };

        let first = pick(None);
        let second = pick(Some(&first));
        Some((first, second))
    }

    /// Get the age of a tip since it was added.
    pub fn tip_age(&self, hash: &TransactionHash) -> Option<std::time::Duration> {
        self.added_at.get(hash).map(|t| t.elapsed())
    }

    /// Prune tips older than the given duration.
    pub fn prune_stale(&self, max_age: std::time::Duration) -> Vec<TransactionHash> {
        let mut stale = Vec::new();
        self.added_at.retain(|hash, instant| {
            if instant.elapsed() > max_age {
                stale.push(hash.clone());
                false
            } else {
                true
            }
        });

        let mut tips = self.tips.write();
        for hash in &stale {
            tips.remove(hash);
            self.weights.remove(hash);
        }
        stale
    }

    pub fn len(&self) -> usize {
        self.tips.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn all(&self) -> Vec<TransactionHash> {
        self.tips.read().iter().cloned().collect()
    }
}

impl Default for TipSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_remove() {
        let tips = TipSelector::new();
        let h1 = TransactionHash([1u8; 32]);
        let h2 = TransactionHash([2u8; 32]);
        let h3 = TransactionHash([3u8; 32]);

        tips.add(h1.clone());
        tips.add(h2.clone());
        tips.add(h3.clone());
        assert_eq!(tips.len(), 3);

        tips.remove(&h1);
        assert_eq!(tips.len(), 2);
    }

    #[test]
    fn test_select_two() {
        let tips = TipSelector::new();
        assert!(tips.select_two().is_none());

        tips.add(TransactionHash([1u8; 32]));
        assert!(tips.select_two().is_none());

        tips.add(TransactionHash([2u8; 32]));
        let (a, b) = tips.select_two().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_weighted_selection() {
        let tips = TipSelector::new();
        tips.add_weighted(TransactionHash([1u8; 32]), 100);
        tips.add_weighted(TransactionHash([2u8; 32]), 1);

        let (a, b) = tips.select_weighted().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_tip_age() {
        let tips = TipSelector::new();
        let h1 = TransactionHash([1u8; 32]);
        tips.add(h1.clone());
        assert!(tips.tip_age(&h1).is_some());
    }
}
