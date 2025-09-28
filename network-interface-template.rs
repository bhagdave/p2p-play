//! Core P2P Network interface
//!
//! This module provides the main P2PNetwork struct that encapsulates
//! all networking functionality in a clean, reusable API.

use crate::crypto::CryptoService;
use crate::bootstrap::AutoBootstrap;
use crate::relay::RelayService;
use crate::errors::{NetworkError, NetworkResult};
use crate::types::{NetworkConfig, NetworkEvent, PeerInfo, Message};

use libp2p::{PeerId, Swarm, futures::StreamExt, swarm::SwarmEvent};
use libp2p::floodsub::{Behaviour as FloodsubBehaviour, Event as FloodsubEvent, Topic};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::time::Duration;

/// Main P2P Network client
/// 
/// This struct provides a high-level interface for P2P networking operations
/// including peer discovery, message broadcasting, and direct messaging.
pub struct P2PNetwork {
    swarm: Swarm<NetworkBehaviour>,
    config: NetworkConfig,
    crypto_service: CryptoService,
    bootstrap_service: AutoBootstrap,
    relay_service: RelayService,
    peers: HashMap<PeerId, PeerInfo>,
    event_sender: mpsc::UnboundedSender<NetworkEvent>,
    event_receiver: mpsc::UnboundedReceiver<NetworkEvent>,
}

/// Network behaviour combining all libp2p protocols
#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct NetworkBehaviour {
    pub floodsub: FloodsubBehaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub ping: libp2p::ping::Behaviour,
    pub request_response: libp2p::request_response::cbor::Behaviour<Message, Message>,
    pub kad: libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
}

impl P2PNetwork {
    /// Create a new P2P network with the given configuration
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        
        // Create libp2p swarm
        let swarm = create_swarm(&config).await?;
        
        // Initialize services
        let crypto_service = CryptoService::new(swarm.local_peer_id().clone());
        let bootstrap_service = AutoBootstrap::new(config.bootstrap_config.clone());
        let relay_service = RelayService::new();
        
        Ok(Self {
            swarm,
            config,
            crypto_service,
            bootstrap_service,
            relay_service,
            peers: HashMap::new(),
            event_sender,
            event_receiver,
        })
    }

    /// Get the local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    /// Start listening for connections
    pub async fn start_listening(&mut self) -> NetworkResult<()> {
        for addr in &self.config.listen_addresses {
            self.swarm.listen_on(addr.clone())
                .map_err(|e| NetworkError::ListenError(e.to_string()))?;
        }
        Ok(())
    }

    /// Subscribe to a topic for message broadcasting
    pub fn subscribe_to_topic(&mut self, topic: &str) -> NetworkResult<()> {
        let topic = Topic::new(topic);
        self.swarm.behaviour_mut().floodsub.subscribe(topic.clone());
        self.event_sender.send(NetworkEvent::TopicSubscribed { 
            topic: topic.to_string() 
        }).map_err(|_| NetworkError::EventChannelError)?;
        Ok(())
    }

    /// Publish a message to a topic
    pub fn publish_message(&mut self, topic: &str, message: Vec<u8>) -> NetworkResult<()> {
        let topic = Topic::new(topic);
        self.swarm.behaviour_mut().floodsub.publish(topic, message);
        Ok(())
    }

    /// Send a direct message to a specific peer
    pub async fn send_direct_message(&mut self, peer: PeerId, message: Message) -> NetworkResult<()> {
        // Encrypt message if crypto is enabled
        let encrypted_message = if self.config.encryption_enabled {
            self.crypto_service.encrypt_message(&message, &peer).await?
        } else {
            message
        };

        self.swarm.behaviour_mut()
            .request_response
            .send_request(&peer, encrypted_message);
        
        Ok(())
    }

    /// Get the next network event
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        tokio::select! {
            event = self.swarm.select_next_some() => {
                self.handle_swarm_event(event).await
            }
            event = self.event_receiver.recv() => {
                event
            }
        }
    }

    /// Process swarm events and convert to NetworkEvents
    async fn handle_swarm_event(&mut self, event: SwarmEvent<NetworkBehaviourEvent>) -> Option<NetworkEvent> {
        match event {
            SwarmEvent::Behaviour(NetworkBehaviourEvent::Floodsub(FloodsubEvent::Message(message))) => {
                Some(NetworkEvent::MessageReceived {
                    from: message.source,
                    topic: message.topics.first()?.to_string(),
                    data: message.data,
                })
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.peers.insert(peer_id, PeerInfo::new(peer_id));
                Some(NetworkEvent::PeerConnected { peer: peer_id })
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.peers.remove(&peer_id);
                Some(NetworkEvent::PeerDisconnected { peer: peer_id })
            }
            _ => None,
        }
    }

    /// Get connected peers
    pub fn connected_peers(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }

    /// Bootstrap to the DHT network
    pub async fn bootstrap(&mut self) -> NetworkResult<()> {
        self.bootstrap_service.run_bootstrap(&mut self.swarm).await
    }
}

/// Create a libp2p swarm with the network behaviour
async fn create_swarm(config: &NetworkConfig) -> NetworkResult<Swarm<NetworkBehaviour>> {
    use libp2p::{identity, noise, yamux, tcp, dns};

    // Generate or load keypair
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    // Create transport
    let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(noise::Config::new(&local_key).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();

    // Create network behaviour
    let behaviour = NetworkBehaviour {
        floodsub: FloodsubBehaviour::new(local_peer_id),
        mdns: libp2p::mdns::tokio::Behaviour::new(
            libp2p::mdns::Config::default(), local_peer_id
        ).map_err(|e| NetworkError::InitializationError(e.to_string()))?,
        ping: libp2p::ping::Behaviour::new(libp2p::ping::Config::new()),
        request_response: libp2p::request_response::cbor::Behaviour::new(
            std::iter::once((libp2p::StreamProtocol::new("/p2p-network/1.0.0"), 
            libp2p::request_response::ProtocolSupport::Full)), 
            libp2p::request_response::Config::default()
        ),
        kad: libp2p::kad::Behaviour::new(
            local_peer_id,
            libp2p::kad::store::MemoryStore::new(local_peer_id)
        ),
    };

    // Create swarm
    let swarm = Swarm::with_tokio_executor(transport, behaviour, local_peer_id);
    
    Ok(swarm)
}

/// Re-export for convenience
pub use create_swarm as create_network;