use crate::network::{
    DirectMessageRequest, DirectMessageResponse, NodeDescriptionRequest, NodeDescriptionResponse,
};
use libp2p::floodsub::Event;
use libp2p::{PeerId, kad, mdns, ping, request_response};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub type Stories = Vec<Story>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Story {
    pub id: usize,
    pub name: String,
    pub header: String,
    pub body: String,
    pub public: bool,
    pub channel: String,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum ListMode {
    ALL,
    One(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ListRequest {
    pub mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ListResponse {
    pub mode: ListMode,
    pub data: Stories,
    pub receiver: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PublishedStory {
    pub story: Story,
    pub publisher: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PeerName {
    pub peer_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DirectMessage {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Channel {
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ChannelSubscription {
    pub peer_id: String,
    pub channel_name: String,
    pub subscribed_at: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BootstrapConfig {
    pub bootstrap_peers: Vec<String>,
    pub retry_interval_ms: u64,
    pub max_retry_attempts: u32,
    pub bootstrap_timeout_ms: u64,
}

/// Configuration for network connectivity settings
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConfig {
    pub connection_maintenance_interval_seconds: u64,
    pub request_timeout_seconds: u64,
    pub max_concurrent_streams: usize,
    /// Maximum number of established connections per peer (default: 1)
    #[serde(default = "default_max_connections_per_peer")]
    pub max_connections_per_peer: u32,
    /// Maximum number of pending incoming connections (default: 10)
    #[serde(default = "default_max_pending_incoming")]
    pub max_pending_incoming: u32,
    /// Maximum number of pending outgoing connections (default: 10)  
    #[serde(default = "default_max_pending_outgoing")]
    pub max_pending_outgoing: u32,
    /// Maximum total number of established connections (default: 100)
    #[serde(default = "default_max_established_total")]
    pub max_established_total: u32,
    /// Connection establishment timeout in seconds (default: 30)
    #[serde(default = "default_connection_establishment_timeout_seconds")]
    pub connection_establishment_timeout_seconds: u64,
}

fn default_max_connections_per_peer() -> u32 { 1 }
fn default_max_pending_incoming() -> u32 { 10 }
fn default_max_pending_outgoing() -> u32 { 10 }
fn default_max_established_total() -> u32 { 100 }
fn default_connection_establishment_timeout_seconds() -> u64 { 30 }

/// Configuration for ping keep-alive settings
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PingConfig {
    /// Interval between ping messages in seconds (default: 30)
    pub interval_secs: u64,
    /// Timeout for ping responses in seconds (default: 20)  
    pub timeout_secs: u64,
}

/// Configuration for direct message retry logic
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageConfig {
    pub max_retry_attempts: u8,
    pub retry_interval_seconds: u64,
    pub enable_connection_retries: bool,
    pub enable_timed_retries: bool,
}

/// Pending direct message for retry logic
#[derive(Debug, Clone)]
pub struct PendingDirectMessage {
    pub target_peer_id: PeerId,
    pub target_name: String,
    pub message: DirectMessageRequest,
    pub attempts: u8,
    pub last_attempt: Option<Instant>,
    pub max_attempts: u8,
    pub is_placeholder_peer_id: bool,
}

pub type Channels = Vec<Channel>;
pub type ChannelSubscriptions = Vec<ChannelSubscription>;

#[derive(Debug, PartialEq)]
pub enum ActionResult {
    RefreshStories,
    StartStoryCreation,
}

pub enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(Event),
    MdnsEvent(mdns::Event),
    PingEvent(ping::Event),
    RequestResponseEvent(request_response::Event<DirectMessageRequest, DirectMessageResponse>),
    NodeDescriptionEvent(request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>),
    KadEvent(kad::Event),
    PublishStory(Story),
    PeerName(PeerName),
    DirectMessage(DirectMessage),
    Channel(Channel),
    ChannelSubscription(ChannelSubscription),
}

impl Story {
    pub fn new(id: usize, name: String, header: String, body: String, public: bool) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            name,
            header,
            body,
            public,
            channel: "general".to_string(),
            created_at,
        }
    }

    pub fn new_with_channel(
        id: usize,
        name: String,
        header: String,
        body: String,
        public: bool,
        channel: String,
    ) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            name,
            header,
            body,
            public,
            channel,
            created_at,
        }
    }

    pub fn is_public(&self) -> bool {
        self.public
    }

    pub fn set_public(&mut self, public: bool) {
        self.public = public;
    }
}

impl ListRequest {
    pub fn new_all() -> Self {
        Self {
            mode: ListMode::ALL,
        }
    }

    pub fn new_one(peer_id: String) -> Self {
        Self {
            mode: ListMode::One(peer_id),
        }
    }
}

impl ListResponse {
    pub fn new(mode: ListMode, receiver: String, data: Stories) -> Self {
        Self {
            mode,
            receiver,
            data,
        }
    }
}

impl PublishedStory {
    pub fn new(story: Story, publisher: String) -> Self {
        Self { story, publisher }
    }
}

impl PeerName {
    pub fn new(peer_id: String, name: String) -> Self {
        Self { peer_id, name }
    }
}

impl DirectMessage {
    pub fn new(from_peer_id: String, from_name: String, to_name: String, message: String) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            from_peer_id,
            from_name,
            to_name,
            message,
            timestamp,
        }
    }
}

impl Channel {
    pub fn new(name: String, description: String, created_by: String) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            name,
            description,
            created_by,
            created_at,
        }
    }
}

impl ChannelSubscription {
    pub fn new(peer_id: String, channel_name: String) -> Self {
        let subscribed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            peer_id,
            channel_name,
            subscribed_at,
        }
    }
}

