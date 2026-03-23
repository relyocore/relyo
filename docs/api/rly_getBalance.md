# rly_getBalance

Returns the confirmed balance of the specified Relyo address in base units (1 RLY = 100,000,000 base units).
The network enforces absolute supply invariants; requested balances reflect the mathematically verified state of the DAG at the local node.

## RPC Call

**Method:** `POST`
**Content-Type:** `application/json`

```json
{
  "jsonrpc": "2.0",
  "id": "1",
  "method": "rly_getBalance",
  "params": {
    "address": "RLY9x8..."
  }
}
```

## Response

```json
{
  "jsonrpc": "2.0",
  "id": "1",
  "result": 5000000000
}
```

*(Result `5000000000` equals 50 RLY)*

## Usage

```bash
curl -X POST http://127.0.0.1:9001/ \
-H "Content-Type: application/json" \
-d '{"jsonrpc":"2.0", "id":"1", "method":"rly_getBalance", "params":{"address":"RLY9x8..."}}'
```
