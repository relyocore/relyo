# Relyo Core

Relyo is a decentralized, peer-to-peer electronic cash system designed for direct, fast, and secure value transfer.

## Why Relyo Exists

The goal of Relyo is to create a digital currency that does not rely on heavy energy consumption (mining) or centralized bottlenecks. We wanted to build a network where every user can participate freely, and where transactions settle efficiently using purely mathematical scarcity and modern cryptographic principles. 

## How Relyo Works

Instead of using a traditional blockchain where transactions are grouped into slow blocks by miners, Relyo uses a **Blockless DAG (Directed Acyclic Graph)** combined with **Proof-of-Stake (PoS)**. 
- **DAG Architecture:** Every new transaction directly references and verifies previous transactions (similar to a Tangle).
- **Proof of Stake:** Validators secure the network by staking their own coins rather than burning electricity. 
- **No Middlemen:** Transactions flow peer-to-peer and are validated mathematically by the network participants themselves.

---

## Legal Disclaimer & Notice

**Please read this carefully before interacting with this software.**

Relyo is strictly open-source software. AYE Technology (the core developers) are **not** selling anything, there is no ICO, and we are not raising funds. 

Relyo operates purely as a free, open, and permissionless digital entity. We do not control the network, we do not control the market price, and there are absolutely no guarantees of financial return. You use this software independently and entirely at your own risk.

---

## Network Rules & Tokenomics

The following immutable rules are mathematically strictly hardcoded into the network protocol:

- **Maximum Supply**: Exactly 25,000,000,000 RLY (25 Billion max cap). 
- **Pre-mine**: 0%. The genesis starts clean and fair.
- **Validator Yield**: 100%. Validators receive all the transaction fees. There is no dev tax or treasury fund.
- **Admin Keys**: None. There is zero backdoor access. The project relies strictly on the community.

---

## System Requirements for a Main Node

To keep the network stable and process the DAG efficiently, your server (VPS) must meet these minimum requirements:
- **RAM:** 4 GB minimum
- **CPU:** 2 vCPU 
- **Storage:** 40 GB SSD 
- **Network:** IPv4 or IPv6 enabled. You **must** open ports 9740 and 9741 on your firewall for peer discovery and data propagation.

---

## Step-by-Step Installation Instruction

### 1. Install System Dependencies
Open your server terminal (Ubuntu/Debian) and install Rust along with necessary build tools:
```bash
sudo apt update && sudo apt upgrade -y
sudo apt install build-essential curl git -y
curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Download the Source Code
Clone the Relyo repository to your local machine:
```bash
git clone https://github.com/relyocore/Relyo.git
cd Relyo
```

### 3. Compile the Code
Compile the Relyo node and wallet software from scratch. This process will take a few minutes depending on your CPU:
```bash
cargo build --release
```
Once the build is complete, you will find the executables inside the `target/release/` directory.

---

## How to Create a Wallet

You need a cryptographic keypair (wallet) to securely store your RLY or to register a validator node.

1. Ensure you are inside the `Relyo` folder.
2. Run the wallet creation command:
```bash
./target/release/relyo-wallet -- create
```
3. The command will print out your **Public Address** (starts with `RLY...`) and your **Private Key**. 
4. **CRITICAL WARNING:** Save your private key offline, securely written on paper or inside an encrypted password manager. If you lose your private key, your funds cannot be recovered by anyone.

---

## How to Join the Network (Run a Validator)

A validator secures the Relyo network by verifying transaction mathematics and in return earns transaction fees.

1. Setup your firewall to allow external traffic:
```bash
sudo ufw allow 9740/tcp
sudo ufw allow 9741/tcp
```

2. Create a file named `relyo.toml` in your base `Relyo` directory and insert your node configurations. It must include the private key of your validator holding the stake. Example:
```toml
[node]
private_key = "YOUR_PRIVATE_KEY_HERE"
port = 9740
```

3. Launch your node into the main network:
```bash
./target/release/relyo-node --config relyo.toml
```

**Staking Rule:** To become an active validator, your public address must contain a minimum stake of **10,000 RLY**. Once locked in, there is a strict **30-day (10-epoch) unstaking lock period** if you decide to withdraw your coins to prevent network instability.

---

## License

This software is released under the **MIT License**. It is purely free for anyone to use, examine, modify, and distribute.
