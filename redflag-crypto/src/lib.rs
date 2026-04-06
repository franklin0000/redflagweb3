pub use aws_lc_rs::{
    kem::{Ciphertext, DecapsulationKey, EncapsulationKey, ML_KEM_768},
    signature::{UnparsedPublicKey, KeyPair},
    unstable::signature::{ML_DSA_65, ML_DSA_65_SIGNING, PqdsaKeyPair},
    agreement::{X25519, EphemeralPrivateKey, UnparsedPublicKey as AgreementPublicKey, agree_ephemeral},
    rand::SystemRandom,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Error en la generación de llaves")]
    KeyGenError,
    #[error("Error en la firma de datos")]
    SigningError,
    #[error("Error en la verificación de firma")]
    VerificationError,
    #[error("Error en el encapsulamiento de llaves")]
    EncapsulationError,
    #[error("Error en el desencapsulamiento de llaves")]
    DecapsulationError,
    #[error("Error en el acuerdo de llaves (Agreement)")]
    AgreementError,
}

/// Estructura para el secreto híbrido resultante
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HybridSecret {
    pub shared_key: [u8; 32],
}

/// Combina un secreto clásico (X25519) con un secreto post-cuántico (ML-KEM)
pub fn combine_secrets(classic_secret: &[u8], quantum_secret: &[u8]) -> HybridSecret {
    let mut hasher = Sha256::new();
    hasher.update(classic_secret);
    hasher.update(quantum_secret);
    let result = hasher.finalize();
    
    let mut shared_key = [0u8; 32];
    shared_key.copy_from_slice(&result);
    HybridSecret { shared_key }
}

use serde::{Serialize, Deserialize, Serializer, Deserializer};

/// Estructura para el par de llaves de firma post-cuántica (ML-DSA)
pub struct SigningKeyPair {
    inner: PqdsaKeyPair,
}

impl Serialize for SigningKeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Exportar como PKCS#8 DER para persistencia completa de la llave privada
        let doc = self.inner.to_pkcs8().map_err(serde::ser::Error::custom)?;
        doc.as_ref().to_vec().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SigningKeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let pkcs8_bytes = Vec::<u8>::deserialize(deserializer)?;
        // Reconstruir el par de llaves desde los bytes PKCS#8
        let key_pair = PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &pkcs8_bytes)
            .map_err(serde::de::Error::custom)?;
        Ok(Self { inner: key_pair })
    }
}

impl SigningKeyPair {
    /// Genera un nuevo par de llaves ML-DSA_65
    pub fn generate() -> Result<Self, CryptoError> {
        let key_pair =
            PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).map_err(|_| CryptoError::KeyGenError)?;
        Ok(Self { inner: key_pair })
    }

    /// Firma un mensaje
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut signature = vec![0u8; ML_DSA_65_SIGNING.signature_len()];
        self.inner
            .sign(message, &mut signature)
            .map_err(|_| CryptoError::SigningError)?;
        Ok(signature)
    }

    /// Obtiene la llave pública
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.public_key().as_ref().to_vec()
    }
}

/// Mecanismo de verificación de firmas ML-DSA
pub struct Verifier;

impl Verifier {
    /// Verifica una firma ML-DSA contra una llave pública y un mensaje
    pub fn verify(public_key_bytes: &[u8], message: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        let pk = UnparsedPublicKey::new(&ML_DSA_65, public_key_bytes);
        pk.verify(message, signature)
            .map_err(|_| CryptoError::VerificationError)
    }
}

/// Intercambio de llaves híbrido (X25519 + ML-KEM)
pub struct HybridKeyExchange;

