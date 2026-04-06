use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Chain ID único de redflag.web3 mainnet — evita replay attacks entre redes
pub const CHAIN_ID: u64 = 2100;

/// Cuenta del protocolo que acumula fees para financiar validadores
pub const FEE_POOL_ADDRESS: &str = "RedFlag_Protocol_FeePool";

/// Dirección genesis especial con faucet inicial
pub const GENESIS_ADDRESS: &str = "RedFlag_Genesis_Alpha";

/// Balance inicial del genesis
pub const GENESIS_BALANCE: u64 = 1_000_000_000;

/// Fee mínimo por transacción (protección anti-spam)
pub const MIN_FEE: u64 = 1;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u64,
    pub fee: u64,               // Anti-spam: mínimo MIN_FEE
    pub nonce: u64,             // Replay protection: debe ser == account.nonce
    pub chain_id: u64,          // Chain isolation: debe ser == CHAIN_ID
    pub read_set: Vec<String>,  // Cuentas que la TX lee (parallel execution)
    pub write_set: Vec<String>, // Cuentas que la TX escribe (conflict detection)
    pub data: Vec<u8>,          // Payload para smart contracts (vacío = transfer)
    pub signature: Vec<u8>,
    pub timestamp: u64,
}

impl Transaction {
    /// Constructor estándar para transferencias
    pub fn new_transfer(
        sender: String,
        receiver: String,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            read_set: vec![sender.clone(), receiver.clone()],
            write_set: vec![sender.clone(), receiver.clone()],
            sender,
            receiver,
            amount,
            fee: fee.max(MIN_FEE),
            nonce,
            chain_id: CHAIN_ID,
            data: vec![],
            signature: vec![],
            timestamp: now,
        }
    }

    /// TX genesis — sin fee ni nonce (solo para bloque génesis)
    pub fn genesis(receiver: String, amount: u64) -> Self {
        Self {
            sender: GENESIS_ADDRESS.to_string(),
            receiver: receiver.clone(),
            amount,
            fee: 0,
            nonce: 0,
            chain_id: CHAIN_ID,
            read_set: vec![GENESIS_ADDRESS.to_string(), receiver.clone()],
            write_set: vec![GENESIS_ADDRESS.to_string(), receiver],
            data: vec![],
            signature: vec![1], // marcador genesis
            timestamp: 0,
        }
    }

    /// Detecta conflicto de escritura con otra TX — usada para ejecución paralela
    pub fn conflicts_with(&self, other: &Transaction) -> bool {
        self.write_set.iter().any(|w| other.write_set.contains(w))
            || other.write_set.iter().any(|w| self.write_set.contains(w))
    }

    /// Agrupa transacciones en batches no-conflictivos para ejecución paralela
    pub fn parallel_groups(txs: Vec<Transaction>) -> Vec<Vec<Transaction>> {
        let mut groups: Vec<Vec<Transaction>> = Vec::new();

        for tx in txs {
            let slot = groups.iter().position(|group| {
                !group.iter().any(|existing| existing.conflicts_with(&tx))
            });

            match slot {
                Some(i) => groups[i].push(tx),
                None => groups.push(vec![tx]),
            }
        }

        groups
    }
}

/// Payload privado de una TX — cifrado hasta que el bloque se confirma
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrivateTxPayload {
    pub receiver: String,
    pub amount: u64,
    pub data: Vec<u8>,     // Smart contract call data
    pub salt: [u8; 32],    // Randomness para evitar ataques de diccionario
}

/// Transacción con mempool cifrado
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedTransaction {
    pub sender: String,          // Visible: necesario para ordenar
    pub nonce: u64,              // Visible: replay protection
    pub chain_id: u64,           // Visible: chain isolation
    pub fee: u64,                // Visible: anti-spam (validadores lo ven)
    pub round: u64,              // Qué ronda de EK se usó para cifrar
    pub payload_commitment: [u8; 32], // blake3(payload_plaintext) — commitment
    pub kem_ciphertext: Vec<u8>, // ML-KEM ciphertext del symmetric key
    pub encrypted_payload: Vec<u8>, // XOR-cifrado del payload con el shared secret
    pub signature: Vec<u8>,      // ML-DSA sobre todos los campos anteriores
}

/// Llave revelada después del commit — para verificación histórica
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RevealedRoundKey {
    pub round: u64,
    pub ek_bytes: Vec<u8>,
    pub dk_bytes: Vec<u8>, // Revelada post-commit para transparencia
}

/// Bloque legacy (mantenido por compatibilidad — el consenso usa Vertex/DAG)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub index: u64,
    pub timestamp: u64,
    pub prev_hash: String,
    pub hash: String,
    pub transactions: Vec<Transaction>,
    pub nonce: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_detection() {
        let tx1 = Transaction::new_transfer("alice".into(), "bob".into(), 100, 1, 0);
        let tx2 = Transaction::new_transfer("carol".into(), "dave".into(), 50, 1, 0);
        let tx3 = Transaction::new_transfer("alice".into(), "carol".into(), 20, 1, 0);

        assert!(!tx1.conflicts_with(&tx2), "TX sin cuentas comunes no deben conflictuar");
        assert!(tx1.conflicts_with(&tx3), "TX que comparten sender deben conflictuar");
    }

    #[test]
    fn test_parallel_groups() {
        let tx1 = Transaction::new_transfer("alice".into(), "bob".into(), 100, 1, 0);
        let tx2 = Transaction::new_transfer("carol".into(), "dave".into(), 50, 1, 0);
        let tx3 = Transaction::new_transfer("alice".into(), "eve".into(), 10, 1, 1);

        let groups = Transaction::parallel_groups(vec![tx1, tx2, tx3]);
        // tx1 y tx2 no conflictan → mismo grupo; tx3 conflicta con tx1 → grupo separado
        assert_eq!(groups.len(), 2);
    }
}
