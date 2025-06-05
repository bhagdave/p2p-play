use libp2p::swarm::SwarmEvent;
use libp2p::tcp::Config;
use libp2p::tcp;
use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::StreamExt,
    identity,
    mdns,
    yamux,
    noise,
    ping,
    swarm::{Swarm, NetworkBehaviour, Config as SwarmConfig},
    PeerId, Transport,
};
use log::{error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::{fs, io::AsyncBufReadExt, sync::mpsc};
use std::error::Error;
use std::process;
use bytes::Bytes;

const STORAGE_FILE_PATH: &str = "./stories.json";

//type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
type Stories = Vec<Story>;

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| {
    match std::fs::read("peer_key") {
        Ok(bytes) => {
            println!("Found existing peer key file, attempting to load");
            match identity::Keypair::from_protobuf_encoding(&bytes) {
                Ok(keypair) => {
                    let peer_id = PeerId::from(keypair.public());
                    println!("Successfully loaded keypair with PeerId: {}", peer_id);
                    keypair
                },
                Err(e) => {
                    println!("Error loading keypair: {}, generating new one", e);
                    generate_and_save_keypair()
                },
            }
        },
        Err(e) => {
            println!("No existing key file found ({}), generating new one", e);
            generate_and_save_keypair()
        },
    }
});

fn generate_and_save_keypair() -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    println!("Generated new keypair with PeerId: {}", peer_id);
    
    match keypair.to_protobuf_encoding() {
        Ok(bytes) => {
            match std::fs::write("peer_key", bytes) {
                Ok(_) => println!("Successfully saved keypair to file"),
                Err(e) => println!("Failed to save keypair: {}", e),
            }
        },
        Err(e) => println!("Failed to encode keypair: {}", e),
    }
    keypair
}

static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Story {
    id: usize,
    name: String,
    header: String,
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
struct ListResponse {
    mode: ListMode,
    data: Stories,
    receiver: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishedStory {
    story: Story,
    publisher: String,
}

enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(FloodsubEvent),
    MdnsEvent(mdns::Event),
    PublishStory(Story),
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "StoryBehaviourEvent")]
struct StoryBehaviour {
    floodsub: Floodsub,
    mdns: mdns::tokio::Behaviour,
}
#[derive(Debug)]
enum StoryBehaviourEvent {
    Floodsub(FloodsubEvent),
    Mdns(mdns::Event),
}

impl From<FloodsubEvent> for StoryBehaviourEvent {
    fn from(event: FloodsubEvent) -> Self {
        StoryBehaviourEvent::Floodsub(event)
    }
}

impl From<mdns::Event> for StoryBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        StoryBehaviourEvent::Mdns(event)
    }
}

fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_stories().await {
            Ok(stories) => {
                let resp = ListResponse {
                    mode: ListMode::ALL,
                    receiver,
                    data: stories.into_iter().filter(|r| r.public).collect(),
                };
                if let Err(e) = sender.send(resp) {
                    error!("error sending response via channel, {}", e);
                }
            }
            Err(e) => error!("error fetching local stories to answer ALL request, {}", e),
        }
    });
}

async fn create_new_story(name: &str, header: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
    let new_id = match local_stories.iter().max_by_key(|r| r.id) {
        Some(v) => v.id + 1,
        None => 0,
    };
    local_stories.push(Story {
        id: new_id,
        name: name.to_owned(),
        header: header.to_owned(),
        body: body.to_owned(),
        public: false,
    });
    write_local_stories(&local_stories).await?;

    info!("Created story:");
    info!("Name: {}", name);
    info!("Header: {}", header);
    info!("Body:: {}", body);

    Ok(())
}

async fn publish_story(id: usize, sender: mpsc::UnboundedSender<Story>) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
    let mut published_story = None;
    
    for story in local_stories.iter_mut() {
        if story.id == id {
            story.public = true;
            published_story = Some(story.clone());
            break;
        }
    }
    
    write_local_stories(&local_stories).await?;
    
    if let Some(story) = published_story {
        if let Err(e) = sender.send(story) {
            error!("error sending story for broadcast: {}", e);
        }
    }
    
    Ok(())
}

async fn read_local_stories() -> Result<Stories, Box<dyn Error>> {
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

async fn write_local_stories(stories: &Stories) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(&stories)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}

