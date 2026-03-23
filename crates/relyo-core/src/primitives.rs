//! Core data structures for the Relyo DAG payment network.
//!
//! This module defines every protocol-level type: transactions, UTXOs,
//! DAG nodes, emission schedule, peer info, and errors. All types are
//! `serde`-serializable for both on-disk storage and network transport.
//!
//! Design principles:
//! - **UTXO model**: every coin is tracked as an unspent transaction output.
//! - **Pure DAG**: no blocks — each transaction references ≥2 parent tips.
//! - **PoW spam protection**: senders attach a small proof-of-work nonce.
//! - **Blake3 everywhere**: faster than SHA-256 with equivalent security.
//! - **100 % to nodes**: all emission and fees go to validating node runners.
//! - **No admin keys**: after genesis, the creator has zero special power.

use crate::constants::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A 32-byte Blake3 hash used to identify transactions in the DAG.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TxHash(pub [u8; 32]);

impl TxHash {
    /// The all-zero hash — used as a sentinel for genesis parent references.
    pub const ZERO: Self = TxHash([0u8; 32]);

    /// Compute the Blake3 hash of arbitrary data.
    pub fn hash(data: &[u8]) -> Self {
        TxHash(*blake3::hash(data).as_bytes())
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        TxHash(bytes)
    }

