use libp2p::floodsub::{Behaviour, Event, Topic};
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{PeerId, StreamProtocol, identity, kad, mdns, ping, request_response};
use log::{debug, error, warn};
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

pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| match fs::read("peer_key") {
    Ok(bytes) => {
        debug!("Found existing peer key file, attempting to load");
        match identity::Keypair::from_protobuf_encoding(&bytes) {
            Ok(keypair) => {
                let peer_id = PeerId::from(keypair.public());
                debug!("Successfully loaded keypair with PeerId: {}", peer_id);
                keypair
            }
            Err(e) => {
                warn!("Error loading keypair: {}, generating new one", e);
                generate_and_save_keypair()
            }
        }
    }
    Err(e) => {
        debug!("No existing key file found ({}), generating new one", e);
        generate_and_save_keypair()
    }
});

pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));

fn generate_and_save_keypair() -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    debug!("Generated new keypair with PeerId: {}", peer_id);

    match keypair.to_protobuf_encoding() {
        Ok(bytes) => match fs::write("peer_key", bytes) {
            Ok(_) => debug!("Successfully saved keypair to file"),
            Err(e) => error!("Failed to save keypair: {}", e),
        },
        Err(e) => error!("Failed to encode keypair: {}", e),
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
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
}

#[derive(Debug)]
pub enum StoryBehaviourEvent {
    Floodsub(Event),
    Mdns(mdns::Event),
    Ping(ping::Event),
    RequestResponse(request_response::Event<DirectMessageRequest, DirectMessageResponse>),
    NodeDescription(request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>),
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

impl From<kad::Event> for StoryBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        StoryBehaviourEvent::Kad(event)
    }
}

