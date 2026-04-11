use clap::{Parser, Subcommand};
use redflag_crypto::SigningKeyPair;
use redflag_core::Transaction;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

const RF_UNIT: u64 = 1_000_000; // 1 RF = 1,000,000 microRF

#[derive(Parser)]
#[command(name = "redflag-cli")]
#[command(version = "2.1.0")]
#[command(about = "CLI para interactuar con la blockchain RedFlag 2.1 (ML-DSA-65 + Bullshark DAG)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// URL del nodo RPC
    #[arg(short, long, default_value = "http://localhost:8545")]
    node_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Genera un nuevo par de llaves ML-DSA-65 (post-cuántico)
    Keygen,

    /// Consulta el balance de una dirección
    Balance {
        /// Dirección pública RF (hex). Si se omite, usa redflag.pub
        #[arg(short, long)]
        address: Option<String>,
    },

    /// Realiza una transferencia de RF
    Transfer {
        /// Dirección destino
        #[arg(long)]
        to: String,
        /// Cantidad en RF (no microRF)
        #[arg(long)]
        amount: f64,
        /// Ruta al archivo de llave privada
        #[arg(long, default_value = "redflag.priv")]
        key: String,
    },

    /// Solicita RF del faucet de testnet (máx. 10,000 RF / 24h)
    Faucet {
        /// Dirección destino. Si se omite, usa redflag.pub
        #[arg(short, long)]
        address: Option<String>,
        /// Cantidad en RF (máx. 10,000 RF)
        #[arg(long, default_value_t = 1000.0)]
        amount: f64,
    },

    /// Stakea RF para convertirte en validador (mín. 10,000 RF)
    Stake {
        /// Cantidad en RF a stakear
        #[arg(long)]
        amount: f64,
        /// Ruta al archivo de llave privada
        #[arg(long, default_value = "redflag.priv")]
        key: String,
    },

    /// Inicia el proceso de unstaking (espera 10 rondas)
    Unstake {
        /// Ruta al archivo de llave privada
        #[arg(long, default_value = "redflag.priv")]
        key: String,
    },

    /// Retira RF después del período de unbonding (10 rondas)
    Withdraw {
        /// Ruta al archivo de llave privada
        #[arg(long, default_value = "redflag.priv")]
        key: String,
    },

    /// Consulta información de staking de una dirección
    StakingInfo {
        /// Dirección a consultar. Si se omite, usa redflag.pub
        #[arg(short, long)]
        address: Option<String>,
    },

    /// Realiza un swap en el DEX AMM (x·y=k, 0.3% fee)
    DexSwap {
        /// ID del pool (ej: RF_wETH, RF_wBNB, RF_wMATIC)
        #[arg(long)]
        pool: String,
        /// Dirección del swap: "rf_to_b" o "b_to_rf"
        #[arg(long)]
        direction: String,
        /// Cantidad de entrada en RF (o tokens si direction=b_to_rf)
        #[arg(long)]
        amount: f64,
        /// Ruta al archivo de llave privada
        #[arg(long, default_value = "redflag.priv")]
        key: String,
    },

    /// Lista todos los pools del DEX
    DexPools,

    /// Cotización de swap sin ejecutar
    DexQuote {
        /// ID del pool
        #[arg(long)]
        pool: String,
        /// Dirección: "rf_to_b" o "b_to_rf"
        #[arg(long)]
        direction: String,
        /// Cantidad de entrada en RF
        #[arg(long)]
        amount: f64,
    },

    /// Consulta el estado del nodo
    Status,

    /// Muestra el historial de transacciones de una dirección
    History {
        /// Dirección a consultar. Si se omite, usa redflag.pub
        #[arg(short, long)]
        address: Option<String>,
        /// Número máximo de transacciones a mostrar
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Deserialize)]
struct BalanceResponse {
    address: String,
    balance: u64,
    nonce: u64,
}

#[derive(Deserialize, Serialize)]
struct StatusResponse {
    peer_id: String,
    current_round: u64,
    pending_txs: usize,
    committed_vertices: usize,
    validator_count: usize,
    fee_pool_balance: u64,
    version: String,
}

fn load_keypair(key_path: &str) -> anyhow::Result<SigningKeyPair> {
    if !Path::new(key_path).exists() {
        anyhow::bail!("No se encontró la llave privada en '{}'. Ejecuta 'redflag-cli keygen' primero.", key_path);
    }
    let priv_hex = fs::read_to_string(key_path)?;
    let bytes = hex::decode(priv_hex.trim())
        .map_err(|_| anyhow::anyhow!("Archivo de llave inválido (no es hex)"))?;
    postcard::from_bytes::<SigningKeyPair>(&bytes)
        .map_err(|_| anyhow::anyhow!("Llave privada corrupta o incompatible"))
}

