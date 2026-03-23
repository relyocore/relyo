use serde::{Deserialize, Serialize};

use crate::crypto::sha3_256;

/// Merkle tree built from SHA3-256 leaf hashes.
/// Used for state proofs, checkpoint verification, and DAG integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    /// All nodes in the tree, bottom-up. The last element is the root.
    nodes: Vec<[u8; 32]>,
    /// Number of leaf nodes.
    leaf_count: usize,
}

impl MerkleTree {
    /// Build a Merkle tree from leaf data. Each leaf is hashed with SHA3-256.
    pub fn from_leaves(leaves: &[&[u8]]) -> Self {
        if leaves.is_empty() {
            return MerkleTree {
                nodes: vec![[0u8; 32]],
                leaf_count: 0,
            };
        }

        let leaf_hashes: Vec<[u8; 32]> = leaves.iter().map(|l| sha3_256(l)).collect();
        Self::from_hashes(&leaf_hashes)
    }

    /// Build a Merkle tree from pre-computed leaf hashes.
    pub fn from_hashes(leaf_hashes: &[[u8; 32]]) -> Self {
        if leaf_hashes.is_empty() {
            return MerkleTree {
                nodes: vec![[0u8; 32]],
                leaf_count: 0,
            };
        }

        let leaf_count = leaf_hashes.len();
        // Pad to next power of 2
        let padded_len = leaf_count.next_power_of_two();
        let mut nodes = Vec::with_capacity(2 * padded_len);

        // Add leaf hashes
        nodes.extend_from_slice(leaf_hashes);
        // Pad with zero hashes
        for _ in leaf_count..padded_len {
            nodes.push([0u8; 32]);
        }

        // Build internal nodes bottom-up
        let mut level_start = 0;
        let mut level_size = padded_len;

        while level_size > 1 {
            let next_level_size = level_size / 2;
            for i in 0..next_level_size {
                let left = nodes[level_start + 2 * i];
                let right = nodes[level_start + 2 * i + 1];
                let mut combined = [0u8; 64];
                combined[..32].copy_from_slice(&left);
                combined[32..].copy_from_slice(&right);
                nodes.push(sha3_256(&combined));
            }
            level_start += level_size;
            level_size = next_level_size;
        }

        MerkleTree { nodes, leaf_count }
    }

    /// Get the Merkle root hash.
    pub fn root(&self) -> [u8; 32] {
        *self.nodes.last().unwrap_or(&[0u8; 32])
    }

    /// Generate a proof for a leaf at the given index.
    pub fn proof(&self, leaf_index: usize) -> Option<MerkleProof> {
        if leaf_index >= self.leaf_count {
            return None;
        }

        let padded_len = self.leaf_count.next_power_of_two();
        let mut siblings = Vec::new();
        let mut directions = Vec::new();
        let mut index = leaf_index;
        let mut level_start = 0;
        let mut level_size = padded_len;

        while level_size > 1 {
            let sibling_index = if index.is_multiple_of(2) { index + 1 } else { index - 1 };
            siblings.push(self.nodes[level_start + sibling_index]);
            directions.push(index.is_multiple_of(2)); // true = sibling is on right

            index /= 2;
            level_start += level_size;
            level_size /= 2;
        }

        Some(MerkleProof {
            leaf_hash: self.nodes[leaf_index],
            siblings,
            directions,
        })
    }

    /// Number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }
}

/// A Merkle inclusion proof for a single leaf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Hash of the leaf being proved.
    pub leaf_hash: [u8; 32],
    /// Sibling hashes along the path to the root.
    pub siblings: Vec<[u8; 32]>,
    /// Direction for each sibling: true = sibling is right child, false = left child.
    pub directions: Vec<bool>,
}

impl MerkleProof {
    /// Verify this proof against an expected root hash.
    pub fn verify(&self, expected_root: &[u8; 32]) -> bool {
        let mut current = self.leaf_hash;

        for (sibling, &is_right) in self.siblings.iter().zip(self.directions.iter()) {
            let mut combined = [0u8; 64];
            if is_right {
                // current is left, sibling is right
                combined[..32].copy_from_slice(&current);
                combined[32..].copy_from_slice(sibling);
            } else {
                // sibling is left, current is right
                combined[..32].copy_from_slice(sibling);
                combined[32..].copy_from_slice(&current);
            }
            current = sha3_256(&combined);
        }

        current == *expected_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_leaf() {
        let tree = MerkleTree::from_leaves(&[b"hello"]);
        let root = tree.root();
        assert_ne!(root, [0u8; 32]);
        assert_eq!(tree.leaf_count(), 1);
    }

    #[test]
    fn test_two_leaves() {
        let tree = MerkleTree::from_leaves(&[b"hello", b"world"]);
        let root = tree.root();

        // Root should be hash(hash("hello") || hash("world"))
        let h1 = sha3_256(b"hello");
        let h2 = sha3_256(b"world");
        let mut combined = [0u8; 64];
        combined[..32].copy_from_slice(&h1);
        combined[32..].copy_from_slice(&h2);
        let expected_root = sha3_256(&combined);
        assert_eq!(root, expected_root);
    }

    #[test]
    fn test_proof_verification() {
        let leaves: Vec<&[u8]> = vec![b"alpha", b"beta", b"gamma", b"delta"];
        let tree = MerkleTree::from_leaves(&leaves);
        let root = tree.root();

        for i in 0..leaves.len() {
            let proof = tree.proof(i).expect("should have proof");
            assert!(proof.verify(&root), "proof for leaf {} should verify", i);
        }
    }

    #[test]
    fn test_proof_fails_wrong_root() {
        let tree = MerkleTree::from_leaves(&[b"a", b"b", b"c", b"d"]);
        let proof = tree.proof(0).unwrap();
        let wrong_root = [0xFFu8; 32];
        assert!(!proof.verify(&wrong_root));
    }

    #[test]
    fn test_odd_number_of_leaves() {
        let leaves: Vec<&[u8]> = vec![b"one", b"two", b"three"];
        let tree = MerkleTree::from_leaves(&leaves);
        let root = tree.root();
        assert_ne!(root, [0u8; 32]);
        assert_eq!(tree.leaf_count(), 3);

        // All proofs should verify
        for i in 0..3 {
            let proof = tree.proof(i).unwrap();
            assert!(proof.verify(&root));
        }
    }

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::from_leaves(&[]);
        assert_eq!(tree.leaf_count(), 0);
    }

    #[test]
    fn test_large_tree() {
        let data: Vec<Vec<u8>> = (0..100u32).map(|i| i.to_le_bytes().to_vec()).collect();
        let leaves: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let tree = MerkleTree::from_leaves(&leaves);
        let root = tree.root();
        assert_eq!(tree.leaf_count(), 100);

        // Spot-check a few proofs
        for &i in &[0, 1, 50, 99] {
            let proof = tree.proof(i).unwrap();
            assert!(proof.verify(&root), "proof for leaf {} failed", i);
        }
    }

    #[test]
    fn test_deterministic_root() {
        let tree1 = MerkleTree::from_leaves(&[b"x", b"y"]);
        let tree2 = MerkleTree::from_leaves(&[b"x", b"y"]);
        assert_eq!(tree1.root(), tree2.root());
    }
}