impl BootstrapConfig {
    pub fn new() -> Self {
        Self {
            bootstrap_peers: vec![
                "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
                    .to_string(),
                "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa"
                    .to_string(),
            ],
            retry_interval_ms: 5000,
            max_retry_attempts: 5,
            bootstrap_timeout_ms: 30000,
        }
    }

    pub fn add_peer(&mut self, peer: String) -> bool {
        if !self.bootstrap_peers.contains(&peer) {
            self.bootstrap_peers.push(peer);
            true
        } else {
            false
        }
    }

    pub fn remove_peer(&mut self, peer: &str) -> bool {
        let initial_len = self.bootstrap_peers.len();
        self.bootstrap_peers.retain(|p| p != peer);
        initial_len != self.bootstrap_peers.len()
    }

    pub fn clear_peers(&mut self) {
        self.bootstrap_peers.clear();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.bootstrap_peers.is_empty() {
            return Err("No bootstrap peers configured".to_string());
        }

        for peer in &self.bootstrap_peers {
            if let Err(e) = peer.parse::<libp2p::Multiaddr>() {
                return Err(format!("Invalid multiaddr '{}': {}", peer, e));
            }
        }

        Ok(())
    }
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkConfig {
    pub fn new() -> Self {
        Self {
            connection_maintenance_interval_seconds: 300, // 5 minutes default
            request_timeout_seconds: 60,
            max_concurrent_streams: 100,
            max_connections_per_peer: 1,     // Single connection per peer to avoid resource waste
            max_pending_incoming: 10,        // Allow reasonable number of pending connections
            max_pending_outgoing: 10,        // Balance connectivity with resource usage
            max_established_total: 100,      // Total connection pool size
            connection_establishment_timeout_seconds: 30, // 30 second connection timeout
        }
    }

    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let config: NetworkConfig = serde_json::from_str(&content)?;
                config.validate()?;
                Ok(config)
            }
            Err(_) => {
                // File doesn't exist, create with defaults
                let config = Self::new();
                config.save_to_file(path)?;
                Ok(config)
            }
        }
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.connection_maintenance_interval_seconds < 60 {
            return Err("connection_maintenance_interval_seconds must be at least 60 seconds to avoid excessive connection churn".to_string());
        }

        if self.connection_maintenance_interval_seconds > 3600 {
            return Err("connection_maintenance_interval_seconds must be at most 3600 seconds (1 hour) to maintain network responsiveness".to_string());
        }

        if self.request_timeout_seconds < 10 {
            return Err("request_timeout_seconds must be at least 10 seconds".to_string());
        }

        if self.request_timeout_seconds > 300 {
            return Err(
                "request_timeout_seconds must not exceed 300 seconds (5 minutes)".to_string(),
            );
        }

        if self.max_concurrent_streams == 0 {
            return Err("max_concurrent_streams must be greater than 0".to_string());
        }

        if self.max_concurrent_streams > 1000 {
            return Err("max_concurrent_streams must not exceed 1000".to_string());
        }

        if self.max_connections_per_peer == 0 {
            return Err("max_connections_per_peer must be greater than 0".to_string());
        }

        if self.max_connections_per_peer > 10 {
            return Err("max_connections_per_peer should not exceed 10 to avoid resource waste".to_string());
        }

        if self.max_pending_incoming == 0 {
            return Err("max_pending_incoming must be greater than 0".to_string());
        }

        if self.max_pending_outgoing == 0 {
            return Err("max_pending_outgoing must be greater than 0".to_string());
        }

        if self.max_established_total == 0 {
            return Err("max_established_total must be greater than 0".to_string());
        }

        if self.connection_establishment_timeout_seconds < 5 {
            return Err("connection_establishment_timeout_seconds must be at least 5 seconds".to_string());
        }

        if self.connection_establishment_timeout_seconds > 300 {
            return Err("connection_establishment_timeout_seconds must not exceed 300 seconds".to_string());
        }

        Ok(())
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectMessageConfig {
    pub fn new() -> Self {
        Self {
            max_retry_attempts: 3,
            retry_interval_seconds: 30,
            enable_connection_retries: true,
            enable_timed_retries: true,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_retry_attempts == 0 {
            return Err("max_retry_attempts must be greater than 0".to_string());
        }

        if self.retry_interval_seconds == 0 {
            return Err("retry_interval_seconds must be greater than 0".to_string());
        }

        Ok(())
    }
}

impl Default for DirectMessageConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PingConfig {
    /// Create a new PingConfig with lenient default values
    pub fn new() -> Self {
        Self {
            interval_secs: 30, // Ping every 30s instead of default 15s
            timeout_secs: 20,  // 20s timeout instead of default 10s
        }
    }

    /// Load ping configuration from a file, falling back to defaults if file doesn't exist
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let config: PingConfig = serde_json::from_str(&content)?;
                config.validate()?;
                Ok(config)
            }
            Err(_) => {
                // File doesn't exist, use defaults
                Ok(Self::new())
            }
        }
    }

    /// Validate the configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.interval_secs == 0 {
            return Err("interval_secs must be greater than 0".to_string());
        }
        if self.timeout_secs == 0 {
            return Err("timeout_secs must be greater than 0".to_string());
        }
        if self.timeout_secs >= self.interval_secs {
            return Err("timeout_secs should be less than interval_secs".to_string());
        }
        Ok(())
    }

    /// Convert to libp2p Duration for interval
    pub fn interval_duration(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    /// Convert to libp2p Duration for timeout
    pub fn timeout_duration(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

impl Default for PingConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingDirectMessage {
    pub fn new(
        target_peer_id: PeerId,
        target_name: String,
        message: DirectMessageRequest,
        max_attempts: u8,
        is_placeholder_peer_id: bool,
    ) -> Self {
        Self {
            target_peer_id,
            target_name,
            message,
            attempts: 0,
            last_attempt: None,
            max_attempts,
            is_placeholder_peer_id,
        }
    }

    pub fn should_retry(&self, retry_interval_seconds: u64) -> bool {
        if self.attempts >= self.max_attempts {
            return false;
        }

        match self.last_attempt {
            None => true, // First attempt
            Some(last) => {
                let elapsed = last.elapsed().as_secs();
                elapsed >= retry_interval_seconds
            }
        }
    }

    pub fn increment_attempt(&mut self) {
        self.attempts += 1;
        self.last_attempt = Some(Instant::now());
    }

    pub fn is_exhausted(&self) -> bool {
        self.attempts >= self.max_attempts
    }
}
