use libp2p::{
    gossipsub::{self, MessageAuthenticity, ValidationMode},
    identify, kad, noise, ping,
    tcp, yamux, Swarm, SwarmBuilder,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::behaviour::RelyoBehaviour;
use crate::config::NetworkConfig;
use crate::messages::NetworkMessage;
use crate::peer_manager::PeerManager;

/// Gossipsub topic for transaction broadcasts.
pub const TOPIC_TRANSACTIONS: &str = "relyo/txs/1.0";
/// Gossipsub topic for consensus votes.
pub const TOPIC_VOTES: &str = "relyo/votes/1.0";
/// Gossipsub topic for node heartbeats.
pub const TOPIC_HEARTBEAT: &str = "relyo/heartbeat/1.0";

/// The main libp2p swarm wrapper for the Relyo network.
pub struct RelyoSwarm {
    peer_manager: Arc<PeerManager>,
    config: NetworkConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum SwarmError {
    #[error("Gossipsub config error: {0}")]
    GossipsubConfig(String),
    #[error("Gossipsub build error: {0}")]
    GossipsubBuild(String),
}

impl RelyoSwarm {
    pub fn new(config: NetworkConfig) -> Self {
        let peer_manager = Arc::new(PeerManager::new(config.max_peers));
        RelyoSwarm {
            peer_manager,
            config,
        }
    }

    /// Build and return the libp2p Swarm with all protocols configured.
    pub fn build_swarm(&self) -> Result<Swarm<RelyoBehaviour>, Box<dyn std::error::Error>> {
        let local_key = if std::path::Path::new(&self.config.identity_path).exists() {
            let bytes = std::fs::read(&self.config.identity_path)?;
            libp2p::identity::Keypair::from_protobuf_encoding(&bytes)?
        } else {
            let key = libp2p::identity::Keypair::generate_ed25519();
            if let Some(parent) = std::path::Path::new(&self.config.identity_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&self.config.identity_path, key.to_protobuf_encoding()?)?;
            key
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_millis(self.config.gossip_heartbeat_ms))
            .validation_mode(ValidationMode::Strict)
            .max_transmit_size(1024 * 1024) // 1 MB
            .build()
            .map_err(|e| Box::new(SwarmError::GossipsubConfig(e.to_string())) as Box<dyn std::error::Error>)?;

        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .map_err(|e| Box::new(SwarmError::GossipsubBuild(e.to_string())) as Box<dyn std::error::Error>)?;

        let swarm = SwarmBuilder::with_existing_identity(local_key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(move |key| {
                // Kademlia DHT.
                let peer_id = key.public().to_peer_id();
                let store = kad::store::MemoryStore::new(peer_id);
                let kademlia = kad::Behaviour::new(peer_id, store);

                // Identify protocol.
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/relyo/1.0.0".to_string(),
                    key.public(),
                ));

                // Ping.
                let ping = ping::Behaviour::new(ping::Config::new());

                RelyoBehaviour {
                    gossipsub,
                    kademlia,
                    identify,
                    ping,
                }
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        Ok(swarm)
    }

    /// Get a reference to the peer manager.
    pub fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }

    /// Get the network configuration.
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }
}

/// Subscribe a swarm to the standard Relyo gossipsub topics.
pub fn subscribe_topics(swarm: &mut Swarm<RelyoBehaviour>) -> Result<(), gossipsub::SubscriptionError> {
    let topics = [TOPIC_TRANSACTIONS, TOPIC_VOTES, TOPIC_HEARTBEAT];
    for topic_str in topics {
        let topic = gossipsub::IdentTopic::new(topic_str);
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        info!("subscribed to gossipsub topic: {}", topic_str);
    }
    Ok(())
}

/// Publish a message to a gossipsub topic.
pub fn publish_message(
    swarm: &mut Swarm<RelyoBehaviour>,
    topic: &str,
    message: &NetworkMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let topic = gossipsub::IdentTopic::new(topic);
    let data = message.to_bytes()?;
    swarm.behaviour_mut().gossipsub.publish(topic, data)?;
    Ok(())
}

/// Publish raw bytes to the votes gossipsub topic.
pub fn publish_vote(
    swarm: &mut Swarm<RelyoBehaviour>,
    data: &[u8],
) -> Result<(), gossipsub::PublishError> {
    let topic = gossipsub::IdentTopic::new(TOPIC_VOTES);
    swarm.behaviour_mut().gossipsub.publish(topic, data)?;
    Ok(())
}

/// Publish a transaction broadcast to the transactions gossipsub topic.
pub fn publish_transaction(
    swarm: &mut Swarm<RelyoBehaviour>,
    data: &[u8],
) -> Result<(), gossipsub::PublishError> {
    let topic = gossipsub::IdentTopic::new(TOPIC_TRANSACTIONS);
    swarm.behaviour_mut().gossipsub.publish(topic, data)?;
    Ok(())
}

