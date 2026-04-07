use anyhow::Result;
use sled::{Db, Tree};
use crate::types::{EvmLockEvent, RfLockEvent, BridgeEventStatus, EvmChain};

/// Persistencia del estado del bridge: eventos procesados, nonces usados, último bloque escaneado.
pub struct BridgeState {
    db:          Db,
    evm_events:  Tree,   // key = evm_tx_hash → EvmLockEvent
    rf_events:   Tree,   // key = rf_tx_hash  → RfLockEvent
    scan_state:  Tree,   // key = chain_id → last_scanned_block (u64 BE)
    rf_nonces:   Tree,   // key = nonce (u64 BE) → "1" (ya procesado en RF→EVM)
}

impl BridgeState {
    pub fn new(path: &str) -> Result<Self> {
        let db = sled::open(path)?;
        let evm_events = db.open_tree("evm_events")?;
        let rf_events  = db.open_tree("rf_events")?;
        let scan_state = db.open_tree("scan_state")?;
        let rf_nonces  = db.open_tree("rf_nonces")?;
        Ok(Self { db, evm_events, rf_events, scan_state, rf_nonces })
    }

    // ── EVM lock events ────────────────────────────────────────────────────────

    pub fn save_evm_event(&self, event: &EvmLockEvent) -> Result<()> {
        let key = format!("{}_{}", event.chain.chain_id(), event.evm_tx_hash);
        self.evm_events.insert(key, bincode::serde::encode_to_vec(event, bincode::config::standard())?)?;
        Ok(())
    }

    pub fn get_evm_event(&self, chain: &EvmChain, tx_hash: &str) -> Option<EvmLockEvent> {
        let key = format!("{}_{}", chain.chain_id(), tx_hash);
        self.evm_events.get(key).ok().flatten()
            .and_then(|b| bincode::serde::decode_from_slice::<_, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
    }

    pub fn is_evm_event_processed(&self, chain: &EvmChain, tx_hash: &str) -> bool {
        self.get_evm_event(chain, tx_hash)
            .map(|e| e.status == BridgeEventStatus::Completed)
            .unwrap_or(false)
    }

    pub fn list_evm_events(&self) -> Vec<EvmLockEvent> {
        self.evm_events.iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| bincode::serde::decode_from_slice::<_, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
            .collect()
    }

    // ── RF lock events ─────────────────────────────────────────────────────────

    pub fn save_rf_event(&self, event: &RfLockEvent) -> Result<()> {
        self.rf_events.insert(&event.rf_tx_hash, bincode::serde::encode_to_vec(event, bincode::config::standard())?)?;
        Ok(())
    }

    pub fn is_rf_event_processed(&self, rf_tx_hash: &str) -> bool {
        self.rf_events.get(rf_tx_hash).ok().flatten()
            .and_then(|b| bincode::serde::decode_from_slice::<RfLockEvent, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
            .map(|e| e.status == BridgeEventStatus::Completed)
            .unwrap_or(false)
    }

    pub fn list_rf_events(&self) -> Vec<RfLockEvent> {
        self.rf_events.iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| bincode::serde::decode_from_slice::<_, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
            .collect()
    }

    // ── Scan checkpoints (último bloque escaneado por cadena) ──────────────────

    pub fn last_scanned_block(&self, chain: &EvmChain) -> u64 {
        self.scan_state.get(chain.chain_id().to_be_bytes())
            .ok().flatten()
            .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(0)
    }

    pub fn set_last_scanned_block(&self, chain: &EvmChain, block: u64) -> Result<()> {
        self.scan_state.insert(chain.chain_id().to_be_bytes(), &block.to_be_bytes())?;
        Ok(())
    }

    // ── Nonces RF→EVM ya procesados ────────────────────────────────────────────

    pub fn is_rf_nonce_processed(&self, nonce: u64) -> bool {
        self.rf_nonces.contains_key(nonce.to_be_bytes()).unwrap_or(false)
    }

    pub fn mark_rf_nonce_processed(&self, nonce: u64) -> Result<()> {
        self.rf_nonces.insert(nonce.to_be_bytes(), b"1")?;
        Ok(())
    }

    // ── Estadísticas ───────────────────────────────────────────────────────────

    pub fn stats(&self) -> (usize, usize, usize, usize) {
        let evm_total = self.evm_events.len();
        let rf_total  = self.rf_events.len();
        let evm_done  = self.list_evm_events().iter().filter(|e| e.status == BridgeEventStatus::Completed).count();
        let rf_done   = self.list_rf_events().iter().filter(|e| e.status == BridgeEventStatus::Completed).count();
        (evm_total, rf_total, evm_done, rf_done)
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}