impl HybridKeyExchange {
    /// Genera un par de llaves efímero X25519
    pub fn generate_x25519_keypair() -> Result<EphemeralPrivateKey, CryptoError> {
        let rng = SystemRandom::new();
        EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| CryptoError::KeyGenError)
    }

    /// Genera un par de llaves ML-KEM
    pub fn generate_kem_keypair() -> Result<(EncapsulationKey, DecapsulationKey), CryptoError> {
        let dk = DecapsulationKey::generate(&ML_KEM_768).map_err(|_| CryptoError::KeyGenError)?;
        let ek = dk.encapsulation_key().map_err(|_| CryptoError::KeyGenError)?;
        Ok((ek, dk))
    }

    /// Ejecuta el acuerdo de llaves X25519
    pub fn agree_x25519(my_private_key: EphemeralPrivateKey, peer_public_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let peer_pk = AgreementPublicKey::new(&X25519, peer_public_key);
        let result = agree_ephemeral(my_private_key, &peer_pk, CryptoError::AgreementError, |secret| {
            Ok(secret.to_vec())
        })?;
        Ok(result)
    }

    /// Encapsula un secreto ML-KEM
    pub fn encapsulate_kem(ek_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        let ek = EncapsulationKey::new(&ML_KEM_768, ek_bytes).map_err(|_| CryptoError::KeyGenError)?;
        let (ciphertext, shared_secret) = ek
            .encapsulate()
            .map_err(|_| CryptoError::EncapsulationError)?;
        Ok((ciphertext.as_ref().to_vec(), shared_secret.as_ref().to_vec()))
    }

    /// Desencapsula un secreto ML-KEM
    pub fn decapsulate_kem(dk: &DecapsulationKey, ciphertext_bytes: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let ct = Ciphertext::from(ciphertext_bytes);
        let shared_secret = dk
            .decapsulate(ct)
            .map_err(|_| CryptoError::DecapsulationError)?;
        Ok(shared_secret.as_ref().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_flow() {
        let keypair = SigningKeyPair::generate().expect("Failed to generate ML-DSA keypair");
        let message = b"Mensaje ultra secreto de RedFlag 2.1";
        let signature = keypair.sign(message).expect("Failed to sign message");
        
        let pubkey = keypair.public_key();
        assert!(Verifier::verify(&pubkey, message, &signature).is_ok());
    }

    #[test]
    fn test_hybrid_flow() {
        // Iniciador (Alice)
        let alice_x25519_priv = HybridKeyExchange::generate_x25519_keypair().unwrap();
        let alice_x25519_pub = alice_x25519_priv.compute_public_key().unwrap().as_ref().to_vec();
        
        let (alice_kem_ek, alice_kem_dk) = HybridKeyExchange::generate_kem_keypair().unwrap();
        // key_bytes() devuelve un Result, por lo que desglosamos y convertimos a Vec<u8>
        let alice_kem_pub = alice_kem_ek.key_bytes().unwrap().as_ref().to_vec();

        // Respondedor (Bob)
        let bob_x25519_priv = HybridKeyExchange::generate_x25519_keypair().unwrap();
        let bob_x25519_pub = bob_x25519_priv.compute_public_key().unwrap().as_ref().to_vec();

        // Bob recibe las llaves de Alice y genera su parte
        let bob_classic_secret = HybridKeyExchange::agree_x25519(bob_x25519_priv, &alice_x25519_pub).unwrap();
        let (kem_ciphertext, bob_quantum_secret) = HybridKeyExchange::encapsulate_kem(&alice_kem_pub).unwrap();
        let bob_hybrid_secret = combine_secrets(&bob_classic_secret, &bob_quantum_secret);

        // Alice recibe la respuesta de Bob
        let alice_classic_secret = HybridKeyExchange::agree_x25519(alice_x25519_priv, &bob_x25519_pub).unwrap();
        let alice_quantum_secret = HybridKeyExchange::decapsulate_kem(&alice_kem_dk, &kem_ciphertext).unwrap();
        let alice_hybrid_secret = combine_secrets(&alice_classic_secret, &alice_quantum_secret);

        // Ambos deben tener el mismo secreto híbrido
        assert_eq!(alice_hybrid_secret, bob_hybrid_secret);
        assert_eq!(alice_hybrid_secret.shared_key.len(), 32);
    }
}
