use axum::{
    extract::{Path, State, WebSocketUpgrade, ConnectInfo},
    extract::ws::{WebSocket, Message},
    response::IntoResponse,
    routing::{get, post, get_service},
    Json, Router,
    http::{StatusCode, HeaderMap, HeaderValue},
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use dashmap::DashMap;
use redflag_consensus::{ConsensusEngine, ConsensusSummary};
use redflag_core::{Transaction, EncryptedTransaction, CHAIN_ID, MIN_FEE};
use redflag_crypto::{SigningKeyPair, Verifier};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

// ── Estado compartido ────────────────────────────────────────────────────────

/// Cooldown del faucet: address → timestamp último uso (Unix secs)
pub type FaucetCooldowns = Arc<DashMap<String, u64>>;
/// Rate limit global: IP → (count en ventana, inicio ventana)
pub type IpRateLimits = Arc<DashMap<String, (u32, u64)>>;

const FAUCET_COOLDOWN_SECS: u64 = 86_400; // 24 horas
const API_RATE_WINDOW_SECS: u64 = 60;      // ventana de 1 minuto
const API_RATE_MAX_REQ: u32 = 60;          // máximo 60 req/min por IP

#[derive(Clone)]
pub struct ApiState {
    pub consensus: Arc<ConsensusEngine>,
    pub peer_id: String,
    pub faucet_key: Arc<SigningKeyPair>,
    pub faucet_address: String,
    pub ws_tx: Arc<broadcast::Sender<String>>,
    pub node_start_time: u64,
    /// Cooldown faucet por dirección
    pub faucet_cooldowns: FaucetCooldowns,
    /// Rate limit por IP
    pub ip_rate_limits: IpRateLimits,
}

// ── Tipos de respuesta ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct BalanceResponse {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub peer_id: String,
    pub chain_id: u64,
    pub current_round: u64,
    pub pending_txs: usize,
    pub committed_vertices: usize,
    pub total_vertices: usize,
    pub validator_count: usize,
    pub fee_pool_balance: u64,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct VertexSummary {
    pub id: String,
    pub round: u64,
    pub author: String,
    pub tx_count: usize,
    pub committed: bool,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub address: String,
    pub count: usize,
    pub history: Vec<Transaction>,
}

#[derive(Serialize)]
pub struct NetworkInfoResponse {
    pub chain_id: u64,
    pub chain_name: &'static str,
    pub consensus: &'static str,
    pub crypto: &'static str,
    pub p2p: &'static str,
    pub min_fee: u64,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct TxResponse {
    pub accepted: bool,
    pub message: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize)]
pub struct MempoolResponse {
    pub count: usize,
    pub txs: Vec<Transaction>,
}

// ── Tipos wallet ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WalletNewResponse {
    address: String,
    private_key_hex: String,
    warning: &'static str,
}

#[derive(Deserialize)]
struct WalletSendRequest {
    private_key_hex: String,
    receiver: String,
    amount: u64,
    fee: Option<u64>,
}

#[derive(Deserialize)]
struct FaucetRequest {
    address: String,
    amount: Option<u64>,
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn create_router(state: ApiState) -> Router {
    let api = Router::new()
        // Estado y red
        .route("/status",           get(get_status))
        .route("/network-info",     get(get_network_info))
        .route("/network/stats",    get(get_network_stats))
        // Cuentas
        .route("/balance/:addr",    get(get_balance))
        .route("/account/:addr",    get(get_account))
        .route("/history/:addr",    get(get_history))
        // TXs plain
        .route("/tx",               post(submit_transaction))
        .route("/mempool",          get(get_mempool))
        // Threshold Encrypted
        .route("/round-ek",         get(get_round_ek))
        .route("/round-dk/:round",  get(get_round_dk))
        .route("/tx/encrypted",     post(submit_encrypted_transaction))
        // DAG
        .route("/dag/vertices",     get(get_dag_vertices))
        .route("/dag/vertex/:id",   get(get_vertex_detail))
        .route("/dag/summary",      get(get_consensus_summary))
        // Wallet
        .route("/wallet/new",       post(wallet_new))
        .route("/wallet/send",      post(wallet_send))
        .route("/wallet/faucet",    post(wallet_faucet))
        // Explorer
        .route("/explorer/search/:query", get(explorer_search))
        .route("/explorer/tx/:hash",      get(explorer_tx))
        // Token balances (wrapped tokens)
        .route("/tokens/:addr",           get(get_token_balances))
        .route("/tokens/:addr/:token",    get(get_token_balance))
        // Bridge mint (llamado por el relayer)
        .route("/bridge/mint",            post(bridge_mint))
        // DEX — trading en tiempo real
        .route("/dex/pools",              get(dex_pools))
        .route("/dex/pool/:id",           get(dex_pool))
        .route("/dex/pool/:id/history",   get(dex_pool_history))
        .route("/dex/pool/:id/prices",    get(dex_price_history))
        .route("/dex/swap",               post(dex_swap))
        .route("/dex/liquidity/add",      post(dex_add_liquidity))
        .route("/dex/liquidity/remove",   post(dex_remove_liquidity))
        .route("/dex/position/:addr/:pool", get(dex_position))
        .route("/dex/quote",              post(dex_quote))
        // Bridge cross-chain info
        .route("/bridge/info",            get(bridge_info))
        .route("/bridge/chains",          get(bridge_chains))
        // Market data (CoinGecko / CMC compatible)
        .route("/api/v1/summary",         get(market_summary))
        .route("/api/v1/ticker",          get(market_ticker))
        .route("/api/v1/orderbook",       get(market_orderbook))
        .route("/api/v1/trades",          get(market_trades))
        .route("/api/v1/assets",          get(market_assets))
        // Validadores
        .route("/validators",             get(get_validators))
        .route("/validators/apply",       post(validator_apply))
        .route("/staking/info",           get(staking_info))
        .route("/staking/stakes",         get(get_stakes))
        // WebSocket tiempo real
        .route("/ws",               get(ws_handler))
        // Métricas Prometheus
        .route("/metrics",          get(get_metrics))
        .with_state(state);

    Router::new()
        .merge(api)
        .fallback_service(
            get_service(ServeDir::new("./redflag-web/dist"))
                .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR })
        )
        .layer(CorsLayer::permissive())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn sign_and_submit(
    kp: &SigningKeyPair,
    sender_addr: &str,
    receiver: String,
    amount: u64,
    fee: u64,
    consensus: &ConsensusEngine,
) -> TxResponse {
    let nonce = consensus.state.get_account(sender_addr).map(|a| a.nonce).unwrap_or(0);

    let mut tx = Transaction {
        sender: sender_addr.to_string(),
        receiver: receiver.clone(),
        amount,
        fee,
        nonce,
        chain_id: CHAIN_ID,
        read_set: vec![sender_addr.to_string(), receiver.clone()],
        write_set: vec![sender_addr.to_string(), receiver],
        data: vec![],
        signature: vec![],
        timestamp: now_secs(),
    };

    let msg = match postcard::to_allocvec(&tx) {
        Ok(b) => b,
        Err(e) => return TxResponse { accepted: false, message: format!("Serialize: {}", e), tx_hash: None },
    };
    let sig = match kp.sign(&msg) {
        Ok(s) => s,
        Err(e) => return TxResponse { accepted: false, message: format!("Sign: {:?}", e), tx_hash: None },
    };
    tx.signature = sig;

    let tx_hash = hex::encode(blake3::hash(&msg).as_bytes());
    consensus.mempool.add_transaction(tx);
    TxResponse { accepted: true, message: "TX enviada al mempool".to_string(), tx_hash: Some(tx_hash) }
}

