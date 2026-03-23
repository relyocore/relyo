use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use relyo_core::token::base_to_rly;
use relyo_core::{Address, TransactionHash};
use std::sync::Arc;

use crate::api::*;
use crate::server::AppState;

/// GET /api/health
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "network": "relyo" }))
}

/// GET /api/token
pub async fn token_info() -> impl IntoResponse {
    Json(TokenInfoResponse::from_config())
}

/// GET /api/stats
pub async fn network_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let dag = &state.dag;
    let all_txs = dag.all_transactions();
    let confirmed = all_txs
        .iter()
        .filter(|n| n.status == relyo_core::TransactionStatus::Confirmed)
        .count() as u64;
    let pending = all_txs
        .iter()
        .filter(|n| {
            n.status == relyo_core::TransactionStatus::Pending
                || n.status == relyo_core::TransactionStatus::Voting
        })
        .count() as u64;
    let stats = NetworkStatsResponse {
        total_transactions: dag.len(),
        confirmed_transactions: confirmed,
        pending_transactions: pending,
        active_nodes: 1,
        tps: 0.0,
        avg_confirmation_ms: 0.0,
        dag_tips: dag.tips().len(),
        total_supply: base_to_rly(relyo_core::token::RELYO_CONFIG.total_supply),
        circulating_supply: base_to_rly(dag.state().total_circulating()),
        ticker: relyo_core::token::RELYO_CONFIG.ticker,
        name: relyo_core::token::RELYO_CONFIG.name,
    };
    Json(stats)
}

/// GET /api/transaction/:hash
pub async fn get_transaction(
    State(state): State<Arc<AppState>>,
    Path(hash_hex): Path<String>,
) -> Result<Json<TransactionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let hash = TransactionHash::from_hex(&hash_hex).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let node = state.dag.get(&hash).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "transaction not found".into(),
            }),
        )
    })?;

    let tx = &node.tx;
    Ok(Json(TransactionResponse {
        hash: hash.to_hex(),
        sender: tx.sender.to_string(),
        receiver: tx.receiver.to_string(),
        amount: base_to_rly(tx.amount),
        amount_base: tx.amount,
        fee: base_to_rly(tx.fee),
        fee_base: tx.fee,
        timestamp: tx.timestamp,
        nonce: tx.nonce,
        parent_1: tx.parent_1.to_hex(),
        parent_2: tx.parent_2.to_hex(),
        status: format!("{:?}", node.status),
    }))
}

/// GET /api/address/:address
pub async fn get_address(
    State(state): State<Arc<AppState>>,
    Path(addr_str): Path<String>,
) -> Result<Json<AddressResponse>, (StatusCode, Json<ErrorResponse>)> {
    Address::validate(&addr_str).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let addr: Address = addr_str.parse().map_err(|e: relyo_core::RelyoError| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let balance = state.dag.balance(&addr);
    let nonce = state.dag.nonce(&addr);

    // Count actual transactions involving this address (sent + received)
    let all_txs = state.dag.all_transactions();
    let tx_count = all_txs
        .iter()
        .filter(|n| n.tx.sender == addr || n.tx.receiver == addr)
        .count() as u64;

    Ok(Json(AddressResponse {
        address: addr.to_string(),
        balance: base_to_rly(balance),
        balance_base: balance,
        nonce,
        transaction_count: tx_count,
    }))
}

/// GET /api/dag
pub async fn dag_visualization(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let all = state.dag.all_transactions();

    let nodes: Vec<DagVizNode> = all
        .iter()
        .map(|n| DagVizNode {
            id: n.hash.to_hex(),
            status: format!("{:?}", n.status),
            timestamp: n.tx.timestamp,
            weight: n.weight,
        })
        .collect();

    let mut edges = Vec::new();
    for n in &all {
        if !n.tx.parent_1.is_zero() {
            edges.push(DagVizEdge {
                from: n.hash.to_hex(),
                to: n.tx.parent_1.to_hex(),
            });
        }
        if !n.tx.parent_2.is_zero() {
            edges.push(DagVizEdge {
                from: n.hash.to_hex(),
                to: n.tx.parent_2.to_hex(),
            });
        }
    }

    Json(DagVisualization { nodes, edges })
}

/// GET /api/tips
pub async fn get_tips(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tips: Vec<String> = state
        .dag
        .tips()
        .all()
        .iter()
        .map(|h| h.to_hex())
        .collect();

    Json(serde_json::json!({ "tips": tips, "count": tips.len() }))
}
