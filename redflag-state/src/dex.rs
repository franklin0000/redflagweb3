/// RedFlag DEX — AMM Constant Product (x * y = k)
/// Permite crear pares de liquidez y hacer swaps nativos en la cadena.
/// Token A siempre es RF (nativo). Token B es un token wrapper (wETH, wBNB, wMATIC).

use serde::{Serialize, Deserialize};
use sled::Tree;
use anyhow::Result;

// ── Tipos ─────────────────────────────────────────────────────────────────────

/// Dirección especial del DEX para recibir fees de swap
pub const DEX_FEE_ADDRESS: &str  = "RedFlag_DEX_FeePool_v1";
/// Fee de swap en basis points (30 = 0.3% como Uniswap V2)
pub const SWAP_FEE_BPS: u64      = 30;
/// Mínimo de liquidez para crear un pool (evita ataques de precio)
pub const MIN_LIQUIDITY: u64     = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityPool {
    /// ID único del pool: "RF_wETH", "RF_wBNB", etc.
    pub pool_id:         String,
    /// Token B (RF es siempre token A)
    pub token_b:         String,
    /// Reserva de RF en el pool
    pub reserve_rf:      u64,
    /// Reserva de token B en el pool
    pub reserve_b:       u64,
    /// Total de LP tokens emitidos
    pub total_lp:        u64,
    /// Precio de RF en token B (reserve_b / reserve_rf * 1e6)
    pub price_rf_in_b:   u64,
    /// Volumen total de swaps (RF)
    pub volume_rf:       u64,
    /// Fees acumulados (RF)
    pub fees_collected:  u64,
    /// Timestamp de creación
    pub created_at:      u64,
    /// Timestamp de última actualización
    pub updated_at:      u64,
}

impl LiquidityPool {
    /// Precio de RF en tokens B (con 6 decimales de precisión)
    pub fn price(&self) -> f64 {
        if self.reserve_rf == 0 { return 0.0; }
        self.reserve_b as f64 / self.reserve_rf as f64
    }

    /// Calcula cuánto token B se recibe al hacer swap de `amount_rf` RF
    /// Fórmula: amount_out = reserve_b * amount_in_with_fee / (reserve_rf + amount_in_with_fee)
    pub fn calc_swap_rf_to_b(&self, amount_rf: u64) -> (u64, u64) {
        let fee = amount_rf * SWAP_FEE_BPS / 10_000;
        let amount_in_after_fee = amount_rf - fee;
        let amount_out = (self.reserve_b as u128 * amount_in_after_fee as u128
            / (self.reserve_rf as u128 + amount_in_after_fee as u128)) as u64;
        (amount_out, fee)
    }

    /// Calcula cuánto RF se recibe al hacer swap de `amount_b` token B
    pub fn calc_swap_b_to_rf(&self, amount_b: u64) -> (u64, u64) {
        let fee = amount_b * SWAP_FEE_BPS / 10_000;
        let amount_in_after_fee = amount_b - fee;
        let amount_out = (self.reserve_rf as u128 * amount_in_after_fee as u128
            / (self.reserve_b as u128 + amount_in_after_fee as u128)) as u64;
        (amount_out, fee)
    }

    /// Calcula LP tokens a emitir al agregar liquidez
    /// Primera liquidez: sqrt(reserve_rf * reserve_b)
    /// Liquidez adicional: min(amount_rf/reserve_rf, amount_b/reserve_b) * total_lp
    pub fn calc_lp_tokens(&self, amount_rf: u64, amount_b: u64) -> u64 {
        if self.total_lp == 0 {
            // Primer proveedor: sqrt(a*b)
            let product = amount_rf as u128 * amount_b as u128;
            integer_sqrt(product)
        } else {
            // Proporcional al pool existente
            let lp_from_rf = amount_rf as u128 * self.total_lp as u128 / self.reserve_rf as u128;
            let lp_from_b  = amount_b  as u128 * self.total_lp as u128 / self.reserve_b  as u128;
            lp_from_rf.min(lp_from_b) as u64
        }
    }

