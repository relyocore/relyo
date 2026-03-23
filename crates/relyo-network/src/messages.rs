use relyo_core::{Transaction, TransactionHash};
use serde::{Deserialize, Serialize};

/// Types of messages exchanged between Relyo nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Broadcast a new transaction for DAG insertion.
    TransactionBroadcast,
    /// Request a consensus vote on a transaction.
    VoteRequest,
    /// A consensus vote response.
    VoteResponse,
    /// Request a specific transaction by hash.
    TransactionRequest,
    /// Response with a requested transaction.
    TransactionResponse,
    /// Node status heartbeat.
    Heartbeat,
    /// Request recent tips.
    TipRequest,
    /// Response with current tips.
    TipResponse,
}

/// A network message with typed payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub msg_type: MessageType,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}

impl NetworkMessage {
    pub fn new(msg_type: MessageType, payload: Vec<u8>) -> Self {
        NetworkMessage {
            msg_type,
            payload,
            timestamp: relyo_core::now_ms(),
        }
    }

    /// Create a transaction broadcast message.
    pub fn tx_broadcast(tx: &Transaction) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(tx)?;
        Ok(Self::new(MessageType::TransactionBroadcast, payload))
    }

    /// Create a vote request message.
    pub fn vote_request(tx_hash: &TransactionHash) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(tx_hash)?;
        Ok(Self::new(MessageType::VoteRequest, payload))
    }

    /// Create a vote response message.
    pub fn vote_response(vote: &crate::messages::VotePayload) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(vote)?;
        Ok(Self::new(MessageType::VoteResponse, payload))
    }

    /// Create a heartbeat message.
    pub fn heartbeat(info: &NodeHeartbeat) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(info)?;
        Ok(Self::new(MessageType::Heartbeat, payload))
    }

    /// Serialize the entire message for wire transport.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from wire bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

/// Payload for vote response messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotePayload {
    pub tx_hash: TransactionHash,
    pub accept: bool,
    pub node_id: String,
}

/// Periodic heartbeat from a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub dag_size: u64,
    pub mempool_size: u64,
    pub peers: u64,
    pub uptime_secs: u64,
    pub version: String,
}
