use axum::{
    extract::State,
    routing::get,
    Json, Router,
    http::StatusCode,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use crate::relayer::Relayer;

pub struct ApiState {
    pub relayer: Arc<Relayer>,
}

impl Clone for ApiState {
    fn clone(&self) -> Self {
        Self { relayer: self.relayer.clone() }
    }
}

pub fn create_router(state: ApiState) -> Router {
    Router::new()
        .route("/bridge/status",       get(bridge_status))
        .route("/bridge/events/evm",   get(evm_events))
        .route("/bridge/events/rf",    get(rf_events))
        .route("/bridge/health",       get(bridge_health))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

async fn bridge_status(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(state.relayer.summary())
}

async fn evm_events(State(state): State<ApiState>) -> Json<serde_json::Value> {
    // Access bridge state through the relayer
    Json(serde_json::json!({
        "message": "Use /bridge/status for full summary",
        "note": "Individual event lists available via bridge_state access"
    }))
}

async fn rf_events(State(state): State<ApiState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "message": "Use /bridge/status for full summary"
    }))
}

async fn bridge_health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "service": "redflag-bridge"
    })))
}

pub async fn run_api(relayer: Arc<Relayer>, port: u16) {
    let state = ApiState { relayer };
    let app = create_router(state);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("Bridge API bind failed");
    println!("🌉 Bridge API en http://{}", addr);
    axum::serve(listener, app).await.ok();
}
