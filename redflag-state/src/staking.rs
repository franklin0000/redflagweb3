use serde::{Serialize, Deserialize};
use sled::Tree;

pub const MIN_STAKE: u64 = 10_000;        // RF mínimo para ser validador
pub const UNSTAKE_DELAY_ROUNDS: u64 = 10; // Rondas de espera para retirar stake

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StakeRecord {
    pub address: String,
    pub amount: u64,
    pub since_round: u64,
    /// Si > 0, el validador solicitó salir y puede retirar en `unbonding_at`
    pub unbonding_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StakingStats {
    pub total_staked: u64,
    pub validator_count: usize,
    pub min_stake: u64,
}

pub struct StakingState {
    tree: Tree,
}

impl StakingState {
    pub fn new(tree: Tree) -> Self {
        Self { tree }
    }

    pub fn get_stake(&self, address: &str) -> Option<StakeRecord> {
        self.tree.get(address).ok().flatten()
            .and_then(|b| postcard::from_bytes::<StakeRecord>(&b).ok())
    }

    pub fn stake(&self, address: &str, amount: u64, current_round: u64) -> Result<(), anyhow::Error> {
        if amount < MIN_STAKE {
            anyhow::bail!("Stake mínimo: {} RF (recibido: {})", MIN_STAKE, amount);
        }

        let record = if let Some(mut existing) = self.get_stake(address) {
            existing.amount += amount;
            existing.unbonding_at = 0; // cancelar unbonding si estaba en proceso
            existing
        } else {
            StakeRecord {
                address: address.to_string(),
                amount,
                since_round: current_round,
                unbonding_at: 0,
            }
        };

        self.tree.insert(address, postcard::to_allocvec(&record)?)?;
        println!("🔒 Stake: {} RF bloqueados por {}…", record.amount, &address[..12.min(address.len())]);
        Ok(())
    }

    /// Inicia el proceso de retiro (unbonding). Requiere esperar UNSTAKE_DELAY_ROUNDS.
    pub fn begin_unstake(&self, address: &str, current_round: u64) -> Result<u64, anyhow::Error> {
        let mut record = self.get_stake(address)
            .ok_or_else(|| anyhow::anyhow!("No hay stake para {}", address))?;

        if record.unbonding_at > 0 {
            anyhow::bail!("Ya está en proceso de unbonding hasta la ronda {}", record.unbonding_at);
        }

        let unlock_round = current_round + UNSTAKE_DELAY_ROUNDS;
        record.unbonding_at = unlock_round;
        self.tree.insert(address, postcard::to_allocvec(&record)?)?;
        println!("⏳ Unbonding: {} puede retirar {} RF en ronda {}",
            &address[..12.min(address.len())], record.amount, unlock_round);
        Ok(unlock_round)
    }

    /// Finaliza el retiro si ya pasó el período de unbonding. Devuelve el monto.
    pub fn complete_unstake(&self, address: &str, current_round: u64) -> Result<u64, anyhow::Error> {
        let record = self.get_stake(address)
            .ok_or_else(|| anyhow::anyhow!("No hay stake para {}", address))?;

        if record.unbonding_at == 0 {
            anyhow::bail!("Debes iniciar el unbonding primero con /staking/unstake");
        }
        if current_round < record.unbonding_at {
            anyhow::bail!("Unbonding activo hasta ronda {} (actual: {})", record.unbonding_at, current_round);
        }

        let amount = record.amount;
        self.tree.remove(address)?;
        println!("🔓 Unstake completado: {} RF devueltos a {}…", amount, &address[..12.min(address.len())]);
        Ok(amount)
    }

    /// Distribuye fees acumulados proporcionalmente entre validadores activos
    pub fn distribute_fees(&self, fee_pool: u64) -> Vec<(String, u64)> {
        if fee_pool == 0 { return vec![]; }

        let validators = self.active_validators();
        if validators.is_empty() { return vec![]; }

        let total_staked: u64 = validators.iter().map(|v| v.amount).sum();
        if total_staked == 0 { return vec![]; }

        let mut rewards = Vec::new();
        for v in &validators {
            let share = (fee_pool as u128 * v.amount as u128 / total_staked as u128) as u64;
            if share > 0 {
                rewards.push((v.address.clone(), share));
            }
        }

        println!("💰 Distribuyendo {} RF entre {} validadores", fee_pool, validators.len());
        rewards
    }

    /// Validadores activos (stake >= MIN_STAKE y no en unbonding)
    pub fn active_validators(&self) -> Vec<StakeRecord> {
        self.tree.iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| postcard::from_bytes::<StakeRecord>(&b).ok())
            .filter(|r| r.amount >= MIN_STAKE && r.unbonding_at == 0)
            .collect()
    }

    pub fn stats(&self) -> StakingStats {
        let validators = self.active_validators();
        let total_staked = validators.iter().map(|v| v.amount).sum();
        StakingStats {
            total_staked,
            validator_count: validators.len(),
            min_stake: MIN_STAKE,
        }
    }
}
