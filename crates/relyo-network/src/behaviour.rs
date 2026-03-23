use libp2p::{
    gossipsub, identify, kad, ping,
    swarm::NetworkBehaviour,
};

/// Composite libp2p behaviour combining all Relyo network protocols.
#[derive(NetworkBehaviour)]
pub struct RelyoBehaviour {
    /// Gossipsub for transaction and vote propagation.
    pub gossipsub: gossipsub::Behaviour,
    /// Kademlia DHT for peer discovery and routing.
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    /// Identify protocol for exchanging peer metadata.
    pub identify: identify::Behaviour,
    /// Ping protocol for liveness checks.
    pub ping: ping::Behaviour,
}
