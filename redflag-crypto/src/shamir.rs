/// Shamir's Secret Sharing — redflag.web3 v2.2
///
/// Fase 2: DKG distribuido — la clave de desencriptación del mempool
/// se divide entre validadores. Ninguno puede descifrar solo.
///
/// Implementación sobre GF(2^8) (campo de Galois de 8 bits) para simplicidad.
/// Para producción con claves de 256 bits: aplica byte a byte.
///
/// threshold t: necesitas t shares para reconstruir
/// total n: se generan n shares (n >= t)

use serde::{Serialize, Deserialize};
use rand::rngs::OsRng;
use rand::RngCore;

/// Un share del secreto para un validador
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretShare {
    /// Índice del validador (1-based, ≠ 0)
    pub index: u8,
    /// Bytes del share (mismo tamaño que el secreto)
    pub value: Vec<u8>,
}

/// Divide un secreto en n shares, threshold t
pub fn split_secret(secret: &[u8], threshold: usize, total: usize) -> anyhow::Result<Vec<SecretShare>> {
    if threshold < 2 {
        anyhow::bail!("threshold mínimo es 2");
    }
    if total < threshold {
        anyhow::bail!("total debe ser >= threshold");
    }
    if total > 255 {
        anyhow::bail!("total máximo es 255");
    }

    let mut rng = OsRng;
    let secret_len = secret.len();

    // Para cada byte del secreto, generamos un polinomio de grado (t-1) en GF(2^8)
    // coef[0] = byte del secreto, coef[1..t-1] = aleatorio
    let mut all_shares: Vec<Vec<u8>> = vec![vec![0u8; secret_len]; total];

    for byte_idx in 0..secret_len {
        // Generar coeficientes del polinomio
        let mut coeffs = vec![0u8; threshold];
        coeffs[0] = secret[byte_idx];
        let mut rnd = vec![0u8; threshold - 1];
        rng.fill_bytes(&mut rnd);
        for i in 1..threshold {
            coeffs[i] = rnd[i - 1];
        }

        // Evaluar el polinomio en x = 1, 2, ..., n
        for i in 0..total {
            let x = (i + 1) as u8;
            all_shares[i][byte_idx] = gf256_poly_eval(&coeffs, x);
        }
    }

    Ok(all_shares.into_iter().enumerate().map(|(i, value)| SecretShare {
        index: (i + 1) as u8,
        value,
    }).collect())
}

/// Reconstruye el secreto a partir de t shares (debe haber exactamente threshold shares distintos)
pub fn reconstruct_secret(shares: &[SecretShare]) -> anyhow::Result<Vec<u8>> {
    if shares.is_empty() {
        anyhow::bail!("No hay shares");
    }

    let secret_len = shares[0].value.len();
    let mut secret = vec![0u8; secret_len];

    for byte_idx in 0..secret_len {
        let xs: Vec<u8> = shares.iter().map(|s| s.index).collect();
        let ys: Vec<u8> = shares.iter().map(|s| s.value[byte_idx]).collect();
        secret[byte_idx] = gf256_lagrange(&xs, &ys);
    }

    Ok(secret)
}

// ── GF(2^8) aritmética ───────────────────────────────────────────────────────
// Polinomio irreducible: x^8 + x^4 + x^3 + x^2 + 1 (0x11d)

const GF_EXP: [u8; 512] = {
    let mut exp = [0u8; 512];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        exp[i + 255] = x as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11d;
        }
        i += 1;
    }
    exp
};

const GF_LOG: [u8; 256] = {
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11d;
        }
        i += 1;
    }
    log
};

fn gf_mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 { return 0; }
    GF_EXP[(GF_LOG[a as usize] as usize + GF_LOG[b as usize] as usize) % 255]
}

fn gf_div(a: u8, b: u8) -> u8 {
    if b == 0 { panic!("división por cero en GF(256)"); }
    if a == 0 { return 0; }
    let log_diff = (GF_LOG[a as usize] as usize + 255 - GF_LOG[b as usize] as usize) % 255;
    GF_EXP[log_diff]
}

fn gf_pow(x: u8, power: usize) -> u8 {
    if power == 0 { return 1; }
    GF_EXP[(GF_LOG[x as usize] as usize * power) % 255]
}

/// Evalúa el polinomio `coeffs` en el punto `x` sobre GF(256)
fn gf256_poly_eval(coeffs: &[u8], x: u8) -> u8 {
    coeffs.iter().enumerate().fold(0u8, |acc, (i, &c)| {
        acc ^ gf_mul(c, gf_pow(x, i))
    })
}

/// Interpolación de Lagrange sobre GF(256) para reconstruir f(0)
fn gf256_lagrange(xs: &[u8], ys: &[u8]) -> u8 {
    let mut result = 0u8;
    for (i, (&xi, &yi)) in xs.iter().zip(ys.iter()).enumerate() {
        let mut num = 1u8;
        let mut den = 1u8;
        for (j, &xj) in xs.iter().enumerate() {
            if i != j {
                // num *= (0 - xj) = xj  en GF (porque -x = x en GF(2^8))
                num = gf_mul(num, xj);
                // den *= (xi - xj) = xi XOR xj en GF
                den = gf_mul(den, xi ^ xj);
            }
        }
        result ^= gf_mul(yi, gf_div(num, den));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shamir_roundtrip() {
        let secret = b"redflag_dk_secret_32bytes_123456"; // 32 bytes
        let shares = split_secret(secret, 3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        // Con 3 shares cualesquiera reconstruimos
        let subset = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
        let recovered = reconstruct_secret(&subset).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_shamir_insufficient_shares() {
        let secret = b"otro_secreto_de_prueba__________";
        let shares = split_secret(secret, 3, 5).unwrap();

        // Con solo 2 shares NO se recupera correctamente
        let subset = vec![shares[0].clone(), shares[1].clone()];
        let recovered = reconstruct_secret(&subset).unwrap();
        assert_ne!(recovered, secret as &[u8], "Con t-1 shares no debe recuperarse");
    }
}
