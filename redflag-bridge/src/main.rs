mod types;
mod evm;
mod redflag;
mod state;
mod relayer;
mod api;

use std::sync::Arc;
use relayer::{Relayer, RelayerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║   RedFlag 2.1 — Cross-Chain Bridge Relayer       ║");
    println!("║   EVM (ETH/BSC/Polygon) <-> RedFlag              ║");
    println!("╚══════════════════════════════════════════════════╝");

    let config = RelayerConfig::default();
    let api_port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "10000".to_string())
        .parse().unwrap_or(10000);

    println!("📋 Configuración del bridge:");
    println!("   RedFlag node:  {}", config.rf_node_url);
    println!("   Data dir:      {}", config.bridge_data_dir);
    println!("   Poll interval: {}s", config.poll_interval_secs);
    println!("   Confirmations: {} bloques", config.confirmations);
    println!("   Max por TX:    {} RF", config.max_amount_per_tx);

    let relayer = Arc::new(Relayer::new(config)?);
    let relayer_clone = relayer.clone();

    // Spawn API server
    tokio::spawn(async move {
        api::run_api(relayer_clone, api_port).await;
    });

    // Run relayer main loop
    relayer.run().await;
    Ok(())
}
