use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum DaemonRequest {
    Peers,
    Channels,
    Conversations { limit: usize },
    Unread { limit: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Peers {
        peers: Vec<PeerInfo>,
    },
    Channels {
        channels: Vec<ChannelInfo>,
    },
    Conversations {
        conversations: Vec<ConversationSummary>,
    },
    Unread {
        messages: Vec<MessagesSummary>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub peer_id: String,
    pub peer_name: String,
    pub unread_count: usize,
    pub last_activity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesSummary {
    pub peer_id: String,
    pub peer_name: String,
    pub messages: Vec<MessageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub content: String,
    pub timestamp: u64,
}

pub type DaemonCommand = (DaemonRequest, oneshot::Sender<DaemonResponse>);
