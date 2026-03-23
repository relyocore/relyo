#![allow(non_snake_case)]
//! JSON-RPC 2.0 server for wallet and dApp interaction.
//!
//! Exposes the following methods:
//! - `rly_getBalance`      — query address balance
//! - `rly_getNonce`        — query address nonce
//! - `rly_getTips`         — get current DAG tip hashes
//! - `rly_submitTransaction` — submit a signed transaction
//! - `rly_getTransaction`  — query a transaction by hash
//! - `rly_getStats`        — network statistics
//! - `rly_getEmission`     — current emission reward info

use axum::{extract::State, response::IntoResponse, routing::{post, get}, Json, Router};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use futures::{sink::SinkExt, stream::StreamExt};
use relyo_core::{Address, Transaction, TransactionHash};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

use crate::node::RelyoNode;

/// Shared state accessible by all RPC handlers.
pub struct RpcState {
    pub node: Arc<RelyoNode>,
    pub ws_sender: broadcast::Sender<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[allow(non_snake_case)]
pub enum WsEvent {
    #[serde(rename = "transaction.finalized")]
    TransactionFinalized { txHash: String, sender: String, receiver: String, amount: u64, fee: u64, timestamp: u64 },
    #[serde(rename = "epoch.changed")]
    EpochChanged { epochNumber: u64, reward: u64, activeNodes: usize },
    #[serde(rename = "node.scoreUpdated")]
    NodeScoreUpdated { address: String, uptime: f64, validation: u64, bandwidth: u64, totalScore: f64 },
    #[serde(rename = "network.stats")]
    NetworkStats { tps: f64, totalNodes: usize, circulatingSupply: u64 },
}

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

/// JSON-RPC 2.0 response envelope.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError { code, message }),
            id,
        }
    }
}

// ─── Standard JSON-RPC error codes ──────────────────────────────────────────

#[allow(dead_code)]
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i32 = -32603;
// Application-specific errors start at -32000
const TX_REJECTED: i32 = -32000;

/// Build the JSON-RPC Axum router.
pub fn rpc_router(node: Arc<RelyoNode>, ws_sender: broadcast::Sender<String>) -> Router {
    let state = Arc::new(RpcState { node, ws_sender });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", post(handle_rpc))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RpcState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_web_socket(socket, state))
}

async fn handle_web_socket(socket: WebSocket, state: Arc<RpcState>) {
    let (mut sender, mut _receiver) = socket.split();
    let mut rx = state.ws_sender.subscribe();

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(WsMessage::Text(msg)).await.is_err() {
                break;
            }
        }
    });
}

/// Start the RPC server on the given address.
pub async fn start_rpc_server(
    bind: SocketAddr,
    node: Arc<RelyoNode>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (ws_sender, _) = broadcast::channel(100);

    // 1. Spawn network stats background task
    let node_clone1 = node.clone();
    let ws_clone1 = ws_sender.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let current_supply = node_clone1.dag.state().total_circulating();
            let network_stats = WsEvent::NetworkStats {
                tps: 0.0, // Placeholder, can be calculated via dag moving average
                totalNodes: 1, // Placeholder
                circulatingSupply: current_supply,
            };
            if let Ok(json) = serde_json::to_string(&network_stats) {
                let _ = ws_clone1.send(json);
            }
        }
    });

    // 2. Spawn consensus event listener task
    if let Some(mut rx) = node.consensus.take_event_receiver() {
        let node_clone2 = node.clone();
        let ws_clone2 = ws_sender.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    relyo_consensus::engine::ConsensusEvent::Finalized { tx_hash, accepted } => {
                        if accepted {
                            if let Some(dag_node) = node_clone2.dag.get(&tx_hash) {
                                let tx = dag_node.tx;
                                let finalized_event = WsEvent::TransactionFinalized {
                                    txHash: tx_hash.to_string(),
                                    sender: tx.sender.to_string(),
                                    receiver: tx.receiver.to_string(),
                                    amount: tx.amount,
                                    fee: tx.fee,
                                    timestamp: tx.timestamp,
                                };
                                if let Ok(json) = serde_json::to_string(&finalized_event) {
                                    let _ = ws_clone2.send(json);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    let router = rpc_router(node, ws_sender);
    info!("JSON-RPC server starting on http://{}", bind);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

/// Main RPC dispatch handler.
async fn handle_rpc(
    State(state): State<Arc<RpcState>>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    if req.jsonrpc != "2.0" {
        return Json(JsonRpcResponse::error(
            req.id,
            INVALID_REQUEST,
            "jsonrpc must be \"2.0\"".into(),
        ));
    }

    let response = match req.method.as_str() {
        "rly_getBalance" => handle_get_balance(&state, &req.params, req.id.clone()),
        "rly_getNonce" => handle_get_nonce(&state, &req.params, req.id.clone()),
        "rly_getTips" => handle_get_tips(&state, req.id.clone()),
        "rly_submitTransaction" => handle_submit_tx(&state, &req.params, req.id.clone()),
        "rly_getTransaction" => handle_get_transaction(&state, &req.params, req.id.clone()),
        "rly_getStats" => handle_get_stats(&state, req.id.clone()),
        "rly_getEmission" => handle_get_emission(&state, req.id.clone()),
        _ => JsonRpcResponse::error(
            req.id,
            METHOD_NOT_FOUND,
            format!("method '{}' not found", req.method),
        ),
    };

    Json(response)
}

// ─── Handler implementations ────────────────────────────────────────────────

/// `rly_getBalance` — params: ["address_string"]
fn handle_get_balance(
    state: &RpcState,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let addr_str = match params.get(0).and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "expected [address]".into());
        }
    };

    let addr: Address = match addr_str.parse() {
        Ok(a) => a,
        Err(e) => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, format!("invalid address: {}", e));
        }
    };

    let balance = state.node.dag.balance(&addr);
    let nonce = state.node.dag.nonce(&addr);

    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "address": addr.to_string(),
            "balance": balance,
            "balance_rly": relyo_core::token::base_to_rly(balance),
            "nonce": nonce,
        }),
    )
}

