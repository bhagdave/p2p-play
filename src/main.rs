use libp2p::floodsub::Topic;
use libp2p::identity;
use libp2p::PeerId;
use libp2p::mplex;
use libp2p::core::upgrade;
use libp2p::tcp;
use libp2p::dns;
use libp2p::Transport;
use libp2p::tcp::GenTcpConfig;
use libp2p::noise::{Keypair, NoiseConfig, X25519Spec};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use log::{info};
use tokio::sync::mpsc;

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

#[tokio::main]
async fn main(){
    pretty_env_logger::init(); // Start up the logging - Only works whwn RUST_LOG=trace is in the environment

    info!("Peer id: {}", PEER_ID.clone());
    // CReate an async channel for comms
    let (response_sender, response_rcv) : (tokio::sync::mpsc::UnboundedSender<ListResult>, tokio::sync::mpsc::UnboundedReceiver<ListResult>)  = mpsc::unbounded_channel();
    // setup keys for securing traffic
    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS) // Sign with our keys from earlier
        .expect("can create auth keys");
    
    // Lets create the transport layer
    let tcp = GenTcpConfig::new().nodelay(true);
    let transport = tcp::TokioTcpTransport::new(tcp);
    let dns = dns::TokioDnsConfig::system(transport)?;
    dns.upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();
}

