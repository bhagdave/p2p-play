//! Core P2P Network interface
//!
//! This module provides the main P2PNetwork struct that encapsulates
//! all networking functionality in a clean, reusable API.

use crate::crypto::CryptoService;
use crate::bootstrap::AutoBootstrap;
use crate::relay::RelayService;
use crate::errors::{NetworkError, NetworkResult};
use crate::types::{NetworkConfig, NetworkEvent, PeerInfo, Message};

use libp2p::{PeerId, Swarm, futures::StreamExt, swarm::SwarmEvent, Multiaddr};
use libp2p::floodsub::{Behaviour as FloodsubBehaviour, Topic};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::time::Duration;
use once_cell::sync::Lazy;

static KEYS: Lazy<libp2p::identity::Keypair> = Lazy::new(|| {
    // Try to load existing key or generate new one
    if let Ok(key_bytes) = std::fs::read("peer_key") {
        if let Ok(keypair) = libp2p::identity::Keypair::from_protobuf_encoding(&key_bytes) {
            return keypair;
        }
    }
    
    // Generate new key and save it
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    if let Ok(encoded) = keypair.to_protobuf_encoding() {
        let _ = std::fs::write("peer_key", encoded);
    }
    keypair
});

static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));

/// Main P2P Network client
/// 
/// This struct provides a high-level interface for P2P networking operations
/// including peer discovery, message broadcasting, and direct messaging.
pub struct P2PNetwork {
    swarm: Swarm<NetworkBehaviour>,
    config: NetworkConfig,
    crypto_service: Option<CryptoService>,
    bootstrap_service: Option<AutoBootstrap>,
    relay_service: Option<RelayService>,
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
        
        // Initialize services based on configuration
        let crypto_service = if config.encryption_enabled {
            Some(CryptoService::new(KEYS.clone()))
        } else {
            None
        };
        
        let bootstrap_service = if config.bootstrap_config.enabled {
            Some(AutoBootstrap::new(config.bootstrap_config.clone()))
        } else {
            None
        };
        
        let relay_service = if config.relay_enabled {
            Some(RelayService::new())
        } else {
            None
        };
        
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
        *PEER_ID
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
        
        // Send notification event
        let _ = self.event_sender.send(NetworkEvent::TopicSubscribed { 
            topic: topic.to_string() 
        });
        Ok(())
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe_from_topic(&mut self, topic: &str) -> NetworkResult<()> {
        let topic = Topic::new(topic);
        self.swarm.behaviour_mut().floodsub.unsubscribe(topic.clone());
        
        // Send notification event
        let _ = self.event_sender.send(NetworkEvent::TopicUnsubscribed { 
            topic: topic.to_string() 
        });
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
        let final_message = if let Some(crypto) = &self.crypto_service {
            match crypto.encrypt_message(&message, &peer).await {
                Ok(encrypted) => encrypted,
                Err(e) => return Err(NetworkError::EncryptionError(e)),
            }
        } else {
            message
        };

        self.swarm.behaviour_mut()
            .request_response
            .send_request(&peer, final_message);
        
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
        use libp2p::swarm::SwarmEvent;
        
        match event {
            SwarmEvent::Behaviour(_behaviour_event) => {
                // Handle network behaviour events
                // For now, simplified - can be expanded later
                None
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.peers.insert(peer_id, PeerInfo::new(peer_id));
                Some(NetworkEvent::PeerConnected { peer: peer_id })
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.peers.remove(&peer_id);
                Some(NetworkEvent::PeerDisconnected { peer: peer_id })
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                Some(NetworkEvent::ListeningOn { address })
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
        if let Some(bootstrap) = &mut self.bootstrap_service {
            bootstrap.run_bootstrap(&mut self.swarm).await?;
        }
        Ok(())
    }

    /// Add a bootstrap peer
    pub fn add_bootstrap_peer(&mut self, address: Multiaddr) -> NetworkResult<()> {
        if let Some(bootstrap) = &mut self.bootstrap_service {
            bootstrap.add_peer(address);
        }
        Ok(())
    }

    /// Get network statistics
    pub fn network_stats(&self) -> NetworkStats {
        NetworkStats {
            connected_peers: self.peers.len(),
            local_peer_id: *PEER_ID,
            listening_addresses: self.swarm.listeners().cloned().collect(),
        }
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub local_peer_id: PeerId,
    pub listening_addresses: Vec<Multiaddr>,
}

/// Create a libp2p swarm with the network behaviour
async fn create_swarm(config: &NetworkConfig) -> NetworkResult<Swarm<NetworkBehaviour>> {
    use libp2p::{core::upgrade, dns, noise, yamux, tcp, request_response, StreamProtocol};
    use std::iter;

    // Create transport
    let tcp_config = tcp::Config::default().nodelay(true);
    let transport = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| NetworkError::TransportError(format!("DNS transport error: {}", e)))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).map_err(|e| NetworkError::InitializationError(e.to_string()))?)
        .multiplex(yamux::Config::default())
        .boxed();

    // Create request-response protocol
    let protocol = request_response::ProtocolSupport::Full;
    let dm_protocol = StreamProtocol::new("/p2p-network/dm/1.0.0");
    let protocols = iter::once((dm_protocol, protocol));
    
    let req_resp_config = request_response::Config::default()
        .with_request_timeout(config.connection_timeout);
    
    let request_response = request_response::cbor::Behaviour::new(protocols, req_resp_config);

    // Create Kademlia DHT
    let store = libp2p::kad::store::MemoryStore::new(*PEER_ID);
    let kad_config = libp2p::kad::Config::default();
    let kad = libp2p::kad::Behaviour::with_config(*PEER_ID, store, kad_config);

    // Create network behaviour
    let behaviour = NetworkBehaviour {
        floodsub: FloodsubBehaviour::new(*PEER_ID),
        mdns: libp2p::mdns::tokio::Behaviour::new(
            libp2p::mdns::Config::default(), 
            *PEER_ID
        ).map_err(|e| NetworkError::InitializationError(e.to_string()))?,
        ping: libp2p::ping::Behaviour::new(
            libp2p::ping::Config::new()
                .with_interval(config.ping_config.interval)
                .with_timeout(config.ping_config.timeout)
        ),
        request_response,
        kad,
    };

    // Create swarm with connection limits
    let swarm_config = libp2p::swarm::Config::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(60));

    let swarm = Swarm::new(transport, behaviour, *PEER_ID, swarm_config);
    Ok(swarm)
}

/// Network behaviour event type - simplified for now
#[derive(Debug)]
pub enum NetworkBehaviourEvent {
    Floodsub(libp2p::floodsub::Event),
    Mdns(libp2p::mdns::Event),
    Ping(libp2p::ping::Event),
    RequestResponse(libp2p::request_response::Event<Message, Message>),
    Kad(libp2p::kad::Event),
}

// Event conversions - these will be handled by the NetworkBehaviour derive macro