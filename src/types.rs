use crate::network::{
    DirectMessageRequest, DirectMessageResponse, NodeDescriptionRequest, NodeDescriptionResponse,
};
use libp2p::floodsub::Event;
use libp2p::{kad, mdns, ping, request_response};
use serde::{Deserialize, Serialize};

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
