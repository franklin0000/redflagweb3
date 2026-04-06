use clap::{Parser, Subcommand};
use redflag_crypto::SigningKeyPair;
use redflag_core::Transaction;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "redflag-cli")]
#[command(about = "CLI para interactuar con la blockchain RedFlag 2.1", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "http://localhost:8545")]
    node_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Genera un nuevo par de llaves ML-DSA (PQC)
    Keygen,
    /// Consulta el balance de una dirección
    Balance {
        #[arg(short, long)]
        address: Option<String>,
    },
    /// Realiza una transferencia de tokens
    Transfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: u64,
        #[arg(short, long, default_value = "redflag.priv")]
        key: String,
    },
    /// Consulta el estado del nodo
    Status,
}

#[derive(Deserialize)]
struct BalanceResponse {
    address: String,
    balance: u64,
}

#[derive(Deserialize, Serialize)]
struct StatusResponse {
    peer_id: String,
    current_round: u64,
    pending_txs: usize,
    committed_vertices: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match &cli.command {
        Commands::Keygen => {
            println!("🔐 Generando llaves Post-Cuánticas (ML-DSA-65)...");
            let keypair = SigningKeyPair::generate()?;
            let pubkey_hex = hex::encode(keypair.public_key());
            let privkey_hex = hex::encode(bincode::serialize(&keypair).unwrap());

            fs::write("redflag.pub", &pubkey_hex)?;
            fs::write("redflag.priv", &privkey_hex)?;

            println!("✅ Llaves generadas con éxito.");
            println!("📍 Dirección Pública: {}", pubkey_hex);
            println!("📁 Archivos guardados: redflag.pub, redflag.priv");
        }
        Commands::Balance { address } => {
            let addr = if let Some(a) = address {
                a.clone()
            } else {
                if Path::new("redflag.pub").exists() {
                    fs::read_to_string("redflag.pub")?
                } else {
                    anyhow::bail!("No se proporcionó dirección ni se encontró redflag.pub");
                }
            };

            let url = format!("{}/balance/{}", cli.node_url, addr);
            let resp = client.get(url).send().await?.json::<BalanceResponse>().await?;
            
            println!("💰 Balance de {}: {} RF", resp.address, resp.balance);
        }
        Commands::Transfer { to, amount, key } => {
            if !Path::new(key).exists() {
                anyhow::bail!("No se encontró la llave privada en {}", key);
            }

            let priv_hex = fs::read_to_string(key)?;
            let keypair: SigningKeyPair = bincode::deserialize(&hex::decode(priv_hex.trim())?)?;
            let sender = hex::encode(keypair.public_key());

            // Obtener nonce actual del nodo
            let nonce_url = format!("{}/account/{}", cli.node_url, sender);
            let nonce: u64 = match client.get(&nonce_url).send().await {
                Ok(r) => r.json::<serde_json::Value>().await
                    .ok().and_then(|v| v["nonce"].as_u64()).unwrap_or(0),
                Err(_) => 0,
            };

            let mut tx = Transaction::new_transfer(
                sender.clone(),
                to.clone(),
                *amount,
                redflag_core::MIN_FEE,
                nonce,
            );

            // Firmar con ML-DSA
            let msg = bincode::serialize(&tx)?;
            tx.signature = keypair.sign(&msg)?;

            let url = format!("{}/tx", cli.node_url);
            let resp = client.post(url).json(&tx).send().await?;

            if resp.status().is_success() {
                println!("✅ TX enviada: {} → {} ({} RF, nonce: {})", sender, to, amount, nonce);
            } else {
                let body = resp.text().await.unwrap_or_default();
                println!("❌ Error: {}", body);
            }
        }
        Commands::Status => {
            let url = format!("{}/status", cli.node_url);
            let resp = client.get(url).send().await?.json::<StatusResponse>().await?;
            
            println!("📊 Estado del Nodo:");
            println!("  - Red: {}", resp.peer_id);
            println!("  - Ronda Actual: {}", resp.current_round);
            println!("  - TXs en Mempool: {}", resp.pending_txs);
            println!("  - Vértices Confirmados: {}", resp.committed_vertices);
        }
    }

    Ok(())
}
