# WebSocket Event Streams

Relyo nodes expose a persistent WebSocket connection. This streams real-time updates regarding network consensus, state changes, and transaction finality directly from the DAG.

Authentication is not required. The stream is read-only.

## Connection

**Endpoint:** `ws://127.0.0.1:8001/ws`

## Protocol

Events are emitted as stringified JSON payloads.

```json
{
    "type": "event_type",
    "data": { ... }
}
```

### Event: `transaction.finalized`
Emitted when a transaction secures enough weight in the DAG to be considered mathematically irreversible.

```json
{
  "type": "transaction.finalized",
  "data": {
    "txHash": "hash...",
    "sender": "RLYsender...",
    "receiver": "RLYreceiver...",
    "amount": 100000000,
    "fee": 1000000,
    "timestamp": 1711019920123
  }
}
```

### Event: `network.stats`
Emitted at regular intervals to broadcast the state of the local node's routing parameters.

```json
{
  "type": "network.stats",
  "data": {
    "tps": 120.5,
    "totalNodes": 140,
    "circulatingSupply": 50000000000
  }
}
```

## Example Implementation

```javascript
const ws = new WebSocket("ws://127.0.0.1:8001/ws");

ws.onmessage = (event) => {
  const payload = JSON.parse(event.data);

  if (payload.type === "transaction.finalized") {
    console.log(`Confirmed: ${payload.data.amount} to ${payload.data.receiver}`);
  }
};

ws.onclose = () => console.log("Stream disconnected.");
```