// Emite evento WS (ignora si no hay listeners)
fn emit(ws_tx: &broadcast::Sender<String>, event_type: &str, data: serde_json::Value) {
    let _ = ws_tx.send(serde_json::json!({ "type": event_type, "data": data }).to_string());
}

/// Comprueba y actualiza el rate limit de una IP. Devuelve true si hay que bloquear.
fn ip_rate_limit(limits: &IpRateLimits, ip: &str) -> bool {
    let now = now_secs();
    let mut entry = limits.entry(ip.to_string()).or_insert((0u32, now));
    let (count, window_start) = entry.value_mut();
    if now - *window_start > API_RATE_WINDOW_SECS {
        *count = 1;
        *window_start = now;
        false
    } else {
        *count += 1;
        *count > API_RATE_MAX_REQ
    }
}

// ── WebSocket ────────────────────────────────────────────────────────────────

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<ApiState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: ApiState) {
    // Enviar estado inicial
    let s = state.consensus.summary();
    let init = serde_json::json!({
        "type": "init",
        "data": {
            "round": s.current_round,
            "validators": s.validator_count,
            "pending": s.pending_txs,
            "tx_count": s.tx_count,
            "vertices": s.total_vertices,
            "committed": s.committed_vertices,
            "fee_pool": s.fee_pool_balance,
            "uptime": now_secs() - state.node_start_time,
        }
    }).to_string();
    if socket.send(Message::Text(init.into())).await.is_err() { return; }

    let mut rx = state.ws_tx.subscribe();
    let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(data) => {
                        if socket.send(Message::Text(data.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            _ = heartbeat.tick() => {
                let s = state.consensus.summary();
                let hb = serde_json::json!({
                    "type": "heartbeat",
                    "data": {
                        "round": s.current_round,
                        "pending": s.pending_txs,
                        "tx_count": s.tx_count,
                        "vertices": s.total_vertices,
                        "fee_pool": s.fee_pool_balance,
                        "ts": now_secs(),
                    }
                }).to_string();
                if socket.send(Message::Text(hb.into())).await.is_err() { break; }
            }
        }
    }
}

// ── Métricas Prometheus ──────────────────────────────────────────────────────

async fn get_metrics(State(state): State<ApiState>) -> (HeaderMap, String) {
    let s = state.consensus.summary();
    let uptime = now_secs().saturating_sub(state.node_start_time);
    let faucet_bal = state.consensus.state.get_balance(&state.faucet_address);

    let body = format!(
        "# HELP redflag_round Current consensus round\n\
         # TYPE redflag_round gauge\nredflag_round {}\n\n\
         # HELP redflag_pending_txs Pending transactions in mempool\n\
         # TYPE redflag_pending_txs gauge\nredflag_pending_txs {}\n\n\
         # HELP redflag_committed_vertices Total committed vertices\n\
         # TYPE redflag_committed_vertices counter\nredflag_committed_vertices {}\n\n\
         # HELP redflag_total_vertices Total DAG vertices\n\
         # TYPE redflag_total_vertices counter\nredflag_total_vertices {}\n\n\
         # HELP redflag_tx_count Total committed transactions\n\
         # TYPE redflag_tx_count counter\nredflag_tx_count {}\n\n\
         # HELP redflag_account_count Total accounts with balance\n\
         # TYPE redflag_account_count gauge\nredflag_account_count {}\n\n\
         # HELP redflag_validator_count Active validators\n\
         # TYPE redflag_validator_count gauge\nredflag_validator_count {}\n\n\
         # HELP redflag_fee_pool_balance Protocol fee pool balance (RF)\n\
         # TYPE redflag_fee_pool_balance gauge\nredflag_fee_pool_balance {}\n\n\
         # HELP redflag_faucet_balance Faucet remaining balance (RF)\n\
         # TYPE redflag_faucet_balance gauge\nredflag_faucet_balance {}\n\n\
         # HELP redflag_uptime_seconds Node uptime in seconds\n\
         # TYPE redflag_uptime_seconds counter\nredflag_uptime_seconds {}\n\n\
         # HELP redflag_threshold_round Current ML-KEM threshold round\n\
         # TYPE redflag_threshold_round gauge\nredflag_threshold_round {}\n",
        s.current_round, s.pending_txs, s.committed_vertices, s.total_vertices,
        s.tx_count, s.account_count, s.validator_count, s.fee_pool_balance,
        faucet_bal, uptime, state.consensus.threshold_mempool.get_current_ek().0,
    );

    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"));
    (headers, body)
}

// ── Network Stats ────────────────────────────────────────────────────────────

async fn get_network_stats(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let s = state.consensus.summary();
    let faucet_bal = state.consensus.state.get_balance(&state.faucet_address);
    let fee_pool = state.consensus.state.get_balance("RedFlag_Protocol_FeePool");
    let genesis_bal = state.consensus.state.get_balance("RedFlag_Genesis_Alpha");
    let total_issued: u64 = 1_500_000_000; // genesis + faucet
    let circulating = total_issued.saturating_sub(faucet_bal).saturating_sub(genesis_bal);
    let uptime = now_secs().saturating_sub(state.node_start_time);
    let (threshold_round, ek) = state.consensus.threshold_mempool.get_current_ek();

    Json(serde_json::json!({
        "supply": {
            "total": total_issued,
            "circulating": circulating,
            "fee_pool": fee_pool,
            "faucet": faucet_bal,
            "genesis": genesis_bal,
        },
        "consensus": {
            "round": s.current_round,
            "committed_vertices": s.committed_vertices,
            "total_vertices": s.total_vertices,
            "pending_txs": s.pending_txs,
            "tx_count": s.tx_count,
            "account_count": s.account_count,
            "validator_count": s.validator_count,
        },
        "threshold": {
            "round": threshold_round,
            "ek_prefix": hex::encode(&ek[..8.min(ek.len())]),
            "algorithm": "ML-KEM-768",
        },
        "node": {
            "peer_id": state.peer_id,
            "uptime_secs": uptime,
            "chain_id": CHAIN_ID,
            "min_fee": MIN_FEE,
            "version": env!("CARGO_PKG_VERSION"),
        }
    }))
}

// ── Explorer Search ──────────────────────────────────────────────────────────

async fn explorer_search(
    Path(query): Path<String>,
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    let q = query.trim().to_string();

    // Buscar como dirección de cuenta
    if let Some(acc) = state.consensus.state.get_account(&q) {
        let history = state.consensus.state.get_history(&q);
        let sent: u64 = history.iter().filter(|t| t.sender == q).map(|t| t.amount + t.fee).sum();
        let received: u64 = history.iter().filter(|t| t.receiver == q).map(|t| t.amount).sum();
        return Json(serde_json::json!({
            "type": "address",
            "address": acc.address,
            "balance": acc.balance,
            "nonce": acc.nonce,
            "tx_count": history.len(),
            "total_sent": sent,
            "total_received": received,
            "history": &history[..20.min(history.len())],
        }));
    }

    // Buscar como ID de vértice DAG (hex prefix)
    if q.len() >= 8 {
        let vertices = state.consensus.dag.recent_vertices(200);
        for v in &vertices {
            let id_hex = hex::encode(v.id());
            if id_hex.starts_with(&q) || id_hex == q {
                return Json(serde_json::json!({
                    "type": "vertex",
                    "id": id_hex,
                    "round": v.round,
                    "tx_count": v.transactions.len(),
                    "etx_count": v.encrypted_transactions.len(),
                    "author": hex::encode(&v.author[..8.min(v.author.len())]),
                    "committed": state.consensus.committed_vertices.contains(&v.id()),
                }));
            }
        }
    }

    Json(serde_json::json!({ "type": "not_found", "query": q }))
}

async fn explorer_tx(
    Path(hash): Path<String>,
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    // Búsqueda O(1) directa por hash en tx_index
    if let Some(tx) = state.consensus.state.get_tx_by_hash(&hash) {
        let tx_bytes = postcard::to_allocvec(&tx).unwrap_or_default();
        let tx_hash = hex::encode(blake3::hash(&tx_bytes).as_bytes());
        return Json(serde_json::json!({
            "type": "transaction",
            "hash": tx_hash,
            "sender": tx.sender,
            "receiver": tx.receiver,
            "amount": tx.amount,
            "fee": tx.fee,
            "nonce": tx.nonce,
            "chain_id": tx.chain_id,
            "timestamp": tx.timestamp,
            "status": "confirmed",
        }));
    }
    // Fallback: buscar por prefijo en historial reciente
    for tx in state.consensus.state.get_recent_txs(500) {
        let tx_bytes = postcard::to_allocvec(&tx).unwrap_or_default();
        let tx_hash = hex::encode(blake3::hash(&tx_bytes).as_bytes());
        if tx_hash.starts_with(&hash) {
            return Json(serde_json::json!({
                "type": "transaction",
                "hash": tx_hash,
                "sender": tx.sender,
                "receiver": tx.receiver,
                "amount": tx.amount,
                "fee": tx.fee,
                "nonce": tx.nonce,
                "chain_id": tx.chain_id,
                "timestamp": tx.timestamp,
                "status": "confirmed",
            }));
        }
    }
    Json(serde_json::json!({ "type": "not_found", "hash": hash }))
}

// ── Handlers estándar ────────────────────────────────────────────────────────

async fn get_status(State(state): State<ApiState>) -> Json<StatusResponse> {
    let s = state.consensus.summary();
    Json(StatusResponse {
        peer_id: state.peer_id.clone(),
        chain_id: CHAIN_ID,
        current_round: s.current_round,
        pending_txs: s.pending_txs,
        committed_vertices: s.committed_vertices,
        total_vertices: s.total_vertices,
        validator_count: s.validator_count,
        fee_pool_balance: s.fee_pool_balance,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn get_network_info() -> Json<NetworkInfoResponse> {
    Json(NetworkInfoResponse {
        chain_id: CHAIN_ID,
        chain_name: "redflag.web3",
        consensus: "Bullshark DAG (Narwhal+Bullshark)",
        crypto: "ML-DSA-65 + ML-KEM-768 (FIPS 204/203)",
        p2p: "libp2p (TCP+QUIC, Gossipsub, Kademlia, mDNS)",
        min_fee: MIN_FEE,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn get_balance(Path(addr): Path<String>, State(state): State<ApiState>) -> Json<BalanceResponse> {
    let acc = state.consensus.state.get_account(&addr);
    Json(BalanceResponse {
        address: addr.clone(),
        balance: acc.as_ref().map(|a| a.balance).unwrap_or(0),
        nonce: acc.map(|a| a.nonce).unwrap_or(0),
    })
}

async fn get_account(Path(addr): Path<String>, State(state): State<ApiState>) -> Json<serde_json::Value> {
    match state.consensus.state.get_account(&addr) {
        Some(acc) => Json(serde_json::json!({ "address": acc.address, "balance": acc.balance, "nonce": acc.nonce, "exists": true })),
        None => Json(serde_json::json!({ "address": addr, "balance": 0, "nonce": 0, "exists": false })),
    }
}

async fn get_history(Path(addr): Path<String>, State(state): State<ApiState>) -> Json<HistoryResponse> {
    let history = state.consensus.state.get_history(&addr);
    let count = history.len();
    Json(HistoryResponse { address: addr, count, history })
}

async fn submit_transaction(
    State(state): State<ApiState>,
    Json(mut tx): Json<Transaction>,
) -> (StatusCode, Json<TxResponse>) {
    if tx.chain_id != CHAIN_ID || tx.fee < MIN_FEE {
        return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: "Chain ID o Fee inválido".into(), tx_hash: None }));
    }
    let pubkey_bytes = hex::decode(&tx.sender).unwrap_or_default();
    let signature = std::mem::take(&mut tx.signature);
    let msg = postcard::to_allocvec(&tx).unwrap_or_default();
    tx.signature = signature.clone();
    if Verifier::verify(&pubkey_bytes, &msg, &signature).is_err() {
        return (StatusCode::UNAUTHORIZED, Json(TxResponse { accepted: false, message: "Firma inválida".into(), tx_hash: None }));
    }
    let tx_hash = hex::encode(blake3::hash(&msg).as_bytes());
    emit(&state.ws_tx, "new_tx", serde_json::json!({
        "sender": &tx.sender[..16.min(tx.sender.len())],
        "receiver": &tx.receiver[..16.min(tx.receiver.len())],
        "amount": tx.amount, "fee": tx.fee, "hash": &tx_hash[..16],
    }));
    state.consensus.mempool.add_transaction(tx);
    (StatusCode::ACCEPTED, Json(TxResponse { accepted: true, message: "TX aceptada".into(), tx_hash: Some(tx_hash) }))
}

async fn get_mempool(State(state): State<ApiState>) -> Json<MempoolResponse> {
    let txs: Vec<Transaction> = state.consensus.mempool.pending_transactions.iter().map(|kv| kv.value().clone()).collect();
    let count = txs.len();
    Json(MempoolResponse { count, txs })
}

async fn get_dag_vertices(State(state): State<ApiState>) -> Json<Vec<VertexSummary>> {
    let vertices = state.consensus.dag.recent_vertices(50);
    let summaries = vertices.iter().map(|v| {
        let id = v.id();
        VertexSummary {
            id: hex::encode(id),
            round: v.round,
            author: hex::encode(&v.author[..8.min(v.author.len())]),
            tx_count: v.transactions.len() + v.encrypted_transactions.len(),
            committed: state.consensus.committed_vertices.contains(&id),
        }
    }).collect();
    Json(summaries)
}

async fn get_vertex_detail(
    Path(id_hex): Path<String>,
    State(state): State<ApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let id_bytes = hex::decode(&id_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut id = [0u8; 32];
    if id_bytes.len() != 32 { return Err(StatusCode::BAD_REQUEST); }
    id.copy_from_slice(&id_bytes);
    match state.consensus.dag.get_vertex(&id) {
        Some(v) => Ok(Json(serde_json::json!({
            "id": id_hex, "round": v.round,
            "tx_count": v.transactions.len(), "etx_count": v.encrypted_transactions.len(),
            "author": hex::encode(&v.author[..8.min(v.author.len())]),
            "committed": state.consensus.committed_vertices.contains(&id),
            "transactions": &v.transactions[..5.min(v.transactions.len())],
        }))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_consensus_summary(State(state): State<ApiState>) -> Json<ConsensusSummary> {
    Json(state.consensus.summary())
}

async fn get_round_ek(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let (round, ek) = state.consensus.threshold_mempool.get_current_ek();
    Json(serde_json::json!({ "round": round, "ek_hex": hex::encode(ek) }))
}

async fn get_round_dk(
    Path(round): Path<u64>,
    State(state): State<ApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.consensus.threshold_mempool.revealed_key_for_round(round) {
        Some(rk) => (StatusCode::OK, Json(serde_json::json!({
            "round": rk.round, "ek_hex": hex::encode(rk.ek_bytes), "dk_hex": hex::encode(rk.dk_bytes),
        }))),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Not revealed" }))),
    }
}

async fn submit_encrypted_transaction(
    State(state): State<ApiState>,
    Json(etx): Json<EncryptedTransaction>,
) -> (StatusCode, Json<TxResponse>) {
    if etx.chain_id != CHAIN_ID || etx.fee < MIN_FEE {
        return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: "Invalid chain/fee".into(), tx_hash: None }));
    }
    state.consensus.mempool.add_encrypted_transaction(etx);
    (StatusCode::ACCEPTED, Json(TxResponse { accepted: true, message: "Encrypted TX accepted".into(), tx_hash: None }))
}

// ── Wallet handlers ──────────────────────────────────────────────────────────

async fn wallet_new() -> Json<WalletNewResponse> {
    match SigningKeyPair::generate() {
        Ok(kp) => {
            let address = hex::encode(kp.public_key());
            let pkcs8_hex = postcard::to_allocvec(&kp).map(hex::encode).unwrap_or_default();
            Json(WalletNewResponse {
                address, private_key_hex: pkcs8_hex,
                warning: "TESTNET ONLY — Guarda tu clave privada de forma segura.",
            })
        }
        Err(_) => Json(WalletNewResponse { address: String::new(), private_key_hex: String::new(), warning: "Error generando llaves" }),
    }
}

async fn wallet_send(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
    Json(req): Json<WalletSendRequest>,
) -> (StatusCode, Json<TxResponse>) {
    if ip_rate_limit(&state.ip_rate_limits, &addr.ip().to_string()) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(TxResponse {
            accepted: false, message: "Rate limit excedido".into(), tx_hash: None,
        }));
    }
    let pkcs8_bytes = match hex::decode(&req.private_key_hex) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: "private_key_hex inválido".into(), tx_hash: None })),
    };
    let kp: SigningKeyPair = match postcard::from_bytes::<SigningKeyPair>(&pkcs8_bytes) {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: "Clave privada inválida".into(), tx_hash: None })),
    };
    if req.amount == 0 {
        return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: "amount > 0 requerido".into(), tx_hash: None }));
    }
    let sender = hex::encode(kp.public_key());
    let fee = req.fee.unwrap_or(MIN_FEE).max(MIN_FEE);
    let resp = sign_and_submit(&kp, &sender, req.receiver, req.amount, fee, &state.consensus);
    if resp.accepted {
        emit(&state.ws_tx, "new_tx", serde_json::json!({
            "sender": &sender[..16.min(sender.len())],
            "amount": req.amount, "fee": fee, "via": "wallet",
        }));
    }
    let status = if resp.accepted { StatusCode::ACCEPTED } else { StatusCode::BAD_REQUEST };
    (status, Json(resp))
}

