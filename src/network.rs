use crate::types::NetworkConfig;
use crate::types::PingConfig;
use libp2p::floodsub::{Behaviour, Event, Topic};
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{PeerId, StreamProtocol, identity, kad, mdns, ping, request_response};
use log::{debug, warn};
use once_cell::sync::Lazy;
use std::fs;
use std::iter;

/// Direct message request/response types
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageResponse {
    pub received: bool,
    pub timestamp: u64,
}

/// Node description request/response types
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionResponse {
    pub description: Option<String>,
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

/// Story sync request/response types for peer-to-peer story synchronization
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StorySyncRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub last_sync_timestamp: u64,
    pub subscribed_channels: Vec<String>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StorySyncResponse {
    pub stories: Vec<crate::types::Story>,
    pub from_peer_id: String,
    pub from_name: String,
    pub sync_timestamp: u64,
}

pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| match fs::read("peer_key") {
    Ok(bytes) => {
        debug!("Found existing peer key file, attempting to load");
        match identity::Keypair::from_protobuf_encoding(&bytes) {
            Ok(keypair) => {
                let peer_id = PeerId::from(keypair.public());
                debug!("Successfully loaded keypair with PeerId: {peer_id}");
                keypair
            }
            Err(e) => {
                warn!("Error loading keypair: {e}, generating new one");
                generate_and_save_keypair()
            }
        }
    }
    Err(e) => {
        debug!("No existing key file found ({e}), generating new one");
        generate_and_save_keypair()
    }
});

pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));
pub static RELAY_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("relay"));

fn generate_and_save_keypair() -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    debug!("Generated new keypair with PeerId: {peer_id}");

    match keypair.to_protobuf_encoding() {
        Ok(bytes) => match fs::write("peer_key", bytes) {
            Ok(_) => debug!("Successfully saved keypair to file"),
            Err(e) => {
                let error_logger = crate::error_logger::ErrorLogger::new("errors.log");
                error_logger.log_error(&format!("Failed to save keypair: {e}"));
            }
        },
        Err(e) => {
            let error_logger = crate::error_logger::ErrorLogger::new("errors.log");
            error_logger.log_error(&format!("Failed to encode keypair: {e}"));
        }
    }
    keypair
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "StoryBehaviourEvent")]
pub struct StoryBehaviour {
    pub floodsub: Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub ping: ping::Behaviour,
    pub request_response:
        request_response::cbor::Behaviour<DirectMessageRequest, DirectMessageResponse>,
    pub node_description:
        request_response::cbor::Behaviour<NodeDescriptionRequest, NodeDescriptionResponse>,
    pub story_sync: request_response::cbor::Behaviour<StorySyncRequest, StorySyncResponse>,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
}

#[derive(Debug)]
pub enum StoryBehaviourEvent {
    Floodsub(Event),
    Mdns(mdns::Event),
    Ping(ping::Event),
    RequestResponse(request_response::Event<DirectMessageRequest, DirectMessageResponse>),
    NodeDescription(request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>),
    StorySync(request_response::Event<StorySyncRequest, StorySyncResponse>),
    Kad(kad::Event),
}

impl From<Event> for StoryBehaviourEvent {
    fn from(event: Event) -> Self {
        StoryBehaviourEvent::Floodsub(event)
    }
}

impl From<mdns::Event> for StoryBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        StoryBehaviourEvent::Mdns(event)
    }
}

impl From<ping::Event> for StoryBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        StoryBehaviourEvent::Ping(event)
    }
}

impl From<request_response::Event<DirectMessageRequest, DirectMessageResponse>>
    for StoryBehaviourEvent
{
    fn from(event: request_response::Event<DirectMessageRequest, DirectMessageResponse>) -> Self {
        StoryBehaviourEvent::RequestResponse(event)
    }
}

impl From<request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>>
    for StoryBehaviourEvent
{
    fn from(
        event: request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>,
    ) -> Self {
        StoryBehaviourEvent::NodeDescription(event)
    }
}

impl From<request_response::Event<StorySyncRequest, StorySyncResponse>> for StoryBehaviourEvent {
    fn from(event: request_response::Event<StorySyncRequest, StorySyncResponse>) -> Self {
        StoryBehaviourEvent::StorySync(event)
    }
}

impl From<kad::Event> for StoryBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        StoryBehaviourEvent::Kad(event)
    }
}

