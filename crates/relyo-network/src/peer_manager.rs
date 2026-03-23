use dashmap::DashMap;
use libp2p::PeerId;
use std::time::Instant;
use tracing::{info, warn};

/// Information about a connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub connected_at: Instant,
    pub last_seen: Instant,
    pub address: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}

/// Manages connected peers and tracks their statistics.
pub struct PeerManager {
    peers: DashMap<PeerId, PeerInfo>,
    connections_per_bucket: DashMap<String, usize>,
    max_peers: usize,
    max_peers_per_bucket: usize,
}

impl PeerManager {
    pub fn new(max_peers: usize) -> Self {
        let max_peers_per_bucket = (max_peers / 5).clamp(2, 16);
        Self::with_limits(max_peers, max_peers_per_bucket)
    }

    pub fn with_limits(max_peers: usize, max_peers_per_bucket: usize) -> Self {
        PeerManager {
            peers: DashMap::new(),
            connections_per_bucket: DashMap::new(),
            max_peers,
            max_peers_per_bucket: max_peers_per_bucket.max(1),
        }
    }

    /// Register a new peer connection.
    pub fn add_peer(&self, peer_id: PeerId, address: String) -> bool {
        if self.peers.contains_key(&peer_id) {
            return true;
        }

        if self.peers.len() >= self.max_peers {
            warn!("max peers reached, rejecting {}", peer_id);
            return false;
        }

        let bucket = Self::bucket_for_address(&address);
        let bucket_count = self
            .connections_per_bucket
            .get(&bucket)
            .map(|v| *v)
            .unwrap_or(0);
        if bucket_count >= self.max_peers_per_bucket {
            warn!(
                "too many peers from same network bucket '{}' (limit={}), rejecting {}",
                bucket,
                self.max_peers_per_bucket,
                peer_id
            );
            return false;
        }

        let now = Instant::now();
        self.peers.insert(
            peer_id,
            PeerInfo {
                peer_id,
                connected_at: now,
                last_seen: now,
                address,
                bytes_sent: 0,
                bytes_received: 0,
                messages_sent: 0,
                messages_received: 0,
            },
        );
        self.connections_per_bucket
            .entry(bucket)
            .and_modify(|v| *v += 1)
            .or_insert(1);

        info!("peer connected: {} (total: {})", peer_id, self.peers.len());
        true
    }

    /// Remove a disconnected peer.
    pub fn remove_peer(&self, peer_id: &PeerId) {
        if let Some((_, info)) = self.peers.remove(peer_id) {
            let bucket = Self::bucket_for_address(&info.address);
            if let Some(mut count) = self.connections_per_bucket.get_mut(&bucket) {
                if *count <= 1 {
                    drop(count);
                    self.connections_per_bucket.remove(&bucket);
                } else {
                    *count -= 1;
                }
            }
            info!("peer disconnected: {} (total: {})", peer_id, self.peers.len());
        }
    }

    /// Update the last-seen timestamp for a peer.
    pub fn touch(&self, peer_id: &PeerId) {
        if let Some(mut info) = self.peers.get_mut(peer_id) {
            info.last_seen = Instant::now();
        }
    }

    /// Record bytes sent to a peer.
    pub fn record_sent(&self, peer_id: &PeerId, bytes: u64) {
        if let Some(mut info) = self.peers.get_mut(peer_id) {
            info.bytes_sent += bytes;
            info.messages_sent += 1;
        }
    }

    /// Record bytes received from a peer.
    pub fn record_received(&self, peer_id: &PeerId, bytes: u64) {
        if let Some(mut info) = self.peers.get_mut(peer_id) {
            info.bytes_received += bytes;
            info.messages_received += 1;
            info.last_seen = Instant::now();
        }
    }

    /// Get a list of all connected peer IDs.
    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.peers.iter().map(|r| *r.key()).collect()
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Check if a peer is connected.
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.peers.contains_key(peer_id)
    }

    /// Get info for a specific peer.
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.peers.get(peer_id).map(|r| r.clone())
    }

    /// Total bandwidth relayed across all peers (for reward scoring).
    pub fn total_bandwidth(&self) -> u64 {
        self.peers
            .iter()
            .map(|r| r.bytes_sent + r.bytes_received)
            .sum()
    }

    /// Select random peers for gossip sampling.
    pub fn sample_peers(&self, count: usize) -> Vec<PeerId> {
        use rand::seq::SliceRandom;
        let all: Vec<PeerId> = self.connected_peers();
        if all.len() <= count {
            return all;
        }
        let mut rng = rand::thread_rng();
        let mut sampled = all;
        sampled.shuffle(&mut rng);
        sampled.truncate(count);
        sampled
    }

    fn bucket_for_address(address: &str) -> String {
        let parts: Vec<&str> = address.split('/').collect();
        let ip = if let Some(pos) = parts.iter().position(|p| *p == "ip4") {
            parts.get(pos + 1).copied().unwrap_or(address)
        } else if let Some(pos) = parts.iter().position(|p| *p == "ip6") {
            parts.get(pos + 1).copied().unwrap_or(address)
        } else {
            address
        };

        // Coarse bucketting limits many identities from one network while allowing diversity.
        if ip.contains(':') {
            ip.split(':').take(4).collect::<Vec<_>>().join(":")
        } else {
            let mut octets = ip.split('.');
            match (octets.next(), octets.next(), octets.next()) {
                (Some(a), Some(b), Some(c)) => format!("{}.{}.{}", a, b, c),
                _ => ip.to_string(),
            }
        }
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_peer_id() -> PeerId {
        PeerId::random()
    }

    #[test]
    fn test_add_remove_peer() {
        let pm = PeerManager::new(10);
        let peer = random_peer_id();

        assert!(pm.add_peer(peer, "/ip4/127.0.0.1/tcp/9741".into()));
        assert_eq!(pm.peer_count(), 1);
        assert!(pm.is_connected(&peer));

        pm.remove_peer(&peer);
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn test_max_peers() {
        let pm = PeerManager::new(2);
        assert!(pm.add_peer(random_peer_id(), "a".into()));
        assert!(pm.add_peer(random_peer_id(), "b".into()));
        assert!(!pm.add_peer(random_peer_id(), "c".into())); // rejected
    }

    #[test]
    fn test_bandwidth_tracking() {
        let pm = PeerManager::new(10);
        let peer = random_peer_id();
        pm.add_peer(peer, "addr".into());

        pm.record_sent(&peer, 1000);
        pm.record_received(&peer, 2000);

        assert_eq!(pm.total_bandwidth(), 3000);
    }

    #[test]
    fn test_rejects_too_many_from_same_bucket() {
        let pm = PeerManager::with_limits(10, 2);
        assert!(pm.add_peer(random_peer_id(), "/ip4/10.1.2.3/tcp/9001".into()));
        assert!(pm.add_peer(random_peer_id(), "/ip4/10.1.2.4/tcp/9002".into()));
        assert!(!pm.add_peer(random_peer_id(), "/ip4/10.1.2.99/tcp/9003".into()));
    }
}
