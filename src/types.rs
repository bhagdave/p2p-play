use crate::network::{
    DirectMessageRequest, DirectMessageResponse, NodeDescriptionRequest, NodeDescriptionResponse,
};
use libp2p::floodsub::Event;
use libp2p::{PeerId, kad, mdns, ping, request_response};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// Import crypto types that will be used in RelayMessage
use crate::crypto::{EncryptedPayload, MessageSignature};

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
pub struct PublishedChannel {
    pub channel: Channel,
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

/// Encrypted message for relay delivery
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RelayMessage {
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

/// Relay delivery confirmation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayConfirmation {
    pub message_id: String,
    pub delivered_to: String,  // Target peer ID
    pub relay_path_length: u8, // Number of hops used
    pub delivery_timestamp: u64,
}

/// Configuration for relay behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub enable_relay: bool,       // Master relay switch
    pub enable_forwarding: bool,  // Act as relay for others
    pub max_hops: u8,             // Default: 3
    pub relay_timeout_ms: u64,    // Default: 5000ms
    pub prefer_direct: bool,      // Try direct first
    pub rate_limit_per_peer: u32, // Messages per minute
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
pub struct StoryReadStatus {
    pub story_id: usize,
    pub peer_id: String,
    pub read_at: u64,
    pub channel_name: String,
}

#[derive(Debug, Clone)]
pub struct ChannelWithUnreadCount {
    pub channel: Channel,
    pub unread_count: usize,
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

fn default_max_connections_per_peer() -> u32 {
    1
}
fn default_max_pending_incoming() -> u32 {
    10
}
fn default_max_pending_outgoing() -> u32 {
    10
}
fn default_max_established_total() -> u32 {
    100
}
fn default_connection_establishment_timeout_seconds() -> u64 {
    30
}

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

/// Configuration for channel auto-subscription behavior
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelAutoSubscriptionConfig {
    /// Auto-subscribe to new channels when discovered
    pub auto_subscribe_to_new_channels: bool,
    /// Show notifications for new channel discoveries
    pub notify_new_channels: bool,
    /// Limit auto-subscriptions to prevent spam
    pub max_auto_subscriptions: usize,
}

/// Unified network configuration that consolidates all network-related settings
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnifiedNetworkConfig {
    pub bootstrap: BootstrapConfig,
    pub network: NetworkConfig,
    pub ping: PingConfig,
    pub direct_message: DirectMessageConfig,
    pub channel_auto_subscription: ChannelAutoSubscriptionConfig,
    pub relay: RelayConfig,
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
    RefreshChannels,
    RebroadcastRelayMessage(crate::types::RelayMessage),
}

pub enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(Event),
    MdnsEvent(mdns::Event),
    PingEvent(ping::Event),
    RequestResponseEvent(request_response::Event<DirectMessageRequest, DirectMessageResponse>),
    NodeDescriptionEvent(request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>),
    StorySyncEvent(
        request_response::Event<
            crate::network::StorySyncRequest,
            crate::network::StorySyncResponse,
        >,
    ),
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

impl PublishedChannel {
    pub fn new(channel: Channel, publisher: String) -> Self {
        Self { channel, publisher }
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

impl RelayConfirmation {
    pub fn new(message_id: String, delivered_to: String, relay_path_length: u8) -> Self {
        let delivery_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            message_id,
            delivered_to,
            relay_path_length,
            delivery_timestamp,
        }
    }
}

impl RelayConfig {
    pub fn new() -> Self {
        Self {
            enable_relay: true,
            enable_forwarding: true,
            max_hops: 3,
            relay_timeout_ms: 5000,
            prefer_direct: true,
            rate_limit_per_peer: 10,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_hops == 0 {
            return Err("max_hops must be greater than 0".to_string());
        }

        if self.max_hops > 10 {
            return Err("max_hops should not exceed 10 to prevent network spam".to_string());
        }

        if self.relay_timeout_ms == 0 {
            return Err("relay_timeout_ms must be greater than 0".to_string());
        }

        if self.rate_limit_per_peer == 0 {
            return Err("rate_limit_per_peer must be greater than 0".to_string());
        }

        Ok(())
    }
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self::new()
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

impl StoryReadStatus {
    pub fn new(story_id: usize, peer_id: String, channel_name: String) -> Self {
        let read_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            story_id,
            peer_id,
            read_at,
            channel_name,
        }
    }
}

impl ChannelWithUnreadCount {
    pub fn new(channel: Channel, unread_count: usize) -> Self {
        Self {
            channel,
            unread_count,
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
                return Err(format!("Invalid multiaddr '{peer}': {e}"));
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
            max_connections_per_peer: 1, // Single connection per peer to avoid resource waste
            max_pending_incoming: 10,    // Allow reasonable number of pending connections
            max_pending_outgoing: 10,    // Balance connectivity with resource usage
            max_established_total: 100,  // Total connection pool size
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
        if self.connection_maintenance_interval_seconds < 10 {
            return Err("connection_maintenance_interval_seconds must be at least 10 seconds to avoid excessive connection churn".to_string());
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
            return Err(
                "max_connections_per_peer should not exceed 10 to avoid resource waste".to_string(),
            );
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
            return Err(
                "connection_establishment_timeout_seconds must be at least 5 seconds".to_string(),
            );
        }

        if self.connection_establishment_timeout_seconds > 300 {
            return Err(
                "connection_establishment_timeout_seconds must not exceed 300 seconds".to_string(),
            );
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

impl ChannelAutoSubscriptionConfig {
    pub fn new() -> Self {
        Self {
            auto_subscribe_to_new_channels: false, // Conservative default
            notify_new_channels: true,             // Enable notifications by default
            max_auto_subscriptions: 10,            // Reasonable limit for spam prevention
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_auto_subscriptions == 0 {
            return Err("max_auto_subscriptions must be greater than 0".to_string());
        }

        if self.max_auto_subscriptions > 100 {
            return Err("max_auto_subscriptions should not exceed 100 to prevent spam".to_string());
        }

        Ok(())
    }
}

impl Default for ChannelAutoSubscriptionConfig {
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

impl UnifiedNetworkConfig {
    /// Create a new UnifiedNetworkConfig with all default values
    pub fn new() -> Self {
        Self {
            bootstrap: BootstrapConfig::new(),
            network: NetworkConfig::new(),
            ping: PingConfig::new(),
            direct_message: DirectMessageConfig::new(),
            channel_auto_subscription: ChannelAutoSubscriptionConfig::new(),
            relay: RelayConfig::new(),
        }
    }

    /// Validate all configuration sections
    pub fn validate(&self) -> Result<(), String> {
        self.bootstrap.validate()?;
        self.network.validate()?;
        self.ping.validate()?;
        self.direct_message.validate()?;
        self.channel_auto_subscription.validate()?;
        self.relay.validate()?;
        Ok(())
    }

    /// Load unified configuration from a file, falling back to defaults if file doesn't exist
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let config: UnifiedNetworkConfig = serde_json::from_str(&content)?;
                config
                    .validate()
                    .map_err(|e| format!("Configuration validation failed: {e}"))?;
                Ok(config)
            }
            Err(_) => {
                // File doesn't exist, use defaults
                Ok(Self::new())
            }
        }
    }

    /// Save unified configuration to a file
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.validate()
            .map_err(|e| format!("Configuration validation failed: {e}"))?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for UnifiedNetworkConfig {
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

/// Cross-platform icon utility for Windows compatibility
/// On Windows, Unicode emojis often display as empty squares, so we provide ASCII alternatives
pub struct Icons;

impl Icons {
    /// Target/ready indicator
    pub fn target() -> &'static str {
        #[cfg(windows)]
        return "[!]";
        #[cfg(not(windows))]
        return "🎯";
    }

    /// Note/memo/input indicator
    pub fn memo() -> &'static str {
        #[cfg(windows)]
        return ">";
        #[cfg(not(windows))]
        return "📝";
    }

    /// Help/configuration indicator
    pub fn wrench() -> &'static str {
        #[cfg(windows)]
        return "[?]";
        #[cfg(not(windows))]
        return "🔧";
    }

    /// Clear/cleanup indicator
    pub fn broom() -> &'static str {
        #[cfg(windows)]
        return "[CLR]";
        #[cfg(not(windows))]
        return "🧹";
    }

    /// Error/cancel indicator
    pub fn cross() -> &'static str {
        #[cfg(windows)]
        return "[X]";
        #[cfg(not(windows))]
        return "❌";
    }