pub fn create_swarm(
    ping_config: &PingConfig,
) -> Result<Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    use libp2p::tcp::Config;
    use libp2p::{Transport, core::upgrade, dns, noise, swarm::Config as SwarmConfig, tcp, yamux};
    use std::num::NonZeroU8;
    use std::time::Duration;

    // Enhanced TCP configuration with connection limits and pooling optimization
    // Windows doesn't allow immediate port reuse for sockets in TIME_WAIT state
    #[cfg(windows)]
    let tcp_config = Config::default()
        .nodelay(true)
        .listen_backlog(1024) // Increase backlog to handle more connections
        .ttl(64); // Set explicit TTL for better routing

    #[cfg(not(windows))]
    let tcp_config = Config::default()
        .nodelay(true)
        .listen_backlog(1024) // Increase backlog to handle more connections
        .ttl(64); // Set explicit TTL for better routing

    // Enhanced yamux configuration for better connection pooling
    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_num_streams(512); // Allow more concurrent streams for better connection reuse

    let transp = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| format!("Failed to create DNS transport: {e}"))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).unwrap())
        .multiplex(yamux_config)
        .boxed();

    // Load network configuration
    let network_config = NetworkConfig::load_from_file("network_config.json").unwrap_or_else(|e| {
        warn!("Failed to load network config: {e}, using defaults");
        NetworkConfig::new()
    });

    debug!(
        "Using network config - timeout: {}s, streams: {}, connections_per_peer: {}, max_total: {}",
        network_config.request_timeout_seconds,
        network_config.max_concurrent_streams,
        network_config.max_connections_per_peer,
        network_config.max_established_total
    );

    // Create request-response protocol for direct messaging
    let protocol = request_response::ProtocolSupport::Full;
    let dm_protocol = StreamProtocol::new("/dm/1.0.0");
    let dm_protocols = iter::once((dm_protocol, protocol));

    // Configure request-response protocol with timeouts and retry policies
    let cfg = request_response::Config::default()
        .with_request_timeout(std::time::Duration::from_secs(
            network_config.request_timeout_seconds,
        ))
        .with_max_concurrent_streams(network_config.max_concurrent_streams);

    let request_response = request_response::cbor::Behaviour::new(dm_protocols, cfg.clone());

    // Create request-response protocol for node descriptions
    let desc_protocol_support = request_response::ProtocolSupport::Full;
    let desc_protocol = StreamProtocol::new("/node-desc/1.0.0");
    let desc_protocols = iter::once((desc_protocol, desc_protocol_support));

    let node_description = request_response::cbor::Behaviour::new(desc_protocols, cfg.clone());

    // Create request-response protocol for story synchronization
    let story_sync_protocol_support = request_response::ProtocolSupport::Full;
    let story_sync_protocol = StreamProtocol::new("/story-sync/1.0.0");
    let story_sync_protocols = iter::once((story_sync_protocol, story_sync_protocol_support));

    let story_sync = request_response::cbor::Behaviour::new(story_sync_protocols, cfg);

    // Create Kademlia DHT
    let store = kad::store::MemoryStore::new(*PEER_ID);
    let kad_config = kad::Config::default();
    let mut kad = kad::Behaviour::with_config(*PEER_ID, store, kad_config);

    // Set Kademlia mode to server to accept queries and provide records
    kad.set_mode(Some(kad::Mode::Server));

    debug!(
        "Using ping config: interval={}s, timeout={}s",
        ping_config.interval_secs, ping_config.timeout_secs
    );

    let mut behaviour = StoryBehaviour {
        floodsub: Behaviour::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), *PEER_ID).expect("can create mdns"),
        ping: ping::Behaviour::new(
            ping::Config::new()
                .with_interval(ping_config.interval_duration())
                .with_timeout(ping_config.timeout_duration()),
        ),
        request_response,
        node_description,
        story_sync,
        kad,
    };

    debug!("Created floodsub with peer id: {:?}", PEER_ID.clone());
    debug!("Subscribing to topic: {:?}", TOPIC.clone());
    behaviour.floodsub.subscribe(TOPIC.clone());
    debug!("Subscribing to relay topic: {:?}", RELAY_TOPIC.clone());
    behaviour.floodsub.subscribe(RELAY_TOPIC.clone());

    // Enhanced swarm configuration with improved connection management
    let swarm_config = SwarmConfig::with_tokio_executor()
        .with_dial_concurrency_factor(
            NonZeroU8::new(network_config.max_pending_outgoing as u8)
                .unwrap_or(NonZeroU8::new(8).unwrap()),
        ) // Configurable concurrent dial attempts
        .with_idle_connection_timeout(Duration::from_secs(60)); // Idle connection timeout for resource management

    let swarm = Swarm::<StoryBehaviour>::new(transp, behaviour, *PEER_ID, swarm_config);
    Ok(swarm)
}
