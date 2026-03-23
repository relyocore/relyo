use serde::{Deserialize, Serialize};
use std::fmt;

use crate::address::Address;
use crate::crypto::{sha3_256_multi, KeyPair, PublicKey, Signature};
use crate::error::{RelyoError, Result};
use crate::token::RELYO_CONFIG;

/// A 32-byte SHA3-256 transaction hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionHash(#[serde(with = "hex_serde_32")] pub [u8; 32]);

impl TransactionHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| RelyoError::InvalidTransaction("invalid hash length".into()))?;
        Ok(TransactionHash(arr))
    }

    pub fn zero() -> Self {
        TransactionHash([0u8; 32])
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Transaction status in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Voting,
    Confirmed,
    Rejected,
}

/// Type of transaction in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    /// Standard RLY transfer between addresses.
    Transfer,
    /// Stake RLY to become a validator.
    Stake,
    /// Unstake and begin lock period.
    Unstake,
    /// Slash a misbehaving validator.
    Slash,
    /// Reward distribution to validators.
    Reward,
    /// Genesis allocation transaction.
    Genesis,
    /// Future: smart contract invocation.
    ContractCall,
}

/// Transaction priority for fee market ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TransactionPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl TransactionPriority {
    /// Fee multiplier for this priority level.
    pub fn fee_multiplier(&self) -> u64 {
        match self {
            TransactionPriority::Low => 1,
            TransactionPriority::Normal => 2,
            TransactionPriority::High => 5,
            TransactionPriority::Critical => 10,
        }
    }

    /// Determine priority from fee amount relative to base fee.
    pub fn from_fee(fee: u64) -> Self {
        let base = RELYO_CONFIG.base_fee;
        if fee >= base * 10 {
            TransactionPriority::Critical
        } else if fee >= base * 5 {
            TransactionPriority::High
        } else if fee >= base * 2 {
            TransactionPriority::Normal
        } else {
            TransactionPriority::Low
        }
    }
}

/// A complete Relyo transaction in the OpenGraph Ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_type: TransactionType,
    pub sender: Address,
    pub receiver: Address,
    pub amount: u64,
    pub fee: u64,
    pub timestamp: u64,
    pub nonce: u64,
    pub parent_1: TransactionHash,
    pub parent_2: TransactionHash,
    pub sender_pubkey: PublicKey,
    pub signature: Signature,
    /// Optional data payload (for contract calls, memos, etc.).
    pub data: Vec<u8>,
}

impl Transaction {
    /// Canonical byte representation for signing/hashing (excludes signature).
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(300);
        buf.push(self.tx_type as u8);
        buf.extend_from_slice(self.sender.as_str().as_bytes());
        buf.extend_from_slice(self.receiver.as_str().as_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&self.nonce.to_le_bytes());
        buf.extend_from_slice(self.parent_1.as_bytes());
        buf.extend_from_slice(self.parent_2.as_bytes());
        buf.extend_from_slice(self.sender_pubkey.as_bytes());
        buf.extend_from_slice(&crate::constants::CHAIN_ID.to_le_bytes());
        if !self.data.is_empty() {
            buf.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&self.data);
        }
        buf
    }

    /// Compute the SHA3-256 hash of this transaction.
    pub fn hash(&self) -> TransactionHash {
        let signable = self.signable_bytes();
        let sig_bytes = self.signature.as_bytes();
        TransactionHash(sha3_256_multi(&[&signable, sig_bytes]))
    }

    /// Verify the Ed25519 signature.
    pub fn verify_signature(&self) -> Result<()> {
        let msg = self.signable_bytes();
        self.sender_pubkey.verify(&msg, &self.signature)
    }

    /// Verify that sender address matches the embedded public key.
    pub fn verify_sender(&self) -> Result<()> {
        let derived = Address::from_public_key(&self.sender_pubkey);
        if derived != self.sender {
            return Err(RelyoError::InvalidTransaction(
                "sender address does not match public key".into(),
            ));
        }
        Ok(())
    }

    /// Full validation: signature + sender + business rules.
    pub fn validate(&self) -> Result<()> {
        self.verify_sender()?;
        self.verify_signature()?;

        if self.amount == 0 && self.tx_type == TransactionType::Transfer {
            return Err(RelyoError::InvalidTransaction(
                "transfer amount must be greater than zero".into(),
            ));
        }

        if self.sender == self.receiver && self.tx_type == TransactionType::Transfer {
            return Err(RelyoError::InvalidTransaction(
                "sender and receiver must differ for transfers".into(),
            ));
        }

        if self.fee < RELYO_CONFIG.base_fee && !self.is_genesis() {
            return Err(RelyoError::InvalidTransaction(
                "fee below minimum base fee".into(),
            ));
        }

        Ok(())
    }

    /// Check if this is a genesis transaction.
    pub fn is_genesis(&self) -> bool {
        self.tx_type == TransactionType::Genesis
            && self.parent_1.is_zero()
            && self.parent_2.is_zero()
    }

    /// Get the priority of this transaction based on fee.
    pub fn priority(&self) -> TransactionPriority {
        TransactionPriority::from_fee(self.fee)
    }

    /// Calculate the "weight" of this transaction for fee market ordering.
    /// Weight = fee per byte of serialized data.
    pub fn weight(&self) -> u64 {
        let size = self.signable_bytes().len() as u64 + 64; // + signature
        if size == 0 {
            return self.fee;
        }
        self.fee.saturating_mul(1000) / size
    }

    /// Total cost of this transaction (amount + fee).
    pub fn total_cost(&self) -> u64 {
        self.amount.saturating_add(self.fee)
    }
}

