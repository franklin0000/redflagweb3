/// Identidad persistente del nodo — sobrevive reinicios
///
/// Guarda en disco:
/// - `node_libp2p.key`    → Ed25519 keypair para la capa P2P (protobuf)
/// - `node_validator.key` → ML-DSA keypair para firmar vértices (PKCS#8 hex)
///
/// En futuros releases: cifrar con passphrase o hardware key.

use std::fs;
use std::path::{Path, PathBuf};
use libp2p::identity;
use redflag_crypto::SigningKeyPair;

const LIBP2P_KEY_FILE: &str = "node_libp2p.key";
const VALIDATOR_KEY_FILE: &str = "node_validator.key";

pub struct NodeIdentity {
    pub libp2p_keypair: identity::Keypair,
    pub signing_keypair: SigningKeyPair,
    pub data_dir: PathBuf,
}

impl NodeIdentity {
    /// Carga identidad desde disco o genera una nueva
    pub fn load_or_generate(data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = PathBuf::from(data_dir);
        fs::create_dir_all(&data_dir)?;

        let libp2p_path = data_dir.join(LIBP2P_KEY_FILE);
        let validator_path = data_dir.join(VALIDATOR_KEY_FILE);

        let libp2p_keypair = Self::load_or_generate_libp2p(&libp2p_path)?;
        let signing_keypair = Self::load_or_generate_validator(&validator_path)?;

        let peer_id = libp2p::PeerId::from(libp2p_keypair.public());
        let validator_pub = signing_keypair.public_key();

        println!("🔑 Identidad del nodo:");
        println!("   PeerID    : {}", peer_id);
        println!("   Validador : {}", hex::encode(&validator_pub[..12.min(validator_pub.len())]));
        println!("   Directorio: {}", data_dir.display());

        Ok(Self { libp2p_keypair, signing_keypair, data_dir })
    }

    fn load_or_generate_libp2p(path: &Path) -> Result<identity::Keypair, Box<dyn std::error::Error>> {
        if path.exists() {
            let bytes = fs::read(path)?;
            let kp = identity::Keypair::from_protobuf_encoding(&bytes)?;
            println!("✅ Identidad P2P cargada desde disco");
            Ok(kp)
        } else {
            let kp = identity::Keypair::generate_ed25519();
            let bytes = kp.to_protobuf_encoding()?;
            fs::write(path, &bytes)?;
            println!("🆕 Nueva identidad P2P generada → {}", path.display());
            Ok(kp)
        }
    }

    fn load_or_generate_validator(path: &Path) -> Result<SigningKeyPair, Box<dyn std::error::Error>> {
        if path.exists() {
            let hex_str = fs::read_to_string(path)?;
            let raw = hex::decode(hex_str.trim())?;
            let kp: SigningKeyPair = postcard::from_bytes::<_>(&raw)?;
            println!("✅ Llave validadora ML-DSA cargada desde disco");
            Ok(kp)
        } else {
            let kp = SigningKeyPair::generate()?;
            let bytes = postcard::to_allocvec(&kp)?;
            fs::write(path, hex::encode(&bytes))?;
            println!("🆕 Nueva llave ML-DSA generada → {}", path.display());
            Ok(kp)
        }
    }

    pub fn peer_id(&self) -> libp2p::PeerId {
        libp2p::PeerId::from(self.libp2p_keypair.public())
    }

    pub fn validator_pubkey(&self) -> Vec<u8> {
        self.signing_keypair.public_key()
    }
}
