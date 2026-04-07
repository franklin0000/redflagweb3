use libp2p::{swarm::SwarmEvent, Multiaddr};
use std::error::Error;
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
        let kp: SigningKeyPair = bincode::deserialize(&bytes)?;
        println!("💧 Faucet: llave cargada desde {}", faucet_key_path);
        kp
    } else {
        let kp = SigningKeyPair::generate()?;
        let pkcs8_bytes = bincode::serialize(&kp)?;
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
    state_db.ensure_faucet(&faucet_address, 500_000_000)?;
    println!("💾 Estado: {}", state_db_path);

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
    tokio::spawn(async move {
        if let Err(e) = rpc::run_server(rpc_consensus, rpc_port, rpc_peer_id, rpc_faucet_key, rpc_faucet_addr, rpc_ws_tx, node_start_time).await {
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
    // Soporta BOOTSTRAP_PEERS=addr1,addr2,addr3 o BOOTSTRAP_PEER=addr
    let bootstrap_list: Vec<String> = if let Ok(peers) = env::var("BOOTSTRAP_PEERS") {
        peers.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else if let Ok(peer) = env::var("BOOTSTRAP_PEER") {
        vec![peer]
    } else {
        vec![]
    };
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

                            let msg = NetworkMessage::NewBlock(bincode::serialize(&vertex).unwrap());
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
                            println!("📍 Escuchando: {}/p2p/{}", address, peer_id);
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
                            match bincode::deserialize::<NetworkMessage>(&message.data) {
                                Ok(NetworkMessage::NewTransaction(data)) => {
                                    if let Ok(tx) = bincode::deserialize::<Transaction>(&data) {
                                        node.consensus.mempool.add_transaction(tx);
                                    }
                                }
                                Ok(NetworkMessage::NewEncryptedTransaction(data)) => {
                                    if let Ok(etx) = bincode::deserialize::<EncryptedTransaction>(&data) {
                                        node.consensus.mempool.add_encrypted_transaction(etx);
                                    }
                                }
                                Ok(NetworkMessage::NewBlock(data)) => {
                                    if let Ok(vertex) = bincode::deserialize::<redflag_consensus::Vertex>(&data) {
                                        let v_id = vertex.id();
                                        // Parent-fetching: solicitar padres faltantes
                                        for parent_id in &vertex.parents {
                                            if node.consensus.dag.get_vertex(parent_id).is_none() {
                                                node.request_vertex(propagation_source, *parent_id);
                                            }
                                        }
                                        let _ = node.consensus.dag.insert_vertex(vertex);
                                        println!("📦 Bloque recibido de {}: {}",
                                            propagation_source,
                                            hex::encode(&v_id[..4])
                                        );
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
