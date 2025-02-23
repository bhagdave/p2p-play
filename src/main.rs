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
use bytes::Bytes;

const STORAGE_FILE_PATH: &str = "./stories.json";

//type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
type Stories = Vec<Story>;

static KEYS: Lazy<identity::Keypair> = Lazy::new(identity::Keypair::generate_ed25519);
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));

#[derive(Debug, Serialize, Deserialize)]
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

enum EventType {
    Response(ListResponse),
    Input(String),
    FloodsubEvent(FloodsubEvent),
    MdnsEvent(mdns::Event),
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

async fn publish_story(id: usize) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
    local_stories
        .iter_mut()
        .filter(|r| r.id == id)
        .for_each(|r| r.public = true);
    write_local_stories(&local_stories).await?;
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

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();

//    let auth_keys = noise::Config::<X25519Spec>::new()
//        .into_authentic(&KEYS)
//        .expect("can create auth keys");
	
    let auth_keys = identity::Keypair::generate_ed25519();

    let transp = tcp::tokio::Transport::new(Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&auth_keys).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();
    let _ping = crate::ping::Behaviour::new(libp2p::ping::Config::new());
    let mut behaviour = StoryBehaviour {
        floodsub: Floodsub::new(*PEER_ID),
        mdns: mdns::tokio::Behaviour::new(Default::default(), PEER_ID.clone())
            .expect("can create mdns"),
    };

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
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => Some(EventType::FloodsubEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => Some(EventType::MdnsEvent(event)),
                        _ => {
                            info!("Unhandled Swarm Event: {:?}", event);
                            None
                        }
                    }
                },
            }
        };

        if let Some(event) = evt {
            match event {
                EventType::Response(resp) => {
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
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
                    cmd if cmd.starts_with("publish s") => handle_publish_story(cmd).await,
                    _ => error!("unknown command"),
                },
                EventType::MdnsEvent(mdns_event) => match mdns_event {
                    mdns::Event::Discovered(discovered_list) => {
                        for (peer, _addr) in discovered_list {
                            info!("Disocvered a peer:{} at {}", peer, _addr);
                            swarm
                                .behaviour_mut()
                                .floodsub
                                .add_node_to_partial_view(peer);
                        }
                    }
                    mdns::Event::Expired(expired_list) => {
                        for (peer, _addr) in expired_list {
                            info!("Expired a peer:{} at {}", peer, _addr);
                            if !swarm.behaviour_mut().mdns.has_node(&peer) {
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
                        if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                            if resp.receiver == PEER_ID.to_string() {
                                info!("Response from {}:", msg.source);
                                resp.data.iter().for_each(|r| info!("{:?}", r));
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
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
	    let json_bytes = Bytes::from(json.into_bytes());		
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        Some(story_peer_id) => {
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
	    let json_bytes = Bytes::from(json.into_bytes());		
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        None => {
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

async fn handle_publish_story(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("publish s") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_story(id).await {
                    info!("error publishing story with id {}, {}", id, e)
                } else {
                    info!("Published story with id: {}", id);
                }
            }
            Err(e) => error!("invalid id: {}, {}", rest.trim(), e),
        };
    }
}
