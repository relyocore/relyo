# Relyo Network

A peer-to-peer digital ledger based on a Proof-of-Stake Directed Acyclic Graph (DAG) architecture.

Relyo is built on absolute mathematical constraints. It is designed to be highly scalable, explicitly fair, and completely resistant to central control.

## Principles

1. **Hard Cap:** The maximum supply is mathematically locked at exactly 25,000,000,000 RLY. Not a fraction more can ever exist.
2. **Zero Pre-mine:** The genesis block contains zero balances. No tokens were created out of thin air to enrich founders or early investors.
3. **100% Validator Yield:** Every transaction fee and newly minted token goes exclusively to the validating nodes. There is no "treasury tax" or "developer fund" skimming off network activity.
4. **No Admins:** There are no master keys, no admin overrides, and no backdoor pauses. The code is the final authority.

## Architecture

Relyo operates as a Layer-1 PoS DAG. Unlike traditional blockchains that batch transactions into singular blocks every few minutes, Relyo allows every transaction to validate two previous transactions (`parent_1` and `parent_2`). This dual-parent structure creates a self-weaving ledger, drastically reducing latency and removing block-wait times entirely.

Consensus is achieved strictly via Proof of Stake (PoS). Nodes require a minimum of 10,000 RLY locked in the state vault to participate in validation and earn network rewards.

## Compiling from Source

The entire protocol is written in Rust for memory safety, concurrency, and zero-cost abstractions.

```bash
# Clone the repository
git clone https://github.com/AYE-Technology/relyo
cd relyo

# Build the release binary
cargo build --release

# The compiled core node will be available at:
# ./target/release/relyo-node.exe
```

## Running a Node

Running a node is how you secure the network and earn RLY. The minimum hardware requirements are deliberately kept low: 1GB RAM, 2 vCPU, and a light SSD.

```bash
# Start the node and connect to the decentralized network
./target/release/relyo-node --config relyo.toml run
```

## License

This software is released under the MIT License. It belongs to no single entity. You are free to read it, build upon it, and run it.

The math runs the network. The nodes secure it. Nothing else.