async fn save_received_story(mut story: Story) -> Result<(), Box<dyn Error>> {
    let mut local_stories = match read_local_stories().await {
        Ok(stories) => stories,
        Err(_) => Vec::new(), // Create empty vec if no file exists
    };
    
    // Check if story already exists (by name and content to avoid duplicates)
    let already_exists = local_stories.iter().any(|s| 
        s.name == story.name && s.header == story.header && s.body == story.body
    );
    
    if !already_exists {
        // Assign new local ID
        let new_id = match local_stories.iter().max_by_key(|r| r.id) {
            Some(v) => v.id + 1,
            None => 0,
        };
        story.id = new_id;
        story.public = true; // Mark as public since it was published
        
        local_stories.push(story);
        write_local_stories(&local_stories).await?;
        info!("Saved received story to local storage with ID: {}", new_id);
    } else {
        info!("Story already exists locally, skipping save");
    }
    
    Ok(())
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();
    let (story_sender, mut story_rcv) = mpsc::unbounded_channel();
	
    let transp = tcp::tokio::Transport::new(Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&KEYS).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();
    let _ping = crate::ping::Behaviour::new(libp2p::ping::Config::new());
    let mut behaviour = StoryBehaviour {
    floodsub: Floodsub::new(*PEER_ID),
    mdns: mdns::tokio::Behaviour::new(Default::default(), PEER_ID.clone())
        .expect("can create mdns"),
    };

    info!("Created floodsub with peer id: {:?}", PEER_ID.clone());
    info!("Subscribing to topic: {:?}", TOPIC.clone());
    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = Swarm::<StoryBehaviour>::new(transp, behaviour, *PEER_ID, SwarmConfig::with_tokio_executor());

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can get a local socket"),
    )
    .expect("swarm can be started");

    loop {
        let evt = {
            tokio::select! {
                line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
                story = story_rcv.recv() => Some(EventType::PublishStory(story.expect("story exists"))),
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => Some(EventType::FloodsubEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => Some(EventType::MdnsEvent(event)),
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Local node is listening on {}", address);
                            None
                        },
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            info!("Connection established to {} via {:?}", peer_id, endpoint);
                            swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer_id);
                            None
                        },
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed to {}: {:?}", peer_id, cause);
                            None
                        },
                        SwarmEvent::OutgoingConnectionError { peer_id, error, connection_id, .. } => {
                            error!("Failed to connect to {:?} (connection id: {:?}): {}", peer_id, connection_id, error);
                            None
                        },
                        SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, connection_id, .. } => {
                            error!("Failed incoming connection from {} to {} (connection id: {:?}): {}", 
                                   send_back_addr, local_addr, connection_id, error);
                            None
                        },
                        SwarmEvent::Dialing { peer_id, connection_id, .. } => {
                            info!("Dialing peer: {:?} (connection id: {:?})", peer_id, connection_id);
                            None
                        },
                        _ => {
                            info!("Unhandled Swarm Event: {:?}", event);
                            None
                        }
                    }
                },
            }
        };

        if let Some(event) = evt {
            info!("Event Received");
            match event {
                EventType::Response(resp) => {
                    info!("Response received");
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
                    let json_bytes = Bytes::from(json.into_bytes());
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(TOPIC.clone(), json_bytes);
                }
                EventType::PublishStory(story) => {
                    info!("Broadcasting published story: {}", story.name);
                    let published_story = PublishedStory {
                        story,
                        publisher: PEER_ID.to_string(),
                    };
                    let json = serde_json::to_string(&published_story).expect("can jsonify published story");
                    let json_bytes = Bytes::from(json.into_bytes());
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(TOPIC.clone(), json_bytes);
                }
                EventType::Input(line) => match line.as_str() {
                    "ls p" => handle_list_peers(&mut swarm).await,
                    cmd if cmd.starts_with("ls s") => handle_list_stories(cmd, &mut swarm).await,
                    cmd if cmd.starts_with("create s") => handle_create_stories(cmd).await,
                    cmd if cmd.starts_with("publish s") => handle_publish_story(cmd, story_sender.clone()).await,
                    cmd if cmd.starts_with("help") => handle_help(cmd).await,
                    cmd if cmd.starts_with("quit") => process::exit(0),
                    cmd if cmd.starts_with("connect ") => {
                        if let Some(addr) = cmd.strip_prefix("connect ") {
                            establish_direct_connection(&mut swarm, addr).await;
                        }
                    },
                    _ => error!("unknown command"),
                },
                EventType::MdnsEvent(mdns_event) => match mdns_event {
                    mdns::Event::Discovered(discovered_list) => {
                        info!("Discovered Peers event");
                        for (peer, _addr) in discovered_list {
                            info!("Disocvered a peer:{} at {}", peer, _addr);
                            info!("Adding peer to partial view: {}", peer);
                            swarm
                                .behaviour_mut()
                                .floodsub
                                .add_node_to_partial_view(peer);
                        }
                    }
                    mdns::Event::Expired(expired_list) => {
                        info!("Expired Peers event");
                        for (peer, _addr) in expired_list {
                            info!("Expired a peer:{} at {}", peer, _addr);
                            let discovered_nodes : Vec<_> = swarm.behaviour().mdns.discovered_nodes().collect();
                            if !discovered_nodes.iter().any(|&n| n == &peer) {
                                info!("Removing peer from partial view: {}", peer);
                                swarm
                                    .behaviour_mut()
                                    .floodsub
                                    .remove_node_from_partial_view(&peer);
                            }
                        }
                    }
                },
                EventType::FloodsubEvent(floodsub_event) => match floodsub_event {
                    FloodsubEvent::Message(msg) => {
                        info!("Message event received from {:?}", msg.source);
                        info!("Message data: {:?}", msg.data);
                        if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                            if resp.receiver == PEER_ID.to_string() {
                                info!("Response from {}:", msg.source);
                                resp.data.iter().for_each(|r| info!("{:?}", r));
                            }
                        } else if let Ok(published) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                            if published.publisher != PEER_ID.to_string() {
                                info!("Received published story '{}' from {}", published.story.name, msg.source);
                                info!("Story: {:?}", published.story);
                                
                                // Save received story to local storage
                                if let Err(e) = save_received_story(published.story).await {
                                    error!("Failed to save received story: {}", e);
                                }
                            }
                        } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                            match req.mode {
                                ListMode::ALL => {
                                    info!("Received ALL req: {:?} from {:?}", req, msg.source);
                                    respond_with_public_stories(
                                        response_sender.clone(),
                                        msg.source.to_string(),
                                    );
                                }
                                ListMode::One(ref peer_id) => {
                                    if peer_id == &PEER_ID.to_string() {
                                        info!("Received req: {:?} from {:?}", req, msg.source);
                                        respond_with_public_stories(
                                            response_sender.clone(),
                                            msg.source.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        info!("Subscription events");
                    }
                },
            }
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<StoryBehaviour>) {
    info!("Discovered Peers:");
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| info!("{}", p));
}

async fn handle_list_stories(cmd: &str, swarm: &mut Swarm<StoryBehaviour>) {
    let rest = cmd.strip_prefix("ls s ");
    match rest {
        Some("all") => {
            info!("Requesting all stories from all peers");
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            info!("JSON od request: {}", json);
            let json_bytes = Bytes::from(json.into_bytes());		
            info!("Publiishing to topic: {:?} from peer:{:?}", TOPIC.clone(), PEER_ID.clone());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
            info!("Published request");
        }
        Some(story_peer_id) => {
            info!("Requesting all stories from peer: {}", story_peer_id);
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            info!("JSON od request: {}", json);
            let json_bytes = Bytes::from(json.into_bytes());		
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        None => {
            info!("Local stories:");
            match read_local_stories().await {
                Ok(v) => {
                    info!("Local stories ({})", v.len());
                    v.iter().for_each(|r| info!("{:?}", r));
                }
                Err(e) => error!("error fetching local stories: {}", e),
            };
        }
    };
}

async fn handle_create_stories(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("create s") {
        let elements: Vec<&str> = rest.split('|').collect();
        if elements.len() < 3 {
            info!("too few arguments - Format: name|header|body");
        } else {
            let name = elements.get(0).expect("name is there");
            let header = elements.get(1).expect("header is there");
            let body = elements.get(2).expect("body is there");
            if let Err(e) = create_new_story(name, header, body).await {
                error!("error creating story: {}", e);
            };
        }
    }
}

async fn handle_publish_story(cmd: &str, story_sender: mpsc::UnboundedSender<Story>) {
    if let Some(rest) = cmd.strip_prefix("publish s") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_story(id, story_sender).await {
                    info!("error publishing story with id {}, {}", id, e)
                } else {
                    info!("Published story with id: {}", id);
                }
            }
            Err(e) => error!("invalid id: {}, {}", rest.trim(), e),
        };
    }
}

async fn handle_help(_cmd: &str) {
    println!("ls p to list peers");
    println!("ls s to list stories");
    println!("create s to create story");
    println!("publish s to publish story");
    println!("quit to quit");
}

async fn establish_direct_connection(swarm: &mut Swarm<StoryBehaviour>, addr_str: &str) {
    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            info!("Manually dialing address: {}", addr);
            match swarm.dial(addr) {
                Ok(_) => {
                    info!("Dialing initiated successfully");
                    
                    
                    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!("Number of connected peers: {}", connected_peers.len());
                    
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let connected_peers_after: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!("Number of connected peers after 2 seconds: {}", connected_peers_after.len());
                    for peer in connected_peers {
                        info!("Connected to peer: {}", peer);
                        
                        info!("Adding peer to floodsub: {}", peer);
                        swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer);
                    }
                },
                Err(e) => error!("Failed to dial: {}", e),
            }
        },
        Err(e) => error!("Failed to parse address: {}", e),
    }
}