async fn wallet_faucet(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
    Json(req): Json<FaucetRequest>,
) -> (StatusCode, Json<TxResponse>) {
    // IP rate limit: máximo 60 req/min
    let ip = addr.ip().to_string();
    if ip_rate_limit(&state.ip_rate_limits, &ip) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(TxResponse {
            accepted: false,
            message: "Rate limit: demasiadas peticiones. Espera 1 minuto.".into(),
            tx_hash: None,
        }));
    }

    // Cooldown de 24h por dirección
    let now = now_secs();
    if let Some(last) = state.faucet_cooldowns.get(&req.address) {
        let elapsed = now.saturating_sub(*last);
        if elapsed < FAUCET_COOLDOWN_SECS {
            let wait_mins = (FAUCET_COOLDOWN_SECS - elapsed) / 60;
            return (StatusCode::TOO_MANY_REQUESTS, Json(TxResponse {
                accepted: false,
                message: format!("Faucet: espera {} horas {} min antes de volver a pedir.", wait_mins / 60, wait_mins % 60),
                tx_hash: None,
            }));
        }
    }

    // FIX #3: Validar dirección RF (debe ser hex de 32+ bytes = 64+ chars)
    if req.address.len() < 64 || hex::decode(&req.address).is_err() {
        return (StatusCode::BAD_REQUEST, Json(TxResponse {
            accepted: false,
            message: "Dirección RF inválida (debe ser clave pública ML-DSA hex)".into(),
            tx_hash: None,
        }));
    }
    // No permitir faucet a direcciones especiales del protocolo
    if req.address.starts_with("RedFlag_") {
        return (StatusCode::BAD_REQUEST, Json(TxResponse {
            accepted: false, message: "Dirección no permitida".into(), tx_hash: None,
        }));
    }

    const MAX_FAUCET: u64 = 1_000; // Reducido de 10,000 a 1,000 RF máximo por solicitud
    let faucet_bal = state.consensus.state.get_balance(&state.faucet_address);
    if faucet_bal < MIN_FEE + 1 {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(TxResponse { accepted: false, message: "Faucet vacío".into(), tx_hash: None }));
    }
    let amount = req.amount.unwrap_or(100).min(MAX_FAUCET); // default 100, max 1000
    if faucet_bal < amount + MIN_FEE {
        return (StatusCode::BAD_REQUEST, Json(TxResponse { accepted: false, message: format!("Faucet solo tiene {} RF", faucet_bal), tx_hash: None }));
    }
    let resp = sign_and_submit(&state.faucet_key, &state.faucet_address, req.address.clone(), amount, MIN_FEE, &state.consensus);
    if resp.accepted {
        // Registrar cooldown SOLO si la TX fue aceptada
        state.faucet_cooldowns.insert(req.address.clone(), now);
        emit(&state.ws_tx, "faucet", serde_json::json!({ "to": &req.address[..16.min(req.address.len())], "amount": amount }));
    }
    let status = if resp.accepted { StatusCode::ACCEPTED } else { StatusCode::INTERNAL_SERVER_ERROR };
    (status, Json(resp))
}

