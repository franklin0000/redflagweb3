/// Threshold Encryption Mempool — RedFlag 2.1
/// 
/// Movido a redflag-consensus para evitar dependencias circulares y permitir 
/// al motor de consenso desencriptar durante el commit.

use aws_lc_rs::kem::{DecapsulationKey, EncapsulationKey, ML_KEM_768};
use aws_lc_rs::rand::SystemRandom;
use dashmap::DashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use redflag_core::{PrivateTxPayload, EncryptedTransaction, RevealedRoundKey, Transaction};

pub struct ThresholdMempool {
    pub current_round: AtomicU64,
    pub rng: SystemRandom,
    /// Llave de encapsulación actual (pública — clientes la usan para cifrar)
    pub current_ek: RwLock<Vec<u8>>,
    /// Llave de desencapsulación actual (privada del nodo)
    pub current_dk: RwLock<Option<DecapsulationKey>>,
    /// Historial de llaves reveladas (inmutable, para auditoría)
    pub revealed_keys: Arc<DashMap<u64, RevealedRoundKey>>,
}

impl ThresholdMempool {
    pub fn new() -> anyhow::Result<Self> {
        let rng = SystemRandom::new();
        let tm = Self {
            current_round: AtomicU64::new(0),
            rng,
            current_ek: RwLock::new(vec![]),
            current_dk: RwLock::new(None),
            revealed_keys: Arc::new(DashMap::new()),
        };
        tm.rotate_keys(0)?;
        Ok(tm)
    }

    pub fn get_current_ek(&self) -> (u64, Vec<u8>) {
        let round = self.current_round.load(Ordering::SeqCst);
        let ek = self.current_ek.read().unwrap().clone();
        (round, ek)
    }

    pub fn revealed_key_for_round(&self, round: u64) -> Option<RevealedRoundKey> {
        self.revealed_keys.get(&round).map(|rk| rk.clone())
    }

    pub fn rotate_keys(&self, round: u64) -> anyhow::Result<Vec<u8>> {
        let old_round = self.current_round.load(Ordering::Relaxed);

        // Sellar la llave anterior en el historial (cualquier ronda, incluida 0)
        {
            let dk_guard = self.current_dk.read().unwrap();
            if dk_guard.is_some() {
                let ek_snapshot = self.current_ek.read().unwrap().clone();
                if !ek_snapshot.is_empty() {
                    self.revealed_keys.insert(old_round, RevealedRoundKey {
                        round: old_round,
                        ek_bytes: ek_snapshot,
                        dk_bytes: vec![], // Exportación completa en v2.2 con DKG
                    });
                }
            }
        }

        // Generar nuevo par ML-KEM-768 para la ronda entrante
        let dk = DecapsulationKey::generate(&ML_KEM_768)?;
        let ek = dk.encapsulation_key()?;
        let ek_bytes = ek.key_bytes()?.as_ref().to_vec();

        *self.current_ek.write().unwrap() = ek_bytes.clone();
        *self.current_dk.write().unwrap() = Some(dk);
        self.current_round.store(round, Ordering::SeqCst);

        Ok(ek_bytes)
    }

    pub fn decrypt_payload(
        &self,
        etx: &EncryptedTransaction,
    ) -> anyhow::Result<PrivateTxPayload> {
        let dk_guard = self.current_dk.read().unwrap();
        let dk = dk_guard.as_ref().ok_or_else(|| anyhow::anyhow!("DK no inicializada"))?;

        let ct = aws_lc_rs::kem::Ciphertext::from(etx.kem_ciphertext.as_slice());
        let shared_secret = dk.decapsulate(ct)?.as_ref().to_vec();

        let plaintext = xor_cipher(&etx.encrypted_payload, &shared_secret);

        let payload: PrivateTxPayload = bincode::deserialize(&plaintext)?;
        let commitment = blake3::hash(&plaintext);
        if commitment.as_bytes() != &etx.payload_commitment {
            anyhow::bail!("Commitment mismatch");
        }

        Ok(payload)
    }

    /// Convierte una TX cifrada en una TX estándar tras el commit
    pub fn finalize_transaction(&self, etx: &EncryptedTransaction) -> anyhow::Result<Transaction> {
        let payload = self.decrypt_payload(etx)?;
        
        Ok(Transaction {
            sender: etx.sender.clone(),
            receiver: payload.receiver.clone(),
            amount: payload.amount,
            fee: etx.fee,
            nonce: etx.nonce,
            chain_id: etx.chain_id,
            read_set: vec![etx.sender.clone(), payload.receiver.clone()],
            write_set: vec![etx.sender.clone(), payload.receiver.clone()],
            data: payload.data,
            signature: etx.signature.clone(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
    }
}

/// Cifra un payload para enviarlo al mempool — ejecutado por el cliente/wallet
pub fn encrypt_payload(
    ek_bytes: &[u8],
    payload: &PrivateTxPayload,
    _round: u64,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, [u8; 32])> {
    let plaintext = bincode::serialize(payload)?;
    let commitment = *blake3::hash(&plaintext).as_bytes();

    let ek = EncapsulationKey::new(&ML_KEM_768, ek_bytes)?;
    let (ciphertext, shared_secret) = ek.encapsulate()?;
    let kem_ciphertext = ciphertext.as_ref().to_vec();
    let encrypted = xor_cipher(&plaintext, shared_secret.as_ref());

    Ok((kem_ciphertext, encrypted, commitment))
}

fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter().enumerate().map(|(i, &b)| b ^ key[i % key.len()]).collect()
}
