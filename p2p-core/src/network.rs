use crate::errors::NetworkResult;
use crate::types::{
    NetworkConfig, PingConfig, DirectMessageRequest, DirectMessageResponse,
    NodeDescriptionRequest, NodeDescriptionResponse, HandshakeRequest, HandshakeResponse,
};
use libp2p::floodsub::{Behaviour, Event, Topic};
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{PeerId, StreamProtocol, identity, kad, mdns, ping, request_response};
use log::{debug, warn};
use once_cell::sync::Lazy;
use std::fs;
use std::iter;

pub const APP_PROTOCOL: &str = "/p2p-core/handshake/1.0.0";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

/// Global keypair for peer identity
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

/// Global peer ID derived from keypair
pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));

/// Default topic for general messages
pub static DEFAULT_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("p2p-core-messages"));

/// Relay topic for message forwarding
pub static RELAY_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("p2p-core-relay"));

fn generate_and_save_keypair() -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    debug!("Generated new keypair with PeerId: {peer_id}");

    match keypair.to_protobuf_encoding() {
        Ok(bytes) => match fs::write("peer_key", bytes) {
            Ok(_) => debug!("Successfully saved keypair to file"),
            Err(e) => warn!("Failed to save keypair: {e}"),
        },
        Err(e) => warn!("Failed to encode keypair: {e}"),
    }
    keypair
}

/// Network behaviour combining all libp2p protocols
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NetworkBehaviourEvent")]
pub struct P2PBehaviour {
    pub floodsub: Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub ping: ping::Behaviour,
    pub request_response:
        request_response::cbor::Behaviour<DirectMessageRequest, DirectMessageResponse>,
    pub node_description:
        request_response::cbor::Behaviour<NodeDescriptionRequest, NodeDescriptionResponse>,
    pub handshake: request_response::cbor::Behaviour<HandshakeRequest, HandshakeResponse>,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
}

/// Events emitted by the network behaviour
#[derive(Debug)]
pub enum NetworkBehaviourEvent {
    Floodsub(Event),
    Mdns(mdns::Event),
    Ping(ping::Event),
    RequestResponse(request_response::Event<DirectMessageRequest, DirectMessageResponse>),
    NodeDescription(request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>),
    Handshake(request_response::Event<HandshakeRequest, HandshakeResponse>),
    Kad(kad::Event),
}

impl From<Event> for NetworkBehaviourEvent {
    fn from(event: Event) -> Self {
        NetworkBehaviourEvent::Floodsub(event)
    }
}

impl From<mdns::Event> for NetworkBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        NetworkBehaviourEvent::Mdns(event)
    }
}

impl From<ping::Event> for NetworkBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        NetworkBehaviourEvent::Ping(event)
    }
}

impl From<request_response::Event<DirectMessageRequest, DirectMessageResponse>>
    for NetworkBehaviourEvent
{
    fn from(event: request_response::Event<DirectMessageRequest, DirectMessageResponse>) -> Self {
        NetworkBehaviourEvent::RequestResponse(event)
    }
}

impl From<request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>>
    for NetworkBehaviourEvent
{
    fn from(
        event: request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>,
    ) -> Self {
        NetworkBehaviourEvent::NodeDescription(event)
    }
}

impl From<request_response::Event<HandshakeRequest, HandshakeResponse>> for NetworkBehaviourEvent {
    fn from(event: request_response::Event<HandshakeRequest, HandshakeResponse>) -> Self {
        NetworkBehaviourEvent::Handshake(event)
    }
}

impl From<kad::Event> for NetworkBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        NetworkBehaviourEvent::Kad(event)
    }
}

/// High-level network service
pub struct NetworkService {
    pub swarm: Swarm<P2PBehaviour>,
    pub config: NetworkConfig,
}

impl NetworkService {
    /// Create a new network service with configuration
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        let ping_config = PingConfig::default();
        let swarm = create_swarm(&ping_config, &config)?;
        