// ── Token balance handlers ────────────────────────────────────────────────────

async fn get_token_balances(
    Path(addr): Path<String>,
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    let balances = state.consensus.state.tokens.get_all_balances(&addr);
    let rf = state.consensus.state.get_balance(&addr);
    Json(serde_json::json!({
        "address": addr,
        "rf": rf,
        "tokens": balances,
    }))
}

async fn get_token_balance(
    Path((addr, token)): Path<(String, String)>,
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    let bal = state.consensus.state.tokens.get_balance(&addr, &token);
    Json(serde_json::json!({ "address": addr, "token": token, "balance": bal }))
}

#[derive(Deserialize)]
struct BridgeMintRequest {
    /// Clave secreta del bridge (para autenticar el relayer)
    bridge_secret: String,
    to:     String,
    token:  String,
    amount: u64, // en units (6 decimales)
}

async fn bridge_mint(
    State(state): State<ApiState>,
    Json(req): Json<BridgeMintRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // FIX #6: Verificar secreto con timing-safe comparison (evita timing attacks)
    // FIX A: Rechazar si BRIDGE_MINT_SECRET no está configurado (no usar default inseguro)
    let expected = match std::env::var("BRIDGE_MINT_SECRET") {
        Ok(s) if !s.is_empty() && s != "bridge_dev_secret" => s,
        _ => {
            tracing::error!("BRIDGE_MINT_SECRET no configurado o usa valor por defecto inseguro");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Bridge no configurado correctamente" })));
        }
    };
    // Comparación de longitud constante para evitar timing side-channel
    let secret_ok = req.bridge_secret.len() == expected.len()
        && req.bridge_secret.as_bytes().iter()
            .zip(expected.as_bytes().iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    if !secret_ok {
        // Log intento fallido sin revelar el secreto esperado
        tracing::warn!("Bridge mint rechazado: secreto incorrecto (IP desconocida)");
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Unauthorized" })));
    }
    // Validar cantidad máxima por mint (anti-exploit)
    const MAX_MINT_PER_TX: u64 = 1_000_000_000_000; // 1M RF máximo por mint
    if req.amount > MAX_MINT_PER_TX {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Cantidad excede el límite máximo por mint" })));
    }
    // Validar destino no sea dirección del protocolo
    if req.to.starts_with("RedFlag_") {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Destino inválido" })));
    }

    // Validar token soportado (RF = nativo, wETH/wBNB/wMATIC = wrapped)
    let is_native_rf = req.token == "RF";
    if !is_native_rf && !redflag_state::SUPPORTED_TOKENS.contains(&req.token.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Token {} no soportado", req.token) })));
    }

    // RF nativo: acreditar directamente en StateDB
    if is_native_rf {
        if let Some(mut acc) = state.consensus.state.get_account(&req.to) {
            acc.balance = acc.balance.saturating_add(req.amount);
            let _ = state.consensus.state.save_account_pub(&acc);
        } else {
            let _ = state.consensus.state.save_account_pub(&redflag_state::Account {
                address: req.to.clone(), balance: req.amount, nonce: 0,
            });
        }
        emit(&state.ws_tx, "bridge_mint", serde_json::json!({ "to": &req.to[..16.min(req.to.len())], "token": "RF", "amount": req.amount }));
        return (StatusCode::OK, Json(serde_json::json!({ "success": true, "to": req.to, "token": "RF", "amount": req.amount })));
    }

    match state.consensus.state.tokens.credit(&req.to, &req.token, req.amount) {
        Ok(new_bal) => {
            emit(&state.ws_tx, "bridge_mint", serde_json::json!({
                "to": &req.to[..16.min(req.to.len())],
                "token": req.token, "amount": req.amount,
            }));
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "to": req.to, "token": req.token,
                "amount": req.amount, "new_balance": new_bal,
            })))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

