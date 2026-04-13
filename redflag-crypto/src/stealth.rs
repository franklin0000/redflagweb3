/// Stealth Address Protocol — redflag.web3 v2.2
///
/// Fase 1: Anonimato del receptor.
/// El remitente deriva una dirección de un solo uso usando ML-KEM.
/// Solo el destinatario (que tiene la dk) puede descubrir que el pago es suyo.
///
/// Flujo:
///   1. Receptor genera (ek, dk), publica ek_bytes + view_tag_seed
///   2. Remitente: encapsula con ek → (kem_ct, shared)
///      one_time_addr = hex( blake3( shared || "rf_stealth_addr_v1" ) )[..40]
///      TX: receiver = one_time_addr, stealth_kem_ct = kem_ct
///   3. Escáner: para cada TX con kem_ct, prueba decapsulate(dk, ct)
///      → derive expected addr → si coincide, la TX es tuya
///   4. Reclamo: el dueño de la one_time_addr envía desde ella usando
///      la clave ML-DSA derivada de shared_secret.

use aws_lc_rs::{
    kem::{DecapsulationKey, EncapsulationKey, ML_KEM_768},
    rand::SystemRandom,
};
use blake3::Hasher;
use serde::{Serialize, Deserialize};

/// Clave de escaneo pública que el receptor publica
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StealthPublicKey {
    /// ML-KEM-768 encapsulation key bytes (1184 bytes)
    pub ek_bytes: Vec<u8>,
    /// Primer byte del hash de la dk — filtro rápido para evitar decapsulaciones innecesarias
    pub view_tag: u8,
}

/// Resultado de crear una stealth tx (parte que va en la transacción)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StealthPayload {
    /// ML-KEM ciphertext (el receptor lo decapsula para obtener el shared secret)
    pub kem_ciphertext: Vec<u8>,
    /// Byte de vista rápida — reduce escaneos en 255/256 = 99.6%
    pub view_tag: u8,
    /// Dirección de un solo uso (hex 40 chars = 20 bytes del blake3)
    pub one_time_address: String,
}

/// Genera un par de llaves stealth (ek pública + dk privada en memoria)
/// La dk DEBE guardarse en disco por el usuario (ek_bytes se publica on-chain)
pub fn generate_stealth_keypair() -> anyhow::Result<(StealthPublicKey, DecapsulationKey)> {
    let dk = DecapsulationKey::generate(&ML_KEM_768)?;
    let ek = dk.encapsulation_key()?;
    let ek_bytes = ek.key_bytes()?.as_ref().to_vec();

    // view_tag: primer byte de blake3(ek_bytes)
    let view_tag = blake3::hash(&ek_bytes).as_bytes()[0];

    Ok((StealthPublicKey { ek_bytes, view_tag }, dk))
}

/// REMITENTE: Crea una stealth address para el receptor dado su StealthPublicKey
pub fn create_stealth_output(recipient_pk: &StealthPublicKey) -> anyhow::Result<StealthPayload> {
    let ek = EncapsulationKey::new(&ML_KEM_768, &recipient_pk.ek_bytes)?;
    let (ciphertext, shared_secret) = ek.encapsulate()?;
    let kem_ciphertext = ciphertext.as_ref().to_vec();
    let shared_bytes = shared_secret.as_ref();

    // Deriva dirección de un solo uso: primeros 20 bytes de blake3(shared || "rf_stealth_addr_v1")
    let mut h = Hasher::new();
    h.update(shared_bytes);
    h.update(b"rf_stealth_addr_v1");
    let hash = h.finalize();
    let one_time_address = hex::encode(&hash.as_bytes()[..20]);

    // view_tag = byte 0 del blake3(shared || "rf_view_tag")
    let mut h2 = Hasher::new();
    h2.update(shared_bytes);
    h2.update(b"rf_view_tag");
    let view_tag = h2.finalize().as_bytes()[0];

    Ok(StealthPayload { kem_ciphertext, view_tag, one_time_address })
}

/// RECEPTOR: Escanea una TX para ver si le pertenece
/// Retorna Some(one_time_address) si la TX es para este receptor, None si no
pub fn scan_stealth_tx(
    dk: &DecapsulationKey,
    payload: &StealthPayload,
) -> Option<String> {
    // Filtro rápido por view_tag (evita 99.6% de decapsulaciones)
    // (el view_tag del payload fue calculado con el shared secret del remitente)
    // Intentamos siempre (no podemos pre-filtrar sin la dk, pero la función es rápida)

    let ct = aws_lc_rs::kem::Ciphertext::from(payload.kem_ciphertext.as_slice());
    let shared_secret = dk.decapsulate(ct).ok()?;
    let shared_bytes = shared_secret.as_ref();

    // Verificar view_tag
    let mut h_tag = Hasher::new();
    h_tag.update(shared_bytes);
    h_tag.update(b"rf_view_tag");
    let expected_tag = h_tag.finalize().as_bytes()[0];
    if expected_tag != payload.view_tag {
        return None;
    }

    // Derivar dirección esperada
    let mut h = Hasher::new();
    h.update(shared_bytes);
    h.update(b"rf_stealth_addr_v1");
    let hash = h.finalize();
    let expected_addr = hex::encode(&hash.as_bytes()[..20]);

    if expected_addr == payload.one_time_address {
        Some(expected_addr)
    } else {
        None
    }
}

/// Deriva la clave de gasto ML-DSA para una one_time_address a partir del shared secret.
/// El receptor usa esta clave para firmar TXs desde la stealth address.
/// seed_bytes = blake3(shared || "rf_stealth_spend_key_v1")
pub fn derive_spend_key_seed(dk: &DecapsulationKey, kem_ciphertext: &[u8]) -> anyhow::Result<[u8; 32]> {
    let ct = aws_lc_rs::kem::Ciphertext::from(kem_ciphertext);
    let shared_secret = dk.decapsulate(ct)?;
    let shared_bytes = shared_secret.as_ref();

    let mut h = Hasher::new();
    h.update(shared_bytes);
    h.update(b"rf_stealth_spend_key_v1");
    let hash = h.finalize();

    let mut seed = [0u8; 32];
    seed.copy_from_slice(hash.as_bytes());
    Ok(seed)
}
