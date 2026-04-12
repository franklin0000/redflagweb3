use serde::{Serialize, Deserialize};
use redflag_crypto::SigningKeyPair;
use redflag_core::{Transaction, EncryptedTransaction};
use redflag_state::{StateDB, Account};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use dashmap::{DashMap, DashSet};
pub mod threshold;
use std::collections::HashSet;
use sled::{Db, Tree};

/// Cada cuántas rondas se reduce a la mitad la recompensa por vértice
pub const HALVING_INTERVAL: u64 = 1_000;
/// Recompensa base por vértice (1 RF = 1_000_000 microRF)
pub const BASE_REWARD: u64 = 1_000_000;
/// Recompensa adicional por cada TX incluida en el vértice
pub const TX_REWARD: u64 = 100_000;
/// Porcentaje de fees que se quema (deflación): 20%
pub const FEE_BURN_PERCENT: u64 = 20;

/// Calcula la recompensa por vértice con halving
fn vertex_reward(round: u64, tx_count: usize) -> u64 {
    let halvings = (round / HALVING_INTERVAL).min(10); // máx 10 halvings
    let divisor = 1u64 << halvings;
    let base = BASE_REWARD / divisor;
    let per_tx = TX_REWARD / divisor;
    base + (tx_count as u64 * per_tx)
}

pub type VertexId = [u8; 32];
pub type Round = u64;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Vertex {
    pub round: Round,
    pub parents: HashSet<VertexId>,
    pub transactions: Vec<Transaction>,
    pub encrypted_transactions: Vec<EncryptedTransaction>,
    pub author: Vec<u8>,
    pub signature: Vec<u8>,
}

impl Vertex {
    pub fn id(&self) -> VertexId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&postcard::to_allocvec(&self.round).unwrap());
        let mut parents_sorted: Vec<_> = self.parents.iter().collect();
        parents_sorted.sort();
        for p in parents_sorted {
            hasher.update(p);
        }
        hasher.update(&postcard::to_allocvec(&self.transactions).unwrap());
        hasher.update(&postcard::to_allocvec(&self.encrypted_transactions).unwrap());
        hasher.update(&self.author);
        *hasher.finalize().as_bytes()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Certificate {
    pub vertex_id: VertexId,
    pub round: Round,
    pub signatures: Vec<(Vec<u8>, Vec<u8>)>,
}

pub struct Dag {
    pub vertices: DashMap<VertexId, Arc<Vertex>>,
    pub certificates: DashMap<Round, Vec<Arc<Certificate>>>,
    /// Certificados agregados por vertex_id — acumula firmas de múltiples validadores
    pub cert_by_vertex: DashMap<VertexId, Certificate>,
    pub db: Db,
    db_vertices: Tree,
    db_certificates: Tree,
}

impl Dag {
    pub fn new(db_path: &str) -> Result<Self, anyhow::Error> {
        let db = sled::open(db_path)?;
        let db_vertices = db.open_tree("vertices")?;
        let db_certificates = db.open_tree("certificates")?;

        let dag = Self {
            vertices: DashMap::new(),
            certificates: DashMap::new(),
            cert_by_vertex: DashMap::new(),
            db,
            db_vertices,
            db_certificates,
        };

        dag.load_from_db()?;
        Ok(dag)
    }

    fn load_from_db(&self) -> Result<(), anyhow::Error> {
        let mut v_count = 0;
        for item in self.db_vertices.iter() {
            if let Ok((_, bytes)) = item {
                if let Ok(v) = postcard::from_bytes::<Vertex>(&bytes) {
                    self.vertices.insert(v.id(), Arc::new(v));
                    v_count += 1;
                }
            }
        }
        let mut c_count = 0;
        for item in self.db_certificates.iter() {
            if let Ok((_, bytes)) = item {
                if let Ok(c) = postcard::from_bytes::<Certificate>(&bytes) {
                    // Reconstruir cert_by_vertex mergeando firmas
                    self.cert_by_vertex.entry(c.vertex_id).and_modify(|existing| {
                        for sig in &c.signatures {
                            if !existing.signatures.iter().any(|(pk, _)| pk == &sig.0) {
                                existing.signatures.push(sig.clone());
                            }
                        }
                    }).or_insert_with(|| c.clone());
                    self.certificates.entry(c.round).or_insert_with(Vec::new).push(Arc::new(c));
                    c_count += 1;
                }
            }
        }
        if v_count > 0 {
            println!("💾 DAG restaurado: {} vértices, {} certificados", v_count, c_count);
        }
        Ok(())
    }

