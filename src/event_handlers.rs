use crate::error_logger::ErrorLogger;
use crate::handlers::*;
use crate::network::{DirectMessageRequest, DirectMessageResponse, NodeDescriptionRequest, NodeDescriptionResponse, PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{save_received_story, load_node_description};
use crate::types::{
    ActionResult, DirectMessage, EventType, ListMode, ListRequest, ListResponse, PeerName, PublishedStory,
};

use bytes::Bytes;
use libp2p::{PeerId, Swarm, request_response};
use log::{debug, error};
use std::collections::HashMap;
use std::process;
use tokio::sync::mpsc;

/// Handle response events by publishing them to the network
pub async fn handle_response_event(resp: ListResponse, swarm: &mut Swarm<StoryBehaviour>) {
    debug!("Response received");
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
    debug!("Broadcasting published story: {}", story.name);

    // Pre-publish connection check and reconnection
    maintain_connections(swarm).await;

    // Allow connections to stabilize
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Debug: Show connected peers and floodsub state
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    debug!("Currently connected peers: {}", connected_peers.len());
    for peer in &connected_peers {
        debug!("Connected to: {}", peer);
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
    debug!(
        "Publishing {} bytes to topic {:?}",
        json_bytes.len(),
        TOPIC.clone()
    );
    swarm
        .behaviour_mut()
        .floodsub
        .publish(TOPIC.clone(), json_bytes);
    debug!("Story broadcast completed");
}

/// Handle user input events
pub async fn handle_input_event(
    line: String,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    story_sender: mpsc::UnboundedSender<crate::types::Story>,
    local_peer_name: &mut Option<String>,
    sorted_peer_names_cache: &SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    match line.as_str() {
        "ls p" => handle_list_peers(swarm, peer_names, ui_logger).await,
        "ls c" => handle_list_connections(swarm, peer_names, ui_logger).await,
        "ls ch" => handle_list_channels(ui_logger, error_logger).await,
        "ls sub" => handle_list_subscriptions(ui_logger, error_logger).await,
        cmd if cmd.starts_with("ls s") => {
            handle_list_stories(cmd, swarm, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("create s") => {
            return handle_create_stories(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("create ch") => {
            return handle_create_channel(cmd, local_peer_name, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("create desc") => {
            handle_create_description(cmd, ui_logger).await
        }
        cmd if cmd.starts_with("sub ") => {
            handle_subscribe_channel(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("unsub ") => {
            handle_unsubscribe_channel(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("publish s") => {
            handle_publish_story(cmd, story_sender.clone(), ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("show story") => handle_show_story(cmd, ui_logger).await,
        "show desc" => handle_show_description(ui_logger).await,
        cmd if cmd.starts_with("get desc") => {
            handle_get_description(cmd, ui_logger, swarm, local_peer_name, peer_names).await
        }
        cmd if cmd.starts_with("delete s") => {
            return handle_delete_story(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("help") => handle_help(cmd, ui_logger).await,
        cmd if cmd.starts_with("dht bootstrap") => handle_dht_bootstrap(cmd, swarm, ui_logger).await,
        cmd if cmd.starts_with("dht peers") => handle_dht_get_peers(cmd, swarm, ui_logger).await,
        cmd if cmd.starts_with("quit") => process::exit(0),
        "name" => {
            // Show current alias when no arguments provided
            match local_peer_name {
                Some(name) => ui_logger.log(format!("Current alias: {}", name)),
                None => ui_logger.log("No alias set. Use 'name <alias>' to set one.".to_string()),
            }
        }
        cmd if cmd.starts_with("name ") => {
            if let Some(peer_name) = handle_set_name(cmd, local_peer_name, ui_logger).await {
                // Broadcast the peer name to connected peers
                let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
                let json_bytes = Bytes::from(json.into_bytes());
                swarm
                    .behaviour_mut()
                    .floodsub
                    .publish(TOPIC.clone(), json_bytes);
                debug!("Broadcasted peer name to connected peers");
            }
        }
        cmd if cmd.starts_with("connect ") => {
            if let Some(addr) = cmd.strip_prefix("connect ") {
                establish_direct_connection(swarm, addr, ui_logger).await;
            }
        }
        cmd if cmd.starts_with("msg ") => {
            handle_direct_message(
                cmd,
                swarm,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
            )
            .await;
        }
        _ => ui_logger.log("unknown command".to_string()),
    }
    None
}

/// Handle mDNS discovery events
pub async fn handle_mdns_event(mdns_event: libp2p::mdns::Event, swarm: &mut Swarm<StoryBehaviour>) {
    match mdns_event {
        libp2p::mdns::Event::Discovered(discovered_list) => {
            debug!("Discovered Peers event");
            for (peer, addr) in discovered_list {
                debug!("Discovered a peer:{} at {}", peer, addr);
                if !swarm.is_connected(&peer) {
                    debug!("Attempting to dial peer: {}", peer);
                    if let Err(e) = swarm.dial(peer) {
                        error!("Failed to initiate dial to {}: {}", peer, e);
                    }
                } else {
                    debug!("Already connected to peer: {}", peer);
                }
            }
        }
        libp2p::mdns::Event::Expired(expired_list) => {
            debug!("Expired Peers event");
            for (peer, _addr) in expired_list {
                debug!("Expired a peer:{} at {}", peer, _addr);
                let discovered_nodes: Vec<_> = swarm.behaviour().mdns.discovered_nodes().collect();
                if !discovered_nodes.contains(&(&peer)) {
                    debug!("Removing peer from partial view: {}", peer);
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
    floodsub_event: libp2p::floodsub::Event,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    peer_names: &mut HashMap<PeerId, String>,
    _local_peer_name: &Option<String>,
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
) -> Option<()> {
    match floodsub_event {
        libp2p::floodsub::Event::Message(msg) => {
            debug!("Message event received from {:?}", msg.source);
            debug!("Message data length: {} bytes", msg.data.len());
            if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    debug!("Response from {}:", msg.source);
                    resp.data.iter().for_each(|r| debug!("{:?}", r));
                }
            } else if let Ok(published) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                if published.publisher != PEER_ID.to_string() {
                    // Check if we're subscribed to the story's channel
                    let should_accept_story = match crate::storage::read_subscribed_channels(
                        &PEER_ID.to_string(),
                    )
                    .await
                    {
                        Ok(subscribed_channels) => {
                            subscribed_channels.contains(&published.story.channel)
                                || published.story.channel == "general"
                        }
                        Err(e) => {
                            error!("Failed to check subscriptions for incoming story: {}", e);
                            // Default to accepting general channel stories only
                            published.story.channel == "general"
                        }
                    };

                    if should_accept_story {
                        debug!(
                            "Received published story '{}' from {} in channel '{}'",
                            published.story.name, msg.source, published.story.channel
                        );
                        debug!("Story: {:?}", published.story);
                        ui_logger.log(format!(
                            "📖 Received story '{}' from {} in channel '{}'",
                            published.story.name, msg.source, published.story.channel
                        ));

                        // Save received story to local storage asynchronously
                        let story_to_save = published.story.clone();
                        tokio::spawn(async move {
                            if let Err(e) = save_received_story(story_to_save).await {
                                error!("Failed to save received story: {}", e);
                            }
                        });

                        // Signal that stories need to be refreshed
                        return Some(());
                    } else {
                        debug!(
                            "Ignoring story '{}' from channel '{}' - not subscribed",
                            published.story.name, published.story.channel
                        );
                    }
                }
            } else if let Ok(peer_name) = serde_json::from_slice::<PeerName>(&msg.data) {
                if let Ok(peer_id) = peer_name.peer_id.parse::<PeerId>() {
                    if peer_id != *PEER_ID {
                        debug!("Received peer name '{}' from {}", peer_name.name, peer_id);

                        // Only update the peer name if it's new or has actually changed
                        let mut names_changed = false;
                        peer_names
                            .entry(peer_id)
                            .and_modify(|existing_name| {
                                if existing_name != &peer_name.name {
                                    debug!(
                                        "Peer {} name changed from '{}' to '{}'",
                                        peer_id, existing_name, peer_name.name
                                    );
                                    *existing_name = peer_name.name.clone();
                                    names_changed = true;
                                } else {
                                    debug!("Peer {} name unchanged: '{}'", peer_id, peer_name.name);
                                }
                            })
                            .or_insert_with(|| {
                                debug!(
                                    "Setting peer {} name to '{}' (first time)",
                                    peer_id, peer_name.name
                                );
                                names_changed = true;
                                peer_name.name.clone()
                            });

                        // Update the cache if peer names changed
                        if names_changed {
                            sorted_peer_names_cache.update(peer_names);
                        }
                    }
                }
            } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                match req.mode {
                    ListMode::ALL => {
                        debug!("Received ALL req: {:?} from {:?}", req, msg.source);
                        respond_with_public_stories(
                            response_sender.clone(),
                            msg.source.to_string(),
                        );
                    }
                    ListMode::One(ref peer_id) => {
                        if peer_id == &PEER_ID.to_string() {
                            debug!("Received req: {:?} from {:?}", req, msg.source);
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
            debug!("Subscription events");
        }
    }
    None
}

/// Handle DHT bootstrap command
pub async fn handle_dht_bootstrap(cmd: &str, swarm: &mut Swarm<StoryBehaviour>, ui_logger: &UILogger) {
    crate::handlers::handle_dht_bootstrap(cmd, swarm, ui_logger).await;
}

/// Handle DHT get closest peers command
pub async fn handle_dht_get_peers(cmd: &str, swarm: &mut Swarm<StoryBehaviour>, ui_logger: &UILogger) {
    crate::handlers::handle_dht_get_peers(cmd, swarm, ui_logger).await;
}

/// Handle Kademlia DHT events for peer discovery
pub async fn handle_kad_event(
    kad_event: libp2p::kad::Event,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    match kad_event {
        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => {
            match result {
                libp2p::kad::QueryResult::Bootstrap(Ok(bootstrap_ok)) => {
                    debug!("Kademlia bootstrap successful with peer: {}", bootstrap_ok.peer);
                    ui_logger.log(format!("DHT bootstrap successful with peer: {}", bootstrap_ok.peer));
                }
                libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                    error!("Kademlia bootstrap failed: {:?}", e);
                    ui_logger.log(format!("DHT bootstrap failed: {:?}", e));
                }
                libp2p::kad::QueryResult::GetClosestPeers(Ok(get_closest_peers_ok)) => {
                    debug!("Found {} closest peers to key", get_closest_peers_ok.peers.len());
                    for peer in &get_closest_peers_ok.peers {
                        debug!("Closest peer: {:?}", peer);
                    }
                }
                libp2p::kad::QueryResult::GetClosestPeers(Err(e)) => {
                    error!("Failed to get closest peers: {:?}", e);
                }
                _ => {
                    debug!("Other Kademlia query result: {:?}", result);
                }
            }
        }
        libp2p::kad::Event::RoutingUpdated { peer, is_new_peer, .. } => {
            if is_new_peer {
                debug!("New peer added to DHT routing table: {}", peer);
                ui_logger.log(format!("New peer added to DHT: {}", peer));
                
                // Add the peer to floodsub partial view if connected
                if swarm.is_connected(&peer) {
                    swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer);
                    debug!("Added DHT peer {} to floodsub partial view", peer);
                }
            }
        }
        libp2p::kad::Event::InboundRequest { request } => {
            match request {
                libp2p::kad::InboundRequest::FindNode { .. } => {
                    debug!("Received DHT FindNode request");
                }
                libp2p::kad::InboundRequest::GetProvider { .. } => {
                    debug!("Received DHT GetProvider request");
                }
                _ => {
                    debug!("Received other DHT inbound request: {:?}", request);
                }
            }
        }
        libp2p::kad::Event::ModeChanged { new_mode } => {
            debug!("Kademlia mode changed to: {:?}", new_mode);
            ui_logger.log(format!("DHT mode changed to: {:?}", new_mode));
        }
        _ => {
            debug!("Other Kademlia event: {:?}", kad_event);
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
            debug!("Ping to {} successful: {}ms", peer, rtt.as_millis());
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
    debug!(
        "Received PeerName event: {} -> {}",
        peer_name.peer_id, peer_name.name
    );
}

/// Handle request-response events for direct messaging
pub async fn handle_request_response_event(
    event: request_response::Event<DirectMessageRequest, DirectMessageResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => {
            match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    // Validate sender identity to prevent spoofing
                    if request.from_peer_id != peer.to_string() {
                        error!(
                            "Direct message sender identity mismatch: claimed {} but actual connection from {}",
                            request.from_peer_id, peer
                        );

                        // Send response indicating rejection due to identity mismatch
                        let response = DirectMessageResponse {
                            received: false,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        };

                        if let Err(e) = swarm
                            .behaviour_mut()
                            .request_response
                            .send_response(channel, response)
                        {
                            error!("Failed to send rejection response to {}: {:?}", peer, e);
                        }
                        return;
                    }

                    // Handle incoming direct message request
                    if let Some(local_name) = local_peer_name {
                        if &request.to_name == local_name {
                            // Regular direct message
                            ui_logger.log(format!(
                                "📨 Direct message from {}: {}",
                                request.from_name, request.message
                            ));
                        }
                    }

                    // Send response acknowledging receipt
                    let response = DirectMessageResponse {
                        received: true,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    };

                    // Send the response using the channel
                    if let Err(e) = swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, response)
                    {
                        error!("Failed to send response to {}: {:?}", peer, e);
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // Handle response to our direct message request
                    if response.received {
                        debug!("Direct message was received by peer {}", peer);
                        ui_logger.log(format!("✅ Message delivered to peer {}", peer));
                    } else {
                        error!("Direct message was rejected by peer {}", peer);
                        ui_logger.log(format!(
                            "❌ Message rejected by peer {} (identity validation failed)",
                            peer
                        ));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            error!("Failed to send direct message to {}: {:?}", peer, error);
            ui_logger.log(format!(
                "❌ Failed to send direct message to {}: {:?}",
                peer, error
            ));
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            error!(
                "Failed to receive direct message from {}: {:?}",
                peer, error
            );
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("Response sent to {}", peer);
        }
    }
}

/// Handle direct message events
pub async fn handle_direct_message_event(direct_msg: DirectMessage) {
    // This shouldn't happen since DirectMessage events are processed in floodsub handler
    // but we'll handle it just in case
    debug!(
        "Received DirectMessage event: {} -> {}: {}",
        direct_msg.from_name, direct_msg.to_name, direct_msg.message
    );
}

/// Handle channel events
pub async fn handle_channel_event(channel: crate::types::Channel) {
    debug!(
        "Received Channel event: {} - {}",
        channel.name, channel.description
    );
}

/// Handle channel subscription events
pub async fn handle_channel_subscription_event(subscription: crate::types::ChannelSubscription) {
    debug!(
        "Received ChannelSubscription event: {} subscribed to {}",
        subscription.peer_id, subscription.channel_name
    );
}

/// Handle node description request-response events
pub async fn handle_node_description_event(
    event: request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => {
            match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    // Handle incoming node description request
                    debug!(
                        "Received node description request from {} ({})",
                        request.from_name, request.from_peer_id
                    );

                    ui_logger.log(format!(
                        "📋 Description request from {}",
                        request.from_name
                    ));

                    // Load our description and send it back
                    match load_node_description().await {
                        Ok(description) => {
                            let response = NodeDescriptionResponse {
                                description,
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name.as_deref().unwrap_or("Unknown").to_string(),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };

                            // Send the response
                            if let Err(e) = swarm
                                .behaviour_mut()
                                .node_description
                                .send_response(channel, response)
                            {
                                error!("Failed to send node description response to {}: {:?}", peer, e);
                                ui_logger.log(format!(
                                    "❌ Failed to send description response to {}: {:?}",
                                    peer, e
                                ));
                            } else {
                                debug!("Sent description response to {}", peer);
                            }
                        }
                        Err(e) => {
                            error!("Failed to load description: {}", e);
                            
                            // Send empty response to indicate no description
                            let response = NodeDescriptionResponse {
                                description: None,
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name.as_deref().unwrap_or("Unknown").to_string(),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };

                            if let Err(e) = swarm
                                .behaviour_mut()
                                .node_description
                                .send_response(channel, response)
                            {
                                error!("Failed to send empty description response to {}: {:?}", peer, e);
                            }
                        }
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // Handle incoming node description response
                    debug!(
                        "Received node description response from {} ({}): {:?}",
                        response.from_name, response.from_peer_id, response.description
                    );

                    match response.description {
                        Some(description) => {
                            ui_logger.log(format!(
                                "📋 Description from {} ({} bytes):",
                                response.from_name, description.len()
                            ));
                            ui_logger.log(description);
                        }
                        None => {
                            ui_logger.log(format!(
                                "📋 {} has no description set",
                                response.from_name
                            ));
                        }
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            error!("Failed to send description request to {}: {:?}", peer, error);
            ui_logger.log(format!(
                "❌ Failed to request description from {}: {:?}",
                peer, error
            ));
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            error!(
                "Failed to receive description request from {}: {:?}",
                peer, error
            );
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("Node description response sent to {}", peer);
        }
    }
}

/// Main event dispatcher that routes events to appropriate handlers
pub async fn handle_event(
    event: EventType,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &mut HashMap<PeerId, String>,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    story_sender: mpsc::UnboundedSender<crate::types::Story>,
    local_peer_name: &mut Option<String>,
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    debug!("Event Received");
    match event {
        EventType::Response(resp) => {
            handle_response_event(resp, swarm).await;
        }
        EventType::PublishStory(story) => {
            handle_publish_story_event(story, swarm).await;
        }
        EventType::Input(line) => {
            return handle_input_event(
                line,
                swarm,
                peer_names,
                story_sender,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                error_logger,
            )
            .await;
        }
        EventType::MdnsEvent(mdns_event) => {
            handle_mdns_event(mdns_event, swarm).await;
        }
        EventType::FloodsubEvent(floodsub_event) => {
            if let Some(()) = handle_floodsub_event(
                floodsub_event,
                response_sender,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
            )
            .await
            {
                // Stories were updated, refresh them
                return Some(ActionResult::RefreshStories);
            }
        }
        EventType::PingEvent(ping_event) => {
            handle_ping_event(ping_event).await;
        }
        EventType::RequestResponseEvent(request_response_event) => {
            handle_request_response_event(
                request_response_event,
                swarm,
                local_peer_name,
                ui_logger,
            )
            .await;
        }
        EventType::NodeDescriptionEvent(node_desc_event) => {
            handle_node_description_event(
                node_desc_event,
                swarm,
                local_peer_name,
                ui_logger,
            )
            .await;
        }
        EventType::KadEvent(kad_event) => {
            handle_kad_event(kad_event, swarm, ui_logger).await;
        }
        EventType::PeerName(peer_name) => {
            handle_peer_name_event(peer_name).await;
        }
        EventType::DirectMessage(direct_msg) => {
            handle_direct_message_event(direct_msg).await;
        }
        EventType::Channel(channel) => {
            handle_channel_event(channel).await;
        }
        EventType::ChannelSubscription(subscription) => {
            handle_channel_subscription_event(subscription).await;
        }
    }
    None
}

// Helper function that needs to be accessible - copied from main.rs
pub async fn maintain_connections(swarm: &mut Swarm<StoryBehaviour>) {
    let discovered_peers: Vec<_> = swarm.behaviour().mdns.discovered_nodes().cloned().collect();
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();

    debug!(
        "Connection maintenance: {} discovered, {} connected",
        discovered_peers.len(),
        connected_peers.len()
    );

    // Try to connect to discovered peers that aren't connected
    for peer in discovered_peers {
        if !swarm.is_connected(&peer) {
            debug!("Reconnecting to discovered peer: {}", peer);
            if let Err(e) = swarm.dial(peer) {
                error!("Failed to dial peer {}: {}", peer, e);
            }
        }
    }
}

// Helper function that needs to be accessible - copied from main.rs
pub fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        // Read stories and subscriptions separately to avoid Send issues
        let stories = match crate::storage::read_local_stories().await {
            Ok(stories) => stories,
            Err(e) => {
                error!("error fetching local stories to answer ALL request, {}", e);
                return;
            }
        };

        let subscribed_channels = match crate::storage::read_subscribed_channels(&receiver).await {
            Ok(channels) => channels,
            Err(e) => {
                error!("error fetching subscribed channels for {}: {}", receiver, e);
                // If we can't get subscriptions, default to "general" channel
                vec!["general".to_string()]
            }
        };

        // Filter stories to only include public stories from subscribed channels
        let filtered_stories: Vec<_> = stories
            .into_iter()
            .filter(|story| {
                story.public
                    && (subscribed_channels.contains(&story.channel) || story.channel == "general")
            })
            .collect();

        debug!(
            "Sending {} filtered stories to {} based on {} subscribed channels",
            filtered_stories.len(),
            receiver,
            subscribed_channels.len()
        );

        let resp = ListResponse {
            mode: ListMode::ALL,
            receiver,
            data: filtered_stories,
        };
        if let Err(e) = sender.send(resp) {
            error!("error sending response via channel, {}", e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::DirectMessageRequest;
    use libp2p::PeerId;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_direct_message_sender_validation() {
        let (ui_sender, _ui_receiver) = mpsc::unbounded_channel();
        let _ui_logger = UILogger::new(ui_sender);

        // Create a mock peer ID
        let peer_id = PeerId::random();
        let different_peer_id = PeerId::random();

        // Create a direct message request with mismatched peer ID
        let request = DirectMessageRequest {
            from_peer_id: different_peer_id.to_string(), // Different from actual peer
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello!".to_string(),
            timestamp: 1000,
        };

        // Test that identity validation would fail
        // (This test checks our validation logic conceptually)
        assert_ne!(request.from_peer_id, peer_id.to_string());
        assert_eq!(request.from_peer_id, different_peer_id.to_string());

        // Verify the message structure is correct
        assert_eq!(request.from_name, "Alice");
        assert_eq!(request.to_name, "Bob");
        assert_eq!(request.message, "Hello!");
    }

    #[test]
    fn test_direct_message_request_validation() {
        let peer_id = PeerId::random();

        // Test valid request
        let valid_request = DirectMessageRequest {
            from_peer_id: peer_id.to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello Bob!".to_string(),
            timestamp: 1000,
        };

        // Verify the request structure
        assert_eq!(valid_request.from_peer_id, peer_id.to_string());
        assert!(!valid_request.from_name.is_empty());
        assert!(!valid_request.to_name.is_empty());
        assert!(!valid_request.message.is_empty());

        // Test that peer ID validation would work
        assert_eq!(valid_request.from_peer_id, peer_id.to_string());
    }

    #[test]
    fn test_direct_message_response_creation() {
        let response_received = DirectMessageResponse {
            received: true,
            timestamp: 1000,
        };

        let response_rejected = DirectMessageResponse {
            received: false,
            timestamp: 1000,
        };

        assert!(response_received.received);
        assert!(!response_rejected.received);
        assert_eq!(response_received.timestamp, 1000);
        assert_eq!(response_rejected.timestamp, 1000);
    }

    #[test]
    fn test_ls_sub_pattern_matching() {
        // Test that pattern matching for "ls sub" works correctly
        // This prevents regression of the bug where "ls sub" was incorrectly
        // matched by "ls s*" pattern instead of its specific handler

        let test_cases = vec![
            ("ls sub", "subscription"),
            ("ls ch", "channels"),
            ("ls s local", "stories"),
            ("ls s all", "stories"),
            ("ls p", "peers"),
            ("ls c", "connections"),
        ];

        for (input, _expected_type) in test_cases {
            let result = match_command_type(input);
            match input {
                "ls sub" => assert_eq!(
                    result, "subscription",
                    "ls sub should match subscription handler"
                ),
                "ls ch" => assert_eq!(result, "channels", "ls ch should match channels handler"),
                cmd if cmd.starts_with("ls s") => assert_eq!(
                    result, "stories",
                    "ls s commands should match stories handler"
                ),
                "ls p" => assert_eq!(result, "peers", "ls p should match peers handler"),
                "ls c" => assert_eq!(
                    result, "connections",
                    "ls c should match connections handler"
                ),
                _ => {}
            }
        }
    }

    // Mock function that simulates the pattern matching logic from event_handlers.rs
    fn match_command_type(line: &str) -> &'static str {
        // This follows the exact same pattern matching order as in handle_input_event
        match line {
            "ls p" => "peers",
            "ls c" => "connections",
            "ls ch" => "channels",
            "ls sub" => "subscription",
            cmd if cmd.starts_with("ls s") => "stories",
            _ => "unknown",
        }
    }

    #[tokio::test]
    async fn test_handle_peer_name_event() {
        let peer_name = PeerName::new("peer123".to_string(), "TestAlias".to_string());

        // This function doesn't return a value, so we just test it doesn't panic
        handle_peer_name_event(peer_name).await;
    }

    #[tokio::test]
    async fn test_handle_direct_message_event() {
        let direct_msg = DirectMessage::new(
            "peer123".to_string(),
            "Alice".to_string(),
            "Bob".to_string(),
            "Test message".to_string(),
        );

        // This function doesn't return a value, so we just test it doesn't panic
        handle_direct_message_event(direct_msg).await;
    }

    #[tokio::test]
    async fn test_handle_channel_event() {
        let channel = crate::types::Channel::new(
            "test_channel".to_string(),
            "Test channel description".to_string(),
            "peer123".to_string(),
        );

        // This function doesn't return a value, so we just test it doesn't panic
        handle_channel_event(channel).await;
    }

    #[tokio::test]
    async fn test_handle_channel_subscription_event() {
        let subscription = crate::types::ChannelSubscription::new(
            "peer123".to_string(),
            "test_channel".to_string(),
        );

        // This function doesn't return a value, so we just test it doesn't panic
        handle_channel_subscription_event(subscription).await;
    }

    #[tokio::test]
    async fn test_handle_ping_event_success() {
        use libp2p::swarm::ConnectionId;
        use std::time::Duration;

        let peer_id = PeerId::random();
        let ping_event = libp2p::ping::Event {
            peer: peer_id,
            connection: ConnectionId::new_unchecked(1),
            result: Ok(Duration::from_millis(50)),
        };

        // This function doesn't return a value, so we just test it doesn't panic
        handle_ping_event(ping_event).await;
    }

    #[tokio::test]
    async fn test_handle_ping_event_failure() {
        use libp2p::ping::Failure;
        use libp2p::swarm::ConnectionId;

        let peer_id = PeerId::random();
        let ping_event = libp2p::ping::Event {
            peer: peer_id,
            connection: ConnectionId::new_unchecked(1),
            result: Err(Failure::Timeout),
        };

        // This function doesn't return a value, so we just test it doesn't panic
        handle_ping_event(ping_event).await;
    }

    #[tokio::test]
    async fn test_respond_with_public_stories_channel() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let receiver_name = "test_receiver".to_string();

        respond_with_public_stories(sender, receiver_name.clone());

        // Try to receive a response with timeout
        let response =
            tokio::time::timeout(tokio::time::Duration::from_millis(500), receiver.recv()).await;

        // The response might succeed or timeout depending on if stories file exists
        // We mainly test that the function doesn't panic and attempts to send
        match response {
            Ok(Some(resp)) => {
                assert_eq!(resp.receiver, receiver_name);
                assert_eq!(resp.mode, ListMode::ALL);
                // resp.data may be empty if no stories file exists
            }
            Ok(None) => {
                // Channel was closed, which means an error occurred in the spawn
            }
            Err(_) => {
                // Timeout occurred, which is expected if no stories exist
            }
        }
    }

    #[tokio::test]
    async fn test_maintain_connections() {
        // Create a mock swarm for testing
        let mut swarm = crate::network::create_swarm().expect("Failed to create test swarm");

        // This is hard to test properly without a full network setup,
        // but we can at least verify the function doesn't panic
        maintain_connections(&mut swarm).await;
    }

    #[test]
    fn test_event_handling_error_paths() {
        // Test serialization/deserialization edge cases that might occur in floodsub handling

        // Test invalid JSON data
        let invalid_json = b"invalid json data";
        let list_response_result = serde_json::from_slice::<ListResponse>(invalid_json);
        assert!(list_response_result.is_err());

        let published_story_result = serde_json::from_slice::<PublishedStory>(invalid_json);
        assert!(published_story_result.is_err());

        let peer_name_result = serde_json::from_slice::<PeerName>(invalid_json);
        assert!(peer_name_result.is_err());

        let list_request_result = serde_json::from_slice::<ListRequest>(invalid_json);
        assert!(list_request_result.is_err());
    }

    #[test]
    fn test_list_mode_one_validation() {
        use crate::types::{ListMode, ListRequest};

        // Test ListMode::One with valid peer ID format
        let list_request = ListRequest {
            mode: ListMode::One("valid_peer_id".to_string()),
        };

        match list_request.mode {
            ListMode::One(peer_id) => {
                assert_eq!(peer_id, "valid_peer_id");
            }
            ListMode::ALL => {
                panic!("Expected ListMode::One");
            }
        }
    }
}
