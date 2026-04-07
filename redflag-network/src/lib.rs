use libp2p::{
    gossipsub, identify, kad, mdns,
    request_response,
    swarm::{NetworkBehaviour, SwarmEvent},
    identity,
    Multiaddr,
    PeerId,
    Swarm,
    StreamProtocol,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use redflag_consensus::{ConsensusEngine, Vertex, Certificate};
use futures::StreamExt;

pub mod identity_manager;
pub mod rpc;

pub const TOPIC_TRANSACTIONS: &str = "redflag/txs/1.0.0";
pub const TOPIC_BLOCKS: &str = "redflag/blocks/1.0.0";
pub const PROTOCOL_VERSION: &str = "/redflag/2.1.0";

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    NewTransaction(Vec<u8>),
    NewEncryptedTransaction(Vec<u8>),
    NewBlock(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PqcRequest {
    InitHandshake {
        x25519_public: Vec<u8>,
        kem_public: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PqcResponse {
    HandshakeResponse {
        x25519_public: Vec<u8>,
        kem_ciphertext: Vec<u8>,
    },
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusRequest {
    GetVertex([u8; 32]),
    GetCertificate(u64),
    GetValidatorKey,
    SyncFrom(u64), // Sincronizar vértices desde esta ronda
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusResponse {
    Vertex(Option<Vertex>),
    Certificates(Vec<Certificate>),
    ValidatorKey(Vec<u8>),
    SyncBatch(Vec<Vertex>), // Batch de vértices para sincronización
    Pong,
}

#[derive(NetworkBehaviour)]
pub struct RedFlagBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub pqc_handshake: request_response::cbor::Behaviour<PqcRequest, PqcResponse>,
    pub dag_sync: request_response::cbor::Behaviour<ConsensusRequest, ConsensusResponse>,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
}

pub struct RedFlagNode {
    pub swarm: Swarm<RedFlagBehaviour>,
    pub consensus: Arc<ConsensusEngine>,
    pub own_validator_key: Vec<u8>,
    pub session_keys: std::collections::HashMap<PeerId, redflag_crypto::HybridSecret>,
    pub pending_handshakes: std::collections::HashMap<PeerId, (redflag_crypto::EphemeralPrivateKey, redflag_crypto::DecapsulationKey)>,
    pub connected_peers: std::collections::HashSet<PeerId>,
}

impl RedFlagNode {
    pub async fn new_with_consensus(
        keypair: identity::Keypair,
        consensus: Arc<ConsensusEngine>,
        own_validator_key: Vec<u8>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let local_peer_id = PeerId::from(keypair.public());

        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|key| {
                // Gossipsub — propagación de TXs y bloques
                let message_id_fn = |message: &gossipsub::Message| {
                    let mut s = std::collections::hash_map::DefaultHasher::new();
                    std::hash::Hash::hash(&message.data, &mut s);
                    gossipsub::MessageId::from(std::hash::Hasher::finish(&s).to_string())
                };
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(std::time::Duration::from_secs(1))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(message_id_fn)
                    .build()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )?;

                // Kademlia DHT — descubrimiento global de peers
                let store = kad::store::MemoryStore::new(local_peer_id);
                let mut kademlia = kad::Behaviour::new(local_peer_id, store);
                kademlia.set_mode(Some(kad::Mode::Server));

                // mDNS — descubrimiento local automático (LAN)
                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    local_peer_id,
                ).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

                // Identify — intercambio de versión y capacidades
                let identify = identify::Behaviour::new(
                    identify::Config::new(PROTOCOL_VERSION.to_string(), key.public())
                        .with_agent_version(format!("redflag/{}", env!("CARGO_PKG_VERSION")))
                );

                Ok(RedFlagBehaviour {
                    gossipsub,
                    pqc_handshake: request_response::cbor::Behaviour::new(
                        [(StreamProtocol::new("/redflag/pqc/1.0.0"), request_response::ProtocolSupport::Full)],
                        request_response::Config::default(),
                    ),
                    dag_sync: request_response::cbor::Behaviour::new(
                        [(StreamProtocol::new("/redflag/dag/1.0.0"), request_response::ProtocolSupport::Full)],
                        request_response::Config::default(),
                    ),
                    kademlia,
                    mdns,
                    identify,
                })
            })?
            .build();

        let mut node = Self {
            swarm,
            consensus,
            own_validator_key,
            session_keys: std::collections::HashMap::new(),
            pending_handshakes: std::collections::HashMap::new(),
            connected_peers: std::collections::HashSet::new(),
        };

        let tx_topic = gossipsub::IdentTopic::new(TOPIC_TRANSACTIONS);
        let block_topic = gossipsub::IdentTopic::new(TOPIC_BLOCKS);
        node.swarm.behaviour_mut().gossipsub.subscribe(&tx_topic)?;
        node.swarm.behaviour_mut().gossipsub.subscribe(&block_topic)?;

        Ok(node)
    }

    pub fn initiate_pqc_handshake(&mut self, peer_id: PeerId) -> Result<(), Box<dyn std::error::Error>> {
        use redflag_crypto::HybridKeyExchange;

        let x25519_priv = HybridKeyExchange::generate_x25519_keypair().map_err(|e| e.to_string())?;
        let x25519_pub = x25519_priv.compute_public_key().map_err(|_| "pubkey error")?.as_ref().to_vec();

        let (kem_ek, kem_dk) = HybridKeyExchange::generate_kem_keypair().map_err(|e| e.to_string())?;
        let kem_pub = kem_ek.key_bytes().map_err(|_| "kem bytes error")?.as_ref().to_vec();

        self.pending_handshakes.insert(peer_id, (x25519_priv, kem_dk));
        self.swarm.behaviour_mut().pqc_handshake.send_request(
            &peer_id,
            PqcRequest::InitHandshake { x25519_public: x25519_pub, kem_public: kem_pub },
        );
        Ok(())
    }

    pub fn handle_pqc_handshake_event(
        &mut self,
        event: request_response::Event<PqcRequest, PqcResponse>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use libp2p::request_response::Message;
        use redflag_crypto::HybridKeyExchange;

        match event {
            request_response::Event::Message { peer, message, connection_id: _ } => match message {
                Message::Request { request, channel, .. } => {
                    let PqcRequest::InitHandshake { x25519_public, kem_public } = request;
                    let our_x25519_priv = HybridKeyExchange::generate_x25519_keypair().map_err(|e| e.to_string())?;
                    let our_x25519_pub = our_x25519_priv.compute_public_key().map_err(|_| "pubkey")?.as_ref().to_vec();
                    let classic_secret = HybridKeyExchange::agree_x25519(our_x25519_priv, &x25519_public).map_err(|e| e.to_string())?;
                    let (kem_ciphertext, quantum_secret) = HybridKeyExchange::encapsulate_kem(&kem_public).map_err(|e| e.to_string())?;
                    let hybrid_secret = redflag_crypto::combine_secrets(&classic_secret, &quantum_secret);
                    self.session_keys.insert(peer, hybrid_secret);
                    println!("🛡️  Handshake PQC completado con {:?}", peer);
                    self.swarm.behaviour_mut().pqc_handshake.send_response(
                        channel,
                        PqcResponse::HandshakeResponse { x25519_public: our_x25519_pub, kem_ciphertext },
                    ).map_err(|_| "send response failed")?;
                }
                Message::Response { response, .. } => {
                    if let PqcResponse::HandshakeResponse { x25519_public, kem_ciphertext } = response {
                        if let Some((our_x25519_priv, our_kem_dk)) = self.pending_handshakes.remove(&peer) {
                            let classic_secret = HybridKeyExchange::agree_x25519(our_x25519_priv, &x25519_public).map_err(|e| e.to_string())?;
                            let quantum_secret = HybridKeyExchange::decapsulate_kem(&our_kem_dk, &kem_ciphertext).map_err(|e| e.to_string())?;
                            let hybrid_secret = redflag_crypto::combine_secrets(&classic_secret, &quantum_secret);
                            self.session_keys.insert(peer, hybrid_secret);
                            println!("🎊 Canal PQC híbrido activo con {:?}", peer);
                        }
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }

    pub fn handle_dag_sync_event(
        &mut self,
        event: request_response::Event<ConsensusRequest, ConsensusResponse>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use libp2p::request_response::Message;

        match event {
            request_response::Event::Message { peer: _, message, connection_id: _ } => match message {
                Message::Request { request, channel, .. } => match request {
                    ConsensusRequest::GetVertex(id) => {
                        let vertex = self.consensus.dag.get_vertex(&id).map(|v| (*v).clone());
                        self.swarm.behaviour_mut().dag_sync.send_response(channel, ConsensusResponse::Vertex(vertex)).ok();
                    }
                    ConsensusRequest::GetCertificate(round) => {
                        let certs: Vec<_> = self.consensus.dag.get_round_certificates(round)
                            .into_iter().map(|c| (*c).clone()).collect();
                        self.swarm.behaviour_mut().dag_sync.send_response(channel, ConsensusResponse::Certificates(certs)).ok();
                    }
                    ConsensusRequest::GetValidatorKey => {
                        self.swarm.behaviour_mut().dag_sync.send_response(
                            channel,
                            ConsensusResponse::ValidatorKey(self.own_validator_key.clone()),
                        ).ok();
                    }
                    ConsensusRequest::SyncFrom(from_round) => {
                        // Enviar vértices desde `from_round` (máx 50 a la vez)
                        let batch: Vec<Vertex> = self.consensus.dag.recent_vertices(50)
                            .into_iter()
                            .filter(|v| v.round >= from_round)
                            .map(|v| (*v).clone())
                            .collect();
                        println!("📤 Enviando {} vértices de sincronización (desde ronda {})", batch.len(), from_round);
                        self.swarm.behaviour_mut().dag_sync.send_response(channel, ConsensusResponse::SyncBatch(batch)).ok();
                    }
                    ConsensusRequest::Ping => {
                        self.swarm.behaviour_mut().dag_sync.send_response(channel, ConsensusResponse::Pong).ok();
                    }
                },
                Message::Response { response, .. } => match response {
                    ConsensusResponse::Vertex(Some(vertex)) => {
                        println!("📥 Vértice recibido: {} ronda {}", hex::encode(&vertex.id()[..4]), vertex.round);
                        let _ = self.consensus.dag.insert_vertex(vertex);
                    }
                    ConsensusResponse::Certificates(certs) => {
                        for cert in certs {
                            let _ = self.consensus.dag.insert_certificate(cert);
                        }
                    }
                    ConsensusResponse::ValidatorKey(pubkey) => {
                        self.consensus.add_validator(pubkey);
                    }
                    ConsensusResponse::SyncBatch(vertices) => {
                        let count = vertices.len();
                        for v in vertices {
                            let _ = self.consensus.dag.insert_vertex(v);
                        }
                        println!("✅ Sincronizados {} vértices del bootstrap", count);
                    }
                    _ => {}
                },
            },
            _ => {}
        }
        Ok(())
    }

    /// Maneja eventos de Kademlia (descubrimiento global)
    pub fn handle_kademlia_event(&mut self, event: kad::Event) {
        match event {
            kad::Event::RoutingUpdated { peer, .. } => {
                println!("🌐 Kademlia: peer {} añadido a la tabla de enrutamiento", peer);
            }
            kad::Event::OutboundQueryProgressed { result, .. } => {
                if let kad::QueryResult::GetClosestPeers(Ok(ok)) = result {
                    println!("🔍 Kademlia: {} peers cercanos encontrados", ok.peers.len());
                }
            }
            _ => {}
        }
    }

    /// Maneja eventos de mDNS (descubrimiento local)
    pub fn handle_mdns_event(&mut self, event: mdns::Event) {
        match event {
            mdns::Event::Discovered(peers) => {
                for (peer_id, addr) in peers {
                    println!("📡 mDNS: peer local descubierto {} @ {}", peer_id, addr);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                    if !self.connected_peers.contains(&peer_id) {
                        let _ = self.swarm.dial(addr);
                    }
                }
            }
            mdns::Event::Expired(peers) => {
                for (peer_id, _) in peers {
                    println!("📡 mDNS: peer local expirado {}", peer_id);
                }
            }
        }
    }

    /// Maneja eventos de Identify (intercambio de capacidades)
    pub fn handle_identify_event(&mut self, event: identify::Event) {
        if let identify::Event::Received { peer_id, info, connection_id: _ } = event {
            println!("🔖 Identify: {} — {}", peer_id, info.agent_version);
            // Registrar sus direcciones en Kademlia
            for addr in info.listen_addrs {
                self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
        }
    }

    /// Solicita sincronización completa del DAG al peer
    pub fn request_full_sync(&mut self, peer: PeerId) {
        let current_round = self.consensus.get_current_round();
        println!("🔄 Solicitando sync completo a {:?} desde ronda 0", peer);
        self.swarm.behaviour_mut().dag_sync.send_request(&peer, ConsensusRequest::SyncFrom(0));
        // También pedir certificados recientes
        if current_round > 0 {
            self.swarm.behaviour_mut().dag_sync.send_request(&peer, ConsensusRequest::GetCertificate(current_round.saturating_sub(1)));
        }
    }

    pub fn request_vertex(&mut self, peer: PeerId, id: [u8; 32]) {
        self.swarm.behaviour_mut().dag_sync.send_request(&peer, ConsensusRequest::GetVertex(id));
    }

    pub async fn listen(&mut self, addr: Multiaddr) -> Result<(), Box<dyn std::error::Error>> {
        self.swarm.listen_on(addr)?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Option<SwarmEvent<RedFlagBehaviourEvent>> {
        self.swarm.next().await
    }

    pub async fn broadcast_message(&mut self, msg: NetworkMessage) -> Result<(), Box<dyn std::error::Error>> {
        let topic = match &msg {
            NetworkMessage::NewTransaction(_) | NetworkMessage::NewEncryptedTransaction(_) => TOPIC_TRANSACTIONS,
            NetworkMessage::NewBlock(_) => TOPIC_BLOCKS,
        };
        let data = bincode::serde::encode_to_vec(&msg, bincode::config::standard())?;
        self.swarm.behaviour_mut().gossipsub.publish(gossipsub::IdentTopic::new(topic), data)?;
        Ok(())
    }

    pub fn peer_count(&self) -> usize {
        self.connected_peers.len()
    }
}
