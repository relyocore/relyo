pub mod address;
pub mod crypto;
pub mod error;
pub mod merkle;
pub mod primitives;
pub mod token;
pub mod transaction;
pub mod types;

pub use address::{Address, AddressType};
pub use crypto::{KeyPair, PublicKey, SecretKey, Signature};
pub use error::{RelyoError, Result};
pub use merkle::{MerkleProof, MerkleTree};
pub use token::{TokenConfig, RELYO_CONFIG};
pub use transaction::{
    Transaction, TransactionBuilder, TransactionHash, TransactionPriority, TransactionReceipt,
    TransactionStatus, TransactionType,
};
pub use types::*;

pub mod constants;
pub use constants::*;

pub fn get_consensus_hash() -> &'static str {
    env!("CONSENSUS_HASH")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_consensus_hash_matches() {
        let content = include_str!("constants.rs");
        let normalized = content.replace("\r\n", "\n");
        let hash = blake3::hash(normalized.as_bytes());
        assert_eq!(hash.to_hex().to_string(), get_consensus_hash());
    }
}