    /// View the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Is this the zero hash?
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Hex-encode the hash.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from a hex string.
    pub fn from_hex(s: &str) -> std::result::Result<Self, RelyoError> {
        let bytes = hex::decode(s).map_err(|_| RelyoError::InvalidHex(s.to_string()))?;
        if bytes.len() != 32 {
            return Err(RelyoError::InvalidHashLength(bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(TxHash(arr))
    }

    /// Count the number of leading zero bits (used for PoW verification).
    pub fn leading_zero_bits(&self) -> u32 {
        let mut zeros = 0u32;
        for byte in &self.0 {
            if *byte == 0 {
                zeros += 8;
            } else {
                zeros += byte.leading_zeros();
                break;
            }
        }
        zeros
    }
}

impl fmt::Debug for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxHash({}…)", &self.to_hex()[..12])
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Default for TxHash {
    fn default() -> Self {
        Self::ZERO
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  PubKeyHash — Blake3 hash of an Ed25519 public key (32 bytes)
// ═══════════════════════════════════════════════════════════════════════════

/// A pay-to-public-key-hash (P2PKH) address — the Blake3 hash of the
/// recipient's Ed25519 public key. This is the "address" users see.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PubKeyHash(pub [u8; 32]);

impl PubKeyHash {
    /// Derive a PubKeyHash from raw 32-byte Ed25519 public key bytes.
    pub fn from_pubkey_bytes(pubkey: &[u8; 32]) -> Self {
        PubKeyHash(*blake3::hash(pubkey).as_bytes())
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        PubKeyHash(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> std::result::Result<Self, RelyoError> {
        let bytes = hex::decode(s).map_err(|_| RelyoError::InvalidHex(s.to_string()))?;
        if bytes.len() != 32 {
            return Err(RelyoError::InvalidHashLength(bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(PubKeyHash(arr))
    }

    /// Verify that a given public key corresponds to this hash.
    pub fn matches_pubkey(&self, pubkey: &[u8; 32]) -> bool {
        *blake3::hash(pubkey).as_bytes() == self.0
    }
}

impl fmt::Debug for PubKeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PKH({}…)", &self.to_hex()[..12])
    }
}

impl fmt::Display for PubKeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Human-readable address: "rly1" prefix + base58 of hash
        write!(f, "rly1{}", bs58::encode(&self.0).into_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  UtxoId — unique identifier for an unspent transaction output
// ═══════════════════════════════════════════════════════════════════════════

/// Uniquely identifies a UTXO within the DAG: the hash of the transaction
/// that created it plus the index of the output within that transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UtxoId {
    /// Hash of the transaction containing this output.
    pub tx_hash: TxHash,
    /// Zero-based index into the transaction's `outputs` vector.
    pub output_index: u32,
}

impl UtxoId {
    pub fn new(tx_hash: TxHash, output_index: u32) -> Self {
        UtxoId { tx_hash, output_index }
    }

    /// Serialize to a deterministic 36-byte key (for storage lookups).
    pub fn to_key_bytes(&self) -> [u8; 36] {
        let mut key = [0u8; 36];
        key[..32].copy_from_slice(self.tx_hash.as_bytes());
        key[32..36].copy_from_slice(&self.output_index.to_le_bytes());
        key
    }

    /// Deserialize from a 36-byte key.
    pub fn from_key_bytes(key: &[u8; 36]) -> Self {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&key[..32]);
        let index = u32::from_le_bytes([key[32], key[33], key[34], key[35]]);
        UtxoId {
            tx_hash: TxHash(hash),
            output_index: index,
        }
    }
}

impl fmt::Display for UtxoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.tx_hash, self.output_index)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  TxOutput — a single transaction output (creates a UTXO)
// ═══════════════════════════════════════════════════════════════════════════

/// A transaction output that creates a new UTXO.
///
/// Coins are locked to a `PubKeyHash` — only the holder of the corresponding
/// Ed25519 private key can spend them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxOutput {
    /// Amount in smallest base units (1 RLY = 10^8).
    pub amount: u64,
    /// Blake3 hash of the recipient's Ed25519 public key.
    pub pubkey_hash: PubKeyHash,
}

impl TxOutput {
    pub fn new(amount: u64, pubkey_hash: PubKeyHash) -> Self {
        TxOutput { amount, pubkey_hash }
    }

    /// Is this output below the dust threshold?
    pub fn is_dust(&self) -> bool {
        self.amount < DUST_THRESHOLD
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  TxInput — references and spends an existing UTXO
// ═══════════════════════════════════════════════════════════════════════════

/// A transaction input that spends an existing UTXO.
///
/// The spender must prove ownership by providing the full public key
/// (whose Blake3 hash matches the UTXO's `pubkey_hash`) and an Ed25519
/// signature over the spending transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxInput {
    /// The UTXO being spent.
    pub prev_output: UtxoId,
    /// Ed25519 signature over the transaction's signable bytes (64 bytes).
    pub signature: Vec<u8>,
    /// The spender's Ed25519 public key (32 bytes).
    pub pubkey: [u8; 32],
}

impl TxInput {
    /// Verify that the public key matches the UTXO's pubkey_hash.
    pub fn pubkey_matches(&self, expected_hash: &PubKeyHash) -> bool {
        expected_hash.matches_pubkey(&self.pubkey)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  TransactionKind — type discriminator
// ═══════════════════════════════════════════════════════════════════════════

/// Discriminator for the three kinds of transactions in the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionKind {
    /// A regular value transfer between addresses.
    Standard,
    /// A reward transaction created by a validating node (no inputs).
    Coinbase,
    /// The initial genesis distribution (only at DAG depth 0).
    Genesis,
}

// ═══════════════════════════════════════════════════════════════════════════
//  Transaction — the fundamental protocol unit
// ═══════════════════════════════════════════════════════════════════════════

/// A single transaction in the Relyo DAG.
///
/// Every transaction references ≥2 existing DAG tips as parents, creating
/// the directed acyclic graph. Senders attach a small proof-of-work nonce
/// as Hashcash-style spam protection.
///
/// # Invariants (enforced by validation)
/// - `inputs` is non-empty for `Standard` transactions.
/// - `outputs` is non-empty.
/// - Sum of input values ≥ sum of output values (difference = optional fee).
/// - Every output amount ≥ `DUST_THRESHOLD`.
/// - `dag_parents.len()` ≥ `MIN_DAG_PARENTS` (except genesis).
/// - All input signatures are valid Ed25519 signatures over `signable_bytes()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Protocol version.
    pub version: u8,
    /// Transaction kind.
    pub kind: TransactionKind,
    /// Inputs spending existing UTXOs (empty for Coinbase/Genesis).
    pub inputs: Vec<TxInput>,
    /// Outputs creating new UTXOs.
    pub outputs: Vec<TxOutput>,
    /// References to ≥2 parent transaction hashes (DAG tips).
    pub dag_parents: Vec<TxHash>,
    /// Milliseconds since UNIX epoch.
    pub timestamp: u64,
    /// Proof-of-work nonce (Hashcash spam protection).
    /// Optional fee in base units — goes 100 % to the propagating node.
    pub fee: u64,
    /// Optional memo (max `MAX_MEMO_BYTES`).
    pub memo: Vec<u8>,
}

impl Transaction {
    /// Compute the canonical Blake3 hash that identifies this transaction.
    ///
    /// The hash covers **everything except** the input signatures, so the
    /// hash is stable before and after signing (the signable message itself).
    pub fn hash(&self) -> TxHash {
        TxHash::hash(&self.signable_bytes())
    }

    /// The byte sequence that signers commit to.
    ///
    /// This is the canonical serialization of all fields **except** the
    /// per-input `signature` bytes. Input public keys *are* included so
    /// that the hash commits to who is allowed to spend.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);
        buf.push(self.version);
        buf.push(self.kind as u8);

        // Inputs (prev_output + pubkey, but NOT signature)
        buf.extend_from_slice(&(self.inputs.len() as u32).to_le_bytes());
        for inp in &self.inputs {
            buf.extend_from_slice(inp.prev_output.tx_hash.as_bytes());
            buf.extend_from_slice(&inp.prev_output.output_index.to_le_bytes());
            buf.extend_from_slice(&inp.pubkey);
        }

        // Outputs
        buf.extend_from_slice(&(self.outputs.len() as u32).to_le_bytes());
        for out in &self.outputs {
            buf.extend_from_slice(&out.amount.to_le_bytes());
            buf.extend_from_slice(out.pubkey_hash.as_bytes());
        }

        // DAG parents
        buf.extend_from_slice(&(self.dag_parents.len() as u32).to_le_bytes());
        for parent in &self.dag_parents {
            buf.extend_from_slice(parent.as_bytes());
        }

        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.extend_from_slice(&(self.memo.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.memo);

        buf
    }

    /// Total value created by all outputs.
    pub fn total_output(&self) -> u64 {
        self.outputs.iter().map(|o| o.amount).fold(0u64, |a, b| a.saturating_add(b))
    }

/// Is this a genesis transaction?
    pub fn is_genesis(&self) -> bool {
        self.kind == TransactionKind::Genesis
    }

    /// Is this a coinbase (node-reward) transaction?
    pub fn is_coinbase(&self) -> bool {
        self.kind == TransactionKind::Coinbase
    }

    /// Check all structural invariants (does **not** verify signatures
    /// or UTXO existence — that requires DAG context).
    pub fn validate_structure(&self) -> std::result::Result<(), RelyoError> {
        // Version check.
        if self.version != PROTOCOL_VERSION {
            return Err(RelyoError::UnsupportedVersion(self.version));
        }

        // Outputs must be non-empty.
        if self.outputs.is_empty() {
            return Err(RelyoError::NoOutputs);
        }

        // Dust check.
        for (i, out) in self.outputs.iter().enumerate() {
            if out.is_dust() {
                return Err(RelyoError::DustOutput {
                    index: i as u32,
                    amount: out.amount,
                    threshold: DUST_THRESHOLD,
                });
            }
        }

        // Memo size.
        if self.memo.len() > MAX_MEMO_BYTES {
            return Err(RelyoError::MemoTooLarge {
                size: self.memo.len(),
                max: MAX_MEMO_BYTES,
            });
        }

        match self.kind {
            TransactionKind::Standard => {
                // Standard transactions must have inputs.
                if self.inputs.is_empty() {
                    return Err(RelyoError::NoInputs);
                }
                // Must reference enough DAG parents.
                if self.dag_parents.len() < MIN_DAG_PARENTS {
                    return Err(RelyoError::InsufficientParents {
                        have: self.dag_parents.len(),
                        need: MIN_DAG_PARENTS,
                    });
                }
            }
            TransactionKind::Coinbase => {
                // Coinbase has no inputs.
                if !self.inputs.is_empty() {
                    return Err(RelyoError::CoinbaseWithInputs);
                }
                if self.dag_parents.len() < MIN_DAG_PARENTS {
                    return Err(RelyoError::InsufficientParents {
                        have: self.dag_parents.len(),
                        need: MIN_DAG_PARENTS,
                    });
                }
            }
            TransactionKind::Genesis => {
                // Genesis has no inputs and parents are zero hashes.
                if !self.inputs.is_empty() {
                    return Err(RelyoError::CoinbaseWithInputs);
                }
            }
        }

        // Check serialized size.
        let size = bincode::serialized_size(self)
            .map_err(|e| RelyoError::Serialization(e.to_string()))? as usize;
        if size > MAX_TX_SIZE {
            return Err(RelyoError::TransactionTooLarge { size, max: MAX_TX_SIZE });
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  TxStatus — lifecycle state of a transaction in the DAG
// ═══════════════════════════════════════════════════════════════════════════

/// The lifecycle state of a transaction within the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TxStatus {
    /// Inserted into the DAG but not yet deeply confirmed.
    Pending,
    /// Gaining confirmation depth (referenced by newer transactions).
    Confirming,
    /// Deeply confirmed — accepted as final by GHOSTDAG ordering.
    Confirmed,
    /// Rejected due to conflict (double spend) — the heavier subgraph won.
    Rejected,
    /// Orphaned — parents are missing or invalid.
    Orphaned,
}

impl fmt::Display for TxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxStatus::Pending => write!(f, "pending"),
            TxStatus::Confirming => write!(f, "confirming"),
            TxStatus::Confirmed => write!(f, "confirmed"),
            TxStatus::Rejected => write!(f, "rejected"),
            TxStatus::Orphaned => write!(f, "orphaned"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  DagNode — a vertex in the DAG with GHOSTDAG metadata
// ═══════════════════════════════════════════════════════════════════════════

/// A transaction vertex in the DAG, enriched with GHOSTDAG ordering metadata.
///
/// The `blue_score` determines the canonical ordering of transactions.
/// The `cumulative_weight` drives deterministic tip selection: heavier
/// tips are preferred, which makes the DAG converge and resolves conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// The underlying transaction.
    pub tx: Transaction,
    /// Blake3 hash of the transaction (cached for performance).
    pub hash: TxHash,
    /// GHOSTDAG blue score — monotonically increasing ordering metric.
    pub blue_score: u64,
    /// Cumulative weight: 1 + sum of weights of all descendants.
    pub cumulative_weight: u64,
    /// Hashes of transactions that reference this one as a parent.
    pub children: Vec<TxHash>,
    /// Is this transaction in the GHOSTDAG "blue set"?
    pub is_blue: bool,
    /// Current lifecycle status.
    pub status: TxStatus,
    /// DAG depth (longest path from genesis to this node).
    pub depth: u64,
}

impl DagNode {
    /// Build a new DagNode from a transaction.
    pub fn new(tx: Transaction, depth: u64, blue_score: u64) -> Self {
        let hash = tx.hash();
        DagNode {
            tx,
            hash,
            blue_score,
            cumulative_weight: 1,
            children: Vec::new(),
            is_blue: true,
            status: TxStatus::Pending,
            depth,
        }
    }

    /// Confirmation depth: how many layers of descendants reference this node.
    pub fn confirmation_depth(&self, current_max_depth: u64) -> u64 {
        current_max_depth.saturating_sub(self.depth)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  UtxoEntry — an unspent transaction output tracked in the UTXO set
// ═══════════════════════════════════════════════════════════════════════════

/// A tracked unspent transaction output.
///
/// The UTXO set is the only state that matters for balance queries and
/// double-spend detection. Spent UTXOs are removed; the UTXO set grows
/// with new outputs and shrinks as they are spent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoEntry {
    /// Unique identifier (tx_hash + output_index).
    pub id: UtxoId,
    /// The output data (amount + recipient hash).
    pub output: TxOutput,
    /// DAG depth at which this UTXO was created.
    pub created_at_depth: u64,
    /// True if this UTXO comes from a coinbase transaction.
    pub is_coinbase: bool,
}

impl UtxoEntry {
    /// Can this UTXO be spent at the given DAG depth?
    ///
    /// Coinbase UTXOs require `COINBASE_MATURITY` depth before they can
    /// be spent, preventing manipulation of freshly-minted rewards.
    pub fn is_spendable_at_depth(&self, current_depth: u64) -> bool {
        if self.is_coinbase {
            current_depth >= self.created_at_depth + COINBASE_MATURITY
        } else {
            true // non-coinbase UTXOs are immediately spendable
        }
    }

    /// The amount of this UTXO.
    pub fn amount(&self) -> u64 {
        self.output.amount
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  EmissionSchedule — smooth exponential-decay reward curve
// ═══════════════════════════════════════════════════════════════════════════

/// The emission schedule determines how many coins are minted per epoch.
///
/// Uses a smooth exponential decay rather than sharp Bitcoin-style halvings:
///
/// ```text
///   reward(epoch) = initial_reward × (DECAY_FACTOR_BPS / 10000)^epoch
/// ```
///
/// This means rewards decrease by 0.1 % every epoch, producing a smooth
/// curve that asymptotically approaches zero over 256 years.
///
/// **100 % of emission goes to node runners.** No foundation, no developer
/// allocation, no treasury cut. Period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionSchedule {
    /// Total supply cap in base units.
    pub total_supply: u64,
    /// Reward for epoch 0 in base units.
    pub initial_reward: u64,
    /// Decay factor per epoch in basis points (9990 = ×0.999).
    pub decay_factor_bps: u64,
    /// Number of DAG-depth levels per epoch.
    pub epoch_depth: u64,
}

impl EmissionSchedule {
    /// Create the canonical Relyo emission schedule.
    ///
    /// Initial reward is derived from the total supply and decay factor
    /// so that the infinite geometric series converges to `total_supply`:
    ///
    /// ```text
    ///   sum = initial / (1 − decay)
    ///   initial = total_supply × (1 − decay)
    /// ```
    pub fn new() -> Self {
        let decay = DECAY_FACTOR_BPS as f64 / 10_000.0;
        let initial = (TOTAL_SUPPLY as f64 * (1.0 - decay)) as u64;
        EmissionSchedule {
            total_supply: TOTAL_SUPPLY,
            initial_reward: initial,
            decay_factor_bps: DECAY_FACTOR_BPS,
            epoch_depth: EPOCH_DEPTH,
        }
    }

    /// Reward for a given epoch number (0-indexed).
    ///
    /// Uses integer arithmetic with fixed-point decay to avoid floating
    /// point in production paths:
    ///
    /// ```text
    ///   reward = initial × (decay_bps^epoch) / (10000^epoch)
    /// ```
    ///
    /// For large epoch numbers the reward approaches zero.
    pub fn reward_at_epoch(&self, epoch: u64) -> u64 {
        if epoch == 0 {
            return self.initial_reward;
        }

        // Use repeated multiply-then-divide to maintain precision.
        // Process in chunks to avoid u128 overflow for very large epochs.
        let mut reward = self.initial_reward as u128;
        for _ in 0..epoch {
            reward = reward * self.decay_factor_bps as u128 / 10_000;
            if reward == 0 {
                return 0;
            }
        }
        reward as u64
    }

    /// Cumulative emission from epoch 0 through `epoch` (inclusive).
    pub fn total_emitted_through_epoch(&self, epoch: u64) -> u64 {
        let mut total = 0u64;
        for e in 0..=epoch {
            total = total.saturating_add(self.reward_at_epoch(e));
        }
        total.min(self.total_supply)
    }

    /// Which epoch corresponds to a given DAG depth?
    pub fn epoch_at_depth(&self, depth: u64) -> u64 {
        depth / self.epoch_depth
    }

    /// Reward for the epoch containing a given DAG depth.
    pub fn reward_at_depth(&self, depth: u64) -> u64 {
        self.reward_at_epoch(self.epoch_at_depth(depth))
    }
}

impl Default for EmissionSchedule {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  PeerInfo — metadata about a connected P2P peer
// ═══════════════════════════════════════════════════════════════════════════

/// Information about a peer in the P2P network.
///
/// Peers must solve a PoW challenge to connect (Sybil protection).
/// Reputation is tracked based on behaviour — honest relay earns
/// positive reputation; invalid messages earn negative.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// libp2p peer identifier.
    pub peer_id: String,
    /// Known multiaddresses for this peer.
    pub addresses: Vec<String>,
    /// The PoW challenge hash this peer must solve to join.
    pub pow_challenge: [u8; 32],
    /// The nonce solving the PoW challenge.
    pub pow_solution: u64,
    /// Timestamp of last message received from this peer (ms since epoch).
    pub last_seen: u64,
    /// Reputation score (-100 = banned, 0 = neutral, 100 = excellent).
    pub reputation: i64,
    /// Protocol version string reported by this peer.
    pub version: String,
    /// Total bytes relayed by this peer.
    pub bytes_relayed: u64,
    /// Number of valid transactions relayed.
    pub valid_tx_relayed: u64,
    /// Number of invalid transactions relayed (contributes to banning).
    pub invalid_tx_relayed: u64,
}

impl PeerInfo {
    /// Is this peer currently banned (reputation below threshold)?
    pub fn is_banned(&self) -> bool {
        self.reputation <= -50
    }

}

// ═══════════════════════════════════════════════════════════════════════════
//  NodeRewardInfo — tracks a node runner's earned rewards
// ═══════════════════════════════════════════════════════════════════════════

/// Summary of rewards earned by a node runner in a given epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRewardInfo {
    /// The node's public key hash (their reward address).
    pub node_pubkey_hash: PubKeyHash,
    /// Epoch number.
    pub epoch: u64,
    /// Reward amount in base units.
    pub amount: u64,
    /// Number of transactions this node validated during the epoch.
    pub txs_validated: u64,
    /// Number of transactions this node propagated during the epoch.
    pub txs_propagated: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
//  GenesisData — the immutable initial state of the DAG
// ═══════════════════════════════════════════════════════════════════════════

/// The genesis state that bootstraps the DAG.
///
/// After genesis is published, the creator has **zero special power**.
/// There are no admin keys, no governance tokens, no upgrade keys.
/// The protocol can only be changed by a community hard fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisData {
    /// Human-readable message embedded in genesis (like Bitcoin's headline).
    pub message: String,
    /// Blake3 hash of the entire genesis data (serves as chain ID).
    pub genesis_hash: TxHash,
    /// Timestamp of genesis creation.
    pub timestamp: u64,
    /// Total supply committed at genesis (must equal `TOTAL_SUPPLY`).
    pub total_supply: u64,
    /// The emission schedule parameters (locked at genesis).
    pub emission: EmissionSchedule,
    /// Protocol version.
    pub protocol_version: u8,
    /// Chain ID.
    pub chain_id: u32,
    /// Constant dust threshold.
    pub dust_threshold: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
//  TokenConfig — human-readable token metadata
// ═══════════════════════════════════════════════════════════════════════════

/// Static metadata about the Relyo token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub name: &'static str,
    pub ticker: &'static str,
    pub total_supply: u64,
    pub decimals: u8,
    pub dust_threshold: u64,
}

/// The canonical token configuration.
pub const TOKEN_CONFIG: TokenConfig = TokenConfig {
    name: "Relyo",
    ticker: "RLY",
    total_supply: TOTAL_SUPPLY,
    decimals: RLY_DECIMALS,
    dust_threshold: DUST_THRESHOLD,
};

// ═══════════════════════════════════════════════════════════════════════════
//  Utility functions
// ═══════════════════════════════════════════════════════════════════════════

/// Format a base-unit amount as a human-readable RLY string.
pub fn format_rly(base: u64) -> String {
    let whole = base / RLY_UNIT;
    let frac = base % RLY_UNIT;
    if frac == 0 {
        format!("{} RLY", whole)
    } else {
        let s = format!("{}.{:08}", whole, frac);
        format!("{} RLY", s.trim_end_matches('0').trim_end_matches('.'))
    }
}

/// Convert whole RLY to base units (no floating point).
pub fn rly_to_base(rly: u64) -> u64 {
    rly.saturating_mul(RLY_UNIT)
}

/// Get current timestamp in milliseconds since UNIX epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ═══════════════════════════════════════════════════════════════════════════
//  RelyoError — comprehensive error type
// ═══════════════════════════════════════════════════════════════════════════

/// Comprehensive error type for the Relyo protocol.
///
/// Every variant carries enough context to produce a useful error message.
/// No `.unwrap()` in production — all failures flow through this enum.
#[derive(Debug, thiserror::Error)]
pub enum RelyoError {
    // ── Transaction validation ───────────────────────────────────────────
    #[error("transaction has no inputs")]
    NoInputs,

    #[error("transaction has no outputs")]
    NoOutputs,

    #[error("dust output at index {index}: {amount} < threshold {threshold}")]
    DustOutput { index: u32, amount: u64, threshold: u64 },

    #[error("memo too large: {size} bytes (max {max})")]
    MemoTooLarge { size: usize, max: usize },

    #[error("insufficient DAG parents: have {have}, need {need}")]
    InsufficientParents { have: usize, need: usize },

    #[error("coinbase transaction must not have inputs")]
    CoinbaseWithInputs,

    #[error("transaction too large: {size} bytes (max {max})")]
    TransactionTooLarge { size: usize, max: usize },

    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    #[error("proof-of-work insufficient: {have} leading zeros (need {need})")]
    InsufficientPow { have: u32, need: u32 },

    // ── UTXO / balance ──────────────────────────────────────────────────
    #[error("UTXO not found: {0}")]
    UtxoNotFound(UtxoId),

    #[error("UTXO already spent: {0}")]
    UtxoAlreadySpent(UtxoId),

    #[error("coinbase UTXO not mature: need {need} depth, currently at {current}")]
    CoinbaseNotMature { need: u64, current: u64 },

    #[error("input total {input_total} < output total {output_total} + fee {fee}")]
    InsufficientFunds { input_total: u64, output_total: u64, fee: u64 },

    #[error("double spend detected: UTXO {0} spent by conflicting transactions")]
    DoubleSpend(UtxoId),

    // ── Cryptography ────────────────────────────────────────────────────
    #[error("invalid signature on input {input_index}")]
    InvalidSignature { input_index: usize },

    #[error("public key does not match UTXO pubkey hash on input {input_index}")]
    PubKeyMismatch { input_index: usize },

    // ── DAG ──────────────────────────────────────────────────────────────
    #[error("parent transaction not found: {0}")]
    ParentNotFound(TxHash),

    #[error("duplicate transaction: {0}")]
    DuplicateTransaction(TxHash),

    #[error("DAG cycle detected involving {0}")]
    CycleDetected(TxHash),

    // ── Encoding ────────────────────────────────────────────────────────
    #[error("invalid hex string: {0}")]
    InvalidHex(String),

    #[error("invalid hash length: expected 32 bytes, got {0}")]
    InvalidHashLength(usize),

    #[error("serialization error: {0}")]
    Serialization(String),

    // ── Network ─────────────────────────────────────────────────────────
    #[error("peer PoW challenge failed")]
    PeerPowFailed,

    #[error("peer is banned: {0}")]
    PeerBanned(String),

    #[error("rate limit exceeded for {0}")]
    RateLimited(String),

    // ── Storage ─────────────────────────────────────────────────────────
    #[error("storage error: {0}")]
    Storage(String),

    // ── Generic ─────────────────────────────────────────────────────────
    #[error("{0}")]
    Other(String),
}

impl From<bincode::Error> for RelyoError {
    fn from(e: bincode::Error) -> Self {
        RelyoError::Serialization(e.to_string())
    }
}

impl From<serde_json::Error> for RelyoError {
    fn from(e: serde_json::Error) -> Self {
        RelyoError::Serialization(e.to_string())
    }
}

/// Alias for `std::result::Result<T, RelyoError>`.
pub type Result<T> = std::result::Result<T, RelyoError>;

// ═══════════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── TxHash ──────────────────────────────────────────────────────────

    #[test]
    fn test_txhash_deterministic() {
        let a = TxHash::hash(b"hello world");
        let b = TxHash::hash(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn test_txhash_different_inputs() {
        let a = TxHash::hash(b"hello");
        let b = TxHash::hash(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_txhash_zero() {
        assert!(TxHash::ZERO.is_zero());
        assert!(!TxHash::hash(b"x").is_zero());
    }

    #[test]
    fn test_txhash_hex_roundtrip() {
        let h = TxHash::hash(b"test data");
        let hex = h.to_hex();
        let h2 = TxHash::from_hex(&hex).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn test_txhash_leading_zeros() {
        let zero = TxHash::from_bytes([0u8; 32]);
        assert_eq!(zero.leading_zero_bits(), 256);

        let one = TxHash::from_bytes({
            let mut b = [0u8; 32];
            b[0] = 0x01;
            b
        });
        assert_eq!(one.leading_zero_bits(), 7);

        let half = TxHash::from_bytes({
            let mut b = [0u8; 32];
            b[0] = 0x80;
            b
        });
        assert_eq!(half.leading_zero_bits(), 0);
    }

    // ── PubKeyHash ──────────────────────────────────────────────────────

    #[test]
    fn test_pubkeyhash_matches() {
        let pubkey = [42u8; 32];
        let pkh = PubKeyHash::from_pubkey_bytes(&pubkey);
        assert!(pkh.matches_pubkey(&pubkey));
        assert!(!pkh.matches_pubkey(&[99u8; 32]));
    }

    #[test]
    fn test_pubkeyhash_display() {
        let pkh = PubKeyHash::from_pubkey_bytes(&[1u8; 32]);
        let s = format!("{}", pkh);
        assert!(s.starts_with("rly1"));
    }

    // ── UtxoId ──────────────────────────────────────────────────────────

    #[test]
    fn test_utxoid_key_bytes_roundtrip() {
        let id = UtxoId::new(TxHash::hash(b"tx1"), 7);
        let key = id.to_key_bytes();
        let id2 = UtxoId::from_key_bytes(&key);
        assert_eq!(id, id2);
    }

    // ── TxOutput ────────────────────────────────────────────────────────

    #[test]
    fn test_dust_detection() {
        let dust = TxOutput::new(DUST_THRESHOLD - 1, PubKeyHash::from_bytes([0; 32]));
        assert!(dust.is_dust());

        let ok = TxOutput::new(DUST_THRESHOLD, PubKeyHash::from_bytes([0; 32]));
        assert!(!ok.is_dust());
    }

    // ── Transaction ─────────────────────────────────────────────────────

    #[test]
    fn test_genesis_transaction_structure() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Genesis,
            inputs: vec![],
            outputs: vec![TxOutput::new(
                TOTAL_SUPPLY,
                PubKeyHash::from_pubkey_bytes(&[1u8; 32]),
            )],
            dag_parents: vec![],
            timestamp: 0,
            fee: 0,
            memo: b"Relyo Genesis".to_vec(),
        };

        assert!(tx.is_genesis());
        assert!(!tx.is_coinbase());
        assert_eq!(tx.total_output(), TOTAL_SUPPLY);
        // Genesis is exempt from PoW check and parent check.
        tx.validate_structure().unwrap();
    }

    #[test]
    fn test_standard_tx_needs_inputs() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Standard,
            inputs: vec![], // oops — no inputs
            outputs: vec![TxOutput::new(
                1_000_000,
                PubKeyHash::from_bytes([0; 32]),
            )],
            dag_parents: vec![TxHash::hash(b"p1"), TxHash::hash(b"p2")],
            timestamp: now_ms(),
            fee: 0,
            memo: vec![],
        };
        assert!(matches!(tx.validate_structure(), Err(RelyoError::NoInputs)));
    }

    #[test]
    fn test_dust_output_rejected() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Genesis,
            inputs: vec![],
            outputs: vec![TxOutput::new(1, PubKeyHash::from_bytes([0; 32]))], // too small
            dag_parents: vec![],
            timestamp: 0,
            fee: 0,
            memo: vec![],
        };
        assert!(matches!(tx.validate_structure(), Err(RelyoError::DustOutput { .. })));
    }

    #[test]
    fn test_memo_too_large() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Genesis,
            inputs: vec![],
            outputs: vec![TxOutput::new(
                RLY_UNIT,
                PubKeyHash::from_bytes([0; 32]),
            )],
            dag_parents: vec![],
            timestamp: 0,
            fee: 0,
            memo: vec![0u8; MAX_MEMO_BYTES + 1],
        };
        assert!(matches!(tx.validate_structure(), Err(RelyoError::MemoTooLarge { .. })));
    }

    #[test]
    fn test_insufficient_parents() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Coinbase,
            inputs: vec![],
            outputs: vec![TxOutput::new(
                RLY_UNIT,
                PubKeyHash::from_bytes([0; 32]),
            )],
            dag_parents: vec![TxHash::hash(b"only_one")], // need 2
            timestamp: now_ms(),
            fee: 0,
            memo: vec![],
        };
        assert!(matches!(
            tx.validate_structure(),
            Err(RelyoError::InsufficientParents { .. })
        ));
    }

    #[test]
    fn test_tx_hash_excludes_signatures() {
        // Two transactions identical except for input signatures
        // should produce the same hash (because hash covers signable_bytes).
        let base_input = TxInput {
            prev_output: UtxoId::new(TxHash::hash(b"prev"), 0),
            signature: [0u8; 64].to_vec(),
            pubkey: [1u8; 32],
        };

        let tx1 = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Standard,
            inputs: vec![base_input.clone()],
            outputs: vec![TxOutput::new(50_000, PubKeyHash::from_bytes([9; 32]))],
            dag_parents: vec![TxHash::hash(b"p1"), TxHash::hash(b"p2")],
            timestamp: 12345,
            fee: 0,
            memo: vec![],
        };

        let mut tx2 = tx1.clone();
        tx2.inputs[0].signature = [0xFF; 64].to_vec(); // different sig

        assert_eq!(tx1.hash(), tx2.hash());
    }

    // ── UtxoEntry ───────────────────────────────────────────────────────

    #[test]
    fn test_coinbase_maturity() {
        let entry = UtxoEntry {
            id: UtxoId::new(TxHash::hash(b"cb"), 0),
            output: TxOutput::new(RLY_UNIT, PubKeyHash::from_bytes([0; 32])),
            created_at_depth: 50,
            is_coinbase: true,
        };

        // Not mature yet
        assert!(!entry.is_spendable_at_depth(50));
        assert!(!entry.is_spendable_at_depth(149));

        // Now mature
        assert!(entry.is_spendable_at_depth(150));
        assert!(entry.is_spendable_at_depth(9999));
    }

    #[test]
    fn test_non_coinbase_immediately_spendable() {
        let entry = UtxoEntry {
            id: UtxoId::new(TxHash::hash(b"regular"), 0),
            output: TxOutput::new(RLY_UNIT, PubKeyHash::from_bytes([0; 32])),
            created_at_depth: 100,
            is_coinbase: false,
        };
        assert!(entry.is_spendable_at_depth(100)); // immediately
    }

    // ── EmissionSchedule ────────────────────────────────────────────────

    #[test]
    fn test_emission_decreasing() {
        let sched = EmissionSchedule::new();
        let r0 = sched.reward_at_epoch(0);
        let r1 = sched.reward_at_epoch(1);
        let r100 = sched.reward_at_epoch(100);

        assert!(r0 > 0);
        assert!(r1 < r0);
        assert!(r100 < r1);
    }

    #[test]
    fn test_emission_approaches_zero() {
        let sched = EmissionSchedule::new();
        let r_far = sched.reward_at_epoch(100_000);
        assert_eq!(r_far, 0);
    }

    #[test]
    fn test_emission_total_bounded() {
        let sched = EmissionSchedule::new();
        // After very many epochs, cumulative must not exceed total supply.
        let total = sched.total_emitted_through_epoch(50_000);
        assert!(total <= TOTAL_SUPPLY);
    }

    #[test]
    fn test_epoch_at_depth() {
        let sched = EmissionSchedule::new();
        assert_eq!(sched.epoch_at_depth(0), 0);
        assert_eq!(sched.epoch_at_depth(9_999), 0);
        assert_eq!(sched.epoch_at_depth(10_000), 1);
        assert_eq!(sched.epoch_at_depth(25_000), 2);
    }

    // ── PeerInfo ────────────────────────────────────────────────────────

    #[test]
    fn test_peer_banned() {
        let mut peer = PeerInfo {
            peer_id: "test".into(),
            addresses: vec![],
            pow_challenge: [0; 32],
            pow_solution: 0,
            last_seen: 0,
            reputation: 100,
            version: "1.0".into(),
            bytes_relayed: 0,
            valid_tx_relayed: 0,
            invalid_tx_relayed: 0,
        };
        assert!(!peer.is_banned());
        peer.reputation = -50;
        assert!(peer.is_banned());
    }

    // ── format_rly ──────────────────────────────────────────────────────

    #[test]
    fn test_format_rly() {
        assert_eq!(format_rly(RLY_UNIT), "1 RLY");
        assert_eq!(format_rly(0), "0 RLY");
        assert_eq!(format_rly(RLY_UNIT / 2), "0.5 RLY");
        assert_eq!(format_rly(123_456_789 * RLY_UNIT), "123456789 RLY");
    }

    // ── TxStatus display ────────────────────────────────────────────────

    #[test]
    fn test_txstatus_display() {
        assert_eq!(format!("{}", TxStatus::Confirmed), "confirmed");
        assert_eq!(format!("{}", TxStatus::Rejected), "rejected");
    }

    // ── DagNode ─────────────────────────────────────────────────────────

    #[test]
    fn test_dagnode_confirmation_depth() {
        let tx = Transaction {
            version: PROTOCOL_VERSION,
            kind: TransactionKind::Genesis,
            inputs: vec![],
            outputs: vec![TxOutput::new(
                RLY_UNIT,
                PubKeyHash::from_bytes([0; 32]),
            )],
            dag_parents: vec![],
            timestamp: 0,
            fee: 0,
            memo: vec![],
        };
        let node = DagNode::new(tx, 10, 10);
        assert_eq!(node.confirmation_depth(100), 90);
        assert_eq!(node.confirmation_depth(10), 0);
        assert_eq!(node.confirmation_depth(5), 0); // saturating
    }

    // ── Error variants ──────────────────────────────────────────────────

    #[test]
    fn test_error_display() {
        let err = RelyoError::DustOutput {
            index: 0,
            amount: 100,
            threshold: DUST_THRESHOLD,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("dust"));
    }
}

