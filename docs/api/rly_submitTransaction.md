# rly_submitTransaction

Submits a signed transaction into the node's mempool. 

The node independently verifies the cryptographic signature and mathematical boundaries. If valid, the transaction is gossiped to the P2P network. Transactions must point to two valid parents (`parent_1` and `parent_2`) to weave into the DAG.

## RPC Call

**Method:** `POST`
**Content-Type:** `application/json`

```json
{
  "jsonrpc": "2.0",
  "id": "2",
  "method": "rly_submitTransaction",
  "params": {
    "transaction": {
      "tx_type": "Transfer",
      "sender": "RLYsender...",
      "receiver": "RLYreceiver...",
      "amount": 10000000,
      "fee": 1000000,
      "timestamp": 1711010000000,
      "nonce": 1,
      "parent_1": "hash1...",
      "parent_2": "hash2...",
      "sender_pubkey": "pubkey_hex...",
      "signature": "sig_hex...",
      "data": []
    }
  }
}
```

## Response

Returns the transaction hash if accepted by the mempool.

```json
{
  "jsonrpc": "2.0",
  "id": "2",
  "result": "txhash_hex_string..."
}
```

## Usage (Javascript SDK)

Direct REST submissions require manual parameter serialization. The official Javascript SDK abstracts this.

```javascript
import { RelyoClient, Transaction, KeyPair } from "@relyo/sdk";

const client = new RelyoClient("http://127.0.0.1:9001");
const keys = KeyPair.fromPrivateKey("PRIVATE_KEY_HEX");

const tx = new Transaction({ /* params */ });
await tx.sign(keys);

const txHash = await client.submitTransaction(tx);
```
