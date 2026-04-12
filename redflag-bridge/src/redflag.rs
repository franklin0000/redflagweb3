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

    /// Recoge aprobaciones threshold de todos los nodos configurados en BRIDGE_NODE_URLS.
    /// Devuelve (approvals_json, evm_tx_hash_used, nonce_used).
    async fn collect_approvals(
        &self,
        evm_tx_hash: &str,
        to: &str,
        token: &str,
        amount: u64,
        nonce: u64,
    ) -> Result<Vec<serde_json::Value>> {
        let node_urls: Vec<String> = std::env::var("BRIDGE_NODE_URLS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if node_urls.is_empty() {
            return Ok(vec![]);
        }

        let threshold = (node_urls.len() * 2 + 2) / 3;
        let mut approvals = Vec::new();

        for url in &node_urls {
            match self.http
                .post(format!("{}/bridge/approve-mint", url))
                .json(&serde_json::json!({
                    "evm_tx_hash": evm_tx_hash,
                    "to":          to,
                    "token":       token,
                    "amount":      amount,
                    "nonce":       nonce,
                }))
                .timeout(std::time::Duration::from_secs(10))
                .send().await
            {
                Ok(r) => {
                    if let Ok(v) = r.json::<serde_json::Value>().await {
                        if v["approved"].as_bool() == Some(true) {
                            approvals.push(serde_json::json!({
                                "signer_pubkey": v["signer_pubkey"],
                                "signature":     v["signature"],
                            }));
                            tracing::info!("✅ Aprobación de {}: {}", url, approvals.len());
                        }
                    }
                }
                Err(e) => tracing::warn!("⚠️  Nodo {} no respondió: {}", url, e),
            }
            if approvals.len() >= threshold { break; }
        }

        if approvals.len() < threshold {
            anyhow::bail!("Threshold no alcanzado: {}/{} aprobaciones", approvals.len(), threshold);
        }
        Ok(approvals)
    }

    /// Mintea RF nativo en la cadena RedFlag.
    /// Usa threshold multi-sig si BRIDGE_NODE_URLS está configurado, si no usa secreto legado.
    pub async fn mint_rf(&self, to_address: &str, amount: u64, _bridge_private_key: &str) -> Result<String> {
        let node_urls = std::env::var("BRIDGE_NODE_URLS").unwrap_or_default();
        if !node_urls.is_empty() {
            // Modo threshold
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let evm_tx_hash = format!("rf-{}-{}", to_address, nonce);
            let approvals = self.collect_approvals(&evm_tx_hash, to_address, "RF", amount, nonce).await?;
            let res = self.http
                .post(format!("{}/bridge/mint", self.node_url))
                .json(&serde_json::json!({
                    "to": to_address, "token": "RF", "amount": amount,
                    "evm_tx_hash": evm_tx_hash, "nonce": nonce,
                    "approvals": approvals,
                }))
                .send().await?.json::<serde_json::Value>().await?;
            if res["success"].as_bool() != Some(true) {
                anyhow::bail!("Mint RF threshold falló: {}", res["error"].as_str().unwrap_or("unknown"));
            }
            return Ok(res["tx_hash"].as_str().unwrap_or("").to_string());
        }

        // Modo legado: secreto compartido
        let secret = std::env::var("BRIDGE_MINT_SECRET")
            .map_err(|_| anyhow::anyhow!("BRIDGE_MINT_SECRET no configurado"))?;
        if secret.is_empty() || secret == "bridge_dev_secret" {
            anyhow::bail!("BRIDGE_MINT_SECRET usa valor por defecto inseguro — configura uno seguro");
        }
        let res = self.http
            .post(format!("{}/bridge/mint", self.node_url))
            .json(&serde_json::json!({
                "bridge_secret": secret, "to": to_address, "token": "RF", "amount": amount,
            }))
            .send().await?.json::<serde_json::Value>().await?;
        if res["success"].as_bool() != Some(true) {
            anyhow::bail!("Mint RF falló: {}", res["error"].as_str().unwrap_or("unknown"));
        }
        Ok(res["tx_hash"].as_str().unwrap_or("").to_string())
    }

    /// Mintea wrapped token (wETH, wBNB, wMATIC) cuando el bridge detecta un lock EVM
    pub async fn mint_wrapped_token(&self, to: &str, token: &str, amount_units: u64) -> Result<()> {
        let node_urls = std::env::var("BRIDGE_NODE_URLS").unwrap_or_default();
        if !node_urls.is_empty() {
            // Modo threshold
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let evm_tx_hash = format!("evm-{}-{}-{}", token, to, nonce);
            let approvals = self.collect_approvals(&evm_tx_hash, to, token, amount_units, nonce).await?;
            let res = self.http
                .post(format!("{}/bridge/mint", self.node_url))
                .json(&serde_json::json!({
                    "to": to, "token": token, "amount": amount_units,
                    "evm_tx_hash": evm_tx_hash, "nonce": nonce,
                    "approvals": approvals,
                }))
                .send().await?.json::<serde_json::Value>().await?;
            if res["success"].as_bool() != Some(true) {
                anyhow::bail!("Mint {} threshold falló: {}", token, res["error"].as_str().unwrap_or("unknown"));
            }
            tracing::info!("✅ Minted {} {} → {} (threshold)", amount_units, token, &to[..12.min(to.len())]);
            return Ok(());
        }

        // Modo legado
        let secret = std::env::var("BRIDGE_MINT_SECRET")
            .map_err(|_| anyhow::anyhow!("BRIDGE_MINT_SECRET no configurado"))?;
        if secret.is_empty() || secret == "bridge_dev_secret" {
            anyhow::bail!("BRIDGE_MINT_SECRET inseguro");
        }
        let res = self.http
            .post(format!("{}/bridge/mint", self.node_url))
            .json(&serde_json::json!({
                "bridge_secret": secret, "to": to, "token": token, "amount": amount_units,
            }))
            .send().await?.json::<serde_json::Value>().await?;
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
                    rf_tx_hash:      blake3_hex(&postcard::to_allocvec(tx).unwrap_or_default()),
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