    /// Actualiza el precio almacenado
    pub fn update_price(&mut self, now: u64) {
        if self.reserve_rf > 0 {
            self.price_rf_in_b = (self.reserve_b as u128 * 1_000_000 / self.reserve_rf as u128) as u64;
        }
        self.updated_at = now;
    }
}

fn integer_sqrt(n: u128) -> u64 {
    if n == 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x as u64
}

/// Posición de liquidez de un proveedor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpPosition {
    pub provider:   String,
    pub pool_id:    String,
    pub lp_tokens:  u64,
    pub added_at:   u64,
}

/// Historial de swap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRecord {
    pub pool_id:     String,
    pub direction:   SwapDirection,
    pub amount_in:   u64,
    pub amount_out:  u64,
    pub fee:         u64,
    pub trader:      String,
    pub timestamp:   u64,
    pub tx_hash:     String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwapDirection {
    RfToB,
    BToRf,
}

// ── DEX State ─────────────────────────────────────────────────────────────────

pub struct DexState {
    pools:       Tree,   // pool_id → LiquidityPool
    positions:   Tree,   // "{provider}_{pool_id}" → LpPosition
    swap_history: Tree,  // "{timestamp}_{pool_id}" → SwapRecord
    price_history: Tree, // "{pool_id}_{timestamp}" → price (u64, 6 decimals)
}

impl DexState {
    pub fn new(pools: Tree, positions: Tree, swap_history: Tree, price_history: Tree) -> Self {
        Self { pools, positions, swap_history, price_history }
    }

    // ── Pools ──────────────────────────────────────────────────────────────────

    pub fn get_pool(&self, pool_id: &str) -> Option<LiquidityPool> {
        self.pools.get(pool_id).ok().flatten()
            .and_then(|b| postcard::from_bytes::<_>(&b).ok())
    }

    pub fn save_pool(&self, pool: &LiquidityPool) -> Result<()> {
        self.pools.insert(&pool.pool_id, postcard::to_allocvec(pool)?)?;
        Ok(())
    }

