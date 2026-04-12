/// RedFlag Multi-Token Balances
/// Wrapped tokens (wETH, wBNB, wMATIC) minted by the bridge
/// when users lock real assets on EVM chains.
///
/// Key: "{address}:{token}" → u64 balance
/// Example: "0xabc...:wETH" → 1_500_000 (in smallest unit, 6 decimals)

use sled::Tree;
use anyhow::Result;
use serde::{Serialize, Deserialize};

/// Tokens soportados en RedFlag (todos son wrapped, respaldados 1:1 por activos bloqueados en EVM)
pub const SUPPORTED_TOKENS: &[&str] = &["wETH", "wBNB", "wMATIC", "wUSDC", "wUSDT", "wSOL", "wAVAX", "wARB", "wBTC"];

/// Precisión: 6 decimales para todos los wrapped tokens (1 wETH = 1_000_000 units)
pub const TOKEN_DECIMALS: u64 = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub address: String,
    pub token:   String,
    pub balance: u64,
}

pub struct TokenLedger {
    balances: Tree, // "{address}:{token}" → u64 BE
}

impl TokenLedger {
    pub fn new(balances: Tree) -> Self {
        Self { balances }
    }

    fn key(address: &str, token: &str) -> String {
        format!("{}:{}", address, token)
    }

    pub fn get_balance(&self, address: &str, token: &str) -> u64 {
        self.balances.get(Self::key(address, token))
            .ok().flatten()
            .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(0)
    }

    pub fn set_balance(&self, address: &str, token: &str, balance: u64) -> Result<()> {
        self.balances.insert(Self::key(address, token), &balance.to_be_bytes())?;
        Ok(())
    }

    /// Supply máximo por token (evita mint infinito por bug o exploit del bridge)
    /// 1 billón de unidades = 1,000,000 tokens con 6 decimales
    pub const MAX_TOKEN_SUPPLY: u64 = 1_000_000_000_000_000;