// ── DEX handlers ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SwapRequest {
    private_key_hex: String,
    pool_id:         String,
    direction:       String,  // "rf_to_b" | "b_to_rf"
    amount_in:       u64,
    min_amount_out:  Option<u64>,
}

#[derive(Deserialize)]
struct AddLiquidityRequest {
    private_key_hex: String,
    pool_id:         String,
    amount_rf:       u64,
    amount_b:        u64,
}

#[derive(Deserialize)]
struct RemoveLiquidityRequest {
    private_key_hex: String,
    pool_id:         String,
    lp_tokens:       u64,
}

#[derive(Deserialize)]
struct QuoteRequest {
    pool_id:    String,
    direction:  String,
    amount_in:  u64,
}

async fn dex_pools(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let pools = state.consensus.state.dex.list_pools();
    let list: Vec<serde_json::Value> = pools.iter().map(|p| serde_json::json!({
        "pool_id":        p.pool_id,
        "token_b":        p.token_b,
        "reserve_rf":     p.reserve_rf,
        "reserve_b":      p.reserve_b,
        "total_lp":       p.total_lp,
        "price":          p.price(),
        "volume_rf":      p.volume_rf,
        "fees_collected": p.fees_collected,
        "updated_at":     p.updated_at,
    })).collect();
    Json(serde_json::json!({ "pools": list, "count": list.len() }))
}

async fn dex_pool(Path(id): Path<String>, State(state): State<ApiState>) -> Json<serde_json::Value> {
    match state.consensus.state.dex.get_pool(&id) {
        Some(p) => Json(serde_json::json!({
            "pool_id":        p.pool_id,
            "token_b":        p.token_b,
            "reserve_rf":     p.reserve_rf,
            "reserve_b":      p.reserve_b,
            "total_lp":       p.total_lp,
            "price":          p.price(),
            "volume_rf":      p.volume_rf,
            "fees_collected": p.fees_collected,
            "created_at":     p.created_at,
            "updated_at":     p.updated_at,
        })),
        None => Json(serde_json::json!({ "error": "Pool no encontrado" })),
    }
}

async fn dex_pool_history(Path(id): Path<String>, State(state): State<ApiState>) -> Json<serde_json::Value> {
    let history = state.consensus.state.dex.get_swap_history(&id, 100);
    Json(serde_json::json!({ "pool_id": id, "swaps": history, "count": history.len() }))
}

async fn dex_price_history(Path(id): Path<String>, State(state): State<ApiState>) -> Json<serde_json::Value> {
    let prices: Vec<serde_json::Value> = state.consensus.state.dex.get_price_history(&id, 200)
        .into_iter()
        .map(|(ts, price)| serde_json::json!({ "ts": ts, "price": price as f64 / 1_000_000.0 }))
        .collect();
    Json(serde_json::json!({ "pool_id": id, "prices": prices }))
}

