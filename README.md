# Relyo Core

Relyo is a decentralized, peer-to-peer electronic cash system. 

It is built to be fast, highly secure, and extremely simple. Relyo does not use a traditional blockchain. Instead, it uses a Blockless DAG (Directed Acyclic Graph) combined with Proof-of-Stake. This means there are no miners, no expensive energy costs, and transactions are processed directly by the network.

## Legal Disclaimer & Notice

**Please read this carefully before doing anything.**

Relyo is purely a piece of open-source software. AYE Technology (the creators of this code) is **not** selling anything. We are not a company raising funds, there was no ICO, and we are not asking for money. 

Relyo is meant to function exactly like Bitcoin—as a free, open, and permissionless digital currency. We do not control the network, we do not control the price, and we do not guarantee any financial return. You use this software entirely at your own risk.

---

## Tokenomics (The Hard Rules)

We believe in absolute mathematical scarcity. We have written strict rules into the core code that can never be changed:

- **Maximum Supply**: Exactly 25,000,000,000 RLY (25 Billion). Not a single coin more can ever exist.
- **Pre-mine**: 0%. The code starts completely fair.
- **Validator Yield**: 100%. Validators get all the transaction fees. There is no "admin" tax or "dev fund" fee.
- **Admin Keys**: None. There are no backdoor access keys. The community owns the network.

---

## How to Create a Wallet

You need a wallet to hold your RLY coins or to stake them.

1. Make sure you have Rust installed on your computer.
2. Clone this repository to your local machine.
3. Open your terminal or command prompt inside the folder.
4. Run the following command to generate a new keypair:

```bash
cargo run --bin relyo-wallet -- create
```

This will output your public address (starting with `RLY...`) and your private key. 
**Save your private key somewhere extremely safe.** If you lose it, you lose your coins. Nobody can recover it for you.

---

## How to Run a Node & Become a Validator

If you want to secure the network and earn transaction fees, you can run a validator node.

### Server Requirements
- **RAM**: 1GB minimum (+2GB swap file recommended)
- **CPU**: 2 Cores
- **Storage**: 40GB SSD
- **Network**: IPv6 enabled, ports 9740 and 9741 must be open

### Installation Steps

1. Install Rust (`curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
2. Clone this source code to your server.
3. Build the node software:

```bash
cargo build --release --bin relyo-node
```

4. Create your node configuration file (`relyo.toml`). You will need to put your validator private key inside this file.
5. Start the node:

```bash
./target/release/relyo-node --config relyo.toml
```

To become an active validator, you must stake a minimum of `10,000 RLY`. Note that if you decide to unstake, there is a strict 30-day lock period before your coins are free to move again. This keeps the network stable.

---

## License

This project is released under the **MIT License**. It is free for anyone to use, modify, and distribute.
