use libp2p::identity;
use redflag_network::{RedFlagNode, RedFlagBehaviourEvent, NetworkMessage};
use redflag_consensus::{ConsensusEngine, Dag, Mempool, Vertex, threshold::ThresholdMempool};
use redflag_state::StateDB;
use std::sync::Arc;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time;

#[tokio::test]
async fn test_dag_sync_parent_fetching() -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_dir_all("./test_sync_db_a");
    let _ = std::fs::remove_dir_all("./test_sync_db_b");
    let _ = std::fs::remove_dir_all("./test_sync_dag_a");
    let _ = std::fs::remove_dir_all("./test_sync_dag_b");

    // Nodo A
    let state_db_a = Arc::new(StateDB::new("./test_sync_db_a")?);
    let key_a = identity::Keypair::generate_ed25519();
    let peer_id_a = libp2p::PeerId::from(key_a.public());
    let signing_key_a = redflag_crypto::SigningKeyPair::generate()?;
    let mempool_a = Arc::new(Mempool::new(signing_key_a));
    let dag_a = Arc::new(Dag::new("./test_sync_dag_a")?);
    let threshold_a = Arc::new(ThresholdMempool::new()?);
    let consensus_a = Arc::new(ConsensusEngine::new(dag_a, mempool_a, state_db_a, threshold_a, vec![vec![0; 32]]));
    let mut node_a = RedFlagNode::new_with_consensus(key_a, consensus_a, vec![]).await?;

    // Nodo B
    let state_db_b = Arc::new(StateDB::new("./test_sync_db_b")?);
    let key_b = identity::Keypair::generate_ed25519();
    let peer_id_b = libp2p::PeerId::from(key_b.public());
    let signing_key_b = redflag_crypto::SigningKeyPair::generate()?;
    let mempool_b = Arc::new(Mempool::new(signing_key_b));
    let dag_b = Arc::new(Dag::new("./test_sync_dag_b")?);
    let threshold_b = Arc::new(ThresholdMempool::new()?);
    let consensus_b = Arc::new(ConsensusEngine::new(dag_b, mempool_b, state_db_b, threshold_b, vec![vec![0; 32]]));
    let mut node_b = RedFlagNode::new_with_consensus(key_b, consensus_b, vec![]).await?;

    // Vértice padre V1 (con encrypted_transactions vacío)
    let v1 = Vertex {
        round: 1,
        parents: HashSet::new(),
        transactions: vec![],
        encrypted_transactions: vec![],
        author: vec![1],
        signature: vec![1],
    };
    let v1_id = v1.id();
    let _ = node_a.consensus.dag.insert_vertex(v1.clone());

    node_a.listen("/ip4/127.0.0.1/tcp/0".parse()?).await?;
    let mut addr_a = None;
    while addr_a.is_none() {
        if let Some(libp2p::swarm::SwarmEvent::NewListenAddr { address, .. }) = node_a.next_event().await {
            addr_a = Some(address);
        }
    }
    node_b.swarm.dial(addr_a.unwrap())?;

    // Esperar handshake
    let mut handshake_done = false;
    while !handshake_done {
        tokio::select! {
            event_a = node_a.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::PqcHandshake(e))) = event_a {
                    node_a.handle_pqc_handshake_event(e)?;
                } else if let Some(libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. }) = event_a {
                    node_a.initiate_pqc_handshake(peer_id)?;
                }
            }
            event_b = node_b.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::PqcHandshake(e))) = event_b {
                    node_b.handle_pqc_handshake_event(e)?;
                    if node_b.session_keys.contains_key(&peer_id_a) { handshake_done = true; }
                }
            }
        }
    }

    // V2 hijo de V1 — Bob debe pedir V1 al recibirlo
    let mut parents = HashSet::new();
    parents.insert(v1_id);
    let v2 = Vertex {
        round: 2,
        parents,
        transactions: vec![],
        encrypted_transactions: vec![],
        author: vec![1],
        signature: vec![2],
    };
    let v2_id = v2.id();
    let _ = node_a.consensus.dag.insert_vertex(v2.clone());
    node_a.broadcast_message(NetworkMessage::NewBlock(bincode::serialize(&v2).unwrap())).await?;

    let timeout = time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event_a = node_a.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::DagSync(e))) = event_a {
                    node_a.handle_dag_sync_event(e)?;
                }
            }
            event_b = node_b.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::DagSync(e))) = event_b {
                    node_b.handle_dag_sync_event(e)?;
                } else if let Some(libp2p::swarm::SwarmEvent::Behaviour(
                    RedFlagBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Message { propagation_source, message, .. })
                )) = event_b {
                    if let Ok(NetworkMessage::NewBlock(data)) = bincode::deserialize::<NetworkMessage>(&message.data) {
                        if let Ok(vertex) = bincode::deserialize::<Vertex>(&data) {
                            for parent_id in &vertex.parents {
                                if node_b.consensus.dag.get_vertex(parent_id).is_none() {
                                    node_b.request_vertex(propagation_source, *parent_id);
                                }
                            }
                            let _ = node_b.consensus.dag.insert_vertex(vertex);
                        }
                    }
                }
            }
            _ = &mut timeout => { panic!("Sync timeout"); }
        }
        if node_b.consensus.dag.get_vertex(&v1_id).is_some()
            && node_b.consensus.dag.get_vertex(&v2_id).is_some()
        {
            println!("✅ Parent-fetching: Bob sincronizó V1 y V2 correctamente");
            break;
        }
    }
    Ok(())
}