async fn dex_quote(
    State(_state): State<ApiState>,
    Json(req): Json<QuoteRequest>,
) -> Json<serde_json::Value> {
    let pool = match _state.consensus.state.dex.get_pool(&req.pool_id) {
        Some(p) => p,
        None => return Json(serde_json::json!({ "error": "Pool no encontrado" })),
    };

    if req.direction == "rf_to_b" {
        let (out, fee) = pool.calc_swap_rf_to_b(req.amount_in);
        Json(serde_json::json!({
            "direction": "rf_to_b",
            "amount_in":  req.amount_in,
            "amount_out": out,
            "fee":        fee,
            "price_impact": if pool.reserve_rf > 0 { req.amount_in as f64 / pool.reserve_rf as f64 * 100.0 } else { 0.0 },
        }))
    } else {
        let (out, fee) = pool.calc_swap_b_to_rf(req.amount_in);
        Json(serde_json::json!({
            "direction": "b_to_rf",
            "amount_in":  req.amount_in,
            "amount_out": out,
            "fee":        fee,
            "price_impact": if pool.reserve_b > 0 { req.amount_in as f64 / pool.reserve_b as f64 * 100.0 } else { 0.0 },
        }))
    }
}

async fn dex_swap(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
    Json(req): Json<SwapRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if ip_rate_limit(&state.ip_rate_limits, &addr.ip().to_string()) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({ "error": "Rate limit" })));
    }

    let pkcs8_bytes = match hex::decode(&req.private_key_hex) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "private_key_hex inválido" }))),
    };
    let kp: redflag_crypto::SigningKeyPair = match postcard::from_bytes::<redflag_crypto::SigningKeyPair>(&pkcs8_bytes) {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Clave inválida" }))),
    };
    let trader = hex::encode(kp.public_key());
    let now = now_secs();

    let min_out = req.min_amount_out.unwrap_or(0);
    let tx_hash = format!("dex_{}_{}", now, &trader[..8]);

    // Determinar token B del pool (pool_id = "RF_wETH" → token_b = "wETH")
    let token_b = req.pool_id.trim_start_matches("RF_");

    let result = if req.direction == "rf_to_b" {
        // Verificar balance RF
        let bal = state.consensus.state.get_balance(&trader);
        if bal < req.amount_in {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Saldo RF insuficiente: tienes {} RF", bal) })));
        }
        // Debitar RF del trader antes del swap
        if let Some(mut acc) = state.consensus.state.get_account(&trader) {
            acc.balance = acc.balance.saturating_sub(req.amount_in);
            let _ = state.consensus.state.save_account_pub(&acc);
        }
        state.consensus.state.dex.execute_swap_rf_to_b(&req.pool_id, &trader, req.amount_in, min_out, &tx_hash, now)
    } else {
        // Verificar balance token B
        let bal_b = state.consensus.state.tokens.get_balance(&trader, token_b);
        if bal_b < req.amount_in {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Saldo {} insuficiente: tienes {}", token_b, bal_b) })));
        }
        // Debitar token B del trader
        if let Err(e) = state.consensus.state.tokens.debit(&trader, token_b, req.amount_in) {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() })));
        }
        state.consensus.state.dex.execute_swap_b_to_rf(&req.pool_id, &trader, req.amount_in, min_out, &tx_hash, now)
    };

    match result {
        Ok(amount_out) => {
            if req.direction == "rf_to_b" {
                // Acreditar token B al trader
                let _ = state.consensus.state.tokens.credit(&trader, token_b, amount_out);
            } else {
                // Swap B→RF: acreditar RF al trader
                if let Some(mut acc) = state.consensus.state.get_account(&trader) {
                    acc.balance = acc.balance.saturating_add(amount_out);
                    let _ = state.consensus.state.save_account_pub(&acc);
                }
            }
            emit(&state.ws_tx, "dex_swap", serde_json::json!({
                "pool": req.pool_id, "dir": req.direction,
                "in": req.amount_in, "out": amount_out,
                "trader": &trader[..16.min(trader.len())],
            }));
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "amount_in": req.amount_in,
                "amount_out": amount_out,
                "tx_hash": tx_hash,
            })))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn dex_add_liquidity(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
    Json(req): Json<AddLiquidityRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if ip_rate_limit(&state.ip_rate_limits, &addr.ip().to_string()) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({ "error": "Rate limit" })));
    }

    let pkcs8_bytes = match hex::decode(&req.private_key_hex) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Clave inválida" }))),
    };
    let kp: redflag_crypto::SigningKeyPair = match postcard::from_bytes::<redflag_crypto::SigningKeyPair>(&pkcs8_bytes) {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Clave inválida" }))),
    };
    let provider = hex::encode(kp.public_key());
    let now = now_secs();

    let bal = state.consensus.state.get_balance(&provider);
    if bal < req.amount_rf {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Saldo RF insuficiente: {}", bal) })));
    }

    let token_b = req.pool_id.trim_start_matches("RF_");
    // Verificar balance token B
    let bal_b = state.consensus.state.tokens.get_balance(&provider, token_b);
    if bal_b < req.amount_b {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Saldo {} insuficiente: tienes {}", token_b, bal_b) })));
    }

    match state.consensus.state.dex.add_liquidity(&req.pool_id, &provider, req.amount_rf, req.amount_b, now) {
        Ok(lp_tokens) => {
            // Debitar RF + token B del proveedor
            if let Some(mut acc) = state.consensus.state.get_account(&provider) {
                acc.balance = acc.balance.saturating_sub(req.amount_rf);
                let _ = state.consensus.state.save_account_pub(&acc);
            }
            let _ = state.consensus.state.tokens.debit(&provider, token_b, req.amount_b);
            emit(&state.ws_tx, "dex_liquidity", serde_json::json!({
                "pool": req.pool_id, "action": "add",
                "rf": req.amount_rf, "lp": lp_tokens,
            }));
            (StatusCode::OK, Json(serde_json::json!({
                "success": true, "lp_tokens": lp_tokens,
                "pool_id": req.pool_id,
            })))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn dex_remove_liquidity(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
    Json(req): Json<RemoveLiquidityRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if ip_rate_limit(&state.ip_rate_limits, &addr.ip().to_string()) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({ "error": "Rate limit" })));
    }

    let pkcs8_bytes = match hex::decode(&req.private_key_hex) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Clave inválida" }))),
    };
    let kp: redflag_crypto::SigningKeyPair = match postcard::from_bytes::<redflag_crypto::SigningKeyPair>(&pkcs8_bytes) {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Clave inválida" }))),
    };
    let provider = hex::encode(kp.public_key());
    let now = now_secs();

    let token_b_rm = req.pool_id.trim_start_matches("RF_");
    match state.consensus.state.dex.remove_liquidity(&req.pool_id, &provider, req.lp_tokens, now) {
        Ok((amount_rf, amount_b)) => {
            // Acreditar RF + token B al proveedor
            if let Some(mut acc) = state.consensus.state.get_account(&provider) {
                acc.balance = acc.balance.saturating_add(amount_rf);
                let _ = state.consensus.state.save_account_pub(&acc);
            }
            let _ = state.consensus.state.tokens.credit(&provider, token_b_rm, amount_b);
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "amount_rf": amount_rf, "amount_b": amount_b,
                "lp_burned": req.lp_tokens,
            })))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn dex_position(
    Path((addr, pool)): Path<(String, String)>,
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    match state.consensus.state.dex.get_lp_position(&addr, &pool) {
        Some(pos) => Json(serde_json::json!({
            "provider": pos.provider,
            "pool_id": pos.pool_id,
            "lp_tokens": pos.lp_tokens,
            "added_at": pos.added_at,
        })),
        None => Json(serde_json::json!({ "lp_tokens": 0, "pool_id": pool })),
    }
}

