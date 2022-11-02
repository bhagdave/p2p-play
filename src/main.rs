use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use libp2p::PeerId;
use libp2p::floodsub::Topic;
use libp2p::identity;

const STORAGE_FILE_PATH: &str = "./data.json";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519()); // Generate a key pair using once_cell:Lazy
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public())); // Genereate a peer id from the public key
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories")); // generate a topic to listen to on the network

type Stories = Vec<Story>;

#[derive(Debug, Serialize, Deserialize)]
struct Story {
    id: usize,
    title: String,
    body: String,
    public: bool,
}


#[derive(Debug, Serialize, Deserialize)]
enum ListMode {
    ALL,
    One(String),
}


#[derive(Debug, Serialize, Deserialize)]
struct ListRequest {
    mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListResult {
    mode: ListMode,
    data: Stories,
    receiver: String,
}

enum EventType {
    Response(ListResult),
    Input(String),
}

fn main() {
    println!("Hello, world!");
}
