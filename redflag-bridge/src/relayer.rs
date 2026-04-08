use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{self, Duration};
use anyhow::Result;
use crate::{
    evm::EvmConnector,
    redflag::RedFlagClient,
    state::BridgeState,
    types::{EvmChain, BridgeEventStatus},
};

/// Configuración del relayer
pub struct RelayerConfig {
    pub rf_node_url:        String,
    pub bridge_private_key: String,   // ML-DSA key hex para mintear en RF
    pub bridge_data_dir:    String,
    pub poll_interval_secs: u64,
    pub confirmations:      u64,      // bloques de confirmación en EVM
    pub max_amount_per_tx:  u64,      // límite de seguridad por TX
}

impl Default for RelayerConfig {
    fn default() -> Self {
        Self {
            rf_node_url:        std::env::var("RF_NODE_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            bridge_private_key: std::env::var("BRIDGE_RF_PRIVATE_KEY").unwrap_or_default(),
            bridge_data_dir:    std::env::var("BRIDGE_DATA_DIR").unwrap_or_else(|_| "./bridge_data".to_string()),
            poll_interval_secs: std::env::var("BRIDGE_POLL_SECS").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(60),  // 60s default — safe for public RPCs (~1 req/min)
            confirmations:      std::env::var("BRIDGE_CONFIRMATIONS").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(3),
            max_amount_per_tx:  std::env::var("BRIDGE_MAX_AMOUNT").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(1_000_000),
        }
    }
}

/// Backoff state per EVM chain
struct ChainBackoff {
    consecutive_errors: u32,
    blocked_until:      Option<Instant>,
}

impl ChainBackoff {
    fn new() -> Self { Self { consecutive_errors: 0, blocked_until: None } }

    /// Returns true if this chain is currently in backoff
    fn is_blocked(&self) -> bool {
        self.blocked_until.map(|t| Instant::now() < t).unwrap_or(false)
    }

    /// Call when a chain scan succeeds — reset backoff
    fn on_success(&mut self) {
        self.consecutive_errors = 0;
        self.blocked_until = None;
    }

    /// Call when a chain scan fails — schedule exponential backoff
    /// Delays: 30s → 60s → 120s → 240s → 300s (cap)
    fn on_error(&mut self, chain_name: &str, err: &anyhow::Error) {
        self.consecutive_errors += 1;
        let wait_secs = (30u64 << (self.consecutive_errors - 1).min(4)).min(300);
        self.blocked_until = Some(Instant::now() + Duration::from_secs(wait_secs));
        tracing::warn!(
            "⏳ {} error #{} — backoff {}s: {}",
            chain_name, self.consecutive_errors, wait_secs, err
        );
    }
}

pub struct Relayer {
    config:      RelayerConfig,
    rf_client:   Arc<RedFlagClient>,
    bridge_state: Arc<BridgeState>,
    evm_chains:  Vec<EvmConnector>,
}

impl Relayer {
    pub fn new(config: RelayerConfig) -> Result<Self> {
        let bridge_state = Arc::new(BridgeState::new(&config.bridge_data_dir)?);
        let rf_client = Arc::new(RedFlagClient::new(&config.rf_node_url));

        // Conectar a todas las cadenas EVM configuradas
        let evm_chains: Vec<EvmConnector> = EvmChain::all().into_iter()
            .filter_map(|chain| {
                // Solo conectar si la variable de entorno RPC está configurada
                if std::env::var(chain.rpc_env_var()).is_ok() {
                    match EvmConnector::new(chain.clone()) {
                        Ok(c)  => { println!("✅ EVM conectado: {}", chain.name()); Some(c) }
                        Err(e) => { println!("⚠️  EVM no disponible ({}): {}", chain.name(), e); None }
                    }
                } else {
                    println!("ℹ️  {} no configurado (falta {})", chain.name(), chain.rpc_env_var());
                    None
                }
            })
            .collect();

        Ok(Self { config, rf_client, bridge_state, evm_chains })
    }

