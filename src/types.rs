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

pub enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(FloodsubEvent),
    MdnsEvent(mdns::Event),
    PingEvent(ping::Event),
    PublishStory(Story),
}

impl Story {
    pub fn new(id: usize, name: String, header: String, body: String, public: bool) -> Self {
        Self { id, name, header, body, public }
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
        Self { mode: ListMode::ALL }
    }
    
    pub fn new_one(peer_id: String) -> Self {
        Self { mode: ListMode::One(peer_id) }
    }
}

impl ListResponse {
    pub fn new(mode: ListMode, receiver: String, data: Stories) -> Self {
        Self { mode, receiver, data }
    }
}

impl PublishedStory {
    pub fn new(story: Story, publisher: String) -> Self {
        Self { story, publisher }
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
        let story = Story::new(1, "Test".to_string(), "H".to_string(), "B".to_string(), true);
        let stories = vec![story];
        
        let response = ListResponse::new(
            ListMode::ALL,
            "receiver123".to_string(),
            stories.clone(),
        );
        
        assert_eq!(response.mode, ListMode::ALL);
        assert_eq!(response.receiver, "receiver123");
        assert_eq!(response.data, stories);
    }
    
    #[test]
    fn test_published_story_creation() {
        let story = Story::new(1, "Pub".to_string(), "Header".to_string(), "Body".to_string(), true);
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
        let story = Story::new(1, "Test".to_string(), "H".to_string(), "B".to_string(), true);
        let response = ListResponse::new(
            ListMode::ALL,
            "test_receiver".to_string(),
            vec![story],
        );
        
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }
    
    #[test]
    fn test_published_story_serialization() {
        let story = Story::new(1, "Test".to_string(), "H".to_string(), "B".to_string(), true);
        let published = PublishedStory::new(story, "publisher".to_string());
        
        let json = serde_json::to_string(&published).unwrap();
        let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
        assert_eq!(published, deserialized);
    }
}