// ── Validators ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ValidatorApplyRequest {
    name:        String,
    address:     String,
    description: Option<String>,
    multiaddr:   Option<String>,
    /// Firma ML-DSA de "name:address" con la clave privada del nodo (autenticación)
    signature:   Option<String>,
}

async fn get_validators(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let peer_id = &state.peer_id;
    let validator_count = state.consensus.validator_count();
    // Leer solicitudes pendientes almacenadas en disco
    let pending: Vec<serde_json::Value> = std::fs::read_to_string("./node_data/validator_applications.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    Json(serde_json::json!({
        "chain_id": redflag_core::CHAIN_ID,
        "active_validators": validator_count,
        "bootstrap_peer": format!("/dns4/redflagweb3-node1.onrender.com/tcp/9000/p2p/{}", peer_id),
        "pending_applications": pending.len(),
        "join_instructions": "https://github.com/franklin0000/redflagweb3/blob/main/docs/validator.md",
    }))
}

async fn validator_apply(
    State(state): State<ApiState>,
    Json(req): Json<ValidatorApplyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.name.trim().is_empty() || req.address.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "name y address son obligatorios" })));
    }
    // FIX #4a: Dirección debe ser hex válido de al menos 64 chars (clave pública ML-DSA)
    if req.address.len() < 64 || hex::decode(&req.address).is_err() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "address debe ser clave publica ML-DSA hex" })));
    }
    // FIX #4b: Si se provee firma, verificarla. Si no, guardar como "unverified"
    let verified = if let Some(sig_hex) = &req.signature {
        let pubkey = hex::decode(&req.address).unwrap_or_default();
        let sig    = hex::decode(sig_hex).unwrap_or_default();
        let msg    = format!("{}:{}", req.name.trim(), req.address.trim());
        Verifier::verify(&pubkey, msg.as_bytes(), &sig).is_ok()
    } else {
        false
    };

    let entry = serde_json::json!({
        "name":        req.name.trim(),
        "address":     req.address.trim(),
        "description": req.description.unwrap_or_default(),
        "multiaddr":   req.multiaddr.unwrap_or_default(),
        "applied_at":  now_secs(),
        "verified":    verified,
        "status":      if verified { "pending_verified" } else { "pending_unverified" },
    });

    // Persistir en disco
    let path = "./node_data/validator_applications.json";
    let mut apps: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    apps.push(entry);
    let _ = std::fs::write(path, serde_json::to_string_pretty(&apps).unwrap_or_default());

    emit(&state.ws_tx, "validator_apply", serde_json::json!({ "name": req.name, "address": &req.address[..16.min(req.address.len())] }));

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "accepted": true,
        "message": "Solicitud recibida. El owner de la red la revisará pronto.",
        "next": "Únete al Discord/Telegram de redflag.web3 para seguimiento.",
    })))
}

// ── Bridge info ──────────────────────────────────────────────────────────────

async fn bridge_info() -> Json<serde_json::Value> {
    let bridge_url = std::env::var("BRIDGE_URL")
        .unwrap_or_else(|_| "https://redflagweb3-bridge.onrender.com".to_string());
    if let Ok(resp) = reqwest::Client::new()
        .get(format!("{}/bridge/status", bridge_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            return Json(data);
        }
    }
    Json(serde_json::json!({
        "bridge_active": true,
        "bridge_url": bridge_url,
        "lock_address": "RedFlag_Bridge_Lock_v1",
        "burn_address": "RedFlag_Bridge_Burn_v1",
        "supported_chains": [
            { "name": "Ethereum Mainnet", "chain_id": 1,   "contract": "0x92E83A72b3CD6d699cc8F16D756d5f31aCF55659" },
            { "name": "BSC Mainnet",      "chain_id": 56,  "contract": "0x06436bf6E71964A99bD4078043aa4cDfA0eadEe6" },
            { "name": "Polygon Mainnet",  "chain_id": 137, "contract": "0x19D2A913a6df973a7ad600F420960235307c6Cbf" },
        ],
        "how_to_bridge_to_rf": {
            "step1": "Llama a lock(rfAddress) en el contrato BridgeRF con ETH/BNB/MATIC",
            "step2": "El relayer detecta el evento y mintea RF en tu direccion redflag.web3",
        },
        "how_to_bridge_from_rf": {
            "step1": "Envia TX a RedFlag_Bridge_Lock_v1 con {to_evm_address, to_chain_id}",
            "step2": "El relayer llama a unlock() en la cadena EVM destino",
        }
    }))
}

async fn bridge_chains() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        {
            "name":         "Ethereum Mainnet",
            "chain_id":     1,
            "type":         "mainnet",
            "native_token": "ETH",
            "explorer":     "https://etherscan.io",
            "contract":     "0x92E83A72b3CD6d699cc8F16D756d5f31aCF55659",
        },
        {
            "name":         "BSC Mainnet",
            "chain_id":     56,
            "type":         "mainnet",
            "native_token": "BNB",
            "explorer":     "https://bscscan.com",
            "contract":     "0x06436bf6E71964A99bD4078043aa4cDfA0eadEe6",
        },
        {
            "name":         "Polygon Mainnet",
            "chain_id":     137,
            "type":         "mainnet",
            "native_token": "MATIC",
            "explorer":     "https://polygonscan.com",
            "contract":     "0x19D2A913a6df973a7ad600F420960235307c6Cbf",
            "token":        "RFLAG (0x06436bf6e71964a99bd4078043aa4cdfa0eadee6)",
        }
    ]))
}

// ── Market Data API (CoinGecko / CMC compatible) ─────────────────────────────

async fn market_summary(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let pools = state.consensus.state.dex.list_pools();
    let now = now_secs();
    let mut pairs = serde_json::Map::new();
    for p in &pools {
        let key = format!("RF_{}", p.token_b);
        let price = p.price() as f64 / 1_000_000.0;
        pairs.insert(key.clone(), serde_json::json!({
            "trading_pairs": key,
            "base_currency": "RF",
            "quote_currency": p.token_b,
            "last_price":     price,
            "lowest_ask":     price * 1.001,
            "highest_bid":    price * 0.999,
            "base_volume":    p.volume_rf as f64 / 1_000_000.0,
            "quote_volume":   p.volume_rf as f64 / 1_000_000.0 * price,
            "price_change_percent_24h": 0.0,
            "highest_price_24h": price * 1.05,
            "lowest_price_24h":  price * 0.95,
        }));
    }
    Json(serde_json::json!({ "timestamp": now, "pairs": pairs }))
}