fn load_address(address: &Option<String>) -> anyhow::Result<String> {
    if let Some(a) = address {
        return Ok(a.clone());
    }
    if Path::new("redflag.pub").exists() {
        Ok(fs::read_to_string("redflag.pub")?.trim().to_string())
    } else {
        anyhow::bail!("No se proporcionó dirección y no se encontró redflag.pub. Usa --address o ejecuta 'keygen'.");
    }
}

fn fmt_rf(microrf: u64) -> String {
    if microrf >= RF_UNIT {
        format!("{:.6} RF", microrf as f64 / RF_UNIT as f64)
    } else {
        format!("{} μRF", microrf)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match &cli.command {
        // ── Keygen ──────────────────────────────────────────────────────────────
        Commands::Keygen => {
            println!("🔐 Generando par de llaves Post-Cuánticas (ML-DSA-65)…");
            let keypair = SigningKeyPair::generate()?;
            let pubkey_hex = hex::encode(keypair.public_key());
            let privkey_hex = hex::encode(postcard::to_allocvec(&keypair).unwrap());

            fs::write("redflag.pub", &pubkey_hex)?;
            fs::write("redflag.priv", &privkey_hex)?;

            println!("✅ Llaves generadas con éxito.");
            println!("📍 Dirección: {}", pubkey_hex);
            println!("📁 Guardado:  redflag.pub, redflag.priv");
            println!("⚠️  Guarda redflag.priv de forma segura. Quien lo tenga controla tus fondos.");
        }

        // ── Balance ─────────────────────────────────────────────────────────────
        Commands::Balance { address } => {
            let addr = load_address(address)?;
            let url = format!("{}/balance/{}", cli.node_url, addr.trim());
            let resp = client.get(&url).send().await?.json::<BalanceResponse>().await?;
            println!("💰 Balance de {}…", &resp.address[..20.min(resp.address.len())]);
            println!("   RF:    {}", fmt_rf(resp.balance));
            println!("   Nonce: {}", resp.nonce);
        }

        // ── Transfer ────────────────────────────────────────────────────────────
        Commands::Transfer { to, amount, key } => {
            let keypair = load_keypair(key)?;
            let sender = hex::encode(keypair.public_key());
            let raw_amount = (*amount * RF_UNIT as f64) as u64;

            let nonce_url = format!("{}/account/{}", cli.node_url, sender);
            let nonce: u64 = match client.get(&nonce_url).send().await {
                Ok(r) => r.json::<serde_json::Value>().await
                    .ok().and_then(|v| v["nonce"].as_u64()).unwrap_or(0),
                Err(_) => 0,
            };

            let mut tx = Transaction::new_transfer(
                sender.clone(), to.clone(), raw_amount, redflag_core::MIN_FEE, nonce,
            );
            let msg = postcard::to_allocvec(&tx)?;
            tx.signature = keypair.sign(&msg)?;

            let url = format!("{}/tx", cli.node_url);
            let resp = client.post(&url).json(&tx).send().await?;
            if resp.status().is_success() {
                println!("✅ TX enviada:");
                println!("   De:      {}…", &sender[..20.min(sender.len())]);
                println!("   Para:    {}…", &to[..20.min(to.len())]);
                println!("   Monto:   {}", fmt_rf(raw_amount));
                println!("   Nonce:   {}", nonce);
            } else {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("❌ Error: {}", body);
            }
        }

        // ── Faucet ──────────────────────────────────────────────────────────────
        Commands::Faucet { address, amount } => {
            let addr = load_address(address)?;
            let raw_amount = (*amount * RF_UNIT as f64) as u64;

            let url = format!("{}/wallet/faucet", cli.node_url);
            let body = serde_json::json!({ "address": addr.trim(), "amount": raw_amount });
            let resp = client.post(&url).json(&body).send().await?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.unwrap_or_default();

            if status.is_success() {
                println!("💧 Faucet: {} enviados a {}…", fmt_rf(raw_amount), &addr[..20.min(addr.len())]);
                if let Some(hash) = json["tx_hash"].as_str() {
                    println!("   TX: {}", hash);
                }
            } else {
                eprintln!("❌ Faucet error: {}", json["message"].as_str().unwrap_or_default());
            }
        }

        // ── Stake ───────────────────────────────────────────────────────────────
        Commands::Stake { amount, key } => {
            let keypair = load_keypair(key)?;
            let priv_hex = hex::encode(postcard::to_allocvec(&keypair)?);
            let raw_amount = (*amount * RF_UNIT as f64) as u64;

            if raw_amount < redflag_core::MIN_STAKE {
                anyhow::bail!(
                    "Stake mínimo: {} RF. Pedido: {} RF",
                    redflag_core::MIN_STAKE / RF_UNIT,
                    amount
                );
            }

            let url = format!("{}/staking/stake", cli.node_url);
            let body = serde_json::json!({ "private_key_hex": priv_hex, "amount": raw_amount });
            let resp = client.post(&url).json(&body).send().await?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.unwrap_or_default();

            if status.is_success() && json["success"].as_bool().unwrap_or(false) {
                let address = hex::encode(keypair.public_key());
                println!("🔒 Stake registrado:");
                println!("   Dirección: {}…", &address[..20.min(address.len())]);
                println!("   Monto:     {}", fmt_rf(raw_amount));
                println!("   Estado:    Validador activo en siguiente ronda");
            } else {
                eprintln!("❌ Error: {}", json["error"].as_str().unwrap_or("desconocido"));
            }
        }

        // ── Unstake ─────────────────────────────────────────────────────────────
        Commands::Unstake { key } => {
            let keypair = load_keypair(key)?;
            let priv_hex = hex::encode(postcard::to_allocvec(&keypair)?);

            let url = format!("{}/staking/unstake", cli.node_url);
            let body = serde_json::json!({ "private_key_hex": priv_hex });
            let resp = client.post(&url).json(&body).send().await?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.unwrap_or_default();

            if status.is_success() && json["success"].as_bool().unwrap_or(false) {
                println!("⏳ Unbonding iniciado:");
                println!("   {}", json["message"].as_str().unwrap_or("OK"));
                if let Some(r) = json["unlock_round"].as_u64() {
                    println!("   Retira en ronda: {}", r);
                }
            } else {
                eprintln!("❌ Error: {}", json["error"].as_str().unwrap_or("desconocido"));
            }
        }

        // ── Withdraw ────────────────────────────────────────────────────────────
        Commands::Withdraw { key } => {
            let keypair = load_keypair(key)?;
            let priv_hex = hex::encode(postcard::to_allocvec(&keypair)?);

            let url = format!("{}/staking/withdraw", cli.node_url);
            let body = serde_json::json!({ "private_key_hex": priv_hex });
            let resp = client.post(&url).json(&body).send().await?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.unwrap_or_default();

            if status.is_success() && json["success"].as_bool().unwrap_or(false) {
                let amount = json["amount"].as_u64().unwrap_or(0);
                println!("✅ Unstake completado:");
                println!("   {} devueltos a tu wallet", fmt_rf(amount));
            } else {
                eprintln!("❌ Error: {}", json["error"].as_str().unwrap_or("desconocido"));
            }
        }

        // ── StakingInfo ─────────────────────────────────────────────────────────
        Commands::StakingInfo { address } => {
            let addr = load_address(address)?;
            let url = format!("{}/staking/rewards/{}", cli.node_url, addr.trim());
            let json: serde_json::Value = client.get(&url).send().await?.json().await?;

            println!("📊 Staking de {}…", &addr[..20.min(addr.len())]);
            if let Some(stake) = json["stake"].as_object() {
                let amount = stake["amount"].as_u64().unwrap_or(0);
                let since = stake["since_round"].as_u64().unwrap_or(0);
                let unbonding = stake["unbonding_at"].as_u64().unwrap_or(0);
                println!("   Stake:       {}", fmt_rf(amount));
                println!("   Desde ronda: {}", since);
                if unbonding > 0 {
                    println!("   Unbonding:   hasta ronda {}", unbonding);
                }
            } else {
                println!("   Sin stake activo");
            }
            if let Some(reward) = json["estimated_reward"].as_u64() {
                println!("   Reward est.: {}", fmt_rf(reward));
            }
            let total = json["total_staked"].as_u64().unwrap_or(0);
            println!("   Total red:   {}", fmt_rf(total));
        }

        // ── DexPools ────────────────────────────────────────────────────────────
        Commands::DexPools => {
            let url = format!("{}/dex/pools", cli.node_url);
            let json: serde_json::Value = client.get(&url).send().await?.json().await?;
            let pools = json["pools"].as_array().cloned().unwrap_or_default();

            println!("💱 DEX Pools ({}):", pools.len());
            println!("  {:15} {:>15} {:>15} {:>12} {:>10}", "Pool", "RF Reserva", "Token Reserva", "Precio", "Volume");
            println!("  {:-<15} {:-<15} {:-<15} {:-<12} {:-<10}", "", "", "", "", "");
            for p in &pools {
                let id       = p["pool_id"].as_str().unwrap_or("?");
                let res_rf   = p["reserve_rf"].as_u64().unwrap_or(0);
                let res_b    = p["reserve_b"].as_u64().unwrap_or(0);
                let price    = p["price"].as_f64().unwrap_or(0.0);
                let volume   = p["volume_rf"].as_u64().unwrap_or(0);
                println!("  {:15} {:>15} {:>15} {:>12.6} {:>10}",
                    id,
                    fmt_rf(res_rf),
                    fmt_rf(res_b),
                    price / 1_000_000.0,
                    fmt_rf(volume),
                );
            }
        }

        // ── DexQuote ────────────────────────────────────────────────────────────
        Commands::DexQuote { pool, direction, amount } => {
            let raw_in = (*amount * RF_UNIT as f64) as u64;
            let url = format!("{}/dex/quote", cli.node_url);
            let body = serde_json::json!({ "pool_id": pool, "direction": direction, "amount_in": raw_in });
            let json: serde_json::Value = client.post(&url).json(&body).send().await?.json().await?;

            if let Some(err) = json["error"].as_str() {
                eprintln!("❌ Error: {}", err);
                return Ok(());
            }
            let out  = json["amount_out"].as_u64().unwrap_or(0);
            let fee  = json["fee"].as_u64().unwrap_or(0);
            let impact = json["price_impact"].as_f64().unwrap_or(0.0);
            println!("📊 Cotización DEX ({}):", pool);
            println!("   Dirección:    {}", direction);
            println!("   Entrada:      {}", fmt_rf(raw_in));
            println!("   Salida est.:  {}", fmt_rf(out));
            println!("   Fee (0.3%):   {}", fmt_rf(fee));
            println!("   Price impact: {:.3}%", impact);
        }

        // ── DexSwap ─────────────────────────────────────────────────────────────
        Commands::DexSwap { pool, direction, amount, key } => {
            let keypair = load_keypair(key)?;
            let priv_hex = hex::encode(postcard::to_allocvec(&keypair)?);
            let raw_in = (*amount * RF_UNIT as f64) as u64;

            let url = format!("{}/dex/swap", cli.node_url);
            let body = serde_json::json!({
                "private_key_hex": priv_hex,
                "pool_id": pool,
                "direction": direction,
                "amount_in": raw_in,
                "min_amount_out": 0,
            });
            let resp = client.post(&url).json(&body).send().await?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.unwrap_or_default();

            if status.is_success() {
                let out = json["amount_out"].as_u64().unwrap_or(0);
                let price = json["price"].as_f64().unwrap_or(0.0);
                println!("✅ Swap ejecutado:");
                println!("   Pool:    {}", pool);
                println!("   Entrada: {}", fmt_rf(raw_in));
                println!("   Salida:  {}", fmt_rf(out));
                println!("   Precio:  {:.6}", price / 1_000_000.0);
            } else {
                eprintln!("❌ Error: {}", json["error"].as_str().unwrap_or("desconocido"));
            }
        }

        // ── Status ──────────────────────────────────────────────────────────────
        Commands::Status => {
            let url = format!("{}/status", cli.node_url);
            let resp = client.get(&url).send().await?.json::<StatusResponse>().await?;

            println!("📊 redflag.web3 — Estado del Nodo");
            println!("   PeerID:             {}…", &resp.peer_id[..20.min(resp.peer_id.len())]);
            println!("   Ronda actual:       {}", resp.current_round);
            println!("   TXs en mempool:     {}", resp.pending_txs);
            println!("   Vértices conf.:     {}", resp.committed_vertices);
            println!("   Validadores:        {}", resp.validator_count);
            println!("   Fee pool:           {}", fmt_rf(resp.fee_pool_balance));
            println!("   Versión:            {}", resp.version);
        }

        // ── History ─────────────────────────────────────────────────────────────
        Commands::History { address, limit } => {
            let addr = load_address(address)?;
            let url = format!("{}/history/{}", cli.node_url, addr.trim());
            let json: serde_json::Value = client.get(&url).send().await?.json().await?;

            let txs = json["history"].as_array().cloned().unwrap_or_default();
            let show = txs.len().min(*limit);
            println!("📜 Historial de {}… ({} TXs):", &addr[..20.min(addr.len())], txs.len());
            for tx in txs.iter().take(show) {
                let sender   = tx["sender"].as_str().unwrap_or("?");
                let receiver = tx["receiver"].as_str().unwrap_or("?");
                let amount   = tx["amount"].as_u64().unwrap_or(0);
                let ts       = tx["timestamp"].as_u64().unwrap_or(0);
                let arrow = if sender.starts_with(&addr[..20.min(addr.len())]) { "→" } else { "←" };
                println!("   {} {} {}… | {} | ts:{}", arrow,
                    fmt_rf(amount),
                    if arrow == "→" { &receiver[..16.min(receiver.len())] } else { &sender[..16.min(sender.len())] },
                    if arrow == "→" { "enviado" } else { "recibido" },
                    ts,
                );
            }
        }
    }

    Ok(())
}