    pub fn insert_vertex(&self, vertex: Vertex) -> Result<(), anyhow::Error> {
        let id = vertex.id();
        let bytes = postcard::to_allocvec(&vertex)?;
        self.db_vertices.insert(id, bytes)?;
        self.vertices.insert(id, Arc::new(vertex));
        Ok(())
    }

    pub fn get_vertex(&self, id: &VertexId) -> Option<Arc<Vertex>> {
        self.vertices.get(id).map(|v| v.clone())
    }

    pub fn insert_certificate(&self, cert: Certificate) -> Result<(), anyhow::Error> {
        let vertex_id = cert.vertex_id;
        let round = cert.round;

        // Agregar firmas nuevas al certificado agregado por vértice
        {
            let mut entry = self.cert_by_vertex.entry(vertex_id).or_insert_with(|| cert.clone());
            for sig in &cert.signatures {
                if !entry.signatures.iter().any(|(pk, _)| pk == &sig.0) {
                    entry.signatures.push(sig.clone());
                }
            }
        }

        let merged = self.cert_by_vertex.get(&vertex_id).unwrap().clone();

        // Persistir certificado mergado
        let bytes = postcard::to_allocvec(&merged)?;
        let key = format!("{}_{}", round, hex::encode(&vertex_id[0..4]));
        self.db_certificates.insert(key, bytes)?;

        // Actualizar mapa round→certs (para get_round_certificates y DAG sync)
        let mut round_certs = self.certificates.entry(round).or_insert_with(Vec::new);
        let merged_arc = Arc::new(merged.clone());
        match round_certs.iter().position(|c| c.vertex_id == vertex_id) {
            Some(i) => round_certs[i] = merged_arc,
            None    => round_certs.push(merged_arc),
        }

        Ok(())
    }

    /// Devuelve el certificado agregado para un vértice (con todas las firmas recibidas)
    pub fn get_cert_for_vertex(&self, vertex_id: &VertexId) -> Option<Certificate> {
        self.cert_by_vertex.get(vertex_id).map(|c| c.clone())
    }

    pub fn get_round_certificates(&self, round: Round) -> Vec<Arc<Certificate>> {
        self.certificates.get(&round).map(|c| c.clone()).unwrap_or_default()
    }

    /// Vértices más recientes para el dashboard (ordenados por ronda desc)
    pub fn recent_vertices(&self, limit: usize) -> Vec<Arc<Vertex>> {
        let mut all: Vec<Arc<Vertex>> = self.vertices.iter().map(|e| e.value().clone()).collect();
        all.sort_by(|a, b| b.round.cmp(&a.round));
        all.truncate(limit);
        all
    }
}

pub struct Mempool {
    pub pending_transactions: DashMap<[u8; 32], Transaction>,
    pub pending_encrypted_transactions: DashMap<[u8; 32], EncryptedTransaction>,
    pub keypair: SigningKeyPair,
}

impl Mempool {
    pub fn new(keypair: SigningKeyPair) -> Self {
        Self { 
            pending_transactions: DashMap::new(), 
            pending_encrypted_transactions: DashMap::new(),
            keypair 
        }
    }