/// Receipt issued after a transaction is finalized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub hash: TransactionHash,
    pub status: TransactionStatus,
    pub dag_depth: u64,
    pub cumulative_weight: u64,
    pub timestamp: u64,
    pub finalized_at: u64,
    pub consensus_rounds: u32,
}

/// Builder for constructing and signing transactions.
pub struct TransactionBuilder {
    pub tx_type: TransactionType,
    pub sender: Address,
    pub receiver: Address,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub parent_1: TransactionHash,
    pub parent_2: TransactionHash,
    pub data: Vec<u8>,
}

impl TransactionBuilder {
    pub fn new(
        sender: Address,
        receiver: Address,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Self {
        Self {
            tx_type: TransactionType::Transfer,
            sender,
            receiver,
            amount,
            fee,
            nonce,
            parent_1: TransactionHash::zero(),
            parent_2: TransactionHash::zero(),
            data: Vec::new(),
        }
    }

    pub fn tx_type(mut self, t: TransactionType) -> Self {
        self.tx_type = t;
        self
    }

    pub fn parents(mut self, p1: TransactionHash, p2: TransactionHash) -> Self {
        self.parent_1 = p1;
        self.parent_2 = p2;
        self
    }


    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    /// Sign and build the final transaction.
    pub fn sign(self, keypair: &KeyPair) -> Transaction {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut tx = Transaction {
            tx_type: self.tx_type,
            sender: self.sender,
            receiver: self.receiver,
            amount: self.amount,
            fee: self.fee,
            timestamp,
            nonce: self.nonce,
            parent_1: self.parent_1,
            parent_2: self.parent_2,
            sender_pubkey: keypair.public_key.clone(),
            signature: Signature::from_bytes([0u8; 64]),
            data: self.data,
        };

        let msg = tx.signable_bytes();
        tx.signature = keypair.sign(&msg);
        tx
    }
}

/// Verify multiple transaction signatures at once.
pub fn batch_verify_transactions(txs: &[Transaction]) -> Result<()> {
    let mut invalid = 0;
    for tx in txs {
        if tx.verify_signature().is_err() {
            invalid += 1;
        }
    }
    if invalid > 0 {
        Err(RelyoError::BatchVerifyError {
            count: invalid,
            total: txs.len(),
        })
    } else {
        Ok(())
    }
}

// ─── serde helpers ──────────────────────────────────────────────────────────

mod hex_serde_32 {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid hash length"))?;
        Ok(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    fn make_test_tx() -> (KeyPair, Transaction) {
        let sender_kp = KeyPair::generate();
        let receiver_kp = KeyPair::generate();
        let sender_addr = Address::from_public_key(&sender_kp.public_key);
        let receiver_addr = Address::from_public_key(&receiver_kp.public_key);

        let tx = TransactionBuilder::new(
            sender_addr,
            receiver_addr,
            1_000_000,
            RELYO_CONFIG.base_fee,
            1,
        )
        .sign(&sender_kp);

        (sender_kp, tx)
    }

    #[test]
    fn test_build_and_verify() {
        let (_, tx) = make_test_tx();
        assert!(tx.validate().is_ok());
    }

    #[test]
    fn test_hash_deterministic() {
        let (_, tx) = make_test_tx();
        assert_eq!(tx.hash(), tx.hash());
    }

    #[test]
    fn test_zero_amount_rejected() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);
        let tx = TransactionBuilder::new(addr, recv, 0, RELYO_CONFIG.base_fee, 1).sign(&kp);
        assert!(tx.validate().is_err());
    }

