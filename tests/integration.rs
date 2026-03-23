//! Integration tests for the DAG engine — exercises the full pipeline:
//! genesis → fund → transfer → conflict detection → tip management.

use relyo_core::{
    crypto::{KeyPair, Signature},
    token::RELYO_CONFIG,
    transaction::{TransactionBuilder, TransactionHash, TransactionType},
    Address, Transaction,
};
use relyo_dag::{ConflictDetector, DagGraph, LedgerState, Mempool, TipSelector};
use std::sync::Arc;

/// Bootstrap a DAG with genesis allocating `amount` to `receiver`.
fn setup_dag_with_genesis(
    receiver: &Address,
    sender_kp: &KeyPair,
    amount: u64,
) -> (Arc<DagGraph>, TransactionHash) {
    let state = Arc::new(LedgerState::new());
    let tips = Arc::new(TipSelector::new());
    let conflicts = Arc::new(ConflictDetector::new());
    let dag = Arc::new(DagGraph::new(state, tips, conflicts));

    let gen_addr = Address::genesis();
    let mut tx = Transaction {
        tx_type: TransactionType::Genesis,
        sender: gen_addr,
        receiver: receiver.clone(),
        amount,
        fee: 0,
        timestamp: relyo_core::now_ms(),
        nonce: 0,
        parent_1: TransactionHash::zero(),
        parent_2: TransactionHash::zero(),
        sender_pubkey: sender_kp.public_key.clone(),
        signature: Signature::from_bytes([0; 64]),
        data: Vec::new(),
    };
    let msg = tx.signable_bytes();
    tx.signature = sender_kp.sign(&msg);

    let hash = dag.insert_genesis(tx).unwrap();
    (dag, hash)
}

#[test]
fn test_full_transfer_flow() {
    // Setup: Alice gets funded via genesis.
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob_kp = KeyPair::generate();
    let bob = Address::from_public_key(&bob_kp.public_key);

    let fund_amount = 10_000_000_000u64; // 100 RLY
    let (dag, genesis_hash) = setup_dag_with_genesis(&alice, &alice_kp, fund_amount);

    assert_eq!(dag.balance(&alice), fund_amount);
    assert_eq!(dag.balance(&bob), 0);
    assert_eq!(dag.len(), 1);

    // Transfer from Alice to Bob.
    let transfer_amount = 1_000_000_000u64; // 10 RLY
    let tx = TransactionBuilder::new(
        alice.clone(),
        bob.clone(),
        transfer_amount,
        RELYO_CONFIG.base_fee,
        1,
    )
    .parents(genesis_hash.clone(), genesis_hash.clone())
    .sign(&alice_kp);

    let tx_hash = dag.insert(tx).unwrap();
    assert_eq!(dag.len(), 2);

    // Verify balances.
    let expected_alice = fund_amount - transfer_amount - RELYO_CONFIG.base_fee;
    assert_eq!(dag.balance(&alice), expected_alice);
    assert_eq!(dag.balance(&bob), transfer_amount);

    // Verify DAG structure.
    assert!(!dag.tips().all().contains(&genesis_hash));
    assert!(dag.tips().all().contains(&tx_hash));
    assert_eq!(dag.max_depth(), 1);
}

#[test]
fn test_chain_of_transfers() {
    // Alice -> Bob -> Charlie in sequence.
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob_kp = KeyPair::generate();
    let bob = Address::from_public_key(&bob_kp.public_key);
    let charlie_kp = KeyPair::generate();
    let charlie = Address::from_public_key(&charlie_kp.public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 100_000_000_000);

    // Alice sends 50 RLY to Bob.
    let tx1 = TransactionBuilder::new(
        alice.clone(),
        bob.clone(),
        50_000_000_000,
        RELYO_CONFIG.base_fee,
        1,
    )
    .parents(g_hash.clone(), g_hash.clone())
    .sign(&alice_kp);
    let h1 = dag.insert(tx1).unwrap();

    // Credit Bob so he can transact.
    assert_eq!(dag.balance(&bob), 50_000_000_000);

    // Bob sends 25 RLY to Charlie.
    let tx2 = TransactionBuilder::new(
        bob.clone(),
        charlie.clone(),
        25_000_000_000,
        RELYO_CONFIG.base_fee,
        1,
    )
    .parents(h1.clone(), h1.clone())
    .sign(&bob_kp);
    let h2 = dag.insert(tx2).unwrap();

    assert_eq!(dag.balance(&charlie), 25_000_000_000);
    assert_eq!(dag.len(), 3);
    assert_eq!(dag.max_depth(), 2);

    // Only the latest tx should be a tip.
    assert!(dag.tips().all().contains(&h2));
    assert!(!dag.tips().all().contains(&h1));
}

