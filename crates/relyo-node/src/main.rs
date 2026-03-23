use clap::{Parser, Subcommand};
use relyo_node::config::NodeConfig;
use relyo_node::node::RelyoNode;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

// Simple RocksDB wrapper over redb
pub mod rocksdb {
    use redb::{Database, TableDefinition};
    use std::path::Path;
    
    const BANNED_PEERS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("banned_peers");

    #[derive(Default)]
    pub struct Options {
        create_if_missing: bool,
    }
    
    impl Options {
        pub fn create_if_missing(&mut self, val: bool) {
            self.create_if_missing = val;
        }
    }
    
    pub struct DB {
        db: Database,
    }
    
    impl DB {
        pub fn open(_opts: &Options, path: impl AsRef<Path>) -> anyhow::Result<Self> {
            let db = Database::create(path.as_ref())?;
            let write_txn = db.begin_write()?;
            write_txn.open_table(BANNED_PEERS)?;
            write_txn.commit()?;
            Ok(Self { db })
        }

        pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> anyhow::Result<()> {
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(BANNED_PEERS)?;
                table.insert(key.as_ref(), value.as_ref())?;
            }
            write_txn.commit()?;
            Ok(())
        }

        pub fn get(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Option<Vec<u8>>> {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(BANNED_PEERS)?;
            let value = table.get(key.as_ref())?;
            Ok(value.map(|v| v.value().to_vec()))
        }
    }
}