    /// Success indicator
    pub fn check() -> &'static str {
        #[cfg(windows)]
        return "[OK]";
        #[cfg(not(windows))]
        return "✅";
    }

    /// Folder/channel indicator
    pub fn folder() -> &'static str {
        #[cfg(windows)]
        return "[DIR]";
        #[cfg(not(windows))]
        return "📂";
    }

    /// Document/header indicator
    pub fn document() -> &'static str {
        #[cfg(windows)]
        return "[DOC]";
        #[cfg(not(windows))]
        return "📄";
    }

    /// Book/story indicator (open)
    pub fn book() -> &'static str {
        #[cfg(windows)]
        return "[BOOK]";
        #[cfg(not(windows))]
        return "📖";
    }

    /// Calendar/date indicator
    pub fn calendar() -> &'static str {
        #[cfg(windows)]
        return "[DATE]";
        #[cfg(not(windows))]
        return "📅";
    }

    /// Label/ID indicator
    pub fn label() -> &'static str {
        #[cfg(windows)]
        return "[ID]";
        #[cfg(not(windows))]
        return "🏷️ ";
    }

    /// Visibility indicator
    pub fn eye() -> &'static str {
        #[cfg(windows)]
        return "[VIS]";
        #[cfg(not(windows))]
        return "👁️ ";
    }

    /// Message indicator
    pub fn envelope() -> &'static str {
        #[cfg(windows)]
        return "[MSG]";
        #[cfg(not(windows))]
        return "📨";
    }

    /// Description/clipboard indicator
    pub fn clipboard() -> &'static str {
        #[cfg(windows)]
        return "[DESC]";
        #[cfg(not(windows))]
        return "📋";
    }

    /// Statistics indicator
    pub fn chart() -> &'static str {
        #[cfg(windows)]
        return "[STAT]";
        #[cfg(not(windows))]
        return "📊";
    }

    /// Direct message/speech indicator
    pub fn speech() -> &'static str {
        #[cfg(windows)]
        return "[DM]";
        #[cfg(not(windows))]
        return "💬";
    }

    /// Private/closed book indicator
    pub fn closed_book() -> &'static str {
        #[cfg(windows)]
        return "[PRIV]";
        #[cfg(not(windows))]
        return "📕";
    }

    /// Pin/pushpin indicator
    pub fn pin() -> &'static str {
        #[cfg(windows)]
        return "[PIN]";
        #[cfg(not(windows))]
        return "📌";
    }

    /// Ping/table tennis indicator
    pub fn ping() -> &'static str {
        #[cfg(windows)]
        return "[PING]";
        #[cfg(not(windows))]
        return "🏓";
    }

    /// Warning indicator
    pub fn warning() -> &'static str {
        #[cfg(windows)]
        return "[WARN]";
        #[cfg(not(windows))]
        return "⚠️ ";
    }

    /// Relay/antenna indicator for forwarded messages
    pub fn antenna() -> &'static str {
        #[cfg(windows)]
        return "[RELAY]";
        #[cfg(not(windows))]
        return "📡";
    }

    /// Synchronization/refresh indicator
    pub fn sync() -> &'static str {
        #[cfg(windows)]
        return "[SYNC]";
        #[cfg(not(windows))]
        return "🔄";
    }

    /// Checkmark/success indicator (alternative to check)
    pub fn checkmark() -> &'static str {
        #[cfg(windows)]
        return "[✓]";
        #[cfg(not(windows))]
        return "✓";
    }
}

