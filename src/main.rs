mod handlers;
mod network;
mod storage;
mod types;

use handlers::*;
use network::{PEER_ID, StoryBehaviourEvent, TOPIC, create_swarm};
use storage::save_received_story;
use types::{EventType, ListMode, ListRequest, ListResponse, PeerName, PublishedStory};

use bytes::Bytes;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, futures::StreamExt};
use log::{error, info};
use std::collections::HashMap;
use std::process;
use tokio::{io::AsyncBufReadExt, sync::mpsc};

fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match storage::read_local_stories().await {
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

async fn maintain_connections(swarm: &mut Swarm<network::StoryBehaviour>) {
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

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();
    let (story_sender, mut story_rcv) = mpsc::unbounded_channel();

    // Create a timer for periodic connection maintenance
    let mut connection_maintenance_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(30));

    let mut swarm = create_swarm().expect("Failed to create swarm");
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    // Storage for peer names (peer_id -> alias)
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut local_peer_name: Option<String> = None;

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
                _ = connection_maintenance_interval.tick() => {
                    // Periodic connection maintenance
                    maintain_connections(&mut swarm).await;
                    None
                },
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => Some(EventType::FloodsubEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => Some(EventType::MdnsEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(event)) => Some(EventType::PingEvent(event)),
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Local node is listening on {}", address);
                            None
                        },
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            info!("Connection established to {} via {:?}", peer_id, endpoint);
                            info!("Adding peer {} to floodsub partial view", peer_id);
                            swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer_id);

                            // If we have a local peer name set, broadcast it to the newly connected peer
                            if let Some(ref name) = local_peer_name {
                                let peer_name = PeerName::new(PEER_ID.to_string(), name.clone());
                                let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
                                let json_bytes = Bytes::from(json.into_bytes());
                                swarm.behaviour_mut().floodsub.publish(TOPIC.clone(), json_bytes);
                                info!("Sent local peer name '{}' to newly connected peer {}", name, peer_id);
                            }

                            None
                        },
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed to {}: {:?}", peer_id, cause);
                            info!("Removing peer {} from floodsub partial view", peer_id);
                            swarm.behaviour_mut().floodsub.remove_node_from_partial_view(&peer_id);

                            // Remove the peer name when connection is closed
                            if let Some(name) = peer_names.remove(&peer_id) {
                                info!("Removed peer name '{}' for disconnected peer {}", name, peer_id);
                            }

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

                    // Pre-publish connection check and reconnection
                    maintain_connections(&mut swarm).await;

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
                    let json = serde_json::to_string(&published_story)
                        .expect("can jsonify published story");
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
                EventType::Input(line) => match line.as_str() {
                    "ls p" => handle_list_peers(&mut swarm, &peer_names).await,
                    "ls c" => handle_list_connections(&mut swarm, &peer_names).await,
                    cmd if cmd.starts_with("ls s") => handle_list_stories(cmd, &mut swarm).await,
                    cmd if cmd.starts_with("create s") => handle_create_stories(cmd).await,
                    cmd if cmd.starts_with("publish s") => {
                        handle_publish_story(cmd, story_sender.clone()).await
                    }
                    cmd if cmd.starts_with("help") => handle_help(cmd).await,
                    cmd if cmd.starts_with("quit") => process::exit(0),
                    cmd if cmd.starts_with("name ") => {
                        if let Some(peer_name) = handle_set_name(cmd, &mut local_peer_name).await {
                            // Broadcast the peer name to connected peers
                            let json =
                                serde_json::to_string(&peer_name).expect("can jsonify peer name");
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
                            establish_direct_connection(&mut swarm, addr).await;
                        }
                    }
                    _ => eprintln!("unknown command"),
                },
                EventType::MdnsEvent(mdns_event) => match mdns_event {
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
                            let discovered_nodes: Vec<_> =
                                swarm.behaviour().mdns.discovered_nodes().collect();
                            if !discovered_nodes.contains(&(&peer)) {
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
                    libp2p::floodsub::FloodsubEvent::Message(msg) => {
                        info!("Message event received from {:?}", msg.source);
                        info!("Message data length: {} bytes", msg.data.len());
                        if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                            if resp.receiver == PEER_ID.to_string() {
                                info!("Response from {}:", msg.source);
                                resp.data.iter().for_each(|r| info!("{:?}", r));
                            }
                        } else if let Ok(published) =
                            serde_json::from_slice::<PublishedStory>(&msg.data)
                        {
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
                        } else if let Ok(peer_name) = serde_json::from_slice::<PeerName>(&msg.data)
                        {
                            if let Ok(peer_id) = peer_name.peer_id.parse::<PeerId>() {
                                if peer_id != *PEER_ID {
                                    info!(
                                        "Received peer name '{}' from {}",
                                        peer_name.name, peer_id
                                    );
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
                },
                EventType::PingEvent(ping_event) => {
                    // Handle ping events for connection monitoring
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
                EventType::PeerName(peer_name) => {
                    // This shouldn't happen since PeerName events are created from floodsub messages
                    // but we'll handle it just in case
                    info!(
                        "Received PeerName event: {} -> {}",
                        peer_name.peer_id, peer_name.name
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use crate::types::{ListResponse, ListMode, Story};

    #[test]
    fn test_respond_with_public_stories() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        rt.block_on(async {
            let (sender, mut receiver) = mpsc::unbounded_channel::<ListResponse>();
            let receiver_name = "test_receiver".to_string();
            
            // Test the function (it will read from default stories file)
            respond_with_public_stories(sender, receiver_name.clone());
            
            // Try to receive a response (may timeout if no stories file exists)
            // This mainly tests that the function doesn't panic
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                receiver.recv()
            ).await.ok();
        });
    }

    #[test] 
    fn test_event_type_variants() {
        use crate::types::{EventType, PeerName, ListResponse, ListMode};
        use libp2p::floodsub::FloodsubEvent;
        use libp2p::mdns::Event as MdnsEvent;
        use libp2p::ping::Event as PingEvent;
        use std::time::Duration;
        use bytes::Bytes;

        // Test all EventType variants can be created
        let _input_event = EventType::Input("test input".to_string());
        
        let list_response = ListResponse::new(
            ListMode::ALL,
            "test".to_string(),
            vec![]
        );
        let _response_event = EventType::Response(list_response);
        
        let story = Story::new(1, "Test".to_string(), "Header".to_string(), "Body".to_string(), true);
        let _publish_event = EventType::PublishStory(story);
        
        let peer_name = PeerName::new("peer123".to_string(), "Alice".to_string());
        let _peer_name_event = EventType::PeerName(peer_name);
        
        // Test floodsub event
        let mock_message = libp2p::floodsub::FloodsubMessage {
            source: *PEER_ID,
            data: Bytes::from("test"),
            sequence_number: b"seq123".to_vec(),
            topics: vec![TOPIC.clone()],
        };
        let floodsub_event = FloodsubEvent::Message(mock_message);
        let _floodsub_event_type = EventType::FloodsubEvent(floodsub_event);
        
        // Test mDNS event
        let mdns_event = MdnsEvent::Discovered(
            std::iter::once((*PEER_ID, "/ip4/127.0.0.1/tcp/8080".parse().unwrap())).collect()
        );
        let _mdns_event_type = EventType::MdnsEvent(mdns_event);
        
        // Test ping event
        let ping_event = PingEvent {
            peer: *PEER_ID,
            connection: libp2p::swarm::ConnectionId::new_unchecked(1),
            result: Ok(Duration::from_millis(50)),
        };
        let _ping_event_type = EventType::PingEvent(ping_event);
    }

    #[test]
    fn test_peer_id_consistency() {
        // Test that PEER_ID is consistent in main module
        let peer_id_1 = *PEER_ID;
        let peer_id_2 = *PEER_ID;
        assert_eq!(peer_id_1, peer_id_2);
    }

    #[test]
    fn test_topic_consistency() {
        // Test that TOPIC is accessible from main module
        let topic = TOPIC.clone();
        let topic_str = format!("{:?}", topic);
        assert!(topic_str.contains("stories"));
    }
}
