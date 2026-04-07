use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Serialize, Deserialize};
use crate::types::{EvmChain, RfLockEvent, BridgeEventStatus, BRIDGE_LOCK_ADDRESS};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct TxResponse {
    pub accepted: bool,
    pub message: String,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryResponse {
    pub history: Vec<RfTransaction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RfTransaction {
    pub sender:    String,
    pub receiver:  String,
    pub amount:    u64,
    pub fee:       u64,
    pub nonce:     u64,
    pub timestamp: u64,
    pub data:      Vec<u8>,
}

/// Payload codificado en `data` de una TX de puente RF → EVM
#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeData {
    pub to_evm_address: String,
    pub to_chain_id:    u64,
    pub bridge_nonce:   u64,
}

pub struct RedFlagClient {
    pub node_url: String,
    pub http:     Client,
}

impl RedFlagClient {
    pub fn new(node_url: &str) -> Self {
        Self {
            node_url: node_url.to_string(),
            http: Client::new(),
        }
    }

    /// Mintea RF nativo en la cadena RedFlag usando BRIDGE_MINT_SECRET (sin exponer clave privada)
    /// FIX #5: La clave privada RF nunca viaja por HTTP.
    /// El nodo firma internamente usando su faucet/relayer key almacenada localmente.
    pub async fn mint_rf(&self, to_address: &str, amount: u64, _bridge_private_key: &str) -> Result<String> {
        // FIX A: Fallar explícitamente si el secreto no está configurado
        let secret = std::env::var("BRIDGE_MINT_SECRET")
            .map_err(|_| anyhow::anyhow!("BRIDGE_MINT_SECRET no configurado en el entorno del relayer"))?;
        if secret.is_empty() || secret == "bridge_dev_secret" {
            anyhow::bail!("BRIDGE_MINT_SECRET usa valor por defecto inseguro — configura uno seguro");
        }
        // Usamos el endpoint /bridge/mint que autentica con BRIDGE_MINT_SECRET
        // y ejecuta la TX internamente en el nodo sin exponer ninguna clave privada.
        let res = self.http
            .post(format!("{}/bridge/mint", self.node_url))
            .json(&serde_json::json!({
                "bridge_secret": secret,
                "to":     to_address,
                "token":  "RF",
                "amount": amount,
            }))
            .send().await?
            .json::<serde_json::Value>().await?;

        if res["success"].as_bool() != Some(true) {
            anyhow::bail!("Mint RF falló: {}", res["error"].as_str().unwrap_or("unknown"));
        }
        Ok(res["tx_hash"].as_str().unwrap_or("").to_string())
    }

    /// Mintea wrapped token (wETH, wBNB, wMATIC) cuando el bridge detecta un lock EVM
    pub async fn mint_wrapped_token(&self, to: &str, token: &str, amount_units: u64) -> Result<()> {
        let secret = std::env::var("BRIDGE_MINT_SECRET")
            .map_err(|_| anyhow::anyhow!("BRIDGE_MINT_SECRET no configurado"))?;
        if secret.is_empty() || secret == "bridge_dev_secret" {
            anyhow::bail!("BRIDGE_MINT_SECRET inseguro");
        }

        let res = self.http
            .post(format!("{}/bridge/mint", self.node_url))
            .json(&serde_json::json!({
                "bridge_secret": secret,
                "to":     to,
                "token":  token,
                "amount": amount_units,
            }))
            .send().await?
            .json::<serde_json::Value>().await?;

        if res["success"].as_bool() != Some(true) {
            anyhow::bail!("Mint {} falló: {}", token, res["error"].as_str().unwrap_or("unknown"));
        }
        tracing::info!("✅ Minted {} {} → {}", amount_units, token, &to[..12.min(to.len())]);
        Ok(())
    }

    /// Quema RF (envía al address de quema del bridge) cuando alguien hace bridge RF → EVM
    /// Esto lo hace el usuario directamente desde su wallet; el relayer lo detecta.
    /// Para verificar: escanear historial del BRIDGE_LOCK_ADDRESS.
    pub async fn scan_lock_events(&self, since_timestamp: u64) -> Vec<RfLockEvent> {
        let url = format!("{}/history/{}", self.node_url, BRIDGE_LOCK_ADDRESS);
        let resp = match self.http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => { tracing::error!("Error al conectar con RedFlag: {}", e); return vec![]; }
        };

        let history: HistoryResponse = match resp.json().await {
            Ok(h) => h,
            Err(_) => return vec![],
        };

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        history.history.iter()
            .filter(|tx| tx.timestamp >= since_timestamp && tx.receiver == BRIDGE_LOCK_ADDRESS)
            .filter_map(|tx| {
                // El campo `data` contiene BridgeData serializado como JSON
                let bridge_data: BridgeData = serde_json::from_slice(&tx.data).ok()?;
                let chain = EvmChain::all().into_iter()
                    .find(|c| c.chain_id() == bridge_data.to_chain_id)?;

                Some(RfLockEvent {
                    rf_tx_hash:      blake3_hex(&bincode::serde::encode_to_vec(tx, bincode::config::standard()).unwrap_or_default()),
                    from_rf_address: tx.sender.clone(),
                    to_evm_address:  bridge_data.to_evm_address,
                    to_chain:        chain,
                    amount:          tx.amount,
                    nonce:           bridge_data.bridge_nonce,
                    status:          BridgeEventStatus::Pending,
                    created_at:      now,
                })
            })
            .collect()
    }

    /// Obtiene el saldo de una dirección
    pub async fn get_balance(&self, address: &str) -> u64 {
        let url = format!("{}/balance/{}", self.node_url, address);
        match self.http.get(&url).send().await {
            Ok(resp) => resp.json::<serde_json::Value>().await
                .map(|v| v["balance"].as_u64().unwrap_or(0))
                .unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Comprueba si el nodo está vivo
    pub async fn is_alive(&self) -> bool {
        self.http.get(format!("{}/status", self.node_url))
            .send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }
}

fn blake3_hex(data: &[u8]) -> String {
    hex::encode(blake3::hash(data).as_bytes())
}
