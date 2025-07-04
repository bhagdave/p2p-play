use crate::network::{PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{create_new_story, publish_story, read_local_stories};
use crate::types::{ListMode, ListRequest, PeerName, Story};
use bytes::Bytes;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use log::{error, info};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

pub async fn handle_list_peers(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
) {
    info!("Discovered Peers:");
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| {
        let name = peer_names
            .get(p)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        info!("{}{}", p, name);
    });
}

pub async fn handle_list_connections(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
) {
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    info!("Connected Peers: {}", connected_peers.len());
    for peer in connected_peers {
        let name = peer_names
            .get(&peer)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        info!("Connected to: {}{}", peer, name);
    }
}

pub async fn handle_list_stories(cmd: &str, swarm: &mut Swarm<StoryBehaviour>) {
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
            info!(
                "Publishing to topic: {:?} from peer:{:?}",
                TOPIC.clone(),
                PEER_ID.clone()
            );
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

pub async fn handle_create_stories(cmd: &str) {
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

pub async fn handle_publish_story(cmd: &str, story_sender: mpsc::UnboundedSender<Story>) {
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

pub async fn handle_help(_cmd: &str) {
    info!("ls p to list discovered peers");
    info!("ls c to list connected peers");
    info!("ls s to list stories");
    info!("create s to create story");
    info!("publish s to publish story");
    info!("name <alias> to set your peer name");
    info!("quit to quit");
}

pub async fn handle_set_name(cmd: &str, local_peer_name: &mut Option<String>) -> Option<PeerName> {
    if let Some(name) = cmd.strip_prefix("name ") {
        let name = name.trim();
        if name.is_empty() {
            error!("Name cannot be empty");
            return None;
        }

        *local_peer_name = Some(name.to_string());
        info!("Set local peer name to: {}", name);

        // Return a PeerName message to broadcast to connected peers
        Some(PeerName::new(PEER_ID.to_string(), name.to_string()))
    } else {
        error!("Usage: name <alias>");
        None
    }
}

pub async fn establish_direct_connection(swarm: &mut Swarm<StoryBehaviour>, addr_str: &str) {
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
                    info!(
                        "Number of connected peers after 2 seconds: {}",
                        connected_peers_after.len()
                    );
                    for peer in connected_peers {
                        info!("Connected to peer: {}", peer);

                        info!("Adding peer to floodsub: {}", peer);
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                Err(e) => error!("Failed to dial: {}", e),
            }
        }
        Err(e) => error!("Failed to parse address: {}", e),
    }
}
