/// Oracle de precios on-chain — mediana de submissions de validadores
use serde::{Serialize, Deserialize};
use sled::Tree;

/// Pares soportados
pub const PAIRS: &[&str] = &["RF/USD", "RF/ETH", "RF/BNB", "RF/MATIC"];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceSubmission {
    pub pair: String,
    pub price_usd_micro: u64, // precio * 1_000_000 (6 decimales)
    pub validator: String,
    pub round: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OraclePrice {
    pub pair: String,
    pub price_usd_micro: u64,
    pub submissions: usize,
    pub last_updated_round: u64,
}

pub struct OracleState {
    tree: Tree,
}

impl OracleState {
    pub fn new(tree: Tree) -> Self {
        Self { tree }
    }

    /// Validador envía precio para un par
    pub fn submit_price(
        &self,
        validator: &str,
        pair: &str,
        price_usd_micro: u64,
        round: u64,
    ) -> Result<(), anyhow::Error> {
        if !PAIRS.contains(&pair) {
            anyhow::bail!("Par no soportado: {}. Soportados: {:?}", pair, PAIRS);
        }

        let sub = PriceSubmission {
            pair: pair.to_string(),
            price_usd_micro,
            validator: validator.to_string(),
            round,
        };
        let key = format!("sub:{}:{}", pair, validator);
        self.tree.insert(key, postcard::to_allocvec(&sub)?)?;

        // Recalcular mediana para este par
        self.update_median(pair, round)?;
        Ok(())
    }

    fn update_median(&self, pair: &str, round: u64) -> Result<(), anyhow::Error> {
        let prefix = format!("sub:{}:", pair);
        let mut prices: Vec<u64> = self.tree.scan_prefix(&prefix)
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| postcard::from_bytes::<PriceSubmission>(&b).ok())
            .filter(|s| round.saturating_sub(s.round) <= 50) // solo submissions recientes
            .map(|s| s.price_usd_micro)
            .collect();

        if prices.is_empty() { return Ok(()); }
        prices.sort_unstable();
        let median = prices[prices.len() / 2];

        let oracle_price = OraclePrice {
            pair: pair.to_string(),
            price_usd_micro: median,
            submissions: prices.len(),
            last_updated_round: round,
        };
        self.tree.insert(format!("price:{}", pair), postcard::to_allocvec(&oracle_price)?)?;
        Ok(())
    }

    pub fn get_price(&self, pair: &str) -> Option<OraclePrice> {
        self.tree.get(format!("price:{}", pair)).ok().flatten()
            .and_then(|b| postcard::from_bytes::<OraclePrice>(&b).ok())
    }

    pub fn all_prices(&self) -> Vec<OraclePrice> {
        PAIRS.iter()
            .filter_map(|p| self.get_price(p))
            .collect()
    }
}
