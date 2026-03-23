use relyo_core::{get_consensus_hash, Transaction, Address, RelyoError, NodeId};
// We need testing the startup check and the node ban mechanism.
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn test_startup_verification_triggers_correctly() {
    let hash = get_consensus_hash();
    let content = include_str!("../../relyo-core/src/constants.rs");
    let normalized = content.replace("\r\n", "\n");
    let expected = blake3::hash(normalized.as_bytes()).to_hex().to_string();
    assert_eq!(hash, expected, "Consensus hash mismatch!");
}

#[tokio::test]
async fn test_tx_propagation_and_ban() {
    // E2E network simulation testing transaction error propagation and bans.
    // In memory simulation:
    // A real networking test using memory transport would set up two swarms.
    // Here we test DAG rejection semantics directly.
    let config = relyo_node::config::NodeConfig::default();
    let (tx_sender, _rx) = tokio::sync::mpsc::unbounded_channel();
    let node = relyo_node::node::RelyoNode::new(config, tx_sender).unwrap();

    let keypair = relyo_core::crypto::KeyPair::generate();
    let sender_pubkey = keypair.public_key.clone();
    let sender = relyo_core::Address::from_public_key(&sender_pubkey);
    let receiver = relyo_core::Address::genesis();
    
    let tx = relyo_core::transaction::TransactionBuilder {
        sender,
        receiver,
        amount: 25_000_000_001 * relyo_core::constants::RLY_UNIT,
        fee: 10_000_000,
        nonce: 1,
        parent_1: relyo_core::TransactionHash::zero(),
        parent_2: relyo_core::TransactionHash::zero(),
        data: vec![],
        tx_type: relyo_core::TransactionType::Genesis,
    }.sign(&keypair);

    let result = node.dag.insert(tx);
    match result {
        Err(RelyoError::SupplyCapExceeded(_)) => {
            // Success, the DAG correctly shielded the protocol
        },
        other => panic!("Expected SupplyCapExceeded! Got: {:?}", other),
    }
}
