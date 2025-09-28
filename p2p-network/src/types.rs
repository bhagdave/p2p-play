//! Core types for the P2P network library

use libp2p::{PeerId, Multiaddr};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Local peer name/alias
    pub peer_name: Option<String>,
    /// Addresses to listen on
    pub listen_addresses: Vec<Multiaddr>,
    /// Bootstrap peers for DHT
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Enable end-to-end encryption
    pub encryption_enabled: bool,
    /// Enable relay functionality
    pub relay_enabled: bool,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Maximum connections
    pub max_connections: usize,
    /// Bootstrap configuration
    pub bootstrap_config: BootstrapConfig,
    /// Ping configuration
    pub ping_config: PingConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            peer_name: None,
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            ],
            bootstrap_peers: Vec::new(),
            encryption_enabled: true,
            relay_enabled: false,
            connection_timeout: Duration::from_secs(10),
            max_connections: 100,
            bootstrap_config: BootstrapConfig::default(),
            ping_config: PingConfig::default(),
        }
    }
}

/// Bootstrap configuration for DHT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    pub enabled: bool,
    pub auto_bootstrap_interval: Duration,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub max_bootstrap_peers: usize,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_bootstrap_interval: Duration::from_secs(300),
            bootstrap_peers: Vec::new(),
            max_bootstrap_peers: 10,
        }
    }
}

/// Ping protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingConfig {
    pub interval: Duration,
    pub timeout: Duration,
    pub max_failures: u32,
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(15),
            timeout: Duration::from_secs(20),
            max_failures: 3,
        }
    }
}

/// Network events emitted by the P2P network
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A new peer connected
    PeerConnected { peer: PeerId },
    /// A peer disconnected  
    PeerDisconnected { peer: PeerId },
    /// A message was received on a topic
    MessageReceived { from: PeerId, topic: String, data: Vec<u8> },
    /// A direct message was received
    DirectMessageReceived { from: PeerId, message: Message },
    /// Subscribed to a topic
    TopicSubscribed { topic: String },
    /// Unsubscribed from a topic
    TopicUnsubscribed { topic: String },
    /// Started listening on an address
    ListeningOn { address: Multiaddr },
    /// Bootstrap completed
    BootstrapCompleted,
    /// Network error occurred
    NetworkError { error: String },
}

/// Information about a connected peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub name: Option<String>,
    pub connected_at: u64,
    pub last_seen: u64,
    pub connection_count: usize,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            peer_id,
            name: None,
            connected_at: now,
            last_seen: now,
            connection_count: 1,
        }
    }
}

/// Generic message structure for network communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub recipient: Option<String>,
    pub content: MessageContent,
    pub timestamp: u64,
}

/// Different types of message content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageContent {
    Text(String),
    Story(Story),
    DirectMessage(DirectMessage),
    ChannelMessage { channel: String, content: String },
    Handshake { version: String, app_name: String },
    StorySync { stories: Vec<Story> },
    ChannelSync { channels: Vec<Channel> },
    Binary(Vec<u8>),
}

/// Story structure for sharing stories across the network
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Story {
    pub id: usize,
    pub name: String,
    pub header: String,
    pub body: String,
    pub public: bool,
    pub channel: String,
    pub created_at: u64,
    pub auto_share: Option<bool>,
}

/// Direct message between peers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectMessage {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
    pub encrypted: bool,
}

/// Channel for organizing stories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Channel {
    pub name: String,
    pub description: String,
    pub created_at: u64,
    pub subscriber_count: usize,
}

/// Peer message wrapper
#[derive(Debug, Clone)]
pub struct PeerMessage {
    pub from: PeerId,
    pub to: Option<PeerId>,
    pub message: Message,
}

/// Result type alias for network operations
pub type NetworkResult<T> = Result<T, crate::errors::NetworkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert!(config.encryption_enabled);
        assert_eq!(config.max_connections, 100);
        assert!(!config.listen_addresses.is_empty());
    }

    #[test]
    fn test_peer_info_creation() {
        let peer_id = PeerId::random();
        let info = PeerInfo::new(peer_id);
        assert_eq!(info.peer_id, peer_id);
        assert_eq!(info.connection_count, 1);
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            id: "test-123".to_string(),
            sender: "peer-1".to_string(),
            recipient: Some("peer-2".to_string()),
            content: MessageContent::Text("Hello, world!".to_string()),
            timestamp: 1234567890,
        };

        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&serialized).unwrap();
        assert_eq!(message, deserialized);
    }
}