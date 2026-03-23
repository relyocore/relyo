use axum::{routing::get, Router};
use relyo_dag::DagGraph;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::handlers;

/// Shared application state for the explorer API.
pub struct AppState {
    pub dag: Arc<DagGraph>,
}

/// Configuration for the explorer HTTP server.
#[derive(Debug, Clone)]
pub struct ExplorerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for ExplorerConfig {
    fn default() -> Self {
        ExplorerConfig {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
        }
    }
}

/// The explorer HTTP server.
pub struct ExplorerServer {
    config: ExplorerConfig,
    dag: Arc<DagGraph>,
}

impl ExplorerServer {
    pub fn new(config: ExplorerConfig, dag: Arc<DagGraph>) -> Self {
        ExplorerServer { config, dag }
    }

    /// Build the Axum router.
    pub fn router(&self) -> Router {
        let state = Arc::new(AppState {
            dag: self.dag.clone(),
        });

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        Router::new()
            .route("/api/health", get(handlers::health))
            .route("/api/token", get(handlers::token_info))
            .route("/api/stats", get(handlers::network_stats))
            .route("/api/transaction/{hash}", get(handlers::get_transaction))
            .route("/api/address/{address}", get(handlers::get_address))
            .route("/api/dag", get(handlers::dag_visualization))
            .route("/api/tips", get(handlers::get_tips))
            .layer(cors)
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    /// Start the server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.bind_addr;
        let router = self.router();

        info!("Explorer API starting on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use relyo_core::{crypto::KeyPair, crypto::Signature, Address, Transaction, TransactionType};
    use relyo_dag::{ConflictDetector, LedgerState, TipSelector};
    use tower::ServiceExt;

    fn setup_dag() -> Arc<DagGraph> {
        let state = Arc::new(LedgerState::new());
        let tips = Arc::new(TipSelector::new());
        let conflicts = Arc::new(ConflictDetector::new());
        let dag = Arc::new(DagGraph::new(state, tips, conflicts));

        let kp = KeyPair::generate();
        let recv = Address::from_public_key(&kp.public_key);
        let gen_addr = Address::genesis();

        let mut tx = Transaction {
            tx_type: TransactionType::Genesis,
            sender: gen_addr,
            receiver: recv,
            amount: 1_000_000_000,
            fee: 0,
            timestamp: relyo_core::now_ms(),
            nonce: 0,
            parent_1: relyo_core::TransactionHash::zero(),
            parent_2: relyo_core::TransactionHash::zero(),
            sender_pubkey: kp.public_key.clone(),
            signature: Signature::from_bytes([0; 64]),
            data: Vec::new(),
        };
        let msg = tx.signable_bytes();
        tx.signature = kp.sign(&msg);
        dag.insert_genesis(tx).unwrap();

        dag
    }

    fn build_app(dag: Arc<DagGraph>) -> Router {
        let config = ExplorerConfig::default();
        let server = ExplorerServer::new(config, dag);
        server.router()
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let dag = setup_dag();
        let app = build_app(dag);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["network"], "relyo");
    }

    #[tokio::test]
    async fn test_token_endpoint() {
        let dag = setup_dag();
        let app = build_app(dag);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ticker"], "RLY");
        assert_eq!(json["name"], "Relyo");
        assert_eq!(json["decimals"], 8);
    }

    #[tokio::test]
    async fn test_stats_endpoint() {
        let dag = setup_dag();
        let app = build_app(dag);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_transactions"], 1);
        assert_eq!(json["confirmed_transactions"], 1);
    }

    #[tokio::test]
    async fn test_tips_endpoint() {
        let dag = setup_dag();
        let app = build_app(dag);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/tips")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
        assert!(json["tips"].as_array().unwrap().len() == 1);
    }

    #[tokio::test]
    async fn test_transaction_not_found() {
        let dag = setup_dag();
        let app = build_app(dag);

        let fake_hash = "00".repeat(32);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/transaction/{}", fake_hash))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_dag_visualization() {
        let dag = setup_dag();
        let app = build_app(dag);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["nodes"].as_array().unwrap().is_empty());
    }
}
