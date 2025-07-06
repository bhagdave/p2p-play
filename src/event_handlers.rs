use crate::handlers::*;
use crate::network::{PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::save_received_story;
use crate::types::{EventType, ListMode, ListRequest, ListResponse, PeerName, PublishedStory};

use bytes::Bytes;
use libp2p::{PeerId, Swarm};
use log::{error, info};
use std::collections::HashMap;
use std::process;
use tokio::sync::mpsc;

/// Handle response events by publishing them to the network
pub async fn handle_response_event(resp: ListResponse, swarm: &mut Swarm<StoryBehaviour>) {
    info!("Response received");
    let json = serde_json::to_string(&resp).expect("can jsonify response");
    let json_bytes = Bytes::from(json.into_bytes());
    swarm
        .behaviour_mut()
        .floodsub
        .publish(TOPIC.clone(), json_bytes);
}

/// Handle story publishing events
pub async fn handle_publish_story_event(
    story: crate::types::Story,
    swarm: &mut Swarm<StoryBehaviour>,
) {
    info!("Broadcasting published story: {}", story.name);

    // Pre-publish connection check and reconnection
    maintain_connections(swarm).await;

    // Allow connections to stabilize
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Debug: Show connected peers and floodsub state
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    info!("Currently connected peers: {}", connected_peers.len());
    for peer in &connected_peers {
        info!("Connected to: {}", peer);
    }

    if connected_peers.is_empty() {
        error!("No connected peers available for story broadcast!");
    }

    let published_story = PublishedStory {
        story,
        publisher: PEER_ID.to_string(),
    };
    let json = serde_json::to_string(&published_story).expect("can jsonify published story");
    let json_bytes = Bytes::from(json.into_bytes());
    info!(
        "Publishing {} bytes to topic {:?}",
        json_bytes.len(),
        TOPIC.clone()
    );
    swarm
        .behaviour_mut()
        .floodsub
        .publish(TOPIC.clone(), json_bytes);
    info!("Story broadcast completed");
}

/// Handle user input events
pub async fn handle_input_event(
    line: String,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    story_sender: mpsc::UnboundedSender<crate::types::Story>,
    local_peer_name: &mut Option<String>,
) {
    match line.as_str() {
        "ls p" => handle_list_peers(swarm, peer_names).await,
        "ls c" => handle_list_connections(swarm, peer_names).await,
        cmd if cmd.starts_with("ls s") => handle_list_stories(cmd, swarm).await,
        cmd if cmd.starts_with("create s") => handle_create_stories(cmd).await,
        cmd if cmd.starts_with("publish s") => {
            handle_publish_story(cmd, story_sender.clone()).await
        }
        cmd if cmd.starts_with("help") => handle_help(cmd).await,
        cmd if cmd.starts_with("quit") => process::exit(0),
        cmd if cmd.starts_with("name ") => {
            if let Some(peer_name) = handle_set_name(cmd, local_peer_name).await {
                // Broadcast the peer name to connected peers
                let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
                let json_bytes = Bytes::from(json.into_bytes());
                swarm
                    .behaviour_mut()
                    .floodsub
                    .publish(TOPIC.clone(), json_bytes);
                info!("Broadcasted peer name to connected peers");
            }
        }
        cmd if cmd.starts_with("connect ") => {
            if let Some(addr) = cmd.strip_prefix("connect ") {
                establish_direct_connection(swarm, addr).await;
            }
        }
        _ => eprintln!("unknown command"),
    }
}

/// Handle mDNS discovery events
pub async fn handle_mdns_event(mdns_event: libp2p::mdns::Event, swarm: &mut Swarm<StoryBehaviour>) {
    match mdns_event {
        libp2p::mdns::Event::Discovered(discovered_list) => {
            info!("Discovered Peers event");
            for (peer, addr) in discovered_list {
                info!("Discovered a peer:{} at {}", peer, addr);
                if !swarm.is_connected(&peer) {
                    info!("Attempting to dial peer: {}", peer);
                    if let Err(e) = swarm.dial(peer) {
                        error!("Failed to initiate dial to {}: {}", peer, e);
                    }
                } else {
                    info!("Already connected to peer: {}", peer);
                }
            }
        }
        libp2p::mdns::Event::Expired(expired_list) => {
            info!("Expired Peers event");
            for (peer, _addr) in expired_list {
                info!("Expired a peer:{} at {}", peer, _addr);
                let discovered_nodes: Vec<_> = swarm.behaviour().mdns.discovered_nodes().collect();
                if !discovered_nodes.contains(&(&peer)) {
                    info!("Removing peer from partial view: {}", peer);
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .remove_node_from_partial_view(&peer);
                }
            }
        }
    }
}

/// Handle floodsub message events
pub async fn handle_floodsub_event(
    floodsub_event: libp2p::floodsub::FloodsubEvent,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    peer_names: &mut HashMap<PeerId, String>,
) {
    match floodsub_event {
        libp2p::floodsub::FloodsubEvent::Message(msg) => {
            info!("Message event received from {:?}", msg.source);
            info!("Message data length: {} bytes", msg.data.len());
            if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    info!("Response from {}:", msg.source);
                    resp.data.iter().for_each(|r| info!("{:?}", r));
                }
            } else if let Ok(published) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                if published.publisher != PEER_ID.to_string() {
                    info!(
                        "Received published story '{}' from {}",
                        published.story.name, msg.source
                    );
                    info!("Story: {:?}", published.story);

                    // Save received story to local storage asynchronously
                    let story_to_save = published.story.clone();
                    tokio::spawn(async move {
                        if let Err(e) = save_received_story(story_to_save).await {
                            error!("Failed to save received story: {}", e);
                        }
                    });
                }
            } else if let Ok(peer_name) = serde_json::from_slice::<PeerName>(&msg.data) {
                if let Ok(peer_id) = peer_name.peer_id.parse::<PeerId>() {
                    if peer_id != *PEER_ID {
                        info!("Received peer name '{}' from {}", peer_name.name, peer_id);
                        peer_names.insert(peer_id, peer_name.name);
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
    }
}

/// Handle ping events for connection monitoring
pub async fn handle_ping_event(ping_event: libp2p::ping::Event) {
    match ping_event {
        libp2p::ping::Event {
            peer,
            result: Ok(rtt),
            ..
        } => {
            info!("Ping to {} successful: {}ms", peer, rtt.as_millis());
        }
        libp2p::ping::Event {
            peer,
            result: Err(failure),
            ..
        } => {
            error!("Ping to {} failed: {}", peer, failure);
        }
    }
}

/// Handle peer name events
pub async fn handle_peer_name_event(peer_name: PeerName) {
    // This shouldn't happen since PeerName events are created from floodsub messages
    // but we'll handle it just in case
    info!(
        "Received PeerName event: {} -> {}",
        peer_name.peer_id, peer_name.name
    );
}

/// Main event dispatcher that routes events to appropriate handlers
pub async fn handle_event(
    event: EventType,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &mut HashMap<PeerId, String>,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    story_sender: mpsc::UnboundedSender<crate::types::Story>,
    local_peer_name: &mut Option<String>,
) {
    info!("Event Received");
    match event {
        EventType::Response(resp) => {
            handle_response_event(resp, swarm).await;
        }
        EventType::PublishStory(story) => {
            handle_publish_story_event(story, swarm).await;
        }
        EventType::Input(line) => {
            handle_input_event(line, swarm, peer_names, story_sender, local_peer_name).await;
        }
        EventType::MdnsEvent(mdns_event) => {
            handle_mdns_event(mdns_event, swarm).await;
        }
        EventType::FloodsubEvent(floodsub_event) => {
            handle_floodsub_event(floodsub_event, response_sender, peer_names).await;
        }
        EventType::PingEvent(ping_event) => {
            handle_ping_event(ping_event).await;
        }
        EventType::PeerName(peer_name) => {
            handle_peer_name_event(peer_name).await;
        }
    }
}

// Helper function that needs to be accessible - copied from main.rs
pub async fn maintain_connections(swarm: &mut Swarm<StoryBehaviour>) {
    let discovered_peers: Vec<_> = swarm.behaviour().mdns.discovered_nodes().cloned().collect();
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();

    info!(
        "Connection maintenance: {} discovered, {} connected",
        discovered_peers.len(),
        connected_peers.len()
    );

    // Try to connect to discovered peers that aren't connected
    for peer in discovered_peers {
        if !swarm.is_connected(&peer) {
            info!("Reconnecting to discovered peer: {}", peer);
            if let Err(e) = swarm.dial(peer) {
                error!("Failed to dial peer {}: {}", peer, e);
            }
        }
    }
}

// Helper function that needs to be accessible - copied from main.rs
pub fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match crate::storage::read_local_stories().await {
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