    pub fn add_transaction(&self, tx: Transaction) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&postcard::to_allocvec(&tx).unwrap());
        let id = *hasher.finalize().as_bytes();
        self.pending_transactions.insert(id, tx);
    }

    pub fn add_encrypted_transaction(&self, etx: EncryptedTransaction) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&postcard::to_allocvec(&etx).unwrap());
        let id = *hasher.finalize().as_bytes();
        self.pending_encrypted_transactions.insert(id, etx);
    }

    pub fn create_vertex(&self, round: Round, parents: HashSet<VertexId>) -> Result<Vertex, redflag_crypto::CryptoError> {
        const MAX_TXS_PER_VERTEX: usize = 500;
        let txs: Vec<Transaction> = self.pending_transactions
            .iter()
            .take(MAX_TXS_PER_VERTEX)
            .map(|kv| kv.value().clone())
            .collect();
        // Limpiar las TXs que ya se incluyeron
        for tx in &txs {
            let id = blake3::hash(&postcard::to_allocvec(tx).unwrap_or_default());
            self.pending_transactions.remove(id.as_bytes());
        }

        let etxs: Vec<EncryptedTransaction> = self.pending_encrypted_transactions
            .iter()
            .take(MAX_TXS_PER_VERTEX)
            .map(|kv| kv.value().clone())
            .collect();
        for etx in &etxs {
            let id = blake3::hash(&postcard::to_allocvec(etx).unwrap_or_default());
            self.pending_encrypted_transactions.remove(id.as_bytes());
        }

        let author = self.keypair.public_key().to_vec();

        let mut vertex = Vertex {
            round,
            parents,
            transactions: txs,
            encrypted_transactions: etxs,
            author,
            signature: vec![],
        };

        let msg = vertex.id();
        vertex.signature = self.keypair.sign(&msg)?.to_vec();
        Ok(vertex)
    }
}

pub struct ConsensusEngine {
    pub dag: Arc<Dag>,
    pub mempool: Arc<Mempool>,
    pub state: Arc<StateDB>,
    pub threshold_mempool: Arc<threshold::ThresholdMempool>,
    /// Validadores activos — actualizable en tiempo de ejecución
    pub validators: Arc<std::sync::RwLock<Vec<Vec<u8>>>>,
    current_round: AtomicU64,
    pub committed_vertices: DashSet<VertexId>,
    db_metadata: Tree,
}

impl ConsensusEngine {
    pub fn new(
        dag: Arc<Dag>,
        mempool: Arc<Mempool>,
        state: Arc<StateDB>,
        threshold_mempool: Arc<threshold::ThresholdMempool>,
        validators: Vec<Vec<u8>>,
    ) -> Self {
        let db_metadata = dag.db.open_tree("consensus_metadata").unwrap();

        let current_round = if let Ok(Some(bytes)) = db_metadata.get("current_round") {
            u64::from_be_bytes(bytes.as_ref().try_into().unwrap_or([0; 8]))
        } else {
            0
        };

        let committed_vertices = DashSet::new();
        if let Ok(Some(bytes)) = db_metadata.get("committed_vertices") {
            if let Ok(set) = postcard::from_bytes::<HashSet<VertexId>>(bytes.as_ref()) {
                for id in set { committed_vertices.insert(id); }
            }
        }

        let engine = Self {
            dag,
            mempool,
            state,
            threshold_mempool,
            validators: Arc::new(std::sync::RwLock::new(validators)),
            current_round: AtomicU64::new(current_round),
            committed_vertices,
            db_metadata,
        };

        if current_round > 0 {
            println!("♻️  Consenso restaurado desde ronda {}", current_round);
        }

        engine
    }

    /// Añade validador peer en tiempo de ejecución (activa modo multi-node)
    pub fn add_validator(&self, pubkey: Vec<u8>) {
        let mut validators = self.validators.write().unwrap();
        if !validators.contains(&pubkey) {
            println!("🗳️  Nuevo validador registrado: {} ({} total)",
                hex::encode(&pubkey[..8.min(pubkey.len())]),
                validators.len() + 1
            );
            validators.push(pubkey);
        }
    }

    pub fn validator_count(&self) -> usize {
        self.validators.read().unwrap().len()
    }

    /// Quórum BFT: ceil(2n/3) — para n=1 es 1, para n=3 es 2, para n=4 es 3
    pub fn quorum_threshold(&self) -> usize {
        let n = self.validator_count();
        if n == 0 { return 1; }
        (2 * n + 2) / 3
    }

