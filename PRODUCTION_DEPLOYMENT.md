# 🚀 Relyo Protocol: The Easy VPS Deployment Guide

_Dost, yeh guide ek dam simple language mai banayi gayi hai. Agar pehle samajh nahi aaya tha, toh ab ek dam easily samajh aa jayega!_

## 🧠 Basic Concept: Hum kar kya rahe hain?

Socho ki blockchain ek public hisaab-kitab ka register hai.

1. **Node 1 (VPS A):** Yeh pehla computer hai jo register open karega aur rules set karega. Isko hum "Bootnode" bolte hain.
2. **Node 2 (VPS B):** Yeh dusra computer hai, jo pehle wale se internet ke sariye judega aur check karega sab theek hai ya nahi.
3. **Wallet:** Yeh tumhari digital chabi (crypto keys) hai. Ise tumhara paisa secure hota hai jaise MetaMask mai seed phrase hota hai.

---

## Step 1: Server Ready Karna (Dono VPS Par Karo)

Login karo apne dono servers (Node A aur Node B) par, aur line-by-line yeh copy-paste karo. Yeh tumhare server par zaroori softwares daal dega:

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install build-essential pkg-config libssl-dev protobuf-compiler curl -y
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Ab code ko folder mai laao (Github se clone karke ya zip upload karke):

```bash
cd relyo-protocol
```

---

## Step 2: Apna Main Wallet Banao (Sirf Node A Par)

Network start hone se pehle ek address chahiye jahan free/genesis coins aayenge.

Node A par yeh run karo:

```bash
# Yeh command naya wallet banayega (Jaise MetaMask mai 'Create Wallet' hota hai)
cargo build --release --bin relyo-wallet
./target/release/relyo-wallet create
```

**Kamaal ki baat:** Yeh tumhe ek secret file dega `wallet.key`. Isme tumhare private keys hain. Aur tumhe ek Address dega jaisa (e.g., `RLY9x8...`). Apna address Copy karlo!

---

## Step 3: Pehla Node Start Karo (Node A - The Bootnode)

Ab Node A ko start karte hain:

```bash
cargo build --release --bin relyo-node
./target/release/relyo-node init
```

Ek settings file banegi `relyo.toml`. Usko edit karo:

```bash
nano relyo.toml
```

Wahan `miner_address` ki jagah apna naya **Wallet Address** daal do jo abhi mila tha!

Ab server Run karo:

```bash
./target/release/relyo-node run
```

**Congrats!** Pehla node start ho gaya. Screen par ek code aayega jisme `/ip4/...` hoga. Usko copy karlo, yeh Node A ka IP connection point hai.

---

## Step 4: Dusra Node Connect Karo (Node B)

Ab Node B par jao aur same chiz repeat karo:

```bash
cd relyo-protocol
cargo build --release --bin relyo-node
./target/release/relyo-node init
```

Iske `relyo.toml` ko edit karo (`nano relyo.toml`) aur `bootnodes = []` ke andar Node A ka wo lamba sa `/ip4/...` code daal do. Isse Node B ko pata chal jayega ki Node A kahan hai.

Run karo:

```bash
./target/release/relyo-node run
```

**Boom!** Dono servers show karenge: `Peer Connected!`. Tumhara network live hai!

---

## Step 5: Pehla Transaction Validate Karna!

Tumhara Node A start hote hi blocks banana shuru kar dega, aur rules ke hisaab se tumhare address par rewards aa jayenge.

Balance check karo:

```bash
./target/release/relyo-wallet balance
```

_Yahan tumhe apne RLY coins dikh jayenge!_

**Ab kisi aur ko bhejna (Metamask style send):**

```bash
./target/release/relyo-wallet send --to KISI_BHI_DOST_KA_ADDRESS --amount 10.5
```

Yeh transaction tumhari `wallet.key` se sign hoga, Network pe jayega, aur Node B usko validate karke confirm kar dega!

**Ho Gaya!** Ek dam easily tumne live network bana diya!


---

## Step 6: Production Security (Nginx DDoS Protection)

Internet par apna RPC / WebSocket open expose karne se DDoS attack ho sakta hai (jaise ek IP se 100,000 connections banana). Isse bachne ke liye humein Nginx (Reverse Proxy) setup karna chahiye taki har IP sirfh 10 WS connections bana sake.

Nginx install karo:
```bash
sudo apt install nginx -y
```

Nginx ki configuration file `/etc/nginx/sites-available/relyo` me yeh dalo (zaroorat padne par `NODE_PORT` ko apne actual port, jaise `9742`, se replace karein):

```nginx
# IP-based rate limiting zone create karo (10MB space for tracking)
limit_conn_zone $binary_remote_addr zone=ws_limit:10m;

server {
    listen 80;
    server_name YOUR_DOMAIN_YA_IP;

    location / {
        proxy_pass http://localhost:9742; # RPC Port
    }

    location /ws {
        # Max 10 WebSocket connections per IP
        limit_conn ws_limit 10;
        
        proxy_pass http://localhost:9742;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

Enable karke restart maro:
```bash
sudo ln -s /etc/nginx/sites-available/relyo /etc/nginx/sites-enabled/
sudo systemctl restart nginx
```

Ab tumhara Bootnode aur WebSockets hackers se fully protected hain! ???

