use p2p_play::network::*;
use p2p_play::types::*;
use libp2p::{PeerId, Transport, core::upgrade, dns, noise, swarm::Config as SwarmConfig, tcp, yamux, request_response, StreamProtocol};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::iter;

/// Creates a test swarm with a unique generated keypair
pub async fn create_test_swarm() -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    Ok(create_test_swarm_with_keypair(keypair)?)
}

/// Creates a test swarm with a specific keypair for testing
pub fn create_test_swarm_with_keypair(keypair: libp2p::identity::Keypair) -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    use libp2p::tcp::Config;
    
    // Enhanced TCP configuration for testing
    #[cfg(windows)]
    let tcp_config = Config::default()
        .nodelay(true)
        .listen_backlog(1024)
        .ttl(64);
    
    #[cfg(not(windows))]
    let tcp_config = Config::default()
        .nodelay(true)
        .listen_backlog(1024)
        .ttl(64);
        
    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_num_streams(512);
        
    let transp = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| format!("Failed to create DNS transport: {e}"))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair).unwrap())
        .multiplex(yamux_config)
        .boxed();

    // Create behaviour with unique peer ID
    let peer_id = PeerId::from(keypair.public());
    
    // Create request-response protocols using the same patterns as the main code
    let dm_protocol = StreamProtocol::new("/dm/1.0.0");
    let dm_protocols = iter::once((dm_protocol, request_response::ProtocolSupport::Full));
    
    let cfg = request_response::Config::default()
        .with_request_timeout(Duration::from_secs(60))
        .with_max_concurrent_streams(256);
    
    let request_response = request_response::cbor::Behaviour::new(dm_protocols, cfg.clone());
    
    // Node description protocol
    let desc_protocol = StreamProtocol::new("/node-desc/1.0.0");
    let desc_protocols = iter::once((desc_protocol, request_response::ProtocolSupport::Full));
    let node_description = request_response::cbor::Behaviour::new(desc_protocols, cfg.clone());
    
    // Story sync protocol
    let story_sync_protocol = StreamProtocol::new("/story-sync/1.0.0");
    let story_sync_protocols = iter::once((story_sync_protocol, request_response::ProtocolSupport::Full));
    let story_sync = request_response::cbor::Behaviour::new(story_sync_protocols, cfg);
    
    // Create Kademlia DHT
    let store = libp2p::kad::store::MemoryStore::new(peer_id);
    let kad_config = libp2p::kad::Config::default();
    let mut kad = libp2p::kad::Behaviour::with_config(peer_id, store, kad_config);
    kad.set_mode(Some(libp2p::kad::Mode::Server));
    
    let behaviour = StoryBehaviour {
        floodsub: libp2p::floodsub::Behaviour::new(peer_id),
        mdns: libp2p::mdns::tokio::Behaviour::new(Default::default(), peer_id)
            .map_err(|e| format!("Failed to create mDNS: {e}"))?,
        ping: libp2p::ping::Behaviour::new(libp2p::ping::Config::new()),
        request_response,
        node_description,
        story_sync,
        kad,
    };

    let swarm_config = SwarmConfig::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(30));
        
    Ok(libp2p::Swarm::new(
        transp,
        behaviour,
        peer_id,
        swarm_config
    ))
}

/// Creates multiple test swarms with unique peer IDs
pub async fn create_test_swarms(count: usize) -> Result<Vec<libp2p::Swarm<StoryBehaviour>>, Box<dyn std::error::Error>> {
    let mut swarms = Vec::new();
    for i in 0..count {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        println!("Creating test swarm {} with unique PeerId: {}", i, peer_id);
        
        let swarm = create_test_swarm_with_keypair(keypair)?;
        swarms.push(swarm);
    }
    Ok(swarms)
}

/// Helper to get current timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Creates a test swarm using the main create_swarm function (for compatibility)
pub fn create_test_swarm_with_ping_config() -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    let ping_config = PingConfig::new();
    create_swarm(&ping_config).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}