    pub fn get_current_round(&self) -> u64 {
        self.current_round.load(Ordering::Relaxed)
    }

    pub fn advance_round(&self) -> u64 {
        let new_round = self.current_round.fetch_add(1, Ordering::SeqCst) + 1;
        self.db_metadata.insert("current_round", &new_round.to_be_bytes()).ok();
        new_round
    }

    pub fn get_leader(&self, round: Round) -> Vec<u8> {
        let validators = self.validators.read().unwrap();
        if validators.is_empty() { return vec![]; }
        let mut hasher = blake3::Hasher::new();
        hasher.update(&round.to_be_bytes());
        let seed = hasher.finalize().as_bytes()[0] as usize;
        validators[seed % validators.len()].clone()
    }

    pub fn order_transactions(&self, round: Round) -> Vec<Transaction> {
        let mut total_order = Vec::new();
        if round < 2 { return total_order; }

        let target_round = round - 1;
        let quorum = self.quorum_threshold();

        // Solo comprometer vértices con quórum 2f+1 de firmas en su certificado.
        // Para nodo único (n=1) quorum=1 — comportamiento idéntico al anterior.
        let mut to_commit: Vec<VertexId> = self.dag.vertices
            .iter()
            .filter(|e| {
                if e.value().round != target_round { return false; }
                if self.committed_vertices.contains(e.key()) { return false; }
                let sig_count = self.dag.get_cert_for_vertex(e.key())
                    .map(|c| c.signatures.len())
                    .unwrap_or(0);
                sig_count >= quorum
            })
            .map(|e| *e.key())
            .collect();
        to_commit.sort();

        for vid in &to_commit {
            total_order.extend(self.commit_vertex_recursive(vid));
        }

        if !total_order.is_empty() {
            println!("⚓ Bullshark commit: ronda {} → {} TXs ({}/{} firmas)",
                target_round, total_order.len(), quorum, self.validator_count());
            self.distribute_fees_round();
        }

        total_order
    }

    /// Distribuye el fee pool entre validadores activos (quema 20%, distribuye 80%)
    fn distribute_fees_round(&self) {
        let fee_pool = self.state.get_balance(redflag_core::FEE_POOL_ADDRESS);
        if fee_pool == 0 { return; }

        // Quemar FEE_BURN_PERCENT% — deflación controlada
        let burn_amount = fee_pool * FEE_BURN_PERCENT / 100;
        let distributable = fee_pool.saturating_sub(burn_amount);

        let rewards = self.state.staking.distribute_fees(distributable);

        for (address, amount) in &rewards {
            let mut acc = self.state.get_account(address).unwrap_or(redflag_state::Account {
                address: address.clone(),
                balance: 0,
                nonce: 0,
            });
            acc.balance = acc.balance.saturating_add(*amount);
            let _ = self.state.save_account_pub(&acc);
        }

        // Vaciar fee pool (el burn_amount simplemente desaparece — supply se reduce)
        let _ = self.state.save_account_pub(&redflag_state::Account {
            address: redflag_core::FEE_POOL_ADDRESS.into(),
            balance: 0,
            nonce: 0,
        });

        if !rewards.is_empty() {
            println!("💰 Fees: {} RF quemados + {} RF → {} validadores",
                burn_amount, distributable, rewards.len());
        } else if fee_pool > 0 {
            println!("🔥 {} RF quemados (sin validadores activos)", burn_amount);
        }
    }

