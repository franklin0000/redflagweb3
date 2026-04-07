use serde::{Serialize, Deserialize};
use sled::{Db, Tree};
use redflag_core::{Transaction, CHAIN_ID, GENESIS_ADDRESS, FEE_POOL_ADDRESS, GENESIS_BALANCE};
use redflag_crypto::Verifier;
use rayon::prelude::*;
use redflag_vm::ContractVm;

pub mod dex;
pub use dex::{DexState, LiquidityPool, SwapRecord, LpPosition, DEX_FEE_ADDRESS};

pub mod tokens;
pub use tokens::{TokenLedger, TokenBalance, SUPPORTED_TOKENS, TOKEN_DECIMALS};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
}

pub struct StateDB {
    db: Db,
    tx_history: Tree,   // key = timestamp_sender → TX bytes (per-address lookup)
    tx_index: Tree,     // key = tx_hash → TX bytes  (global lookup by hash)
    tx_counter: Tree,   // key = "total" → u64
    pub vm: Option<ContractVm>,
    pub dex: DexState,
    pub tokens: TokenLedger,
}

impl StateDB {
    pub fn new(path: &str) -> Result<Self, anyhow::Error> {
        let db = sled::open(path)?;
        let tx_history  = db.open_tree("tx_history")?;
        let tx_index    = db.open_tree("tx_index")?;
        let tx_counter  = db.open_tree("tx_counter")?;

        // DEX trees
        let dex_pools    = db.open_tree("dex_pools")?;
        let dex_pos      = db.open_tree("dex_positions")?;
        let dex_swaps    = db.open_tree("dex_swaps")?;
        let dex_prices   = db.open_tree("dex_prices")?;
        let dex = DexState::new(dex_pools, dex_pos, dex_swaps, dex_prices);

        // Multi-token ledger (wETH, wBNB, wMATIC, etc.)
        let token_balances = db.open_tree("token_balances")?;
        let tokens = TokenLedger::new(token_balances);

        let vm_path = format!("{}_vm", path);
        let vm = ContractVm::new(&vm_path).ok();

        let state = Self { db, tx_history, tx_index, tx_counter, vm, dex, tokens };
        state.ensure_genesis()?;
        state.ensure_dex_pools()?;
        Ok(state)
    }