        Ok(Self {
            swarm,
            config,
        })
    }

    /// Create a new network service with custom ping configuration
    pub async fn new_with_ping(config: NetworkConfig, ping_config: PingConfig) -> NetworkResult<Self> {
        let swarm = create_swarm(&ping_config, &config)?;
        
        Ok(Self {
            swarm,
            config,
        })
    }

    /// Get the local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        *PEER_ID
    }

    /// Subscribe to a topic for floodsub messaging
    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.swarm.behaviour_mut().floodsub.subscribe(topic)
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &Topic) -> bool {
        self.swarm.behaviour_mut().floodsub.unsubscribe(topic.clone())
    }

    /// Publish a message to a topic
    pub fn publish(&mut self, topic: Topic, message: Vec<u8>) -> NetworkResult<()> {
        self.swarm.behaviour_mut().floodsub.publish(topic, message);
        Ok(())
    }

    /// Send a direct message to a peer
    pub fn send_direct_message(&mut self, peer_id: PeerId, request: DirectMessageRequest) -> request_response::OutboundRequestId {
        self.swarm.behaviour_mut().request_response.send_request(&peer_id, request)
    }

    /// Request node description from a peer
    pub fn request_node_description(&mut self, peer_id: PeerId, request: NodeDescriptionRequest) -> request_response::OutboundRequestId {
        self.swarm.behaviour_mut().node_description.send_request(&peer_id, request)
    }

    /// Send handshake to a peer
    pub fn send_handshake(&mut self, peer_id: PeerId, request: HandshakeRequest) -> request_response::OutboundRequestId {
        self.swarm.behaviour_mut().handshake.send_request(&peer_id, request)
    }

    /// Get connected peers
    pub fn connected_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.swarm.connected_peers()
    }

    /// Dial a peer
    pub fn dial(&mut self, peer_id: PeerId) -> Result<(), libp2p::swarm::DialError> {
        self.swarm.dial(peer_id)
    }

    /// Listen on an address
    pub fn listen_on(&mut self, addr: libp2p::Multiaddr) -> NetworkResult<()> {
        self.swarm.listen_on(addr).map_err(|e| format!("Failed to listen on address: {}", e))?;
        Ok(())
    }
}

/// Create a libp2p swarm with the specified configuration
pub fn create_swarm(ping_config: &PingConfig, network_config: &NetworkConfig) -> NetworkResult<Swarm<P2PBehaviour>> {
    use libp2p::tcp::Config;
    use libp2p::{Transport, core::upgrade, dns, noise, swarm::Config as SwarmConfig, tcp, yamux};
    use std::num::NonZeroU8;
    use std::time::Duration;

    // Enhanced TCP configuration with connection limits and pooling optimization
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

    // Enhanced yamux configuration for better connection pooling
    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_num_streams(512);

    let transp = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| format!("Failed to create DNS transport: {e}"))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).unwrap())
        .multiplex(yamux_config)
        .boxed();

    debug!(
        "Using network config - timeout: {}s, streams: {}, connections_per_peer: {}, max_total: {}",
        network_config.request_timeout_seconds,
        network_config.max_concurrent_streams,
        network_config.max_connections_per_peer,
        network_config.max_established_total
    );

    let protocol = request_response::ProtocolSupport::Full;
    let dm_protocol = StreamProtocol::new("/p2p-core/dm/1.0.0");
    let dm_protocols = iter::once((dm_protocol, protocol));

    let cfg = request_response::Config::default()
        .with_request_timeout(Duration::from_secs(network_config.request_timeout_seconds))
        .with_max_concurrent_streams(network_config.max_concurrent_streams);

    let request_response = request_response::cbor::Behaviour::new(dm_protocols, cfg.clone());

    let desc_protocol_support = request_response::ProtocolSupport::Full;
    let desc_protocol = StreamProtocol::new("/p2p-core/node-desc/1.0.0");
    let desc_protocols = iter::once((desc_protocol, desc_protocol_support));

    let node_description = request_response::cbor::Behaviour::new(desc_protocols, cfg.clone());

    let handshake_protocol_support = request_response::ProtocolSupport::Full;
    let handshake_protocol = StreamProtocol::new(APP_PROTOCOL);
    let handshake_protocols = iter::once((handshake_protocol, handshake_protocol_support));

    let handshake = request_response::cbor::Behaviour::new(handshake_protocols, cfg);

    // Create Kademlia DHT
    let store = kad::store::MemoryStore::new(*PEER_ID);
    let kad_config = kad::Config::default();
    let mut kad = kad::Behaviour::with_config(*PEER_ID, store, kad_config);

    kad.set_mode(Some(kad::Mode::Server));

    debug!(
        "Using ping config: interval={}s, timeout={}s",
        ping_config.interval_secs, ping_config.timeout_secs
    );

    let mut behaviour = P2PBehaviour {
        floodsub: Behaviour::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), *PEER_ID)
            .expect("can create mdns"),
        ping: ping::Behaviour::new(
            ping::Config::new()
                .with_interval(ping_config.interval_duration())
                .with_timeout(ping_config.timeout_duration()),
        ),
        request_response,
        node_description,
        handshake,
        kad,
    };

    debug!("Created floodsub with peer id: {:?}", PEER_ID.clone());
    debug!("Subscribing to default topic: {:?}", DEFAULT_TOPIC.clone());
    behaviour.floodsub.subscribe(DEFAULT_TOPIC.clone());
    debug!("Subscribing to relay topic: {:?}", RELAY_TOPIC.clone());
    behaviour.floodsub.subscribe(RELAY_TOPIC.clone());

    // Enhanced swarm configuration
    let swarm_config = SwarmConfig::with_tokio_executor()
        .with_dial_concurrency_factor(
            NonZeroU8::new(network_config.max_pending_outgoing as u8)
                .unwrap_or(NonZeroU8::new(8).unwrap()),
        )
        .with_idle_connection_timeout(Duration::from_secs(60));

    let swarm = Swarm::<P2PBehaviour>::new(transp, behaviour, *PEER_ID, swarm_config);
    Ok(swarm)
}