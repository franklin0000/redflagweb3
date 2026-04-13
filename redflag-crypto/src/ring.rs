/// Ring Signatures (LSAG) — redflag.web3 v2.2
///
/// Fase 3: Anonimato del remitente.
/// Implementa Linkable Spontaneous Anonymous Group (LSAG) signatures
/// sobre Ristretto255 (variante de Curve25519 con aritmética completa).
///
/// Propiedades:
///   - Anonimato: imposible saber quién firmó dentro del anillo
///   - Enlazabilidad: dos firmas del mismo firmante generan la misma key_image
///                    → detecta doble gasto sin revelar identidad
///   - No-forgeabilidad: sin la clave privada no se puede forjar

use curve25519_dalek::{
    ristretto::{RistrettoPoint, CompressedRistretto},
    scalar::Scalar,
    constants::RISTRETTO_BASEPOINT_POINT,
};
use sha2::{Digest, Sha512};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Serialize, Deserialize};

/// Par de llaves para ring signatures (Ristretto255)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingKeyPair {
    /// Clave privada (scalar de 32 bytes)
    pub private_key: [u8; 32],
    /// Clave pública (punto comprimido de 32 bytes)
    pub public_key: [u8; 32],
}

impl RingKeyPair {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let sk = Scalar::from_bytes_mod_order_wide(&bytes);
        let pk = (RISTRETTO_BASEPOINT_POINT * sk).compress();
        Self {
            private_key: sk.to_bytes(),
            public_key: pk.to_bytes(),
        }
    }

    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut wide = [0u8; 64];
        let hash = Sha512::digest(seed);
        wide.copy_from_slice(&hash);
        let sk = Scalar::from_bytes_mod_order_wide(&wide);
        let pk = (RISTRETTO_BASEPOINT_POINT * sk).compress();
        Self {
            private_key: sk.to_bytes(),
            public_key: pk.to_bytes(),
        }
    }
}

/// Firma de anillo LSAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingSignature {
    /// Anillo de claves públicas (incluye la del firmante real)
    pub ring: Vec<[u8; 32]>,
    /// Imagen de llave: I = sk * H_p(pk) — vincula firmas del mismo firmante
    pub key_image: [u8; 32],
    /// Desafío inicial
    pub c0: [u8; 32],
    /// Respuestas por miembro del anillo
    pub responses: Vec<[u8; 32]>,
}

// ── Hash to point (Ristretto) ────────────────────────────────────────────────

fn hash_to_point(data: &[u8]) -> RistrettoPoint {
    use curve25519_dalek::ristretto::RistrettoPoint as RP;
    // Usamos hash-to-point manual: sha512 → 64 bytes → from_uniform_bytes
    let hash = Sha512::digest(data);
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(&hash);
    RP::from_uniform_bytes(&bytes)
}

fn hash_to_scalar(data: &[u8]) -> Scalar {
    let hash = Sha512::digest(data);
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&hash);
    Scalar::from_bytes_mod_order_wide(&wide)
}

// ── LSAG ────────────────────────────────────────────────────────────────────

/// Firma un mensaje como miembro `signer_index` del anillo
pub fn ring_sign(
    message: &[u8],
    ring_pubkeys: &[[u8; 32]],   // incluye la clave del firmante real
    signer_index: usize,
    signer_private: &[u8; 32],
) -> anyhow::Result<RingSignature> {
    let n = ring_pubkeys.len();
    if n < 2 {
        anyhow::bail!("El anillo necesita al menos 2 miembros");
    }
    if signer_index >= n {
        anyhow::bail!("signer_index fuera de rango");
    }

    let sk = Scalar::from_canonical_bytes(*signer_private)
        .into_option()
        .ok_or_else(|| anyhow::anyhow!("Clave privada inválida"))?;

    // Key image: I = sk * H_p(pk_s)
    let pk_s_bytes = &ring_pubkeys[signer_index];
    let h_p = hash_to_point(pk_s_bytes);
    let key_image = (h_p * sk).compress();

    // Descifrar puntos del anillo
    let points: Vec<RistrettoPoint> = ring_pubkeys.iter().map(|pk_bytes| {
        CompressedRistretto(*pk_bytes).decompress()
            .unwrap_or(RISTRETTO_BASEPOINT_POINT)
    }).collect();

    let mut rng = OsRng;
    let mut alpha_bytes = [0u8; 64];
    rng.fill_bytes(&mut alpha_bytes);
    let alpha = Scalar::from_bytes_mod_order_wide(&alpha_bytes);

    let mut c = vec![[0u8; 32]; n];
    let mut r = vec![[0u8; 32]; n];

    // L_s = alpha * G,  R_s = alpha * H_p(pk_s)
    let l_s = RISTRETTO_BASEPOINT_POINT * alpha;
    let r_s = h_p * alpha;

    // c_{s+1} = H(msg, ring, L_s, R_s)
    let c_next = compute_challenge(message, ring_pubkeys, &key_image, &l_s, &r_s);
    c[(signer_index + 1) % n] = c_next;

    let ki = key_image.decompress().unwrap_or(RISTRETTO_BASEPOINT_POINT);

    // Recorrer el anillo desde s+1 hasta s (inclusive) para cerrar el anillo
    for step in 1..n {
        let i = (signer_index + step) % n;
        let next_i = (i + 1) % n;

        let mut r_i_bytes = [0u8; 64];
        rng.fill_bytes(&mut r_i_bytes);
        let r_i = Scalar::from_bytes_mod_order_wide(&r_i_bytes);
        r[i] = r_i.to_bytes();

        let c_i = Scalar::from_canonical_bytes(c[i])
            .into_option()
            .unwrap_or(hash_to_scalar(&c[i]));

        let h_p_i = hash_to_point(&ring_pubkeys[i]);
        let l_i = RISTRETTO_BASEPOINT_POINT * r_i + points[i] * c_i;
        let r_i_pt = h_p_i * r_i + ki * c_i;

        // Siempre calcular c[next_i], incluyendo cuando next_i == signer_index
        // Eso cierra el anillo y nos da c_s para calcular r_s
        c[next_i] = compute_challenge(message, ring_pubkeys, &key_image, &l_i, &r_i_pt);
    }

    // Cerrar el anillo: r_s = alpha - c_s * sk  (mod q)
    let c_s = Scalar::from_canonical_bytes(c[signer_index])
        .into_option()
        .unwrap_or(hash_to_scalar(&c[signer_index]));
    let r_s_scalar = alpha - c_s * sk;
    r[signer_index] = r_s_scalar.to_bytes();

    Ok(RingSignature {
        ring: ring_pubkeys.to_vec(),
        key_image: key_image.to_bytes(),
        c0: c[0],
        responses: r,
    })
}