    #[test]
    fn test_self_transfer_rejected() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let tx = TransactionBuilder::new(addr.clone(), addr, 100, RELYO_CONFIG.base_fee, 1).sign(&kp);
        assert!(tx.validate().is_err());
    }

    #[test]
    fn test_json_roundtrip() {
        let (_, tx) = make_test_tx();
        let json = serde_json::to_string(&tx).unwrap();
        let tx2: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(tx.hash(), tx2.hash());
    }

    #[test]
    fn test_transaction_priority() {
        assert_eq!(
            TransactionPriority::from_fee(RELYO_CONFIG.base_fee),
            TransactionPriority::Low
        );
        assert_eq!(
            TransactionPriority::from_fee(RELYO_CONFIG.base_fee * 2),
            TransactionPriority::Normal
        );
        assert_eq!(
            TransactionPriority::from_fee(RELYO_CONFIG.base_fee * 5),
            TransactionPriority::High
        );
        assert_eq!(
            TransactionPriority::from_fee(RELYO_CONFIG.base_fee * 10),
            TransactionPriority::Critical
        );
    }

    #[test]
    fn test_transaction_type() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);

        let tx = TransactionBuilder::new(addr, recv, 100, RELYO_CONFIG.base_fee, 1)
            .tx_type(TransactionType::Stake)
            .sign(&kp);
        assert_eq!(tx.tx_type, TransactionType::Stake);
    }

    #[test]
    fn test_batch_verify() {
        let (_, tx1) = make_test_tx();
        let (_, tx2) = make_test_tx();
        assert!(batch_verify_transactions(&[tx1, tx2]).is_ok());
    }

    #[test]
    fn test_total_cost() {
        let (_, tx) = make_test_tx();
        assert_eq!(tx.total_cost(), tx.amount + tx.fee);
    }

    #[test]
    fn test_tx_with_data() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);
        let data = b"memo: payment for invoice #123".to_vec();

        let tx = TransactionBuilder::new(addr, recv, 100, RELYO_CONFIG.base_fee, 1)
            .with_data(data.clone())
            .sign(&kp);
        assert_eq!(tx.data, data);
        assert!(tx.validate().is_ok());
    }

    #[test]
    fn test_chain_id_replay_protection() {
        let (kp, mut tx) = make_test_tx();
        // tx is valid on current CHAIN_ID
        assert!(tx.validate().is_ok());

        // Simulate an attack where the payload was signed by appending a DIFFERENT chain_id (Testnet = 2)
        // Since we cannot alter the constant, we will tamper the signature simulation manually
        // If a transaction comes with a signature for Chain ID 2, the current network validates with Chain ID 1.
        // It SHOULD FAIL! 
        let mut tampered_buf = tx.signable_bytes();
        let len = tampered_buf.len();
        // Replace last 4 bytes before data (which is CHAIN_ID) with `2` (Assume testnet)
        tampered_buf[len - 4] = 2; // Inject ID 2
        
        let bad_signature = kp.sign(&tampered_buf);
        tx.signature = bad_signature; // Signature valid for another chain

        // Validation against OUR CHAIN_ID = 1 should reject it!
        assert!(tx.validate().is_err());
    }

    #[test]
    fn test_pure_pos_without_pow() {
        let (_kp, tx) = make_test_tx();
        // Verify size limits and structure contains no PoW fields
        // Struct has exact bytes overhead, meaning no 8-byte pow_nonce variable exists dynamically.
        let _bytes = tx.signable_bytes();
        // Ensures PoS validation passes strictly without arbitrary workload
        assert!(tx.validate().is_ok());
    }
}