    pub fn credit(&self, address: &str, token: &str, amount: u64) -> Result<u64> {
        if amount == 0 {
            anyhow::bail!("No se puede acreditar 0 tokens");
        }
        let cur = self.get_balance(address, token);
        // FIX D: usar checked_add para evitar overflow y validar supply máximo
        let new_bal = cur.checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("Overflow en balance de {} para {}", token, &address[..12.min(address.len())]))?;
        if new_bal > Self::MAX_TOKEN_SUPPLY {
            anyhow::bail!("Excede supply máximo de {} ({} unidades)", token, Self::MAX_TOKEN_SUPPLY);
        }
        self.set_balance(address, token, new_bal)?;
        Ok(new_bal)
    }

    pub fn debit(&self, address: &str, token: &str, amount: u64) -> Result<u64> {
        let cur = self.get_balance(address, token);
        if cur < amount {
            anyhow::bail!("Saldo insuficiente de {}: tienes {}, necesitas {}", token, cur, amount);
        }
        let new_bal = cur - amount;
        self.set_balance(address, token, new_bal)?;
        Ok(new_bal)
    }

    /// Obtiene todos los balances de tokens de una dirección
    pub fn get_all_balances(&self, address: &str) -> Vec<TokenBalance> {
        SUPPORTED_TOKENS.iter().filter_map(|&token| {
            let bal = self.get_balance(address, token);
            if bal > 0 {
                Some(TokenBalance { address: address.to_string(), token: token.to_string(), balance: bal })
            } else { None }
        }).collect()
    }

    /// Mint: el bridge emite tokens cuando detecta un lock en EVM
    /// amount_wei: cantidad en wei (18 decimales) → se convierte a units (6 decimales)
    pub fn mint_from_bridge(&self, to: &str, token: &str, amount_wei: u64) -> Result<u64> {
        // 1 wETH wei (18 dec) → 1 unit (6 dec) = divide by 1e12
        let amount_units = amount_wei / 1_000_000_000_000;
        if amount_units == 0 {
            anyhow::bail!("Monto demasiado pequeño para mintear");
        }
        let new_bal = self.credit(to, token, amount_units)?;
        println!("🪙 Mint {}: {} units → {} (total: {})", token, amount_units, &to[..12.min(to.len())], new_bal);
        Ok(amount_units)
    }

    /// Burn: el bridge quema tokens cuando el usuario hace bridge de vuelta a EVM
    /// Retorna el amount en wei para liberar en EVM
    pub fn burn_for_bridge(&self, from: &str, token: &str, amount_units: u64) -> Result<u64> {
        self.debit(from, token, amount_units)?;
        let amount_wei = amount_units * 1_000_000_000_000;
        println!("🔥 Burn {}: {} units from {} → {} wei", token, amount_units, &from[..12.min(from.len())], amount_wei);
        Ok(amount_wei)
    }

    /// Pool DEX: el contrato DEX también tiene balances (las reservas)
    pub fn pool_credit(&self, pool_id: &str, token: &str, amount: u64) -> Result<()> {
        self.credit(pool_id, token, amount)?;
        Ok(())
    }

    pub fn pool_debit(&self, pool_id: &str, token: &str, amount: u64) -> Result<()> {
        self.debit(pool_id, token, amount)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ledger() -> TokenLedger {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let tree = db.open_tree("test_balances").unwrap();
        TokenLedger::new(tree)
    }

    // ── Supply cap ────────────────────────────────────────────────────────────

    #[test]
    fn test_supply_cap_enforced() {
        let ledger = make_ledger();
        // Credit up to the cap
        ledger.set_balance("alice", "wETH", TokenLedger::MAX_TOKEN_SUPPLY - 1).unwrap();
        // Adding 2 more should exceed the cap
        let result = ledger.credit("alice", "wETH", 2);
        assert!(result.is_err(), "Debe rechazar balance por encima del supply máximo");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("supply máximo"), "Error debe mencionar supply máximo");
    }

    #[test]
    fn test_supply_cap_exact_boundary() {
        let ledger = make_ledger();
        ledger.set_balance("alice", "wETH", TokenLedger::MAX_TOKEN_SUPPLY - 100).unwrap();
        // Exactly reaching the cap is allowed
        assert!(ledger.credit("alice", "wETH", 100).is_ok());
        // One more unit must fail
        assert!(ledger.credit("alice", "wETH", 1).is_err());
    }

    // ── Overflow protection ───────────────────────────────────────────────────

    #[test]
    fn test_credit_overflow_protection() {
        let ledger = make_ledger();
        ledger.set_balance("bob", "wBNB", u64::MAX - 10).unwrap();
        let result = ledger.credit("bob", "wBNB", 100);
        assert!(result.is_err(), "checked_add debe detectar overflow aritmético");
    }

    #[test]
    fn test_credit_zero_rejected() {
        let ledger = make_ledger();
        assert!(ledger.credit("alice", "wETH", 0).is_err(), "credit(0) debe ser rechazado");
    }

    // ── Debit: insufficient balance ────────────────────────────────────────────

    #[test]
    fn test_debit_insufficient_balance() {
        let ledger = make_ledger();
        ledger.set_balance("alice", "wMATIC", 500).unwrap();
        let result = ledger.debit("alice", "wMATIC", 501);
        assert!(result.is_err(), "Debe rechazar debit que excede el balance");
        assert_eq!(ledger.get_balance("alice", "wMATIC"), 500, "Balance no debe cambiar");
    }

    #[test]
    fn test_debit_exact_balance() {
        let ledger = make_ledger();
        ledger.set_balance("alice", "wUSDC", 1000).unwrap();
        assert!(ledger.debit("alice", "wUSDC", 1000).is_ok());
        assert_eq!(ledger.get_balance("alice", "wUSDC"), 0);
    }

    // ── Mint / Burn ────────────────────────────────────────────────────────────

    #[test]
    fn test_mint_too_small_rejected() {
        let ledger = make_ledger();
        // amount_wei < 1e12 → amount_units == 0 → rejected
        let result = ledger.mint_from_bridge("alice", "wETH", 999_999_999_999);
        assert!(result.is_err(), "Montos demasiado pequeños deben ser rechazados");
    }

    #[test]
    fn test_mint_burn_roundtrip() {
        let ledger = make_ledger();
        let wei = 2_000_000_000_000u64; // 2e12 → 2 units
        let units = ledger.mint_from_bridge("alice", "wETH", wei).unwrap();
        assert_eq!(units, 2);
        assert_eq!(ledger.get_balance("alice", "wETH"), 2);
        let back_wei = ledger.burn_for_bridge("alice", "wETH", 2).unwrap();
        assert_eq!(back_wei, 2_000_000_000_000);
        assert_eq!(ledger.get_balance("alice", "wETH"), 0);
    }

    // ── Isolation: different tokens, different slots ──────────────────────────

    #[test]
    fn test_token_isolation() {
        let ledger = make_ledger();
        ledger.set_balance("alice", "wETH", 1000).unwrap();
        ledger.set_balance("alice", "wBNB", 500).unwrap();
        assert_eq!(ledger.get_balance("alice", "wETH"), 1000);
        assert_eq!(ledger.get_balance("alice", "wBNB"), 500);
        // Debiting wBNB must not touch wETH
        ledger.debit("alice", "wBNB", 200).unwrap();
        assert_eq!(ledger.get_balance("alice", "wETH"), 1000);
        assert_eq!(ledger.get_balance("alice", "wBNB"), 300);
    }
}