async fn market_ticker(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let pools = state.consensus.state.dex.list_pools();
    let now = now_secs();
    let mut tickers = serde_json::Map::new();
    for p in &pools {
        let key = format!("RF_{}", p.token_b);
        let price = p.price() as f64 / 1_000_000.0;
        tickers.insert(key.clone(), serde_json::json!({
            "base_id":        "RF",
            "quote_id":       p.token_b,
            "base_name":      "RedFlag",
            "quote_name":     p.token_b,
            "base_symbol":    "RF",
            "quote_symbol":   p.token_b,
            "last":           price,
            "bid":            price * 0.999,
            "ask":            price * 1.001,
            "volume":         p.volume_rf as f64 / 1_000_000.0,
            "isFrozen":       "0",
            "base_logo":      "https://redflagweb3-app.onrender.com/logo.png",
        }));
    }
    Json(serde_json::json!({ "timestamp": now, "tickers": tickers }))
}

async fn market_orderbook(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let pools = state.consensus.state.dex.list_pools();
    let now = now_secs();
    let mut books = serde_json::Map::new();
    for p in &pools {
        let key = format!("RF_{}", p.token_b);
        let price = p.price() as f64 / 1_000_000.0;
        // AMM: simular libro de ordenes con la curva xy=k
        let asks: Vec<[f64; 2]> = (1..=5).map(|i| [price * (1.0 + i as f64 * 0.001), p.reserve_rf as f64 / 1_000_000.0 / 10.0]).collect();
        let bids: Vec<[f64; 2]> = (1..=5).map(|i| [price * (1.0 - i as f64 * 0.001), p.reserve_rf as f64 / 1_000_000.0 / 10.0]).collect();
        books.insert(key, serde_json::json!({ "timestamp": now, "bids": bids, "asks": asks }));
    }
    Json(serde_json::json!(books))
}

async fn market_trades(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let pools = state.consensus.state.dex.list_pools();
    let mut all_trades = serde_json::Map::new();
    for p in &pools {
        let key = format!("RF_{}", p.token_b);
        let history = state.consensus.state.dex.get_swap_history(&p.pool_id, 50);
        all_trades.insert(key, serde_json::json!(history));
    }
    Json(serde_json::json!(all_trades))
}

async fn market_assets() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "RF": {
            "name":            "RedFlag",
            "unified_cryptoasset_id": "RF",
            "can_withdraw":    true,
            "can_deposit":     true,
            "min_withdraw":    "1",
            "max_withdraw":    "1000000",
            "maker_fee":       "0.3",
            "taker_fee":       "0.3",
            "chain":           "redflag.web3",
            "chain_id":        2100,
            "logo":            "https://redflagweb3-app.onrender.com/logo.png",
            "website":         "https://redflagweb3-app.onrender.com",
        },
        "wETH": {
            "name":          "Wrapped ETH",
            "unified_cryptoasset_id": "ETH",
            "can_withdraw":  true,
            "can_deposit":   true,
            "maker_fee":     "0.3",
            "taker_fee":     "0.3",
        },
        "wBNB": {
            "name":          "Wrapped BNB",
            "unified_cryptoasset_id": "BNB",
            "can_withdraw":  true,
            "can_deposit":   true,
            "maker_fee":     "0.3",
            "taker_fee":     "0.3",
        },
        "wMATIC": {
            "name":          "Wrapped MATIC",
            "unified_cryptoasset_id": "MATIC",
            "can_withdraw":  true,
            "can_deposit":   true,
            "maker_fee":     "0.3",
            "taker_fee":     "0.3",
        },
    }))
}

// ── Staking ──────────────────────────────────────────────────────────────────

async fn staking_info() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "stake_address":  redflag_core::STAKE_ADDRESS,
        "min_stake_rf":   redflag_core::MIN_STAKE,
        "reward_per_vertex": "1 RF base + 0.1 RF por TX incluida",
        "round_interval_ms": 200,
        "how_to_stake": {
            "step1": "Instala el nodo: curl -sSf https://redflagweb3-app.onrender.com/install.sh | bash",
            "step2": "Consigue la direccion de tu nodo en el dashboard",
            "step3": "Envia RF a la direccion STAKE_ADDRESS desde tu wallet de nodo",
            "step4": "El nodo se registra automaticamente como validador",
            "step5": "Empiezas a recibir recompensas por cada vertice que produces",
        },
        "economics": {
            "block_reward": "1,000,000 microRF (1 RF) por vertice",
            "tx_reward":    "100,000 microRF (0.1 RF) por TX incluida",
            "min_stake":    "10,000 RF para ser validador",
        }
    }))
}

async fn get_stakes(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let mut raw = state.consensus.state.get_stakes();
    raw.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = raw.iter().map(|(_, a)| a).sum();
    let stakes: Vec<serde_json::Value> = raw.into_iter()
        .map(|(addr, amount)| serde_json::json!({ "address": addr, "staked_rf": amount }))
        .collect();
    Json(serde_json::json!({ "stakes": stakes, "total_staked": total, "validator_count": stakes.len() }))
}

// ── Reqwest para proxy bridge ─────────────────────────────────────────────────

// ── Servidor ─────────────────────────────────────────────────────────────────

pub async fn run_server(
    consensus: Arc<ConsensusEngine>,
    port: u16,
    peer_id: String,
    faucet_key: Arc<SigningKeyPair>,
    faucet_address: String,
    ws_tx: Arc<broadcast::Sender<String>>,
    node_start_time: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = ApiState {
        consensus,
        peer_id,
        faucet_key,
        faucet_address,
        ws_tx,
        node_start_time,
        faucet_cooldowns: Arc::new(DashMap::new()),
        ip_rate_limits: Arc::new(DashMap::new()),
    };
    let app = create_router(state);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    // ── TLS: si existen CERT_PATH y KEY_PATH en el entorno, usar HTTPS ────────
    let cert_path = std::env::var("TLS_CERT_PATH").ok();
    let key_path  = std::env::var("TLS_KEY_PATH").ok();

    match (cert_path, key_path) {
        (Some(cert), Some(key)) if std::path::Path::new(&cert).exists() && std::path::Path::new(&key).exists() => {
            println!("🔒 HTTPS activado con certificado: {}", cert);
            let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key).await?;
            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await?;
        }
        _ => {
            // Desarrollo: generar certificado auto-firmado en memoria
            if std::env::var("TLS_SELF_SIGNED").as_deref() == Ok("1") {
                println!("🔒 HTTPS auto-firmado (desarrollo) en https://{}", addr);
                let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])?;
                let cert_pem = cert.cert.pem();
                let key_pem  = cert.key_pair.serialize_pem();
                let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem(
                    cert_pem.into_bytes(),
                    key_pem.into_bytes(),
                ).await?;
                axum_server::bind_rustls(addr, tls_config)
                    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                    .await?;
            } else {
                println!("🌐 RPC + Dashboard en http://{}", addr);
                let listener = tokio::net::TcpListener::bind(addr).await?;
                axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
            }
        }
    }
    Ok(())
}
