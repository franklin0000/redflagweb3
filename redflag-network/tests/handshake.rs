use libp2p::identity;
use redflag_network::{RedFlagNode, RedFlagBehaviourEvent};
use redflag_consensus::{ConsensusEngine, Dag, Mempool, threshold::ThresholdMempool};
use redflag_state::StateDB;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

#[tokio::test]
async fn test_hybrid_handshake_integration() -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_dir_all("./test_db_a");
    let _ = std::fs::remove_dir_all("./test_db_b");
    let _ = std::fs::remove_dir_all("./test_dag_a");
    let _ = std::fs::remove_dir_all("./test_dag_b");

    // Nodo A
    let state_db_a = Arc::new(StateDB::new("./test_db_a")?);
    let key_a = identity::Keypair::generate_ed25519();
    let peer_id_a = libp2p::PeerId::from(key_a.public());
    let signing_key_a = redflag_crypto::SigningKeyPair::generate()?;
    let mempool_a = Arc::new(Mempool::new(signing_key_a));
    let dag_a = Arc::new(Dag::new("./test_dag_a")?);
    let threshold_a = Arc::new(ThresholdMempool::new()?);
    let consensus_a = Arc::new(ConsensusEngine::new(dag_a, mempool_a, state_db_a, threshold_a, vec![vec![0; 32]]));
    let mut node_a = RedFlagNode::new_with_consensus(key_a, consensus_a, vec![]).await?;

    // Nodo B
    let state_db_b = Arc::new(StateDB::new("./test_db_b")?);
    let key_b = identity::Keypair::generate_ed25519();
    let peer_id_b = libp2p::PeerId::from(key_b.public());
    let signing_key_b = redflag_crypto::SigningKeyPair::generate()?;
    let mempool_b = Arc::new(Mempool::new(signing_key_b));
    let dag_b = Arc::new(Dag::new("./test_dag_b")?);
    let threshold_b = Arc::new(ThresholdMempool::new()?);
    let consensus_b = Arc::new(ConsensusEngine::new(dag_b, mempool_b, state_db_b, threshold_b, vec![vec![0; 32]]));
    let mut node_b = RedFlagNode::new_with_consensus(key_b, consensus_b, vec![]).await?;

    node_a.listen("/ip4/127.0.0.1/tcp/0".parse()?).await?;
    let mut addr_a = None;
    while addr_a.is_none() {
        if let Some(libp2p::swarm::SwarmEvent::NewListenAddr { address, .. }) = node_a.next_event().await {
            addr_a = Some(address);
        }
    }
    node_b.swarm.dial(addr_a.unwrap())?;

    let mut handshake_done_a = false;
    let mut handshake_done_b = false;
    let timeout = time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event_a = node_a.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::PqcHandshake(e))) = event_a {
                    node_a.handle_pqc_handshake_event(e)?;
                    if node_a.session_keys.contains_key(&peer_id_b) { handshake_done_a = true; }
                } else if let Some(libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. }) = event_a {
                    node_a.initiate_pqc_handshake(peer_id)?;
                }
            }
            event_b = node_b.next_event() => {
                if let Some(libp2p::swarm::SwarmEvent::Behaviour(RedFlagBehaviourEvent::PqcHandshake(e))) = event_b {
                    node_b.handle_pqc_handshake_event(e)?;
                    if node_b.session_keys.contains_key(&peer_id_a) { handshake_done_b = true; }
                }
            }
            _ = &mut timeout => { panic!("Handshake timeout"); }
        }
        if handshake_done_a && handshake_done_b {
            let secret_a = node_a.session_keys.get(&peer_id_b).unwrap();
            let secret_b = node_b.session_keys.get(&peer_id_a).unwrap();
            assert_eq!(secret_a, secret_b);
            println!("✅ Handshake PQC híbrido: secretos coinciden");
            break;
        }
    }
    Ok(())
}
