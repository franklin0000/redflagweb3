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
            .and_then(|b| bincode::deserialize::<Account>(&b).ok())
    }

    /// Historial para una dirección específica
    pub fn get_history(&self, address: &str) -> Vec<Transaction> {
        let mut history = Vec::new();
        for item in self.tx_history.iter() {
            if let Ok((_, bytes)) = item {
                if let Ok(tx) = bincode::deserialize::<Transaction>(&bytes) {
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
            .and_then(|b| bincode::deserialize::<Transaction>(&b).ok())
    }

    /// Últimas N transacciones globales (para dashboard)
    pub fn get_recent_txs(&self, limit: usize) -> Vec<Transaction> {
        let mut txs: Vec<Transaction> = self.tx_history.iter().rev()
            .take(limit)
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| bincode::deserialize::<Transaction>(&b).ok())
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

        // ── FIX #1: Verificar firma ML-DSA de TODA TX antes de ejecutar ─────────
        if !is_genesis && tx.sender != redflag_core::STAKE_ADDRESS && tx.sender != redflag_core::FEE_POOL_ADDRESS {
            let pubkey_bytes = hex::decode(&tx.sender).unwrap_or_default();
            let mut tx_for_verify = tx.clone();
            let signature = std::mem::take(&mut tx_for_verify.signature);
            let msg = bincode::serialize(&tx_for_verify).unwrap_or_default();
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
            // Guardar stake en árbol separado: sender → amount staked
            let stake_tree = self.db.open_tree("stakes").unwrap_or_else(|_| self.db.open_tree("stakes").unwrap());
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
                if let Ok(abi) = bincode::deserialize::<redflag_vm::ContractAbi>(&tx.data) {
                    let _ = vm.deploy(tx.data.clone(), &tx.sender, tx.nonce, 0, vec![], abi);
                }
            } else if !tx.data.is_empty() {
                let _ = vm.call(&tx.receiver, "main", tx.data.clone(), &tx.sender, 0, 1_000_000);
            }
        }

        // Guardar en historial (por dirección) + índice global (por hash)
        let tx_bytes = bincode::serialize(tx)?;
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
        self.db.insert(&account.address, bincode::serialize(account)?)?;
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
