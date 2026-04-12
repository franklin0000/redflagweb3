/// Governance on-chain — propuestas y votación con peso por stake
use serde::{Serialize, Deserialize};
use sled::Tree;

pub const VOTING_PERIOD_ROUNDS: u64 = 100;  // Rondas para votar
pub const QUORUM_PERCENT: u64 = 10;         // Mínimo 10% del stake total debe votar

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum GovernanceParam {
    MinFee,
    HalvingInterval,
    MinStake,
    FeeBurnPercent,
    UnstakeDelay,
    Custom(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Proposal {
    pub id: u64,
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub param: GovernanceParam,
    pub new_value: u64,
    pub votes_for: u64,      // stake-weighted
    pub votes_against: u64,
    pub created_round: u64,
    pub voting_end_round: u64,
    pub executed: bool,
    pub passed: bool,
    pub voters: Vec<String>, // evita doble voto
}

pub struct GovernanceState {
    tree: Tree,
}

impl GovernanceState {
    pub fn new(tree: Tree) -> Self {
        Self { tree }
    }

    fn next_id(&self) -> u64 {
        let id = self.tree.get("__next_id").ok().flatten()
            .and_then(|b| b.as_ref().try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(1);
        self.tree.insert("__next_id", &(id + 1).to_be_bytes()).ok();
        id
    }

    pub fn create_proposal(
        &self,
        proposer: String,
        title: String,
        description: String,
        param: GovernanceParam,
        new_value: u64,
        current_round: u64,
    ) -> Result<u64, anyhow::Error> {
        let id = self.next_id();
        let proposal = Proposal {
            id,
            proposer,
            title,
            description,
            param,
            new_value,
            votes_for: 0,
            votes_against: 0,
            created_round: current_round,
            voting_end_round: current_round + VOTING_PERIOD_ROUNDS,
            executed: false,
            passed: false,
            voters: vec![],
        };
        self.save(&proposal)?;
        println!("📋 Propuesta #{} creada: votación hasta ronda {}", id, proposal.voting_end_round);
        Ok(id)
    }

    pub fn vote(
        &self,
        proposal_id: u64,
        voter: &str,
        vote_for: bool,
        stake_weight: u64,
        current_round: u64,
    ) -> Result<(), anyhow::Error> {
        let mut p = self.get(proposal_id)
            .ok_or_else(|| anyhow::anyhow!("Propuesta #{} no encontrada", proposal_id))?;

        if current_round > p.voting_end_round {
            anyhow::bail!("La votación para la propuesta #{} ya cerró", proposal_id);
        }
        if p.executed {
            anyhow::bail!("La propuesta #{} ya fue ejecutada", proposal_id);
        }
        if p.voters.contains(&voter.to_string()) {
            anyhow::bail!("Ya votaste en la propuesta #{}", proposal_id);
        }
        if stake_weight == 0 {
            anyhow::bail!("Necesitas stake activo para votar");
        }

        p.voters.push(voter.to_string());
        if vote_for {
            p.votes_for = p.votes_for.saturating_add(stake_weight);
        } else {
            p.votes_against = p.votes_against.saturating_add(stake_weight);
        }

        self.save(&p)?;
        println!("🗳️  Voto en propuesta #{}: {} ({} RF stake)",
            proposal_id, if vote_for { "SÍ" } else { "NO" }, stake_weight);
        Ok(())
    }

    /// Evalúa propuestas que han cerrado y las marca como aprobadas/rechazadas
    pub fn finalize_expired(&self, current_round: u64, total_staked: u64) -> Vec<Proposal> {
        let mut finalized = vec![];
        for p in self.list() {
            if !p.executed && current_round > p.voting_end_round {
                let mut p = p;
                let total_votes = p.votes_for + p.votes_against;
                let quorum = total_staked * QUORUM_PERCENT / 100;
                p.passed = total_votes >= quorum && p.votes_for > p.votes_against;
                p.executed = true;
                self.save(&p).ok();
                println!("📊 Propuesta #{} finalizada: {} (SÍ:{} NO:{} quórum:{})",
                    p.id,
                    if p.passed { "APROBADA" } else { "RECHAZADA" },
                    p.votes_for, p.votes_against, quorum
                );
                finalized.push(p);
            }
        }
        finalized
    }

    pub fn get(&self, id: u64) -> Option<Proposal> {
        self.tree.get(format!("prop:{}", id)).ok().flatten()
            .and_then(|b| postcard::from_bytes::<Proposal>(&b).ok())
    }

    pub fn list(&self) -> Vec<Proposal> {
        self.tree.scan_prefix("prop:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, b)| postcard::from_bytes::<Proposal>(&b).ok())
            .collect()
    }

    pub fn active(&self, current_round: u64) -> Vec<Proposal> {
        self.list().into_iter()
            .filter(|p| !p.executed && current_round <= p.voting_end_round)
            .collect()
    }

    fn save(&self, p: &Proposal) -> Result<(), anyhow::Error> {
        let bytes = postcard::to_allocvec(p)?;
        self.tree.insert(format!("prop:{}", p.id), bytes)?;
        Ok(())
    }
}