#[derive(Parser)]
#[command(name = "relyo-node")]
#[command(about = "Relyo OpenGraph Ledger Ã¢â‚¬â€ full node")]
#[command(version)]
struct Cli {
    /// Path to the node configuration file.
    #[arg(short, long, default_value = "relyo.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the full node.
    Run {
        /// Force start the node in testnet mode. (Protects against accidental mainnet usage)
        #[arg(long)]
        testnet: bool,
    },
    /// Initialize a new node with default configuration.
    Init {
        /// Node name.
        #[arg(short, long, default_value = "relyo-node")]
        name: String,
    },
    /// Show current node status.
    Status,
    /// Generate a default configuration file.
    Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let constants_content = include_str!("../../relyo-core/src/constants.rs");
    let normalized_content = constants_content.replace("\r\n", "\n");
    let current_hash = blake3::hash(normalized_content.as_bytes()).to_hex().to_string();
    if current_hash != relyo_core::get_consensus_hash() {
        panic!("CONSENSUS RULES TAMPERED \u{2014} THIS NODE IS INVALID \u{2014} REFUSING TO START");
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => {
            let config = NodeConfig {
                node_name: name,
                ..Default::default()
            };

            // Create data directory.
            std::fs::create_dir_all(&config.data_dir)?;

            config.save(&cli.config)?;
            println!("Node initialized. Configuration written to {:?}", cli.config);
            println!("Data directory: {:?}", config.data_dir);
            println!();
            println!("Start the node with: relyo-node run");
        }

        Commands::Config => {
            let config = NodeConfig::default();
            let toml = toml::to_string_pretty(&config)?;
            println!("{}", toml);
        }

        Commands::Run { testnet } => {
        if !testnet {
            anyhow::bail!("Mainnet is not yet released. Please run the node safely with the `--testnet` flag. `relyo-node run --testnet`");
        }
            let config = NodeConfig::load(&cli.config)?;

            let banned_peers_db_path = config.data_dir.join("banned_peers");
            let mut db_opts = rocksdb::Options::default();
            db_opts.create_if_missing(true);
            let banned_peers_db = match rocksdb::DB::open(&db_opts, &banned_peers_db_path) {
                Ok(db) => Arc::new(db),
                Err(e) => {
                    tracing::error!("Failed to open rocksdb for banned_peers: {}", e);
                    std::process::exit(1);
                }
            };

            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
                )
                .with_target(true)
                .with_thread_ids(true)
                .init();

            info!("starting Relyo node '{}'", config.node_name);
            info!(
                "network: tcp={}, quic={}",
                config.network.tcp_port, config.network.quic_port
            );

            let (tx_sender, mut tx_receiver) = tokio::sync::mpsc::unbounded_channel();
            let node = Arc::new(RelyoNode::new(config, tx_sender)?);

            // Initialize genesis state.
            node.init_genesis();

            info!(
                "DAG initialized with {} transactions, circulating supply: {} RLY",
                node.dag_size(),
                relyo_core::token::base_to_rly(node.dag.state().total_circulating())
            );

            // Start explorer API.
            let _explorer_handle = node.start_explorer();

            // Start JSON-RPC server.
            let _rpc_handle = node.start_rpc();

            // Build and start the p2p swarm.
            let swarm_builder = relyo_network::RelyoSwarm::new(
                node.config.network.clone(),
            );

            let mut swarm = swarm_builder.build_swarm().map_err(|e| anyhow::anyhow!("{}", e))?;

            // Listen on TCP and QUIC.
            let tcp_addr: libp2p::Multiaddr = node
                .config
                .network
                .tcp_multiaddr()
                .parse()?;
            let quic_addr: libp2p::Multiaddr = node
                .config
                .network
                .quic_multiaddr()
                .parse()?;

            swarm.listen_on(tcp_addr)?;
            swarm.listen_on(quic_addr)?;

            // Subscribe to gossipsub topics.
            relyo_network::swarm::subscribe_topics(&mut swarm)?;

            // Dial bootstrap peers.
            for peer_addr in &node.config.network.bootstrap_peers {
                if let Ok(addr) = peer_addr.parse::<libp2p::Multiaddr>() {
                    info!("dialing bootstrap peer: {}", peer_addr);
                    swarm.dial(addr)?;
                }
            }

            let local_peer_id = *swarm.local_peer_id();
            info!("local peer ID: {}", local_peer_id);

            // Main event loop.
            info!("node is running. Press Ctrl+C to stop.");

            // Process mempool periodically.
            let mempool_interval = tokio::time::interval(
                std::time::Duration::from_millis(100),
            );
            tokio::pin!(mempool_interval);

            // Rate limiter cleanup interval.
            let cleanup_interval = tokio::time::interval(
                std::time::Duration::from_secs(10),
            );
            tokio::pin!(cleanup_interval);

            loop {
                tokio::select! {
                    Some(tx) = tx_receiver.recv() => {
                        match relyo_network::messages::NetworkMessage::tx_broadcast(&tx) {
                            Ok(net_msg) => {
                                match net_msg.to_bytes() {
                                    Ok(bytes) => {
                                        if let Err(e) = relyo_network::swarm::publish_transaction(&mut swarm, &bytes) {
                                            tracing::warn!("failed to publish transaction: {}", e);
                                        } else {
                                            tracing::debug!("broadcasted transaction: {}", tx.hash());
                                        }
                                    }
                                    Err(e) => tracing::error!("failed to serialize bytes: {}", e),
                                }
                            }
                            Err(e) => tracing::error!("failed to broadcast tx: {}", e),
                        }
                    }
                    event = swarm.next_swarm_event() => {
                        // Handle swarm events (peer connections, messages, etc.)
                        if let libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } = &event {
                            if banned_peers_db.get(peer_id.to_bytes()).unwrap_or_else(|e| {
                                tracing::error!("DB read failed: {}", e);
                                None
                            }).is_some() {
                                tracing::warn!("Disconnecting banned peer: {}", peer_id);
                                let _ = swarm.disconnect_peer_id(*peer_id);
                            }
                        }

                        match event {
                            libp2p::swarm::SwarmEvent::Behaviour(
                                relyo_network::behaviour::RelyoBehaviourEvent::Gossipsub(
                                    libp2p::gossipsub::Event::Message {
                                        propagation_source,
                                        message,
                                        ..
                                    },
                                ),
                            ) => {
                                    match relyo_network::messages::NetworkMessage::from_bytes(
                                        &message.data,
                                    ) {
                                        Ok(net_msg) => {
                                            use relyo_network::messages::MessageType;
                                            match net_msg.msg_type {
                                                MessageType::TransactionBroadcast => {
                                                    match bincode::deserialize::<relyo_core::Transaction>(&net_msg.payload) {
                                                        Ok(tx) => {
                                                            match node.submit_transaction(tx) {
                                                                Ok(hash) => {
                                                                    tracing::debug!(
                                                                        "accepted tx {} from peer {}",
                                                                        hash,
                                                                        propagation_source
                                                                    );
                                                                }
                                                                Err(e) => {
                                                                    if let relyo_core::RelyoError::SupplyCapExceeded(_) = e {
                                                                        tracing::warn!("Supply cap exceeded. Banning peer: {}", propagation_source);
                                                                        if let Err(db_err) = banned_peers_db.put(propagation_source.to_bytes(), b"1") {
                                                                            tracing::error!("Failed to write to banned peers DB for {}: {}", propagation_source, db_err);
                                                                        }
                                                                        let _ = swarm.disconnect_peer_id(propagation_source);
                                                                    } else {
                                                                        tracing::debug!(
                                                                            "rejected tx from peer {}: {}",
                                                                            propagation_source,
                                                                            e
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!(
                                                                "invalid tx payload from {}: {}",
                                                                propagation_source,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                                MessageType::VoteRequest => {
                                                    match bincode::deserialize::<relyo_core::TransactionHash>(&net_msg.payload) {
                                                        Ok(tx_hash) => {
                                                            // Check if we have this tx and vote on it
                                                            let accept = node.dag.has_transaction(&tx_hash);
                                                            let vote = relyo_network::messages::VotePayload {
                                                                tx_hash,
                                                                accept,
                                                                node_id: node.node_id().to_string(),
                                                            };
                                                            match relyo_network::messages::NetworkMessage::vote_response(&vote) {
                                                                Ok(response) => {
                                                                    match response.to_bytes() {
                                                                        Ok(bytes) => {
                                                                            if let Err(e) = relyo_network::swarm::publish_vote(
                                                                                &mut swarm,
                                                                                &bytes,
                                                                            ) {
                                                                                tracing::debug!("failed to publish vote: {}", e);
                                                                            }
                                                                        }
                                                                        Err(e) => tracing::error!("failed to serialize res bytes: {}", e),
                                                                    }
                                                                }
                                                                Err(e) => tracing::error!("failed to broadcast response: {}", e),
                                                            }
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!(
                                                                "invalid vote request from {}: {}",
                                                                propagation_source,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                                MessageType::VoteResponse => {
                                                    match bincode::deserialize::<relyo_network::messages::VotePayload>(&net_msg.payload) {
                                                        Ok(vote) => {
                                                            let node_id = relyo_core::NodeId::new(&vote.node_id);
                                                            node.consensus.process_remote_vote(
                                                                &vote.tx_hash,
                                                                &node_id,
                                                                vote.accept,
                                                            );
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!(
                                                                "invalid vote response from {}: {}",
                                                                propagation_source,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                                MessageType::Heartbeat => {
                                                    tracing::trace!(
                                                        "heartbeat from {}",
                                                        propagation_source
                                                    );
                                                }
                                                _ => {
                                                    tracing::debug!(
                                                        "received {:?} from {}",
                                                        net_msg.msg_type,
                                                        propagation_source
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "failed to decode message: {}",
                                                e
                                            );
                                        }
                                    }
                            }
                            libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                                info!("listening on {}", address);
                            }
                            libp2p::swarm::SwarmEvent::ConnectionEstablished {
                                peer_id, ..
                            } => {
                                info!("connection established with {}", peer_id);
                            }
                            libp2p::swarm::SwarmEvent::ConnectionClosed {
                                peer_id, ..
                            } => {
                                info!("connection closed with {}", peer_id);
                            }
                            _ => {}
                        }
                    }
                    _ = mempool_interval.tick() => {
                        node.process_mempool_batch(1000);
                    }
                    _ = cleanup_interval.tick() => {
                        node.rate_limiter.cleanup();
                    }
                    _ = tokio::signal::ctrl_c() => {
                        info!("shutting down...");
                        break;
                    }
                }
            }

            info!("node stopped. Final DAG size: {} transactions", node.dag_size());
        }

        Commands::Status => {
            let config = NodeConfig::load(&cli.config)?;
            println!("Node: {}", config.node_name);
            println!("Data dir: {:?}", config.data_dir);
            println!("Network TCP: {}", config.network.tcp_port);
            println!("Network QUIC: {}", config.network.quic_port);
            println!("Explorer: {} ({})", config.explorer_bind, if config.explorer_enabled { "enabled" } else { "disabled" });
            println!("RPC:      {} ({})", config.rpc_bind, if config.rpc_enabled { "enabled" } else { "disabled" });
        }
    }

    Ok(())
}

// Extension trait for Swarm to make the event loop cleaner.
trait SwarmExt {
    async fn next_swarm_event(&mut self) -> libp2p::swarm::SwarmEvent<
        relyo_network::behaviour::RelyoBehaviourEvent,
    >;
}

impl SwarmExt for libp2p::Swarm<relyo_network::behaviour::RelyoBehaviour> {
    async fn next_swarm_event(&mut self) -> libp2p::swarm::SwarmEvent<
        relyo_network::behaviour::RelyoBehaviourEvent,
    > {
        use futures::StreamExt;
        self.select_next_some().await
    }
}