    pub fn list_pools(&self) -> Vec<LiquidityPool> {
        self.pools.iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| postcard::from_bytes::<_>(&b).ok())
            .collect()
    }

    /// Crear pool inicial
    pub fn create_pool(&self, token_b: &str, now: u64) -> Result<LiquidityPool> {
        let pool_id = format!("RF_{}", token_b);
        if self.get_pool(&pool_id).is_some() {
            anyhow::bail!("Pool {} ya existe", pool_id);
        }
        let pool = LiquidityPool {
            pool_id: pool_id.clone(),
            token_b: token_b.to_string(),
            reserve_rf: 0,
            reserve_b: 0,
            total_lp: 0,
            price_rf_in_b: 0,
            volume_rf: 0,
            fees_collected: 0,
            created_at: now,
            updated_at: now,
        };
        self.save_pool(&pool)?;
        println!("🏊 Pool creado: {}", pool_id);
        Ok(pool)
    }

    // ── Add Liquidity ──────────────────────────────────────────────────────────

    pub fn add_liquidity(
        &self,
        pool_id: &str,
        provider: &str,
        amount_rf: u64,
        amount_b: u64,
        now: u64,
    ) -> Result<u64> {
        let mut pool = self.get_pool(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool {} no existe", pool_id))?;

        if amount_rf < MIN_LIQUIDITY || amount_b < MIN_LIQUIDITY {
            anyhow::bail!("Liquidez mínima: {} RF y {} tokens", MIN_LIQUIDITY, MIN_LIQUIDITY);
        }

        let lp_tokens = pool.calc_lp_tokens(amount_rf, amount_b);
        if lp_tokens == 0 {
            anyhow::bail!("LP tokens calculados = 0");
        }

        // FIX E: usar checked_add para evitar overflow silencioso en reservas
        pool.reserve_rf  = pool.reserve_rf.checked_add(amount_rf)
            .ok_or_else(|| anyhow::anyhow!("Overflow en reserve_rf"))?;
        pool.reserve_b   = pool.reserve_b.checked_add(amount_b)
            .ok_or_else(|| anyhow::anyhow!("Overflow en reserve_b"))?;
        pool.total_lp    = pool.total_lp.checked_add(lp_tokens)
            .ok_or_else(|| anyhow::anyhow!("Overflow en total_lp"))?;
        pool.update_price(now);
        self.save_pool(&pool)?;

        // Actualizar posición del proveedor
        let pos_key = format!("{}_{}", provider, pool_id);
        let mut pos = self.positions.get(&pos_key).ok().flatten()
            .and_then(|b| postcard::from_bytes::<LpPosition>(&b).ok())
            .unwrap_or(LpPosition {
                provider: provider.to_string(),
                pool_id: pool_id.to_string(),
                lp_tokens: 0,
                added_at: now,
            });
        pos.lp_tokens += lp_tokens;
        self.positions.insert(&pos_key, postcard::to_allocvec(&pos)?)?;

        println!("💧 Add liquidity: {} RF + {} {} → {} LP ({})",
            amount_rf, amount_b, pool.token_b, lp_tokens, pool_id);
        Ok(lp_tokens)
    }

    // ── Remove Liquidity ───────────────────────────────────────────────────────

    pub fn remove_liquidity(
        &self,
        pool_id: &str,
        provider: &str,
        lp_tokens: u64,
        now: u64,
    ) -> Result<(u64, u64)> {
        let mut pool = self.get_pool(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool no existe"))?;
        let pos_key = format!("{}_{}", provider, pool_id);
        let mut pos = self.positions.get(&pos_key).ok().flatten()
            .and_then(|b| postcard::from_bytes::<LpPosition>(&b).ok())
            .ok_or_else(|| anyhow::anyhow!("No tienes posición en este pool"))?;

        if lp_tokens > pos.lp_tokens {
            anyhow::bail!("LP tokens insuficientes: tienes {}, pediste {}", pos.lp_tokens, lp_tokens);
        }

        let amount_rf = (pool.reserve_rf as u128 * lp_tokens as u128 / pool.total_lp as u128) as u64;
        let amount_b  = (pool.reserve_b  as u128 * lp_tokens as u128 / pool.total_lp as u128) as u64;

        pool.reserve_rf  = pool.reserve_rf.saturating_sub(amount_rf);
        pool.reserve_b   = pool.reserve_b.saturating_sub(amount_b);
        pool.total_lp    = pool.total_lp.saturating_sub(lp_tokens);
        pool.update_price(now);
        self.save_pool(&pool)?;

        pos.lp_tokens -= lp_tokens;
        self.positions.insert(&pos_key, postcard::to_allocvec(&pos)?)?;

        println!("🔥 Remove liquidity: {} LP → {} RF + {} {} ({})",
            lp_tokens, amount_rf, amount_b, pool.token_b, pool_id);
        Ok((amount_rf, amount_b))
    }

    // ── Swap ───────────────────────────────────────────────────────────────────

    pub fn execute_swap_rf_to_b(
        &self,
        pool_id: &str,
        trader: &str,
        amount_rf_in: u64,
        min_amount_out: u64,
        tx_hash: &str,
        now: u64,
    ) -> Result<u64> {
        let mut pool = self.get_pool(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool no existe"))?;

        if pool.reserve_rf == 0 || pool.reserve_b == 0 {
            anyhow::bail!("Pool sin liquidez");
        }

        if amount_rf_in == 0 {
            anyhow::bail!("amount_in no puede ser 0");
        }
        // FIX C: validar que amount_in no cause overflow en las reservas
        let new_reserve_rf = pool.reserve_rf.checked_add(amount_rf_in)
            .ok_or_else(|| anyhow::anyhow!("Overflow en reserva RF"))?;

        let (amount_out, fee) = pool.calc_swap_rf_to_b(amount_rf_in);
        if amount_out == 0 {
            anyhow::bail!("Swap produce 0 tokens de salida");
        }
        if amount_out < min_amount_out {
            anyhow::bail!("Slippage excedido: esperabas {} mín, recibirías {}", min_amount_out, amount_out);
        }
        if amount_out >= pool.reserve_b {
            anyhow::bail!("Liquidez insuficiente en el pool");
        }

        // FIX B: verificar invariante k después del swap (xy=k no debe decrecer)
        let k_before = pool.reserve_rf as u128 * pool.reserve_b as u128;
        let new_reserve_b = pool.reserve_b - amount_out;
        let k_after = new_reserve_rf as u128 * new_reserve_b as u128;
        if k_after < k_before {
            anyhow::bail!("Violación del invariante k: swap inválido");
        }

        pool.reserve_rf     = new_reserve_rf;
        pool.reserve_b      = new_reserve_b;
        pool.volume_rf      += amount_rf_in;
        pool.fees_collected += fee;
        pool.update_price(now);
        self.save_pool(&pool)?;
        self.record_swap(SwapRecord {
            pool_id: pool_id.to_string(),
            direction: SwapDirection::RfToB,
            amount_in: amount_rf_in, amount_out, fee,
            trader: trader.to_string(),
            timestamp: now,
            tx_hash: tx_hash.to_string(),
        })?;
        self.record_price(&pool, now)?;

        println!("🔄 Swap RF→{}: {} RF → {} (fee: {} RF)",
            pool.token_b, amount_rf_in, amount_out, fee);
        Ok(amount_out)
    }

    pub fn execute_swap_b_to_rf(
        &self,
        pool_id: &str,
        trader: &str,
        amount_b_in: u64,
        min_amount_out: u64,
        tx_hash: &str,
        now: u64,
    ) -> Result<u64> {
        let mut pool = self.get_pool(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool no existe"))?;

        if pool.reserve_rf == 0 || pool.reserve_b == 0 {
            anyhow::bail!("Pool sin liquidez");
        }

        if amount_b_in == 0 {
            anyhow::bail!("amount_in no puede ser 0");
        }
        let new_reserve_b = pool.reserve_b.checked_add(amount_b_in)
            .ok_or_else(|| anyhow::anyhow!("Overflow en reserva B"))?;

        let (amount_out, fee) = pool.calc_swap_b_to_rf(amount_b_in);
        if amount_out == 0 {
            anyhow::bail!("Swap produce 0 RF de salida");
        }
        if amount_out < min_amount_out {
            anyhow::bail!("Slippage excedido: esperabas {} mín, recibirías {}", min_amount_out, amount_out);
        }
        if amount_out >= pool.reserve_rf {
            anyhow::bail!("Liquidez RF insuficiente en el pool");
        }

        // FIX B: verificar invariante k
        let k_before = pool.reserve_rf as u128 * pool.reserve_b as u128;
        let new_reserve_rf = pool.reserve_rf - amount_out;
        let k_after  = new_reserve_rf as u128 * new_reserve_b as u128;
        if k_after < k_before {
            anyhow::bail!("Violación del invariante k: swap inválido");
        }

        pool.reserve_b      = new_reserve_b;
        pool.reserve_rf     = new_reserve_rf;
        pool.fees_collected += fee;
        pool.update_price(now);
        self.save_pool(&pool)?;
        self.record_swap(SwapRecord {
            pool_id: pool_id.to_string(),
            direction: SwapDirection::BToRf,
            amount_in: amount_b_in, amount_out, fee,
            trader: trader.to_string(),
            timestamp: now,
            tx_hash: tx_hash.to_string(),
        })?;
        self.record_price(&pool, now)?;

        println!("🔄 Swap {}→RF: {} → {} RF (fee: {})",
            pool.token_b, amount_b_in, amount_out, fee);
        Ok(amount_out)
    }

    // ── Historial ─────────────────────────────────────────────────────────────

    fn record_swap(&self, record: SwapRecord) -> Result<()> {
        let key = format!("{:020}_{}", record.timestamp, &record.pool_id);
        self.swap_history.insert(key, postcard::to_allocvec(&record)?)?;
        Ok(())
    }

    fn record_price(&self, pool: &LiquidityPool, now: u64) -> Result<()> {
        let key = format!("{}_{:020}", pool.pool_id, now);
        self.price_history.insert(key, &pool.price_rf_in_b.to_be_bytes())?;
        Ok(())
    }

    pub fn get_swap_history(&self, pool_id: &str, limit: usize) -> Vec<SwapRecord> {
        self.swap_history.iter().rev()
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| postcard::from_bytes::<SwapRecord>(&b).ok())
            .filter(|s| s.pool_id == pool_id)
            .take(limit)
            .collect()
    }

    pub fn get_price_history(&self, pool_id: &str, limit: usize) -> Vec<(u64, u64)> {
        let prefix = format!("{}_", pool_id);
        self.price_history.scan_prefix(prefix).rev()
            .filter_map(|r| r.ok())
            .take(limit)
            .filter_map(|(k, v)| {
                let key_str = std::str::from_utf8(&k).ok()?;
                let ts: u64 = key_str.split('_').last()?.parse().ok()?;
                let price = v.as_ref().try_into().ok().map(u64::from_be_bytes)?;
                Some((ts, price))
            })
            .collect()
    }

    pub fn get_lp_position(&self, provider: &str, pool_id: &str) -> Option<LpPosition> {
        let key = format!("{}_{}", provider, pool_id);
        self.positions.get(key).ok().flatten()
            .and_then(|b| postcard::from_bytes::<_>(&b).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dex() -> DexState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        DexState::new(
            db.open_tree("pools").unwrap(),
            db.open_tree("pos").unwrap(),
            db.open_tree("swaps").unwrap(),
            db.open_tree("prices").unwrap(),
        )
    }

    fn seeded_pool(dex: &DexState) {
        dex.create_pool("wETH", 1).unwrap();
        dex.add_liquidity("RF_wETH", "lp_provider", 1_000_000, 500_000, 1).unwrap();
    }

    // ── Pool creation ─────────────────────────────────────────────────────────

    #[test]
    fn test_create_pool_once() {
        let dex = make_dex();
        dex.create_pool("wETH", 1).unwrap();
        let result = dex.create_pool("wETH", 2);
        assert!(result.is_err(), "No se puede crear el mismo pool dos veces");
    }

    // ── Add liquidity ─────────────────────────────────────────────────────────

    #[test]
    fn test_add_liquidity_below_minimum() {
        let dex = make_dex();
        dex.create_pool("wETH", 1).unwrap();
        // MIN_LIQUIDITY = 1_000; amounts below threshold must be rejected
        let result = dex.add_liquidity("RF_wETH", "alice", 100, 100, 1);
        assert!(result.is_err(), "Liquidez por debajo del mínimo debe ser rechazada");
    }

    #[test]
    fn test_add_liquidity_increases_reserves() {
        let dex = make_dex();
        seeded_pool(&dex);
        let pool = dex.get_pool("RF_wETH").unwrap();
        assert_eq!(pool.reserve_rf, 1_000_000);
        assert_eq!(pool.reserve_b, 500_000);
        assert!(pool.total_lp > 0);
    }

    // ── AMM invariant k ───────────────────────────────────────────────────────

    #[test]
    fn test_invariant_k_preserved_rf_to_b() {
        let dex = make_dex();
        seeded_pool(&dex);
        let pool_before = dex.get_pool("RF_wETH").unwrap();
        let k_before = pool_before.reserve_rf as u128 * pool_before.reserve_b as u128;

        dex.execute_swap_rf_to_b("RF_wETH", "trader", 10_000, 0, "hash1", 2).unwrap();

        let pool_after = dex.get_pool("RF_wETH").unwrap();
        let k_after = pool_after.reserve_rf as u128 * pool_after.reserve_b as u128;
        // k_after >= k_before (fee makes k slightly bigger, never smaller)
        assert!(k_after >= k_before, "Invariante k debe mantenerse: antes={} después={}", k_before, k_after);
    }

    #[test]
    fn test_invariant_k_preserved_b_to_rf() {
        let dex = make_dex();
        seeded_pool(&dex);
        let pool_before = dex.get_pool("RF_wETH").unwrap();
        let k_before = pool_before.reserve_rf as u128 * pool_before.reserve_b as u128;

        dex.execute_swap_b_to_rf("RF_wETH", "trader", 5_000, 0, "hash2", 2).unwrap();

        let pool_after = dex.get_pool("RF_wETH").unwrap();
        let k_after = pool_after.reserve_rf as u128 * pool_after.reserve_b as u128;
        assert!(k_after >= k_before, "Invariante k debe mantenerse en swap B→RF");
    }

    // ── Slippage protection ───────────────────────────────────────────────────

    #[test]
    fn test_slippage_protection_rf_to_b() {
        let dex = make_dex();
        seeded_pool(&dex);
        // Request min_amount_out higher than what the pool can give
        let result = dex.execute_swap_rf_to_b("RF_wETH", "trader", 10_000, 999_999_999, "hash3", 2);
        assert!(result.is_err(), "Slippage excesivo debe ser rechazado");
        assert!(result.unwrap_err().to_string().contains("Slippage"));
    }

    #[test]
    fn test_slippage_protection_b_to_rf() {
        let dex = make_dex();
        seeded_pool(&dex);
        let result = dex.execute_swap_b_to_rf("RF_wETH", "trader", 5_000, 999_999_999, "hash4", 2);
        assert!(result.is_err(), "Slippage excesivo debe ser rechazado");
    }

    // ── Zero-amount rejection ─────────────────────────────────────────────────

    #[test]
    fn test_zero_amount_swap_rejected() {
        let dex = make_dex();
        seeded_pool(&dex);
        assert!(dex.execute_swap_rf_to_b("RF_wETH", "trader", 0, 0, "hash5", 2).is_err());
        assert!(dex.execute_swap_b_to_rf("RF_wETH", "trader", 0, 0, "hash6", 2).is_err());
    }

    // ── Empty pool protection ─────────────────────────────────────────────────

    #[test]
    fn test_swap_empty_pool_rejected() {
        let dex = make_dex();
        dex.create_pool("wBNB", 1).unwrap();
        // Pool has no liquidity
        assert!(dex.execute_swap_rf_to_b("RF_wBNB", "trader", 1000, 0, "hash7", 2).is_err());
    }

    // ── Remove liquidity ──────────────────────────────────────────────────────

    #[test]
    fn test_remove_liquidity_excess_rejected() {
        let dex = make_dex();
        seeded_pool(&dex);
        let pos = dex.get_lp_position("lp_provider", "RF_wETH").unwrap();
        // Try to remove more LP tokens than held
        let result = dex.remove_liquidity("RF_wETH", "lp_provider", pos.lp_tokens + 1, 2);
        assert!(result.is_err(), "No se puede retirar más LP tokens de los que se poseen");
    }

    #[test]
    fn test_remove_liquidity_from_empty_position() {
        let dex = make_dex();
        seeded_pool(&dex);
        let result = dex.remove_liquidity("RF_wETH", "stranger", 100, 2);
        assert!(result.is_err(), "No se puede retirar liquidez sin posición");
    }

    // ── Calc functions ────────────────────────────────────────────────────────

    #[test]
    fn test_calc_swap_output_nonzero() {
        let pool = LiquidityPool {
            pool_id: "RF_wETH".into(),
            token_b: "wETH".into(),
            reserve_rf: 1_000_000,
            reserve_b: 500_000,
            total_lp: 100,
            price_rf_in_b: 500_000,
            volume_rf: 0,
            fees_collected: 0,
            created_at: 0,
            updated_at: 0,
        };
        let (out, fee) = pool.calc_swap_rf_to_b(10_000);
        assert!(out > 0, "Swap output debe ser > 0");
        assert!(fee > 0, "Fee debe ser > 0");
        // Output must be less than reserve_b
        assert!(out < pool.reserve_b);
    }
}
