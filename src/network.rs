use libp2p::floodsub::{Behaviour, Event, Topic};
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{PeerId, identity, mdns, ping};
use log::info;
use once_cell::sync::Lazy;
use std::fs;

pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| match fs::read("peer_key") {
    Ok(bytes) => {
        println!("Found existing peer key file, attempting to load");
        match identity::Keypair::from_protobuf_encoding(&bytes) {
            Ok(keypair) => {
                let peer_id = PeerId::from(keypair.public());
                println!("Successfully loaded keypair with PeerId: {}", peer_id);
                keypair
            }
            Err(e) => {
                println!("Error loading keypair: {}, generating new one", e);
                generate_and_save_keypair()
            }
        }
    }
    Err(e) => {
        println!("No existing key file found ({}), generating new one", e);
        generate_and_save_keypair()
    }
});

pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));

fn generate_and_save_keypair() -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    println!("Generated new keypair with PeerId: {}", peer_id);

    match keypair.to_protobuf_encoding() {
        Ok(bytes) => match fs::write("peer_key", bytes) {
            Ok(_) => println!("Successfully saved keypair to file"),
            Err(e) => println!("Failed to save keypair: {}", e),
        },
        Err(e) => println!("Failed to encode keypair: {}", e),
    }
    keypair
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "StoryBehaviourEvent")]
pub struct StoryBehaviour {
    pub floodsub: Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub ping: ping::Behaviour,
}

#[derive(Debug)]
pub enum StoryBehaviourEvent {
    Floodsub(Event),
    Mdns(mdns::Event),
    Ping(ping::Event),
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

pub fn create_swarm() -> Result<Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    use libp2p::tcp::Config;
    use libp2p::{Transport, core::upgrade, noise, swarm::Config as SwarmConfig, tcp, yamux};

    let transp = tcp::tokio::Transport::new(Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();

    let mut behaviour = StoryBehaviour {
        floodsub: Behaviour::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), *PEER_ID).expect("can create mdns"),
        ping: ping::Behaviour::new(ping::Config::new()),
    };

    info!("Created floodsub with peer id: {:?}", PEER_ID.clone());
    info!("Subscribing to topic: {:?}", TOPIC.clone());
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

    #[tokio::test]
    async fn test_create_swarm_success() {
        // Test that swarm can be created successfully
        let result = create_swarm();
        assert!(result.is_ok());

        let swarm = result.unwrap();
        assert_eq!(swarm.local_peer_id(), &*PEER_ID);
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
}
