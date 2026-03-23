use thiserror::Error;

#[derive(Error, Debug)]
pub enum RelyoError {
    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("batch signature verification failed: {count} invalid out of {total}")]
    BatchVerifyError { count: usize, total: usize },

    #[error("invalid address: {0}")]
    InvalidAddress(String),

    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance { have: u64, need: u64 },

    #[error("duplicate nonce: {0}")]
    DuplicateNonce(u64),

    #[error("supply cap exceeded: {0}")]
    SupplyCapExceeded(String),

    #[error("invalid parent reference: {0}")]
    InvalidParent(String),

    #[error("transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("double spend detected for transaction {0}")]
    DoubleSpend(String),

    #[error("dag error: {0}")]
    Dag(String),

    #[error("consensus error: {0}")]
    Consensus(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("rate limit exceeded for {0}")]
    RateLimitExceeded(String),

    #[error("node not found: {0}")]
    NodeNotFound(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("wallet error: {0}")]
    Wallet(String),

    #[error("genesis already exists")]
    GenesisExists,

    #[error("operation timed out after {0}ms")]
    Timeout(u64),

    #[error("arithmetic overflow: {0}")]
    Overflow(String),

    #[error("merkle proof verification failed: {0}")]
    MerkleError(String),

    #[error("snapshot error: {0}")]
    SnapshotError(String),

    #[error("staking error: {0}")]
    StakingError(String),

    #[error("slashing error: {0}")]
    SlashingError(String),

    #[error("fee market error: {0}")]
    FeeMarketError(String),

    #[error("proof of work error: {0}")]
    PowError(String),

    #[error("checkpoint error: {0}")]
    CheckpointError(String),

    #[error("channel closed")]
    ChannelClosed,

    #[error("peer error: {0}")]
    PeerError(String),

    #[error("sync error: {0}")]
    SyncError(String),
}

pub type Result<T> = std::result::Result<T, RelyoError>;

impl From<ed25519_dalek::SignatureError> for RelyoError {
    fn from(e: ed25519_dalek::SignatureError) -> Self {
        RelyoError::Crypto(e.to_string())
    }
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

impl From<hex::FromHexError> for RelyoError {
    fn from(e: hex::FromHexError) -> Self {
        RelyoError::Serialization(e.to_string())
    }
}

impl From<std::io::Error> for RelyoError {
    fn from(e: std::io::Error) -> Self {
        RelyoError::Storage(e.to_string())
    }
}
