use libp2p::floodsub::FloodsubEvent;
use libp2p::mdns;
use serde::{Deserialize, Serialize};

pub type Stories = Vec<Story>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Story {
    pub id: usize,
    pub name: String,
    pub header: String,
    pub body: String,
    pub public: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ListMode {
    ALL,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRequest {
    pub mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    pub mode: ListMode,
    pub data: Stories,
    pub receiver: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishedStory {
    pub story: Story,
    pub publisher: String,
}

pub enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(FloodsubEvent),
    MdnsEvent(mdns::Event),
    PublishStory(Story),
}