    fn ensure_dex_pools(&self) -> Result<(), anyhow::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        // Crear los pools principales si no existen
        for token in &["wETH", "wBNB", "wMATIC"] {
            let pool_id = format!("RF_{}", token);
            if self.dex.get_pool(&pool_id).is_none() {
                self.dex.create_pool(token, now)?;
            }
        }
        Ok(())
    }

    fn ensure_genesis(&self) -> Result<(), anyhow::Error> {
        if self.get_account(GENESIS_ADDRESS).is_none() {
            self.save_account(&Account { address: GENESIS_ADDRESS.into(), balance: GENESIS_BALANCE, nonce: 0 })?;
            self.save_account(&Account { address: FEE_POOL_ADDRESS.into(), balance: 0, nonce: 0 })?;
            println!("🌱 Genesis: {} RF en {}", GENESIS_BALANCE, GENESIS_ADDRESS);
        }
        Ok(())
    }

    pub fn ensure_faucet(&self, address: &str, amount: u64) -> Result<(), anyhow::Error> {
        if self.get_account(address).is_none() {
            self.save_account(&Account { address: address.into(), balance: amount, nonce: 0 })?;
            self.db.flush()?;
            println!("💧 Faucet: {} RF en {}…", amount, &address[..16.min(address.len())]);
        }
        Ok(())
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.get_account(address).map(|a| a.balance).unwrap_or(0)
    }

    pub fn get_account(&self, address: &str) -> Option<Account> {
        self.db.get(address).ok().flatten()
            .and_then(|b| bincode::serde::decode_from_slice::<Account, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
    }

    /// Historial para una dirección específica
    pub fn get_history(&self, address: &str) -> Vec<Transaction> {
        let mut history = Vec::new();
        for item in self.tx_history.iter() {
            if let Ok((_, bytes)) = item {
                if let Ok(tx) = bincode::serde::decode_from_slice::<Transaction, _>(&bytes, bincode::config::standard()).map(|(v, _)| v) {
                    if tx.sender == address || tx.receiver == address {
                        history.push(tx);
                    }
                }
            }
        }
        history.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        history
    }

    /// Busca una TX por hash (índice global)
    pub fn get_tx_by_hash(&self, hash_hex: &str) -> Option<Transaction> {
        let hash_bytes = hex::decode(hash_hex).ok()?;
        self.tx_index.get(&hash_bytes).ok().flatten()
            .and_then(|b| bincode::serde::decode_from_slice::<Transaction, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
    }

    /// Últimas N transacciones globales (para dashboard)
    pub fn get_recent_txs(&self, limit: usize) -> Vec<Transaction> {
        let mut txs: Vec<Transaction> = self.tx_history.iter().rev()
            .take(limit)
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| bincode::serde::decode_from_slice::<Transaction, _>(&b, bincode::config::standard()).map(|(v, _)| v).ok())
            .collect();
        txs.sort_by(|a,b| b.timestamp.cmp(&a.timestamp));
        txs
    }

    /// Aplica transacciones con ejecución paralela por grupos no-conflictivos
    pub fn apply_transactions(&self, txs: &[Transaction]) -> Result<(), anyhow::Error> {
        let groups = Transaction::parallel_groups(txs.to_vec());
        if groups.len() > 1 {
            println!("⚡ Paralelo: {} TXs → {} grupos", txs.len(), groups.len());
        }

        let errors: Vec<_> = groups.into_par_iter()
            .flat_map(|group| {
                group.into_iter().filter_map(|tx| {
                    self.apply_single(&tx).err().map(|e| e.to_string())
                }).collect::<Vec<_>>()
            })
            .collect();

        for e in &errors { eprintln!("⚠️  TX error: {}", e); }
        self.db.flush()?;
        Ok(())
    }

    fn apply_single(&self, tx: &Transaction) -> Result<(), anyhow::Error> {
        if tx.chain_id != CHAIN_ID && tx.sender != GENESIS_ADDRESS {
            anyhow::bail!("chain_id inválido: {}", tx.chain_id);
        }

        let is_genesis = tx.sender == GENESIS_ADDRESS;

        // FIX G: Validar campos básicos de la TX
        if !is_genesis {
            if tx.fee < redflag_core::MIN_FEE {
                anyhow::bail!("Fee {} < mínimo {}", tx.fee, redflag_core::MIN_FEE);
            }
            if tx.sender == tx.receiver {
                anyhow::bail!("Sender y receiver no pueden ser la misma dirección");
            }
            if tx.sender.is_empty() || tx.receiver.is_empty() {
                anyhow::bail!("Sender o receiver vacío");
            }
        }

        // ── FIX #1: Verificar firma ML-DSA de TODA TX antes de ejecutar ─────────
        if !is_genesis && tx.sender != redflag_core::STAKE_ADDRESS && tx.sender != redflag_core::FEE_POOL_ADDRESS {
            let pubkey_bytes = hex::decode(&tx.sender).unwrap_or_default();
            let mut tx_for_verify = tx.clone();
            let signature = std::mem::take(&mut tx_for_verify.signature);
            let msg = bincode::serde::encode_to_vec(&tx_for_verify, bincode::config::standard()).unwrap_or_default();
            if Verifier::verify(&pubkey_bytes, &msg, &signature).is_err() {
                anyhow::bail!("Firma ML-DSA inválida para TX de {}", &tx.sender[..16.min(tx.sender.len())]);
            }
        }

        let mut sender = self.get_account(&tx.sender).unwrap_or(Account {
            address: tx.sender.clone(), balance: 0, nonce: 0,
        });

        // Replay protection
        if !is_genesis && tx.nonce != sender.nonce {
            anyhow::bail!("nonce inválido para {}…: got {} expected {}",
                &tx.sender[..8.min(tx.sender.len())], tx.nonce, sender.nonce);
        }

        // Balance check
        let total_cost = tx.amount.saturating_add(tx.fee);
        if !is_genesis && sender.balance < total_cost {
            anyhow::bail!("saldo insuficiente: tiene {} RF, necesita {}",
                sender.balance, total_cost);
        }

        let mut receiver = self.get_account(&tx.receiver).unwrap_or(Account {
            address: tx.receiver.clone(), balance: 0, nonce: 0,
        });

        if !is_genesis { sender.balance -= total_cost; }
        receiver.balance = receiver.balance.saturating_add(tx.amount);
        sender.nonce += 1;

        // Fee pool
        if tx.fee > 0 {
            let mut pool = self.get_account(FEE_POOL_ADDRESS).unwrap_or(Account {
                address: FEE_POOL_ADDRESS.into(), balance: 0, nonce: 0,
            });
            pool.balance = pool.balance.saturating_add(tx.fee);
            self.save_account(&pool)?;
        }

        self.save_account(&sender)?;
        self.save_account(&receiver)?;

        // ── Staking: si envías RF a STAKE_ADDRESS te registras como validador ──
        if tx.receiver == redflag_core::STAKE_ADDRESS && tx.amount >= redflag_core::MIN_STAKE {
            // FIX F: usar ? en vez de unwrap para no paniquear el nodo
            let stake_tree = self.db.open_tree("stakes")
                .map_err(|e| anyhow::anyhow!("Error abriendo árbol de stakes: {}", e))?;
            let prev_stake = stake_tree.get(&tx.sender).ok().flatten()
                .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
                .unwrap_or(0);
            let new_stake = prev_stake + tx.amount;
            let _ = stake_tree.insert(tx.sender.as_bytes(), &new_stake.to_be_bytes());
            println!("🗳️  Stake: {} RF stakeados por {}… (total: {} RF)",
                tx.amount, &tx.sender[..12.min(tx.sender.len())], new_stake);
        }

        // Smart contracts
        if let Some(vm) = &self.vm {
            if tx.receiver == "DEPLOY" && !tx.data.is_empty() {
                if let Ok(abi) = bincode::serde::decode_from_slice::<redflag_vm::ContractAbi, _>(&tx.data, bincode::config::standard()).map(|(v, _)| v) {
                    let _ = vm.deploy(tx.data.clone(), &tx.sender, tx.nonce, 0, vec![], abi);
                }
            } else if !tx.data.is_empty() {
                let _ = vm.call(&tx.receiver, "main", tx.data.clone(), &tx.sender, 0, 1_000_000);
            }
        }

        // Guardar en historial (por dirección) + índice global (por hash)
        let tx_bytes = bincode::serde::encode_to_vec(tx, bincode::config::standard())?;
        let tx_hash = blake3::hash(&tx_bytes);

        let history_key = format!("{:020}_{}_{}",
            tx.timestamp,
            &tx.sender[..8.min(tx.sender.len())],
            hex::encode(&tx_hash.as_bytes()[..4]),
        );
        self.tx_history.insert(&history_key, tx_bytes.as_slice())?;
        self.tx_index.insert(tx_hash.as_bytes(), tx_bytes.as_slice())?;

        // Incrementar contador global
        let prev = self.tx_counter.get("total").ok().flatten()
            .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(0);
        self.tx_counter.insert("total", &(prev + 1).to_be_bytes())?;

        println!("✅ TX {} → {} | {} RF | fee:{} | nonce:{}",
            &tx.sender[..8.min(tx.sender.len())],
            &tx.receiver[..8.min(tx.receiver.len())],
            tx.amount, tx.fee, tx.nonce,
        );
        Ok(())
    }

    fn save_account(&self, account: &Account) -> Result<(), anyhow::Error> {
        self.db.insert(&account.address, bincode::serde::encode_to_vec(account, bincode::config::standard())?)?;
        Ok(())
    }

    /// Public version for DEX/bridge balance updates
    pub fn save_account_pub(&self, account: &Account) -> Result<(), anyhow::Error> {
        self.save_account(account)
    }

    /// Lista de stakers: (address, amount_staked)
    pub fn get_stakes(&self) -> Vec<(String, u64)> {
        let tree = match self.db.open_tree("stakes") {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        tree.iter()
            .flatten()
            .map(|(k, v)| {
                let addr = String::from_utf8_lossy(&k).to_string();
                let amount = v.as_ref().try_into().ok().map(u64::from_be_bytes).unwrap_or(0);
                (addr, amount)
            })
            .collect()
    }

    pub fn stats(&self) -> StateStats {
        // account_count: solo las cuentas reales (no el historial ni metadatos)
        let account_count = self.db.len();
        let tx_count = self.tx_counter.get("total").ok().flatten()
            .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(0) as usize;
        let fee_pool = self.get_balance(FEE_POOL_ADDRESS);
        StateStats { account_count, tx_count, fee_pool_balance: fee_pool }
    }
}

#[derive(Debug, Serialize)]
pub struct StateStats {
    pub account_count: usize,
    pub tx_count: usize,
    pub fee_pool_balance: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use redflag_core::{Transaction, CHAIN_ID, GENESIS_ADDRESS, MIN_FEE};
    use redflag_crypto::SigningKeyPair;

    fn make_state() -> StateDB {
        let tmp = format!("/tmp/rf_test_state_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        StateDB::new(&tmp).expect("StateDB::new failed in test")
    }

    /// Genera un par de llaves ML-DSA y devuelve (hex_pubkey, keypair)
    fn gen_account() -> (String, SigningKeyPair) {
        let kp = SigningKeyPair::generate().unwrap();
        let pubkey_hex = hex::encode(kp.public_key());
        (pubkey_hex, kp)
    }

    /// Crea y firma una Transaction normal (no-genesis)
    fn signed_tx(
        keypair: &SigningKeyPair,
        sender_hex: &str,
        receiver_hex: &str,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let mut tx = Transaction {
            sender:    sender_hex.to_string(),
            receiver:  receiver_hex.to_string(),
            amount,
            fee,
            nonce,
            chain_id: CHAIN_ID,
            read_set:  vec![sender_hex.to_string(), receiver_hex.to_string()],
            write_set: vec![sender_hex.to_string(), receiver_hex.to_string()],
            data: vec![],
            signature: vec![],
            timestamp: 0,
        };
        let msg = bincode::serde::encode_to_vec(&tx, bincode::config::standard()).unwrap();
        tx.signature = keypair.sign(&msg).unwrap();
        tx
    }

    // ── Signature verification ─────────────────────────────────────────────

    #[test]
    fn test_valid_signature_accepted() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        // Fund alice from genesis
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 0);
        assert!(state.apply_transactions(&[tx]).is_ok());
        assert_eq!(state.get_balance(&bob_hex), 100);
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let state = make_state();
        let (alice_hex, _alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        // Use a different keypair to sign — should be rejected
        let (_eve_hex, eve_kp) = gen_account();
        let tx = signed_tx(&eve_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 0);
        // apply_transactions swallows errors internally; check balance unchanged
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "TX con firma inválida no debe mover fondos");
    }

    #[test]
    fn test_empty_signature_rejected() {
        let state = make_state();
        let (alice_hex, _) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        let mut tx = Transaction {
            sender: alice_hex.clone(), receiver: bob_hex.clone(),
            amount: 100, fee: MIN_FEE, nonce: 0, chain_id: CHAIN_ID,
            read_set: vec![], write_set: vec![], data: vec![],
            signature: vec![],   // empty — must fail
            timestamp: 0,
        };
        tx.signature = vec![];
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "Firma vacía no debe ser aceptada");
    }

    // ── Replay / nonce protection ──────────────────────────────────────────

    #[test]
    fn test_replay_attack_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 0);

        state.apply_transactions(&[tx.clone()]).unwrap();
        let balance_after_first = state.get_balance(&bob_hex);

        // Replay the exact same TX (nonce 0 again)
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), balance_after_first,
            "TX repetida con mismo nonce debe ser rechazada (replay protection)");
    }

    #[test]
    fn test_nonce_out_of_order_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        // nonce=5 when account nonce is 0 → rejected
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 5);
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "Nonce incorrecto debe ser rechazado");
    }

    // ── Double-spend / balance check ───────────────────────────────────────

    #[test]
    fn test_double_spend_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 500);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        // Alice only has 500; try to send 600 (+ fee)
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 600, MIN_FEE, 0);
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "Gasto doble debe ser rechazado por saldo insuficiente");
        assert_eq!(state.get_balance(&alice_hex), 500, "Alice no debe perder fondos en TX inválida");
    }

    // ── Fee validation ─────────────────────────────────────────────────────

    #[test]
    fn test_fee_below_minimum_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        // fee=0 < MIN_FEE=1
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, 0, 0);
        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "Fee < MIN_FEE debe ser rechazado");
    }

    // ── Sender == receiver ─────────────────────────────────────────────────

    #[test]
    fn test_self_transfer_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let tx = signed_tx(&alice_kp, &alice_hex, &alice_hex, 100, MIN_FEE, 0);
        state.apply_transactions(&[tx]).unwrap();
        // Balance stays unchanged (no net movement on a valid self-tx would round-trip,
        // but the rule is rejected outright)
        assert_eq!(state.get_balance(&alice_hex), 1_000_000,
            "Transferencia a sí mismo debe ser rechazada");
    }

    // ── Wrong chain_id ─────────────────────────────────────────────────────

    #[test]
    fn test_wrong_chain_id_rejected() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let (bob_hex, _) = gen_account();
        let mut tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 0);
        tx.chain_id = 9999; // wrong chain
        let msg = bincode::serde::encode_to_vec(&tx, bincode::config::standard()).unwrap();
        tx.signature = alice_kp.sign(&msg).unwrap();

        state.apply_transactions(&[tx]).unwrap();
        assert_eq!(state.get_balance(&bob_hex), 0, "chain_id incorrecto debe ser rechazado");
    }

    // ── Fee pool accumulation ──────────────────────────────────────────────

    #[test]
    fn test_fee_goes_to_pool() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let fee_before = state.get_balance(redflag_core::FEE_POOL_ADDRESS);
        let (bob_hex, _) = gen_account();
        let fee = 10;
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, fee, 0);
        state.apply_transactions(&[tx]).unwrap();

        assert_eq!(state.get_balance(redflag_core::FEE_POOL_ADDRESS), fee_before + fee,
            "Fee debe acumularse en FEE_POOL_ADDRESS");
    }

    // ── Staking ────────────────────────────────────────────────────────────

    #[test]
    fn test_staking_registers_validator() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000_000_000); // 1T RF
        state.apply_transactions(&[fund]).unwrap();

        let stake_amount = redflag_core::MIN_STAKE;
        let tx = signed_tx(&alice_kp, &alice_hex, redflag_core::STAKE_ADDRESS, stake_amount, MIN_FEE, 0);
        state.apply_transactions(&[tx]).unwrap();

        let stakes = state.get_stakes();
        let alice_stake = stakes.iter().find(|(addr, _)| addr == &alice_hex);
        assert!(alice_stake.is_some(), "Alice debe aparecer en la lista de stakers");
        assert_eq!(alice_stake.unwrap().1, stake_amount);
    }

    // ── Genesis is idempotent ──────────────────────────────────────────────

    #[test]
    fn test_genesis_not_duplicated() {
        // Opening same path twice should not double the genesis balance
        let path = format!("/tmp/rf_genesis_test_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        let state1 = StateDB::new(&path).unwrap();
        let bal1 = state1.get_balance(GENESIS_ADDRESS);
        drop(state1);
        let state2 = StateDB::new(&path).unwrap();
        let bal2 = state2.get_balance(GENESIS_ADDRESS);
        assert_eq!(bal1, bal2, "Genesis no debe duplicarse al abrir la DB dos veces");
    }

    // ── Stats counter ──────────────────────────────────────────────────────

    #[test]
    fn test_tx_counter_increments() {
        let state = make_state();
        let (alice_hex, alice_kp) = gen_account();
        let fund = Transaction::genesis(alice_hex.clone(), 1_000_000);
        state.apply_transactions(&[fund]).unwrap();

        let stats_before = state.stats();
        let (bob_hex, _) = gen_account();
        let tx = signed_tx(&alice_kp, &alice_hex, &bob_hex, 100, MIN_FEE, 0);
        state.apply_transactions(&[tx]).unwrap();
        let stats_after = state.stats();

        assert!(stats_after.tx_count > stats_before.tx_count,
            "El contador de TXs debe incrementar después de cada TX válida");
    }
}
