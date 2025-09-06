use crate::crypto::{EncryptedPayload, MessageSignature};
use crate::errors::ConfigResult;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Network configuration for P2P connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
    /// Maximum concurrent streams per connection
    pub max_concurrent_streams: usize,
    /// Maximum connections per peer
    pub max_connections_per_peer: u32,
    /// Maximum total established connections
    pub max_established_total: u32,
    /// Maximum pending outgoing connections
    pub max_pending_outgoing: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            request_timeout_seconds: 30,
            max_concurrent_streams: 128,
            max_connections_per_peer: 5,
            max_established_total: 100,
            max_pending_outgoing: 8,
        }
    }
}

/// Ping protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingConfig {
    pub interval_secs: u64,
    pub timeout_secs: u64,
}

impl PingConfig {
    pub fn interval_duration(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    pub fn timeout_duration(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            timeout_secs: 10,
        }
    }
}

/// Request for direct message between peers
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
}

/// Response to direct message request
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageResponse {
    pub received: bool,
    pub timestamp: u64,
}

/// Request for node description
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

/// Response with node description
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionResponse {
    pub description: Option<String>,
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

/// Request for handshake with peer
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeRequest {
    pub app_name: String,
    pub app_version: String,
    pub peer_id: String,
}

/// Response to handshake request
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub app_name: String,
    pub app_version: String,
}

/// Information about a peer awaiting handshake completion
#[derive(Debug, Clone)]
pub struct PendingHandshakePeer {
    pub peer_id: PeerId,
    pub connection_time: Instant,
    pub endpoint: libp2p::core::ConnectedPoint,
}

/// Circuit breaker configuration for network resilience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub timeout_duration_ms: u64,
    pub half_open_retry_timeout_ms: u64,
    pub max_retry_count: u32,
    pub backoff_multiplier: f32,
    pub enabled: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout_duration_ms: 5000,
            half_open_retry_timeout_ms: 10000,
            max_retry_count: 3,
            backoff_multiplier: 1.5,
            enabled: true,
        }
    }
}

/// Direct message with encryption support
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectMessage {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_peer_id: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
    pub is_outgoing: bool,
}

/// Encrypted message for relay
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RelayMessage {
    pub message_type: String,
    pub relay_ttl: u8,
    pub message_id: String,                  // Unique message identifier
    pub target_peer_id: String,              // Intended recipient
    pub target_name: String,                 // Recipient's alias
    pub encrypted_payload: EncryptedPayload, // From crypto module
    pub sender_signature: MessageSignature,  // Authentication
    pub hop_count: u8,                       // Prevent infinite loops
    pub max_hops: u8,                        // Maximum relay hops allowed
    pub timestamp: u64,                      // For replay protection
    pub relay_attempt: bool,                 // Distinguishes from direct attempts
}

impl RelayMessage {
    pub fn new(
        message_id: String,
        target_peer_id: String,
        target_name: String,
        encrypted_payload: EncryptedPayload,
        sender_signature: MessageSignature,
        max_hops: u8,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            message_id,
            target_peer_id,
            target_name,
            encrypted_payload,
            sender_signature,
            hop_count: 0,
            max_hops,
            timestamp,
            relay_attempt: true,
        }
    }

    pub fn increment_hop_count(&mut self) -> Result<(), String> {
        if self.hop_count >= self.max_hops {
            return Err(format!(
                "Max hops exceeded: {}/{}",
                self.hop_count, self.max_hops
            ));
        }
        self.hop_count += 1;
        Ok(())
    }

    pub fn can_forward(&self) -> bool {
        self.hop_count < self.max_hops
    }
}

/// Relay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub max_ttl: u8,
    pub max_message_size: usize,
    pub relay_timeout_secs: u64,
    pub max_pending_relays: usize,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            max_ttl: 10,
            max_message_size: 1024 * 1024, // 1MB
            relay_timeout_secs: 300,        // 5 minutes
            max_pending_relays: 1000,
        }
    }
}

/// Event types for the P2P network
#[derive(Debug, Clone, PartialEq)]
pub enum EventType {
    /// Peer-related events
    Peer(PeerEvent),
    /// Network-related events  
    Network(NetworkEvent),
    /// User input events
    UserInput(String),
    /// Response events
    Response(String),
}

/// Peer-related events
#[derive(Debug, Clone, PartialEq)]
pub enum PeerEvent {
    /// Peer connected
    Connected { peer_id: PeerId, endpoint: String },
    /// Peer disconnected
    Disconnected { peer_id: PeerId },
    /// Peer discovered
    Discovered { peer_id: PeerId, addresses: Vec<String> },
    /// Direct message received
    DirectMessage(DirectMessage),
    /// Node description received
    NodeDescription { peer_id: PeerId, description: String },
}

/// Network-related events
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkEvent {
    /// Bootstrap attempt started
    BootstrapStarted,
    /// Bootstrap completed successfully
    BootstrapCompleted { connected_peers: usize },
    /// Bootstrap failed
    BootstrapFailed { error: String },
    /// Message published to network
    MessagePublished { topic: String, message: String },
    /// Message received from network
    MessageReceived { topic: String, message: String, peer_id: PeerId },
    /// Connection established
    ConnectionEstablished { peer_id: PeerId },
    /// Connection closed
    ConnectionClosed { peer_id: PeerId, error: Option<String> },
}

/// Validate network configuration
impl NetworkConfig {
    pub fn validate(&self) -> ConfigResult<()> {
        if self.request_timeout_seconds == 0 {
            return Err("Request timeout must be greater than 0".into());
        }
        if self.max_concurrent_streams == 0 {
            return Err("Max concurrent streams must be greater than 0".into());
        }
        if self.max_connections_per_peer == 0 {
            return Err("Max connections per peer must be greater than 0".into());
        }
        if self.max_established_total == 0 {
            return Err("Max established total must be greater than 0".into());
        }
        Ok(())
    }
}

/// Validate ping configuration
impl PingConfig {
    pub fn validate(&self) -> ConfigResult<()> {
        if self.interval_secs == 0 {
            return Err("Ping interval must be greater than 0".into());
        }
        if self.timeout_secs == 0 {
            return Err("Ping timeout must be greater than 0".into());
        }
        if self.timeout_secs >= self.interval_secs {
            return Err("Ping timeout must be less than interval".into());
        }
        Ok(())
    }
}
