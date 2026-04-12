use libp2p::{swarm::SwarmEvent, Multiaddr};
use std::error::Error;
use reqwest;
use redflag_network::{
    RedFlagNode, NetworkMessage, RedFlagBehaviourEvent, rpc,
    identity_manager::NodeIdentity,
};
use redflag_core::{Transaction, EncryptedTransaction, CHAIN_ID};
use redflag_consensus::{ConsensusEngine, Dag, Mempool, threshold::ThresholdMempool};
use redflag_crypto::SigningKeyPair;
use redflag_state::StateDB;
use tokio::time::{self, Duration};
use tokio::sync::broadcast;
use std::collections::HashSet;
use std::sync::Arc;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN) // Solo errores de libp2p interno
        .init();

    println!("╔══════════════════════════════════════════════╗");
    println!("║     redflag.web3 — Post-Quantum Blockchain   ║");
    println!("║     Bullshark DAG • ML-KEM • ML-DSA          ║");
    println!("╚══════════════════════════════════════════════╝");

    // ── 1. Identidad persistente ──────────────────────────────────────────────
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./node_data".to_string());
    let identity = NodeIdentity::load_or_generate(&data_dir)?;
    let peer_id = identity.peer_id();
    let own_validator_key = identity.validator_pubkey();

    // ── 2. Faucet keypair persistente ────────────────────────────────────────
    let faucet_key_path = format!("{}/faucet.key", data_dir);
    let faucet_kp = if Path::new(&faucet_key_path).exists() {
        let hex = std::fs::read_to_string(&faucet_key_path)?;
        let bytes = hex::decode(hex.trim())?;
        let kp: SigningKeyPair = postcard::from_bytes::<_>(&bytes)?;
        println!("💧 Faucet: llave cargada desde {}", faucet_key_path);
        kp
    } else {
        let kp = SigningKeyPair::generate()?;
        let pkcs8_bytes = postcard::to_allocvec(&kp)?;
        std::fs::write(&faucet_key_path, hex::encode(&pkcs8_bytes))?;
        println!("💧 Faucet: nueva llave generada en {}", faucet_key_path);
        kp
    };
    let faucet_address = hex::encode(faucet_kp.public_key());
    let faucet_key = Arc::new(faucet_kp);

    // ── 3. Base de datos de estado ───────────────────────────────────────────
    let state_db_path = env::var("STATE_DB_PATH")
        .unwrap_or_else(|_| format!("{}/state", data_dir));
    let state_db = Arc::new(StateDB::new(&state_db_path)?);
    state_db.ensure_faucet(&faucet_address, 500_000_000_000_000)?; // 500M RF para testnet
    println!("💾 Estado: {}", state_db_path);

    // ── 3b. Sincronización inicial desde nodo bootstrap ──────────────────────
    if let Ok(sync_url) = env::var("SYNC_FROM") {
        // Solo sincronizar si el estado local está vacío (primer arranque)
        let existing_accounts = state_db.get_all_accounts().len();
        if existing_accounts <= 2 { // solo genesis + faucet
            println!("🔄 Sincronizando estado desde {}…", sync_url);
            match reqwest::get(format!("{}/sync/state", sync_url)).await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let accounts: Vec<redflag_state::Account> =
                            serde_json::from_value(data["accounts"].clone()).unwrap_or_default();
                        let stakes: Vec<redflag_state::StakeRecord> =
                            serde_json::from_value(data["stakes"].clone()).unwrap_or_default();
                        match state_db.restore_snapshot(accounts.clone(), stakes.clone()) {
                            Ok(()) => println!("✅ Snapshot restaurado: {} cuentas, {} stakes",
                                accounts.len(), stakes.len()),
                            Err(e) => eprintln!("⚠️  Error restaurando snapshot: {}", e),
                        }
                    }
                    Err(e) => eprintln!("⚠️  Snapshot inválido desde {}: {}", sync_url, e),
                },
                Err(e) => eprintln!("⚠️  No se pudo conectar a {}: {}", sync_url, e),
            }
        } else {
            println!("ℹ️  Estado ya existente ({} cuentas), sync omitido", existing_accounts);
        }
    }

    // ── 4. WebSocket broadcast channel ──────────────────────────────────────
    let (ws_tx, _ws_rx) = broadcast::channel::<String>(512);
    let ws_tx = Arc::new(ws_tx);
    let node_start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    // ── 5. Consenso Bullshark ─────────────────────────────────────────────────
    let mempool = Arc::new(Mempool::new(identity.signing_keypair));
    let threshold_mempool = Arc::new(ThresholdMempool::new().expect("Threshold init failed"));
    
    let dag_path = env::var("DAG_PATH")
        .unwrap_or_else(|_| format!("{}/dag", data_dir));
    let dag = Arc::new(Dag::new(&dag_path)?);
    let consensus = Arc::new(ConsensusEngine::new(
        dag.clone(),
        mempool.clone(),
        state_db.clone(),
        threshold_mempool.clone(),
        vec![own_validator_key.clone()],
    ));
    println!("⚡ Consenso: {} validadores activos", consensus.validator_count());
    println!("🔐 Threshold: activo (ML-KEM-768)");

    // ── 5b. Auto-stake para validadores bootstrap ────────────────────────────
    if let Ok(stake_str) = env::var("AUTO_STAKE_RF") {
        if let Ok(stake_amount) = stake_str.parse::<u64>() {
            let validator_addr = hex::encode(consensus.mempool.keypair.public_key());
            if state_db.staking.get_stake(&validator_addr).is_none() {
                // Acreditar RF si no tiene balance
                let mut acc = state_db.get_account(&validator_addr)
                    .unwrap_or(redflag_state::Account { address: validator_addr.clone(), balance: 0, nonce: 0 });
                if acc.balance < stake_amount {
                    acc.balance = stake_amount;
                    let _ = state_db.save_account_pub(&acc);
                }
                match state_db.staking.stake(&validator_addr, stake_amount, 0) {
                    Ok(()) => {
                        consensus.add_validator(consensus.mempool.keypair.public_key().to_vec());
                        println!("🔒 Auto-stake: {} RF → validador {}", stake_amount, &validator_addr[..16]);
                    }
                    Err(e) => eprintln!("⚠️  Auto-stake falló: {}", e),
                }
            } else {
                println!("ℹ️  Validador ya tiene stake, auto-stake omitido");
            }
        }
    }

    // ── 5. Servidor RPC + Dashboard ──────────────────────────────────────────
    let rpc_port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8545".to_string())
        .parse()
        .expect("PORT debe ser un número");

    let rpc_peer_id = peer_id.to_string();
    let rpc_consensus = consensus.clone();
    let rpc_faucet_key = faucet_key.clone();
    let rpc_faucet_addr = faucet_address.clone();
    let rpc_ws_tx = ws_tx.clone();
    let listen_addrs: Arc<std::sync::RwLock<Vec<String>>> = Arc::new(std::sync::RwLock::new(Vec::new()));
    let rpc_listen_addrs = listen_addrs.clone();
    tokio::spawn(async move {
        if let Err(e) = rpc::run_server(rpc_consensus, rpc_port, rpc_peer_id, rpc_faucet_key, rpc_faucet_addr, rpc_ws_tx, node_start_time, rpc_listen_addrs).await {
            eprintln!("❌ Error RPC: {}", e);
        }
    });

    // ── 6. Nodo P2P ──────────────────────────────────────────────────────────
    let mut node = RedFlagNode::new_with_consensus(
        identity.libp2p_keypair,
        consensus.clone(),
        own_validator_key,
    ).await?;

    // Puerto P2P fijo (configurable)
    let p2p_port: u16 = env::var("P2P_PORT")
        .unwrap_or_else(|_| "9000".to_string())
        .parse()
        .expect("P2P_PORT debe ser un número");

    node.listen(format!("/ip4/0.0.0.0/tcp/{}", p2p_port).parse()?).await?;
    node.listen(format!("/ip4/0.0.0.0/udp/{}/quic-v1", p2p_port).parse()?).await?;

    println!("🌐 P2P escuchando en TCP/QUIC puerto {}", p2p_port);
    println!("📊 Dashboard:  http://localhost:{}", rpc_port);
    println!("🔑 PeerID:     {}", peer_id);
    println!("🛡️  Chain ID:   {}", CHAIN_ID);

    // ── 7. Bootstrap peers (opcional) ────────────────────────────────────────
    // Soporta BOOTSTRAP_URL=https://node1/network/addrs (para redes donde TCP P2P externo está bloqueado)
    // o BOOTSTRAP_PEERS=addr1,addr2 / BOOTSTRAP_PEER=addr para multiaddrs directos
    let mut bootstrap_list: Vec<String> = vec![];

    if let Ok(url) = env::var("BOOTSTRAP_URL") {
        println!("📡 Obteniendo bootstrap desde {}…", url);
        match reqwest::get(&url).await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    // Preferir IPs privadas (10.x, 172.16-31.x, 192.168.x) para evitar bloqueo externo
                    let addrs: Vec<String> = data["addrs"].as_array().unwrap_or(&vec![])
                        .iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    let private_addrs: Vec<String> = addrs.iter()
                        .filter(|a| a.contains("/ip4/10.") || a.contains("/ip4/172.") || a.contains("/ip4/192.168."))
                        .cloned().collect();
                    bootstrap_list = if !private_addrs.is_empty() { private_addrs } else { addrs };
                    println!("✅ Bootstrap addrs: {:?}", bootstrap_list);
                }
                Err(e) => eprintln!("⚠️  Bootstrap URL parse error: {}", e),
            },
            Err(e) => eprintln!("⚠️  Bootstrap URL fetch failed: {}", e),
        }
    } else if let Ok(peers) = env::var("BOOTSTRAP_PEERS") {
        bootstrap_list = peers.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    } else if let Ok(peer) = env::var("BOOTSTRAP_PEER") {
        bootstrap_list = vec![peer];
    }

    for addr_str in &bootstrap_list {
        match addr_str.parse::<Multiaddr>() {
            Ok(multiaddr) => {
                println!("📡 Conectando a bootstrap: {}", multiaddr);
                node.swarm.dial(multiaddr)?;
            }
            Err(e) => eprintln!("⚠️  Bootstrap peer inválido '{}': {}", addr_str, e),
        }
    }
    println!("🌐 Bootstrap peers configurados: {}", bootstrap_list.len());

    // ── 8. TX genesis (solo si la red es nueva) ───────────────────────────────
    if consensus.get_current_round() == 0 {
        node.consensus.mempool.add_transaction(Transaction::genesis(
            format!("node_{}", hex::encode(&node.own_validator_key[..8])),
            1_000,
        ));
    }

    println!("💧 Faucet: {} RF disponibles en {}…",
        state_db.get_balance(&faucet_address),
        &faucet_address[..16]);

    // ── 9. Loop principal ────────────────────────────────────────────────────
    let mut round_interval = time::interval(Duration::from_millis(200));
    let mut stats_interval = time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            // Tick de ronda — crear vértice si hay TXs pendientes
            _ = round_interval.tick() => {
                let current_round = node.consensus.get_current_round();

                if node.consensus.mempool.pending_transactions.len() > 0 {
                    let parents: HashSet<_> = node.consensus.dag
                        .get_round_certificates(current_round)
                        .into_iter()
                        .map(|c| c.vertex_id)
                        .collect();

                    match node.consensus.mempool.create_vertex(current_round + 1, parents) {
                        Ok(vertex) => {
                            let vertex_id = vertex.id();
                            println!("✨ [Ronda {}] Vértice {} — {} TXs",
                                vertex.round,
                                hex::encode(&vertex_id[..4]),
                                vertex.transactions.len()
                            );

                            let _ = node.consensus.dag.insert_vertex(vertex.clone());

                            let cert = redflag_consensus::Certificate {
                                vertex_id,
                                round: vertex.round,
                                signatures: vec![(
                                    node.consensus.mempool.keypair.public_key().to_vec(),
                                    vertex.signature.clone(),
                                )],
                            };
                            let _ = node.consensus.dag.insert_certificate(cert);

                            let msg = NetworkMessage::NewBlock(postcard::to_allocvec(&vertex).unwrap());
                            node.broadcast_message(msg).await.ok();

                            let ordered = node.consensus.order_transactions(vertex.round);
                            if !ordered.is_empty() {
                                println!("🎊 Bullshark: {} TXs confirmadas ({} validadores)",
                                    ordered.len(),
                                    node.consensus.validator_count()
                                );
                            }
                            // Emitir evento WebSocket de nuevo bloque
                            let _ = ws_tx.send(serde_json::json!({
                                "type": "new_block",
                                "data": {
                                    "round": vertex.round,
                                    "vertex_id": hex::encode(&vertex_id[..4]),
                                    "tx_count": vertex.transactions.len(),
                                    "validators": node.consensus.validator_count(),
                                }
                            }).to_string());
                            node.consensus.advance_round();
                            
                            // Rotar llaves de threshold para la nueva ronda
                            if let Ok(ek) = node.consensus.threshold_mempool.rotate_keys(node.consensus.get_current_round()) {
                                println!("🔐 Threshold: Llaves rotadas para ronda {} (EK: {}...)", 
                                    node.consensus.get_current_round(),
                                    hex::encode(&ek[..4])
                                );
                            }
                        }
                        Err(e) => eprintln!("❌ Error creando vértice: {}", e),
                    }
                }
            }

            // Stats periódicas
            _ = stats_interval.tick() => {
                let s = node.consensus.summary();
                println!("📊 Stats — Ronda:{} Peers:{} Validadores:{} TXs:{} Vertices:{}",
                    s.current_round,
                    node.peer_count(),
                    s.validator_count,
                    s.tx_count,
                    s.total_vertices,
                );
            }

            // Eventos P2P
            event = node.next_event() => {
                if let Some(event) = event {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            let full_addr = format!("{}/p2p/{}", address, peer_id);
                            println!("📍 Escuchando: {}", full_addr);
                            // Registrar en listen_addrs para que /network/addrs las sirva
                            if let Ok(mut addrs) = listen_addrs.write() {
                                if !addrs.contains(&full_addr) {
                                    addrs.push(full_addr);
                                }
                            }
                        }
                        SwarmEvent::ConnectionEstablished { peer_id: remote_peer, .. } => {
                            println!("🤝 Conectado: {}", remote_peer);
                            node.connected_peers.insert(remote_peer);

                            // Iniciar handshake PQC híbrido
                            if let Err(e) = node.initiate_pqc_handshake(remote_peer) {
                                eprintln!("❌ PQC handshake error: {}", e);
                            }

                            // Solicitar llave validadora
                            node.swarm.behaviour_mut().dag_sync.send_request(
                                &remote_peer,
                                redflag_network::ConsensusRequest::GetValidatorKey,
                            );

                            // Sincronizar DAG si vamos detrás
                            node.request_full_sync(remote_peer);
                        }
                        SwarmEvent::ConnectionClosed { peer_id: remote_peer, .. } => {
                            println!("💔 Desconectado: {}", remote_peer);
                            node.connected_peers.remove(&remote_peer);
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::PqcHandshake(e)) => {
                            if let Err(e) = node.handle_pqc_handshake_event(e) {
                                eprintln!("❌ PQC event error: {}", e);
                            }
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::DagSync(e)) => {
                            if let Err(e) = node.handle_dag_sync_event(e) {
                                eprintln!("❌ DAG sync error: {}", e);
                            }
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::Gossipsub(
                            libp2p::gossipsub::Event::Message { propagation_source, message, .. }
                        )) => {
                            match postcard::from_bytes::<NetworkMessage>(&message.data) {
                                Ok(NetworkMessage::NewTransaction(data)) => {
                                    if let Ok(tx) = postcard::from_bytes::<Transaction>(&data) {
                                        node.consensus.mempool.add_transaction(tx);
                                    }
                                }
                                Ok(NetworkMessage::NewEncryptedTransaction(data)) => {
                                    if let Ok(etx) = postcard::from_bytes::<EncryptedTransaction>(&data) {
                                        node.consensus.mempool.add_encrypted_transaction(etx);
                                    }
                                }
                                Ok(NetworkMessage::NewBlock(data)) => {
                                    if let Ok(vertex) = postcard::from_bytes::<redflag_consensus::Vertex>(&data) {
                                        let v_id = vertex.id();
                                        let v_round = vertex.round;
                                        // Parent-fetching: solicitar padres faltantes
                                        for parent_id in &vertex.parents {
                                            if node.consensus.dag.get_vertex(parent_id).is_none() {
                                                node.request_vertex(propagation_source, *parent_id);
                                            }
                                        }
                                        let _ = node.consensus.dag.insert_vertex(vertex);

                                        // Firmar el vértice y difundir nuestro certificado (cert aggregation)
                                        if let Ok(sig) = node.consensus.mempool.keypair.sign(&v_id) {
                                            let cert = redflag_consensus::Certificate {
                                                vertex_id: v_id,
                                                round: v_round,
                                                signatures: vec![(
                                                    node.consensus.mempool.keypair.public_key().to_vec(),
                                                    sig.to_vec(),
                                                )],
                                            };
                                            let _ = node.consensus.dag.insert_certificate(cert.clone());
                                            let cert_msg = NetworkMessage::NewCertificate(
                                                postcard::to_allocvec(&cert).unwrap_or_default()
                                            );
                                            node.broadcast_message(cert_msg).await.ok();
                                        }

                                        println!("📦 Bloque recibido de {}: {}",
                                            propagation_source,
                                            hex::encode(&v_id[..4])
                                        );
                                    }
                                }
                                Ok(NetworkMessage::NewCertificate(data)) => {
                                    if let Ok(cert) = postcard::from_bytes::<redflag_consensus::Certificate>(&data) {
                                        let _ = node.consensus.dag.insert_certificate(cert);
                                        // Intentar commit si algún vértice alcanzó quórum
                                        let cur = node.consensus.get_current_round();
                                        let newly_ordered = node.consensus.order_transactions(cur);
                                        if !newly_ordered.is_empty() {
                                            println!("🎊 Quórum 2f+1: {} TXs confirmadas",
                                                newly_ordered.len());
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::Kademlia(e)) => {
                            node.handle_kademlia_event(e);
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::Mdns(e)) => {
                            node.handle_mdns_event(e);
                        }
                        SwarmEvent::Behaviour(RedFlagBehaviourEvent::Identify(e)) => {
                            node.handle_identify_event(e);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