#[cfg(test)]
mod tests {
    use super::Icons;

    #[test]
    fn test_icons_return_non_empty_strings() {
        // Test that all icon functions return non-empty strings
        assert!(!Icons::target().is_empty());
        assert!(!Icons::memo().is_empty());
        assert!(!Icons::wrench().is_empty());
        assert!(!Icons::broom().is_empty());
        assert!(!Icons::cross().is_empty());
        assert!(!Icons::check().is_empty());
        assert!(!Icons::folder().is_empty());
        assert!(!Icons::document().is_empty());
        assert!(!Icons::book().is_empty());
        assert!(!Icons::calendar().is_empty());
        assert!(!Icons::label().is_empty());
        assert!(!Icons::eye().is_empty());
        assert!(!Icons::envelope().is_empty());
        assert!(!Icons::clipboard().is_empty());
        assert!(!Icons::chart().is_empty());
        assert!(!Icons::speech().is_empty());
        assert!(!Icons::closed_book().is_empty());
        assert!(!Icons::pin().is_empty());
        assert!(!Icons::ping().is_empty());
        assert!(!Icons::warning().is_empty());
        assert!(!Icons::antenna().is_empty());
    }

    #[test]
    fn test_windows_icons_are_ascii() {
        // On Windows, icons should be ASCII (no Unicode emojis)
        #[cfg(windows)]
        {
            // These should not contain emoji characters
            assert!(Icons::target().chars().all(|c| c.is_ascii()));
            assert!(Icons::memo().chars().all(|c| c.is_ascii()));
            assert!(Icons::folder().chars().all(|c| c.is_ascii()));
            assert!(Icons::book().chars().all(|c| c.is_ascii()));
            assert!(Icons::check().chars().all(|c| c.is_ascii())); // This should now be [OK] instead of [✓]
        }
    }

    #[test]
    fn test_check_icon_no_unicode() {
        // Verify the check icon doesn't contain Unicode characters on Windows
        let check_icon = Icons::check();
        #[cfg(windows)]
        {
            assert_eq!(check_icon, "[OK]");
            // Ensure no Unicode check mark character (U+2713)
            assert!(!check_icon.contains('\u{2713}'));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(check_icon, "✅");
        }
    }

    #[test]
    fn test_non_windows_icons_contain_unicode() {
        // On non-Windows platforms, icons should contain Unicode emojis
        #[cfg(not(windows))]
        {
            // These should contain Unicode emoji characters
            assert!(Icons::target().contains("🎯"));
            assert!(Icons::memo().contains("📝"));
            assert!(Icons::folder().contains("📂"));
            assert!(Icons::book().contains("📖"));
        }
    }
}