pub fn create_swarm() -> Result<Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    use libp2p::tcp::Config;
    use libp2p::{Transport, core::upgrade, noise, swarm::Config as SwarmConfig, tcp, yamux};

    // Configure TCP transport with Windows-specific socket settings
    // Windows doesn't allow immediate port reuse for sockets in TIME_WAIT state
    #[cfg(windows)]
    let tcp_config = Config::default()
        .nodelay(true)
        .port_reuse(false) // Disable port reuse on Windows to avoid WSAEADDRINUSE errors
        .listen_backlog(1024); // Increase backlog to handle more connections

    #[cfg(not(windows))]
    let tcp_config = Config::default().nodelay(true);

    let transp = tcp::tokio::Transport::new(tcp_config)
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();

    // Create request-response protocol for direct messaging
    let protocol = request_response::ProtocolSupport::Full;
    let dm_protocol = StreamProtocol::new("/dm/1.0.0");
    let dm_protocols = iter::once((dm_protocol, protocol));

    // Configure request-response protocol with timeouts and retry policies
    let cfg = request_response::Config::default()
        .with_request_timeout(std::time::Duration::from_secs(30))
        .with_max_concurrent_streams(100);

    let request_response = request_response::cbor::Behaviour::new(dm_protocols, cfg.clone());

    // Create request-response protocol for node descriptions
    let desc_protocol_support = request_response::ProtocolSupport::Full;
    let desc_protocol = StreamProtocol::new("/node-desc/1.0.0");
    let desc_protocols = iter::once((desc_protocol, desc_protocol_support));

    let node_description = request_response::cbor::Behaviour::new(desc_protocols, cfg);

    // Create Kademlia DHT
    let store = kad::store::MemoryStore::new(*PEER_ID);
    let kad_config = kad::Config::default();
    let mut kad = kad::Behaviour::with_config(*PEER_ID, store, kad_config);

    // Set Kademlia mode to server to accept queries and provide records
    kad.set_mode(Some(kad::Mode::Server));

    let mut behaviour = StoryBehaviour {
        floodsub: Behaviour::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), *PEER_ID).expect("can create mdns"),
        ping: ping::Behaviour::new(ping::Config::new()),
        request_response,
        node_description,
        kad,
    };

    debug!("Created floodsub with peer id: {:?}", PEER_ID.clone());
    debug!("Subscribing to topic: {:?}", TOPIC.clone());
    behaviour.floodsub.subscribe(TOPIC.clone());

    let swarm = Swarm::<StoryBehaviour>::new(
        transp,
        behaviour,
        *PEER_ID,
        SwarmConfig::with_tokio_executor(),
    );
    Ok(swarm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_peer_id_consistency() {
        // Test that PEER_ID is consistent across multiple accesses
        let peer_id_1 = *PEER_ID;
        let peer_id_2 = *PEER_ID;
        assert_eq!(peer_id_1, peer_id_2);
    }

    #[test]
    fn test_static_topic_creation() {
        // Test that TOPIC is properly created
        let topic = TOPIC.clone();
        let topic_str = format!("{:?}", topic);
        assert!(topic_str.contains("stories"));
    }

    #[test]
    fn test_story_behaviour_event_from_floodsub() {
        use bytes::Bytes;
        use libp2p::floodsub::{Event, FloodsubMessage};

        // Create a mock floodsub event
        let mock_message = FloodsubMessage {
            source: *PEER_ID,
            data: Bytes::from("test data"),
            sequence_number: b"seq123".to_vec(),
            topics: vec![TOPIC.clone()],
        };
        let floodsub_event = Event::Message(mock_message);

        // Test conversion
        let story_event = StoryBehaviourEvent::from(floodsub_event);
        match story_event {
            StoryBehaviourEvent::Floodsub(event) => {
                // We can't easily compare the exact content due to the enum structure,
                // but we can verify the conversion worked
                assert!(matches!(event, Event::Message(_)));
            }
            _ => panic!("Expected Floodsub event"),
        }
    }

    #[test]
    fn test_story_behaviour_event_from_mdns() {
        use libp2p::mdns::Event as MdnsEvent;

        // Create a mock mDNS event - using the Discovered variant
        let mdns_event = MdnsEvent::Discovered(
            std::iter::once((*PEER_ID, "/ip4/127.0.0.1/tcp/8080".parse().unwrap())).collect(),
        );

        // Test conversion
        let story_event = StoryBehaviourEvent::from(mdns_event);
        match story_event {
            StoryBehaviourEvent::Mdns(_) => {
                // Conversion worked
            }
            _ => panic!("Expected Mdns event"),
        }
    }

    #[test]
    fn test_story_behaviour_event_from_ping() {
        use libp2p::ping::Event as PingEvent;
        use std::time::Duration;

        // Create a mock ping event - use a minimal struct for testing
        let ping_event = PingEvent {
            peer: *PEER_ID,
            connection: libp2p::swarm::ConnectionId::new_unchecked(1),
            result: Ok(Duration::from_millis(50)),
        };

        // Test conversion
        let story_event = StoryBehaviourEvent::from(ping_event);
        match story_event {
            StoryBehaviourEvent::Ping(_) => {
                // Conversion worked
            }
            _ => panic!("Expected Ping event"),
        }
    }

    #[test]
    fn test_story_behaviour_event_from_kad() {
        use libp2p::kad::Event as KadEvent;

        // Create a mock kad event - using the simplest variant
        let kad_event = KadEvent::ModeChanged {
            new_mode: libp2p::kad::Mode::Client,
        };

        // Test conversion
        let story_event = StoryBehaviourEvent::from(kad_event);
        match story_event {
            StoryBehaviourEvent::Kad(_) => {
                // Conversion worked
            }
            _ => panic!("Expected Kad event"),
        }
    }

    #[test]
    fn test_kad_in_story_behaviour() {
        // Test that kad is properly included in the behaviour
        let store = kad::store::MemoryStore::new(*PEER_ID);
        let kad_config = kad::Config::default();
        let mut kad = kad::Behaviour::with_config(*PEER_ID, store, kad_config);
        kad.set_mode(Some(kad::Mode::Server));

        // Verify kad mode was set
        // We can't easily test the internal state, but this verifies compilation
        let _kad = kad;
    }

    #[tokio::test]
    async fn test_create_swarm_success() {
        // Test that swarm can be created successfully
        let result = create_swarm();
        assert!(result.is_ok());

        let swarm = result.unwrap();
        assert_eq!(swarm.local_peer_id(), &*PEER_ID);
    }

    #[test]
    fn test_tcp_config_windows_vs_unix() {
        // Test that TCP configuration is properly set based on target OS
        use libp2p::tcp::Config;

        #[cfg(windows)]
        {
            let tcp_config = Config::default().nodelay(true).port_reuse(false);
            // On Windows, port_reuse should be disabled
            // Note: We can't directly test the internal state of tcp_config,
            // but we can verify the configuration builds without errors
            let _config = tcp_config;
        }

        #[cfg(not(windows))]
        {
            let tcp_config = Config::default().nodelay(true);
            // On Unix systems, default behavior (with port_reuse enabled)
            let _config = tcp_config;
        }
    }

    #[test]
    fn test_story_behaviour_event_debug() {
        use libp2p::ping::Event as PingEvent;
        use std::time::Duration;

        // Test that StoryBehaviourEvent implements Debug properly
        let ping_event = PingEvent {
            peer: *PEER_ID,
            connection: libp2p::swarm::ConnectionId::new_unchecked(1),
            result: Ok(Duration::from_millis(50)),
        };
        let story_event = StoryBehaviourEvent::from(ping_event);

        // This should not panic - tests that Debug is properly derived
        let debug_str = format!("{:?}", story_event);
        assert!(debug_str.contains("Ping"));
    }

    #[test]
    fn test_direct_message_request_response_types() {
        // Test DirectMessageRequest
        let request = DirectMessageRequest {
            from_peer_id: "peer123".to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello!".to_string(),
            timestamp: 1000,
        };

        assert_eq!(request.from_peer_id, "peer123");
        assert_eq!(request.from_name, "Alice");
        assert_eq!(request.to_name, "Bob");
        assert_eq!(request.message, "Hello!");
        assert_eq!(request.timestamp, 1000);

        // Test DirectMessageResponse
        let response = DirectMessageResponse {
            received: true,
            timestamp: 2000,
        };

        assert!(response.received);
        assert_eq!(response.timestamp, 2000);
    }

    #[test]
    fn test_request_response_protocol_configuration() {
        // Test that we can create a request-response configuration
        let cfg = request_response::Config::default()
            .with_request_timeout(std::time::Duration::from_secs(30))
            .with_max_concurrent_streams(100);

        // These tests verify the configuration can be created
        // The actual timeout values are internal to libp2p, so we can't easily test them directly
        // but we can verify the configuration build process works
        let _cfg = cfg; // Just verify it compiles and can be used
    }
}
