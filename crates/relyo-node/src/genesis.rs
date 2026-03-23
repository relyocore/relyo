use relyo_core::{
    crypto::{KeyPair, Signature},
    transaction::TransactionHash,
    Address, Transaction, TransactionType,
};
use relyo_dag::DagGraph;
use tracing::info;

/// Create the genesis transaction that initializes the ledger zero-state.
/// Returns the hash of the genesis transaction.
///
/// Genesis block contains only protocol rules — zero pre-allocated balance.
/// Tokens are only minted via node validation rewards.
pub fn create_genesis(dag: &DagGraph, genesis_keypair: &KeyPair) -> Vec<TransactionHash> {
    let genesis_addr = Address::genesis();

    let mut tx = Transaction {
        tx_type: TransactionType::Genesis,
        sender: genesis_addr.clone(),
        receiver: genesis_addr.clone(), // Receiver doesn't matter since amount is zero
        amount: 0,                      // ZERO pre-allocation
        fee: 0,
        timestamp: 0, // epoch zero
        nonce: 0,
        parent_1: TransactionHash::zero(),
        parent_2: TransactionHash::zero(),
        sender_pubkey: genesis_keypair.public_key.clone(),
        signature: Signature::from_bytes([0; 64]),
        data: b"Relyo Genesis: Let there be light".to_vec(),
    };

    let msg = tx.signable_bytes();
    tx.signature = genesis_keypair.sign(&msg);

    let mut hashes = Vec::new();
    match dag.insert_genesis(tx) {
        Ok(hash) => {
            info!("genesis initialized (0 RLY) hash={}", hash);
            hashes.push(hash);
        }
        Err(e) => {
            tracing::error!("genesis initialization failed: {}", e);
        }
    }

    hashes
}
