use libp2p::floodsub::FloodsubEvent;
use libp2p::{mdns, ping};
use serde::{Deserialize, Serialize};

pub type Stories = Vec<Story>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Story {
    pub id: usize,
    pub name: String,
    pub header: String,
    pub body: String,
    pub public: bool,
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

pub enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(FloodsubEvent),
    MdnsEvent(mdns::Event),
    PingEvent(ping::Event),
    PublishStory(Story),
    PeerName(PeerName),
    DirectMessage(DirectMessage),
}

impl Story {
    pub fn new(id: usize, name: String, header: String, body: String, public: bool) -> Self {
        Self {
            id,
            name,
            header,
            body,
            public,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_story_creation() {
        let story = Story::new(
            1,
            "Test Story".to_string(),
            "Test Header".to_string(),
            "Test Body".to_string(),
            false,
        );

        assert_eq!(story.id, 1);
        assert_eq!(story.name, "Test Story");
        assert_eq!(story.header, "Test Header");
        assert_eq!(story.body, "Test Body");
        assert!(!story.is_public());
    }

    #[test]
    fn test_story_publicity() {
        let mut story = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            false,
        );

        assert!(!story.is_public());
        story.set_public(true);
        assert!(story.is_public());
    }

    #[test]
    fn test_story_serialization() {
        let story = Story::new(
            42,
            "Serialization Test".to_string(),
            "JSON Header".to_string(),
            "JSON Body".to_string(),
            true,
        );

        let json = serde_json::to_string(&story).unwrap();
        let deserialized: Story = serde_json::from_str(&json).unwrap();

        assert_eq!(story, deserialized);
    }

    #[test]
    fn test_list_request_creation() {
        let req_all = ListRequest::new_all();
        assert_eq!(req_all.mode, ListMode::ALL);

        let peer_id = "12D3KooWTest".to_string();
        let req_one = ListRequest::new_one(peer_id.clone());
        assert_eq!(req_one.mode, ListMode::One(peer_id));
    }

    #[test]
    fn test_list_response_creation() {
        let story = Story::new(
            1,
            "Test".to_string(),
            "H".to_string(),
            "B".to_string(),
            true,
        );
        let stories = vec![story];

        let response = ListResponse::new(ListMode::ALL, "receiver123".to_string(), stories.clone());

        assert_eq!(response.mode, ListMode::ALL);
        assert_eq!(response.receiver, "receiver123");
        assert_eq!(response.data, stories);
    }

    #[test]
    fn test_published_story_creation() {
        let story = Story::new(
            1,
            "Pub".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
        let publisher = "publisher123".to_string();

        let published = PublishedStory::new(story.clone(), publisher.clone());

        assert_eq!(published.story, story);
        assert_eq!(published.publisher, publisher);
    }

    #[test]
    fn test_list_request_serialization() {
        let req = ListRequest::new_all();
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ListRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_list_response_serialization() {
        let story = Story::new(
            1,
            "Test".to_string(),
            "H".to_string(),
            "B".to_string(),
            true,
        );
        let response = ListResponse::new(ListMode::ALL, "test_receiver".to_string(), vec![story]);

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_published_story_serialization() {
        let story = Story::new(
            1,
            "Test".to_string(),
            "H".to_string(),
            "B".to_string(),
            true,
        );
        let published = PublishedStory::new(story, "publisher".to_string());

        let json = serde_json::to_string(&published).unwrap();
        let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
        assert_eq!(published, deserialized);
    }

    #[test]
    fn test_peer_name_creation() {
        let peer_name = PeerName::new("12D3KooWTest".to_string(), "Alice".to_string());
        assert_eq!(peer_name.peer_id, "12D3KooWTest");
        assert_eq!(peer_name.name, "Alice");
    }

    #[test]
    fn test_peer_name_serialization() {
        let peer_name = PeerName::new("12D3KooWTest".to_string(), "Bob".to_string());

        let json = serde_json::to_string(&peer_name).unwrap();
        let deserialized: PeerName = serde_json::from_str(&json).unwrap();
        assert_eq!(peer_name, deserialized);
    }

    #[test]
    fn test_story_equality() {
        let story1 = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
        let story2 = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
        let story3 = Story::new(
            2,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );

        assert_eq!(story1, story2);
        assert_ne!(story1, story3);
    }

    #[test]
    fn test_peer_name_equality() {
        let peer1 = PeerName::new("peer1".to_string(), "Alice".to_string());
        let peer2 = PeerName::new("peer1".to_string(), "Alice".to_string());
        let peer3 = PeerName::new("peer2".to_string(), "Alice".to_string());

        assert_eq!(peer1, peer2);
        assert_ne!(peer1, peer3);
    }

    #[test]
    fn test_list_mode_equality() {
        let mode1 = ListMode::ALL;
        let mode2 = ListMode::ALL;
        let mode3 = ListMode::One("peer123".to_string());
        let mode4 = ListMode::One("peer123".to_string());
        let mode5 = ListMode::One("peer456".to_string());

        assert_eq!(mode1, mode2);
        assert_eq!(mode3, mode4);
        assert_ne!(mode1, mode3);
        assert_ne!(mode3, mode5);
    }

    #[test]
    fn test_published_story_fields() {
        let story = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
        let published = PublishedStory::new(story.clone(), "publisher123".to_string());

        assert_eq!(published.story, story);
        assert_eq!(published.publisher, "publisher123");
    }

    #[test]
    fn test_list_response_fields() {
        let stories = vec![
            Story::new(
                1,
                "Story1".to_string(),
                "H1".to_string(),
                "B1".to_string(),
                true,
            ),
            Story::new(
                2,
                "Story2".to_string(),
                "H2".to_string(),
                "B2".to_string(),
                true,
            ),
        ];
        let response = ListResponse::new(ListMode::ALL, "receiver".to_string(), stories.clone());

        assert_eq!(response.mode, ListMode::ALL);
        assert_eq!(response.receiver, "receiver");
        assert_eq!(response.data, stories);
    }

    #[test]
    fn test_empty_story_collections() {
        let empty_stories: Stories = vec![];
        let response =
            ListResponse::new(ListMode::ALL, "receiver".to_string(), empty_stories.clone());

        assert_eq!(response.data.len(), 0);
        assert!(response.data.is_empty());
    }

    #[test]
    fn test_story_with_empty_strings() {
        let story = Story::new(0, "".to_string(), "".to_string(), "".to_string(), false);

        assert_eq!(story.id, 0);
        assert_eq!(story.name, "");
        assert_eq!(story.header, "");
        assert_eq!(story.body, "");
        assert!(!story.public);
    }

    #[test]
    fn test_story_with_large_id() {
        let large_id = usize::MAX;
        let story = Story::new(
            large_id,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );

        assert_eq!(story.id, large_id);
    }

    #[test]
    fn test_story_clone() {
        let story1 = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
        let story2 = story1.clone();

        assert_eq!(story1, story2);
        // Ensure they are separate instances
        assert_eq!(story1.id, story2.id);
        assert_eq!(story1.name, story2.name);
    }

    #[test]
    fn test_direct_message_creation() {
        let dm = DirectMessage::new(
            "peer123".to_string(),
            "Alice".to_string(),
            "Bob".to_string(),
            "Hello Bob!".to_string(),
        );

        assert_eq!(dm.from_peer_id, "peer123");
        assert_eq!(dm.from_name, "Alice");
        assert_eq!(dm.to_name, "Bob");
        assert_eq!(dm.message, "Hello Bob!");
        assert!(dm.timestamp > 0);
    }

    #[test]
    fn test_direct_message_serialization() {
        let dm = DirectMessage::new(
            "peer456".to_string(),
            "Charlie".to_string(),
            "David".to_string(),
            "Test message with special chars: 🌍!".to_string(),
        );

        let json = serde_json::to_string(&dm).unwrap();
        let deserialized: DirectMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(dm, deserialized);
    }

    #[test]
    fn test_direct_message_equality() {
        let dm1 = DirectMessage {
            from_peer_id: "peer1".to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello".to_string(),
            timestamp: 1234567890,
        };
        let dm2 = DirectMessage {
            from_peer_id: "peer1".to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello".to_string(),
            timestamp: 1234567890,
        };
        let dm3 = DirectMessage {
            from_peer_id: "peer1".to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Different message".to_string(),
            timestamp: 1234567890,
        };

        assert_eq!(dm1, dm2);
        assert_ne!(dm1, dm3);
    }
}