/// `rly_getNonce` — params: ["address_string"]
fn handle_get_nonce(
    state: &RpcState,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let addr_str = match params.get(0).and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "expected [address]".into());
        }
    };

    let addr: Address = match addr_str.parse() {
        Ok(a) => a,
        Err(e) => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, format!("invalid address: {}", e));
        }
    };

    let nonce = state.node.dag.nonce(&addr);
    JsonRpcResponse::success(id, serde_json::json!({ "nonce": nonce }))
}

/// `rly_getTips` — no params.
fn handle_get_tips(state: &RpcState, id: serde_json::Value) -> JsonRpcResponse {
    let tips: Vec<String> = state
        .node
        .dag
        .tips()
        .all()
        .iter()
        .map(|h| h.to_hex())
        .collect();

    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "tips": tips,
            "count": tips.len(),
        }),
    )
}

/// `rly_submitTransaction` — params: [serialized_tx_json]
fn handle_submit_tx(
    state: &RpcState,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let tx_value = match params.get(0) {
        Some(v) => v,
        None => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                "expected [transaction_object]".into(),
            );
        }
    };

    let tx: Transaction = match serde_json::from_value(tx_value.clone()) {
        Ok(t) => t,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("invalid transaction: {}", e),
            );
        }
    };

    match state.node.submit_transaction(tx) {
        Ok(hash) => JsonRpcResponse::success(
            id,
            serde_json::json!({
                "hash": hash.to_hex(),
                "status": "accepted",
            }),
        ),
        Err(e) => {
            warn!("RPC tx rejected: {}", e);
            JsonRpcResponse::error(id, TX_REJECTED, format!("transaction rejected: {}", e))
        }
    }
}

/// `rly_getTransaction` — params: ["tx_hash_hex"]
fn handle_get_transaction(
    state: &RpcState,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let hash_hex = match params.get(0).and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "expected [tx_hash]".into());
        }
    };

    let hash = match TransactionHash::from_hex(hash_hex) {
        Ok(h) => h,
        Err(e) => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, format!("invalid hash: {}", e));
        }
    };

    match state.node.dag.get(&hash) {
        Some(node) => {
            let tx = &node.tx;
            JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "hash": hash.to_hex(),
                    "sender": tx.sender.to_string(),
                    "receiver": tx.receiver.to_string(),
                    "amount": tx.amount,
                    "amount_rly": relyo_core::token::base_to_rly(tx.amount),
                    "fee": tx.fee,
                    "timestamp": tx.timestamp,
                    "nonce": tx.nonce,
                    "parent_1": tx.parent_1.to_hex(),
                    "parent_2": tx.parent_2.to_hex(),
                    "status": format!("{:?}", node.status),
                    "weight": node.weight,
                    "depth": node.depth,
                    "confirmation_depth": state.node.dag.confirmation_depth(&hash),
                }),
            )
        }
        None => JsonRpcResponse::error(id, INVALID_PARAMS, "transaction not found".into()),
    }
}

/// `rly_getStats` — no params.
fn handle_get_stats(state: &RpcState, id: serde_json::Value) -> JsonRpcResponse {
    let dag = &state.node.dag;
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "dag_size": dag.len(),
            "max_depth": dag.max_depth(),
            "tip_count": dag.tips().len(),
            "mempool_size": state.node.mempool_size(),
            "uptime_secs": state.node.uptime_secs(),
            "circulating_supply": dag.state().total_circulating(),
            "circulating_supply_rly": relyo_core::token::base_to_rly(dag.state().total_circulating()),
            "total_supply": relyo_core::token::RELYO_CONFIG.total_supply,
            "node_id": state.node.node_id().to_string(),
        }),
    )
}

/// `rly_getEmission` — no params. Returns current emission schedule info.
fn handle_get_emission(state: &RpcState, id: serde_json::Value) -> JsonRpcResponse {
    let dag = &state.node.dag;
    let depth = dag.max_depth();
    let reward = crate::emission::reward_at_depth(depth);
    let epoch = crate::emission::epoch_at_depth(depth);
    let total_emitted = crate::emission::total_emitted_by_depth(depth);

    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "current_depth": depth,
            "current_epoch": epoch,
            "reward_per_tx": reward,
            "reward_per_tx_rly": relyo_core::token::base_to_rly(reward),
            "total_emitted": total_emitted,
            "total_emitted_rly": relyo_core::token::base_to_rly(total_emitted),
            "remaining": relyo_core::token::RELYO_CONFIG.total_supply.saturating_sub(total_emitted),
            "decay_model": "exponential",
            "total_epochs": crate::emission::TOTAL_EPOCHS,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse::success(
            serde_json::json!(1),
            serde_json::json!({"balance": 100}),
        );
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.jsonrpc, "2.0");
    }

    #[test]
    fn test_json_rpc_response_error() {
        let resp = JsonRpcResponse::error(
            serde_json::json!(1),
            METHOD_NOT_FOUND,
            "not found".into(),
        );
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }
}