    /// Loop principal del relayer
    pub async fn run(self: Arc<Self>) {
        println!("🌉 Bridge relayer iniciado — {} cadenas EVM activas", self.evm_chains.len());
        println!("   RedFlag node: {}", self.config.rf_node_url);
        println!("   Poll interval: {}s", self.config.poll_interval_secs);

        let mut poll = time::interval(Duration::from_secs(self.config.poll_interval_secs));
        let mut rf_scan_ts: u64 = 0;

        // Per-chain backoff state
        let mut backoff: HashMap<String, ChainBackoff> = HashMap::new();

        loop {
            poll.tick().await;

            // 1. Escanear eventos EVM → mintear en RF
            for connector in &self.evm_chains {
                let key = connector.chain.name().to_string();
                let cb = backoff.entry(key.clone()).or_insert_with(ChainBackoff::new);

                if cb.is_blocked() {
                    tracing::debug!("⏸  {} en backoff, saltando este ciclo", connector.chain.name());
                    continue;
                }

                match self.process_evm_to_rf(connector, &mut rf_scan_ts).await {
                    Ok(()) => {
                        backoff.get_mut(&key).unwrap().on_success();
                    }
                    Err(e) => {
                        let cb = backoff.get_mut(&key).unwrap();
                        cb.on_error(connector.chain.name(), &e);
                    }
                }
            }

            // 2. Escanear eventos RF → liberar en EVM
            if let Err(e) = self.process_rf_to_evm().await {
                tracing::error!("RF→EVM error: {}", e);
            }

            // 3. Flush estado
            let _ = self.bridge_state.flush();
        }
    }

    /// Procesa: EVM Lock → RF Mint
    async fn process_evm_to_rf(&self, connector: &EvmConnector, _rf_ts: &mut u64) -> Result<()> {
        let current_block = connector.current_block().await;
        if current_block == 0 { return Ok(()); }

        // Leer último bloque escaneado (o empezar desde current - 100)
        let from_block = self.bridge_state.last_scanned_block(&connector.chain)
            .max(current_block.saturating_sub(100));
        let to_block = current_block.saturating_sub(self.config.confirmations);

        if from_block >= to_block { return Ok(()); }

        // Now propagates error so the caller can apply backoff
        let events = connector.scan_lock_events(from_block, to_block).await?;

        for mut event in events {
            let key = event.evm_tx_hash.clone();

            // Skip ya procesados
            if self.bridge_state.is_evm_event_processed(&connector.chain, &key) {
                continue;
            }

            // Límite de seguridad
            if event.amount > self.config.max_amount_per_tx {
                tracing::warn!("⚠️  Evento EVM con amount {} > límite {}, rechazado",
                    event.amount, self.config.max_amount_per_tx);
                event.status = BridgeEventStatus::Failed("Excede límite máximo".to_string());
                self.bridge_state.save_evm_event(&event)?;
                continue;
            }

            // Guardar como en proceso
            event.status = BridgeEventStatus::Processing;
            self.bridge_state.save_evm_event(&event)?;

            println!("🌉 EVM→RF: {} RF de {} ({}) → RF:{}",
                event.amount,
                &event.from_evm_address[..10],
                connector.chain.name(),
                &event.to_rf_address[..12.min(event.to_rf_address.len())],
            );

            // Determinar qué token mintear según la cadena origen
            let token = match event.chain {
                crate::types::EvmChain::EthereumMainnet => "wETH",
                crate::types::EvmChain::BscMainnet      => "wBNB",
                crate::types::EvmChain::PolygonMainnet  => "wMATIC",
            };

            // Mintear wrapped token en RedFlag
            match self.rf_client.mint_wrapped_token(&event.to_rf_address, token, event.amount).await {
                Ok(()) => {
                    event.status = BridgeEventStatus::Completed;
                    println!("✅ EVM→RF completado: {} {} → {}",
                        event.amount, token, &event.to_rf_address[..12.min(event.to_rf_address.len())]);
                }
                Err(e) => {
                    event.status = BridgeEventStatus::Failed(e.to_string());
                    tracing::error!("❌ EVM→RF mint falló: {}", e);
                }
            }
            self.bridge_state.save_evm_event(&event)?;
        }

        self.bridge_state.set_last_scanned_block(&connector.chain, to_block)?;
        Ok(())
    }