/// Verifica una firma de anillo LSAG
pub fn ring_verify(message: &[u8], sig: &RingSignature) -> bool {
    let n = sig.ring.len();
    if n < 2 || sig.responses.len() != n {
        return false;
    }

    let ki_compressed = CompressedRistretto(sig.key_image);
    let ki = match ki_compressed.decompress() {
        Some(p) => p,
        None => return false,
    };

    let points: Vec<RistrettoPoint> = sig.ring.iter().map(|pk_bytes| {
        CompressedRistretto(*pk_bytes).decompress()
            .unwrap_or(RISTRETTO_BASEPOINT_POINT)
    }).collect();

    let mut c = Scalar::from_canonical_bytes(sig.c0)
        .into_option()
        .unwrap_or(hash_to_scalar(&sig.c0));

    for i in 0..n {
        let r_i = Scalar::from_canonical_bytes(sig.responses[i])
            .into_option()
            .unwrap_or(hash_to_scalar(&sig.responses[i]));

        let h_p_i = hash_to_point(&sig.ring[i]);
        let l_i = RISTRETTO_BASEPOINT_POINT * r_i + points[i] * c;
        let r_i_pt = h_p_i * r_i + ki * c;

        let c_next_bytes = compute_challenge(message, &sig.ring, &ki_compressed, &l_i, &r_i_pt);
        c = Scalar::from_canonical_bytes(c_next_bytes)
            .into_option()
            .unwrap_or(hash_to_scalar(&c_next_bytes));
    }

    // El anillo cierra si el c final == c0
    c.to_bytes() == sig.c0
}

fn compute_challenge(
    message: &[u8],
    ring: &[[u8; 32]],
    key_image: &CompressedRistretto,
    l: &RistrettoPoint,
    r: &RistrettoPoint,
) -> [u8; 32] {
    let mut h = Sha512::new();
    h.update(b"rf_lsag_v1");
    h.update(message);
    for pk in ring {
        h.update(pk);
    }
    h.update(key_image.as_bytes());
    h.update(l.compress().as_bytes());
    h.update(r.compress().as_bytes());
    let full = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&full);
    Scalar::from_bytes_mod_order_wide(&wide).to_bytes()
}

/// Verifica si una key_image ya fue usada (doble gasto)
pub fn key_image_used(key_image: &[u8; 32], used: &[[u8; 32]]) -> bool {
    used.contains(key_image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_sign_verify() {
        // Generar 5 claves para el anillo
        let pairs: Vec<RingKeyPair> = (0..5).map(|_| RingKeyPair::generate()).collect();
        let ring: Vec<[u8; 32]> = pairs.iter().map(|p| p.public_key).collect();

        // Firmante real es el índice 2
        let sig = ring_sign(b"tx_hash_redflag", &ring, 2, &pairs[2].private_key).unwrap();

        // Verificar
        assert!(ring_verify(b"tx_hash_redflag", &sig), "La verificación debe pasar");

        // Mensaje diferente falla
        assert!(!ring_verify(b"otro_mensaje", &sig), "Mensaje distinto debe fallar");
    }

    #[test]
    fn test_key_image_linkability() {
        let pairs: Vec<RingKeyPair> = (0..3).map(|_| RingKeyPair::generate()).collect();
        let ring: Vec<[u8; 32]> = pairs.iter().map(|p| p.public_key).collect();

        let sig1 = ring_sign(b"tx1", &ring, 0, &pairs[0].private_key).unwrap();
        let sig2 = ring_sign(b"tx2", &ring, 0, &pairs[0].private_key).unwrap();

        // Mismo firmante → misma key_image (detecta doble gasto)
        assert_eq!(sig1.key_image, sig2.key_image);
    }
}
