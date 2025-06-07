use libp2p::floodsub::{Floodsub, FloodsubEvent, Topic};
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
    pub floodsub: Floodsub,
    pub mdns: mdns::tokio::Behaviour,
    pub ping: ping::Behaviour,
}

#[derive(Debug)]
pub enum StoryBehaviourEvent {
    Floodsub(FloodsubEvent),
    Mdns(mdns::Event),
    Ping(ping::Event),
}

impl From<FloodsubEvent> for StoryBehaviourEvent {
    fn from(event: FloodsubEvent) -> Self {
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
        floodsub: Floodsub::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), PEER_ID.clone())
            .expect("can create mdns"),
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