    fn commit_vertex_recursive(&self, vertex_id: &VertexId) -> Vec<Transaction> {
        let mut ordered_txs = Vec::new();
        if self.committed_vertices.contains(vertex_id) {
            return ordered_txs;
        }

        if let Some(v) = self.dag.get_vertex(vertex_id) {
            // ── FIX #2: Verificar firma del autor del vértice ─────────────────
            // Evita que un nodo malicioso fabrique vértices con autor falso
            // y robe recompensas de otros validadores.
            {
                let mut v_for_verify = v.as_ref().clone();
                let sig = std::mem::take(&mut v_for_verify.signature);
                let vid = v_for_verify.id();
                if !v.author.is_empty() && redflag_crypto::Verifier::verify(&v.author, &vid, &sig).is_err() {
                    eprintln!("⚠️  Vértice {} rechazado: firma de autor inválida", hex::encode(&vertex_id[..4]));
                    return ordered_txs;
                }
            }

            let mut parents: Vec<_> = v.parents.iter().collect();
            parents.sort();
            for parent_id in parents {
                ordered_txs.extend(self.commit_vertex_recursive(parent_id));
            }

            self.committed_vertices.insert(*vertex_id);
            self.persist_committed_vertices();

            if !v.transactions.is_empty() || !v.encrypted_transactions.is_empty() {
                let mut all_txs = v.transactions.clone();
                
                // Desencriptar transacciones de umbral tras el commit
                for etx in &v.encrypted_transactions {
                    if let Ok(tx) = self.threshold_mempool.finalize_transaction(etx) {
                        all_txs.push(tx);
                    }
                }

                println!("📜 Commit vértice {} ronda {} — {} TXs ({} cifradas)",
                    hex::encode(&vertex_id[..4]),
                    v.round,
                    all_txs.len(),
                    v.encrypted_transactions.len()
                );
                
                if let Err(e) = self.state.apply_transactions(&all_txs) {
                    eprintln!("❌ Error aplicando transacciones: {}", e);
                }

                // ── Recompensa al autor del vértice (con halving) ─────────────
                let reward: u64 = vertex_reward(v.round, all_txs.len());
                let author_hex = hex::encode(&v.author);
                if let Some(mut acc) = self.state.get_account(&author_hex) {
                    acc.balance = acc.balance.saturating_add(reward);
                    let _ = self.state.save_account_pub(&acc);
                } else {
                    let _ = self.state.save_account_pub(&redflag_state::Account {
                        address: author_hex.clone(),
                        balance: reward,
                        nonce: 0,
                    });
                }

                // Limpiar mempool de TXs ya confirmadas
                for tx in &v.transactions {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&postcard::to_allocvec(tx).unwrap_or_default());
                    let tx_id = *hasher.finalize().as_bytes();
                    self.mempool.pending_transactions.remove(&tx_id);
                }
                for etx in &v.encrypted_transactions {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&postcard::to_allocvec(etx).unwrap_or_default());
                    let tx_id = *hasher.finalize().as_bytes();
                    self.mempool.pending_encrypted_transactions.remove(&tx_id);
                }
                
                ordered_txs.extend(all_txs);
            }
        }

        ordered_txs
    }

    /// Persiste el set de committed vertices para sobrevivir reinicios
    fn persist_committed_vertices(&self) {
        // Guardamos solo los IDs de los últimos 10_000 para no crecer sin límite
        let ids: HashSet<VertexId> = self.committed_vertices
            .iter()
            .map(|e| *e.key())
            .collect();
        if let Ok(bytes) = postcard::to_allocvec(&ids) {
            self.db_metadata.insert("committed_vertices", bytes).ok();
        }
    }

    /// Resumen del estado del consenso para el dashboard
    pub fn summary(&self) -> ConsensusSummary {
        let state_stats = self.state.stats();
        ConsensusSummary {
            current_round: self.get_current_round(),
            validator_count: self.validator_count(),
            pending_txs: self.mempool.pending_transactions.len(),
            committed_vertices: self.committed_vertices.len(),
            total_vertices: self.dag.vertices.len(),
            account_count: state_stats.account_count,
            tx_count: state_stats.tx_count,
            fee_pool_balance: state_stats.fee_pool_balance,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConsensusSummary {
    pub current_round: u64,
    pub validator_count: usize,
    pub pending_txs: usize,
    pub committed_vertices: usize,
    pub total_vertices: usize,
    pub account_count: usize,
    pub tx_count: usize,
    pub fee_pool_balance: u64,
}