    /// Procesa: RF Lock → EVM Unlock
    async fn process_rf_to_evm(&self) -> Result<()> {
        let since_ts = 0; // Podría persistir el último timestamp escaneado
        let events = self.rf_client.scan_lock_events(since_ts).await;

        for mut event in events {
            // Skip ya procesados
            if self.bridge_state.is_rf_event_processed(&event.rf_tx_hash)
               || self.bridge_state.is_rf_nonce_processed(event.nonce) {
                continue;
            }

            // Límite de seguridad
            if event.amount > self.config.max_amount_per_tx {
                tracing::warn!("⚠️  RF→EVM amount {} > límite, rechazado", event.amount);
                event.status = BridgeEventStatus::Failed("Excede límite máximo".to_string());
                self.bridge_state.save_rf_event(&event)?;
                continue;
            }

            // Buscar el conector EVM correcto
            let connector = self.evm_chains.iter()
                .find(|c| c.chain == event.to_chain);

            let Some(connector) = connector else {
                event.status = BridgeEventStatus::Failed(
                    format!("Cadena {} no configurada en el relayer", event.to_chain.name())
                );
                self.bridge_state.save_rf_event(&event)?;
                continue;
            };

            // Verificar que el nonce no haya sido procesado on-chain
            if connector.is_nonce_processed(event.nonce).await {
                event.status = BridgeEventStatus::Completed; // ya estaba hecho
                self.bridge_state.save_rf_event(&event)?;
                self.bridge_state.mark_rf_nonce_processed(event.nonce)?;
                continue;
            }

            event.status = BridgeEventStatus::Processing;
            self.bridge_state.save_rf_event(&event)?;

            println!("🌉 RF→EVM: {} RF de RF:{} → EVM:{} ({})",
                event.amount,
                &event.from_rf_address[..12.min(event.from_rf_address.len())],
                &event.to_evm_address[..10],
                event.to_chain.name(),
            );

            match connector.execute_unlock(&event.to_evm_address, event.amount, event.nonce).await {
                Ok(evm_tx) => {
                    event.status = BridgeEventStatus::Completed;
                    self.bridge_state.mark_rf_nonce_processed(event.nonce)?;
                    println!("✅ RF→EVM completado: {} RF (evm_tx: {})", event.amount, &evm_tx[..16]);
                }
                Err(e) => {
                    event.status = BridgeEventStatus::Failed(e.to_string());
                    tracing::error!("❌ RF→EVM unlock falló: {}", e);
                }
            }
            self.bridge_state.save_rf_event(&event)?;
        }

        Ok(())
    }

    pub fn summary(&self) -> serde_json::Value {
        let (evm_total, rf_total, evm_done, rf_done) = self.bridge_state.stats();
        let chains: Vec<serde_json::Value> = self.evm_chains.iter()
            .map(|c| serde_json::json!({
                "name":     c.chain.name(),
                "chain_id": c.chain.chain_id(),
                "contract": c.contract_address.map(|a| format!("{:?}", a)),
                "connected": true,
            }))
            .collect();
        serde_json::json!({
            "chains":               chains,
            "evm_lock_events":      evm_total,
            "rf_lock_events":       rf_total,
            "evm_completed":        evm_done,
            "rf_completed":         rf_done,
            "pending":              (evm_total - evm_done) + (rf_total - rf_done),
            "rf_node":              self.config.rf_node_url,
        })
    }
}