#[test]
fn test_insufficient_balance_rejected() {
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob = Address::from_public_key(&KeyPair::generate().public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 1_000_000);

    // Try to send more than balance.
    let tx = TransactionBuilder::new(alice, bob, 999_999_999, RELYO_CONFIG.base_fee, 1)
        .parents(g_hash.clone(), g_hash)
        .sign(&alice_kp);

    assert!(dag.insert(tx).is_err());
}

#[test]
fn test_duplicate_nonce_rejected() {
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob = Address::from_public_key(&KeyPair::generate().public_key);
    let charlie = Address::from_public_key(&KeyPair::generate().public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 100_000_000_000);

    // First tx with nonce 1.
    let tx1 = TransactionBuilder::new(
        alice.clone(),
        bob,
        1_000_000,
        RELYO_CONFIG.base_fee,
        1,
    )
    .parents(g_hash.clone(), g_hash.clone())
    .sign(&alice_kp);
    let h1 = dag.insert(tx1).unwrap();

    // Second tx with same nonce 1 — should fail.
    let tx2 = TransactionBuilder::new(alice, charlie, 1_000_000, RELYO_CONFIG.base_fee, 1)
        .parents(h1.clone(), h1)
        .sign(&alice_kp);

    assert!(dag.insert(tx2).is_err());
}

#[test]
fn test_mempool_drain_into_dag() {
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob = Address::from_public_key(&KeyPair::generate().public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 100_000_000_000);

    let mempool = Mempool::new(100);

    // Add 5 transactions to mempool.
    for i in 1..=5u64 {
        let tx = TransactionBuilder::new(
            alice.clone(),
            bob.clone(),
            100_000,
            RELYO_CONFIG.base_fee,
            i,
        )
        .parents(g_hash.clone(), g_hash.clone())
        .sign(&alice_kp);
        mempool.insert(tx);
    }
    assert_eq!(mempool.len(), 5);

    // Drain and insert into DAG.
    let batch = mempool.drain_batch(10);
    assert_eq!(batch.len(), 5);
    assert_eq!(mempool.len(), 0);

    // Sorted by timestamp (FIFO), nonce ordering handled by DAG.
    // The first batch item should succeed, subsequent ones may fail due to
    // parents pointing at genesis (they share the same parents).
    let mut successes = 0;
    for tx in batch {
        if dag.insert(tx).is_ok() {
            successes += 1;
        }
    }
    assert!(successes >= 1, "at least one tx from mempool should succeed");
}

#[test]
fn test_dag_pruning() {
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob = Address::from_public_key(&KeyPair::generate().public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 100_000_000_000);

    // Build a chain of 5 confirmed transactions.
    let mut prev = g_hash;
    for i in 1..=5u64 {
        let tx = TransactionBuilder::new(
            alice.clone(),
            bob.clone(),
            100_000,
            RELYO_CONFIG.base_fee,
            i,
        )
        .parents(prev.clone(), prev.clone())
        .sign(&alice_kp);
        prev = dag.insert(tx).unwrap();

        // Mark as confirmed.
        dag.set_status(&prev, relyo_core::TransactionStatus::Confirmed);
    }

    assert_eq!(dag.len(), 6); // 1 genesis + 5 transfers

    // Prune transactions deeper than 2 confirmations.
    let pruned = dag.prune_confirmed(2);
    assert!(!pruned.is_empty(), "should prune deeply confirmed transactions");
}

#[test]
fn test_ancestor_traversal() {
    let alice_kp = KeyPair::generate();
    let alice = Address::from_public_key(&alice_kp.public_key);
    let bob = Address::from_public_key(&KeyPair::generate().public_key);

    let (dag, g_hash) = setup_dag_with_genesis(&alice, &alice_kp, 100_000_000_000);

    let tx1 = TransactionBuilder::new(
        alice.clone(),
        bob.clone(),
        100_000,
        RELYO_CONFIG.base_fee,
        1,
    )
    .parents(g_hash.clone(), g_hash.clone())
    .sign(&alice_kp);
    let h1 = dag.insert(tx1).unwrap();

    let tx2 = TransactionBuilder::new(alice, bob, 100_000, RELYO_CONFIG.base_fee, 2)
        .parents(h1.clone(), h1.clone())
        .sign(&alice_kp);
    let h2 = dag.insert(tx2).unwrap();

    // h2's ancestors include h1 and genesis.
    let ancestors = dag.get_ancestors(&h2, 10);
    assert!(ancestors.contains(&h1));
    assert!(ancestors.contains(&g_hash));

    // h2's descendants from genesis.
    let desc = dag.get_descendants(&g_hash, 10);
    assert!(desc.contains(&h1));
    assert!(desc.contains(&h2));
}


