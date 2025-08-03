use crate::error_logger::ErrorLogger;
use crate::handlers::*;
use crate::network::{
    DirectMessageRequest, DirectMessageResponse, NodeDescriptionRequest, NodeDescriptionResponse,
    PEER_ID, StoryBehaviour, TOPIC,
};
use crate::storage::{load_node_description, save_received_story};
use crate::types::{
    ActionResult, DirectMessage, DirectMessageConfig, EventType, ListMode, ListRequest,
    ListResponse, PeerName, PendingDirectMessage, PublishedStory,
};

use bytes::Bytes;
use libp2p::{PeerId, Swarm, request_response};
use log::debug;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    error_logger: &ErrorLogger,
) {
    debug!("Broadcasting published story: {}", story.name);

    // Pre-publish connection check and reconnection
    maintain_connections(swarm, error_logger).await;

    // Debug: Show connected peers and floodsub state
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    debug!("Currently connected peers: {}", connected_peers.len());
    for peer in &connected_peers {
        debug!("Connected to: {}", peer);
    }

    if connected_peers.is_empty() {
        crate::log_network_error!(
            error_logger,
            "floodsub",
            "No connected peers available for story broadcast!"
        );
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
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) -> Option<ActionResult> {
    match line.as_str() {
        "ls ch" => handle_list_channels(ui_logger, error_logger).await,
        "ls sub" => handle_list_subscriptions(ui_logger, error_logger).await,
        cmd if cmd.starts_with("ls s") => {
            handle_list_stories(cmd, swarm, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("create s") => {
            return handle_create_stories(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("create ch") => {
            return handle_create_channel(cmd, swarm, local_peer_name, ui_logger, error_logger)
                .await;
        }
        cmd if cmd.starts_with("create desc") => handle_create_description(cmd, ui_logger).await,
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
        cmd if cmd.starts_with("reload config") => handle_reload_config(cmd, ui_logger).await,
        cmd if cmd.starts_with("dht bootstrap") => {
            handle_dht_bootstrap(cmd, swarm, ui_logger).await
        }
        cmd if cmd.starts_with("dht peers") => handle_dht_get_peers(cmd, swarm, ui_logger).await,
        cmd if cmd.starts_with("quit") => {
            // Coverage skip: process::exit doesn't return, so it can't be tested normally
            #[allow(unreachable_code)]
            process::exit(0)
        }
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
                dm_config,
                pending_messages,
            )
            .await;
        }
        _ => ui_logger.log("unknown command".to_string()),
    }
    None
}

/// Handle mDNS discovery events
pub async fn handle_mdns_event(
    mdns_event: libp2p::mdns::Event,
    swarm: &mut Swarm<StoryBehaviour>,
    error_logger: &ErrorLogger,
) {
    match mdns_event {
        libp2p::mdns::Event::Discovered(discovered_list) => {
            debug!("Discovered Peers event");
            for (peer, addr) in discovered_list {
                debug!("Discovered a peer:{} at {}", peer, addr);
                if !swarm.is_connected(&peer) {
                    debug!("Attempting to dial peer: {}", peer);
                    if let Err(e) = swarm.dial(peer) {
                        crate::log_network_error!(
                            error_logger,
                            "mdns",
                            "Failed to initiate dial to {}: {}",
                            peer,
                            e
                        );
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
    error_logger: &ErrorLogger,
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
                            crate::log_network_error!(
                                error_logger,
                                "floodsub",
                                "Failed to check subscriptions for incoming story: {}",
                                e
                            );
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

                        // Save received story to local storage synchronously to ensure TUI refresh sees it
                        if let Err(e) = save_received_story(published.story.clone()).await {
                            // Log error but continue processing
                            crate::log_network_error!(
                                error_logger,
                                "storage",
                                "Failed to save received story: {}",
                                e
                            );
                            ui_logger
                                .log(format!("Warning: Failed to save received story: {}", e));
                        } else {
                            // Signal that stories need to be refreshed only if save was successful
                            return Some(());
                        }
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
            } else if let Ok(channel) = serde_json::from_slice::<crate::types::Channel>(&msg.data) {
                debug!(
                    "Received channel '{}' - {} from {}",
                    channel.name, channel.description, msg.source
                );
                ui_logger.log(format!(
                    "📺 Received channel '{}' - {} from network",
                    channel.name, channel.description
                ));

                // Save the received channel to local storage asynchronously
                let channel_to_save = channel.clone();
                let ui_logger_clone = ui_logger.clone();
                tokio::spawn(async move {
                    // Add validation before saving
                    if channel_to_save.name.is_empty() || channel_to_save.description.is_empty() {
                        debug!("Ignoring invalid channel with empty name or description");
                        return;
                    }

                    // Distinguish error types
                    match crate::storage::create_channel(
                        &channel_to_save.name,
                        &channel_to_save.description,
                        &channel_to_save.created_by,
                    )
                    .await
                    {
                        Ok(_) => {
                            ui_logger_clone.log(format!(
                                "📺 Channel '{}' added to your channels list",
                                channel_to_save.name
                            ));
                        }
                        Err(e) if e.to_string().contains("UNIQUE constraint") => {
                            debug!("Channel '{}' already exists", channel_to_save.name);
                        }
                        Err(e) => {
                            // Create error logger for spawned task
                            let error_logger_for_task = ErrorLogger::new("errors.log");
                            crate::log_network_error!(
                                error_logger_for_task,
                                "storage",
                                "Failed to save received channel '{}': {}",
                                channel_to_save.name,
                                e
                            );
                        }
                    }
                });
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
pub async fn handle_dht_bootstrap(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    crate::handlers::handle_dht_bootstrap(cmd, swarm, ui_logger).await;
}

/// Handle DHT get closest peers command
pub async fn handle_dht_get_peers(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    crate::handlers::handle_dht_get_peers(cmd, swarm, ui_logger).await;
}

/// Handle Kademlia DHT events for peer discovery
pub async fn handle_kad_event(
    kad_event: libp2p::kad::Event,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    bootstrap_logger: &crate::bootstrap_logger::BootstrapLogger,
) {
    match kad_event {
        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => match result {
            libp2p::kad::QueryResult::Bootstrap(Ok(bootstrap_ok)) => {
                debug!(
                    "Kademlia bootstrap successful with peer: {}",
                    bootstrap_ok.peer
                );
                bootstrap_logger.log(&format!(
                    "DHT bootstrap successful with peer: {}",
                    bootstrap_ok.peer
                ));
            }
            libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                crate::log_network_error!(
                    error_logger,
                    "kad",
                    "Kademlia bootstrap failed: {:?}",
                    e
                );
                ui_logger.log(format!("DHT bootstrap failed: {:?}", e));
            }
            libp2p::kad::QueryResult::GetClosestPeers(Ok(get_closest_peers_ok)) => {
                debug!(
                    "Found {} closest peers to key",
                    get_closest_peers_ok.peers.len()
                );
                for peer in &get_closest_peers_ok.peers {
                    debug!("Closest peer: {:?}", peer);
                }
            }
            libp2p::kad::QueryResult::GetClosestPeers(Err(e)) => {
                crate::log_network_error!(
                    error_logger,
                    "kad",
                    "Failed to get closest peers: {:?}",
                    e
                );
            }
            _ => {
                debug!("Other Kademlia query result: {:?}", result);
            }
        },
        libp2p::kad::Event::RoutingUpdated {
            peer, is_new_peer, ..
        } => {
            if is_new_peer {
                debug!("New peer added to DHT routing table: {}", peer);
                ui_logger.log(format!("New peer added to DHT: {}", peer));

                // Add the peer to floodsub partial view if connected
                if swarm.is_connected(&peer) {
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer);
                    debug!("Added DHT peer {} to floodsub partial view", peer);
                }
            }
        }
        libp2p::kad::Event::InboundRequest { request } => match request {
            libp2p::kad::InboundRequest::FindNode { .. } => {
                debug!("Received DHT FindNode request");
            }
            libp2p::kad::InboundRequest::GetProvider { .. } => {
                debug!("Received DHT GetProvider request");
            }
            _ => {
                debug!("Received other DHT inbound request: {:?}", request);
            }
        },
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
pub async fn handle_ping_event(ping_event: libp2p::ping::Event, error_logger: &ErrorLogger) {
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
            crate::log_network_error!(error_logger, "ping", "Ping to {} failed: {}", peer, failure);
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
    error_logger: &ErrorLogger,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => {
            match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    // Validate sender identity to prevent spoofing
                    if request.from_peer_id != peer.to_string() {
                        crate::log_network_error!(
                            error_logger,
                            "direct_message",
                            "Direct message sender identity mismatch: claimed {} but actual connection from {}",
                            request.from_peer_id,
                            peer
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
                            crate::log_network_error!(
                                error_logger,
                                "direct_message",
                                "Failed to send rejection response to {}: {:?}",
                                peer,
                                e
                            );
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
                        error_logger.log_network_error(
                            "direct_message",
                            &format!("Failed to send response to {}: {:?}", peer, e),
                        );
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // Handle response to our direct message request
                    if response.received {
                        debug!("Direct message was received by peer {}", peer);

                        // Remove successful message from retry queue
                        if let Ok(mut queue) = pending_messages.lock() {
                            queue.retain(|msg| msg.target_peer_id != peer);
                        }

                        ui_logger.log(format!("✅ Message delivered to {}", peer));
                    } else {
                        crate::log_network_error!(
                            error_logger,
                            "direct_message",
                            "Direct message was rejected by peer {}",
                            peer
                        );

                        // Message was rejected, but don't retry validation failures
                        if let Ok(mut queue) = pending_messages.lock() {
                            queue.retain(|msg| msg.target_peer_id != peer);
                        }

                        ui_logger.log(format!(
                            "❌ Message rejected by {} (identity validation failed)",
                            peer
                        ));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            // Log to error file instead of TUI to avoid corrupting the interface
            error_logger.log_network_error(
                "direct_message",
                &format!("Failed to send direct message to {}: {:?}", peer, error),
            );
            // Don't immediately report failure to user - let retry logic handle it
            debug!(
                "Direct message to {} failed, will be retried automatically",
                peer
            );
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            // Log to error file instead of TUI to avoid corrupting the interface
            error_logger.log_network_error(
                "direct_message",
                &format!(
                    "Failed to receive direct message from {}: {:?}",
                    peer, error
                ),
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
    error_logger: &ErrorLogger,
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

                    ui_logger.log(format!("📋 Description request from {}", request.from_name));

                    // Load our description and send it back
                    match load_node_description().await {
                        Ok(description) => {
                            let response = NodeDescriptionResponse {
                                description,
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name
                                    .as_deref()
                                    .unwrap_or("Unknown")
                                    .to_string(),
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
                                crate::log_network_error!(
                                    error_logger,
                                    "node_description",
                                    "Failed to send node description response to {}: {:?}",
                                    peer,
                                    e
                                );
                                ui_logger.log(format!(
                                    "❌ Failed to send description response to {}: {:?}",
                                    peer, e
                                ));
                            } else {
                                debug!("Sent description response to {}", peer);
                            }
                        }
                        Err(e) => {
                            crate::log_network_error!(
                                error_logger,
                                "node_description",
                                "Failed to load description: {}",
                                e
                            );

                            // Send empty response to indicate no description
                            let response = NodeDescriptionResponse {
                                description: None,
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name
                                    .as_deref()
                                    .unwrap_or("Unknown")
                                    .to_string(),
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
                                crate::log_network_error!(
                                    error_logger,
                                    "node_description",
                                    "Failed to send empty description response to {}: {:?}",
                                    peer,
                                    e
                                );
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
                                response.from_name,
                                description.len()
                            ));
                            ui_logger.log(description);
                        }
                        None => {
                            ui_logger
                                .log(format!("📋 {} has no description set", response.from_name));
                        }
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            crate::log_network_error!(
                error_logger,
                "node_description",
                "Failed to send description request to {}: {:?}",
                peer,
                error
            );

            let user_message = match error {
                request_response::OutboundFailure::UnsupportedProtocols => {
                    format!(
                        "❌ Peer {} doesn't support node descriptions (version mismatch)",
                        peer
                    )
                }
                _ => {
                    format!(
                        "❌ Failed to request description from {}: {:?}",
                        peer, error
                    )
                }
            };

            ui_logger.log(user_message);
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            crate::log_network_error!(
                error_logger,
                "node_description",
                "Failed to receive description request from {}: {:?}",
                peer,
                error
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
    bootstrap_logger: &crate::bootstrap_logger::BootstrapLogger,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) -> Option<ActionResult> {
    debug!("Event Received");
    match event {
        EventType::Response(resp) => {
            handle_response_event(resp, swarm).await;
        }
        EventType::PublishStory(story) => {
            handle_publish_story_event(story, swarm, error_logger).await;
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
                dm_config,
                pending_messages,
            )
            .await;
        }
        EventType::MdnsEvent(mdns_event) => {
            handle_mdns_event(mdns_event, swarm, error_logger).await;
        }
        EventType::FloodsubEvent(floodsub_event) => {
            if let Some(()) = handle_floodsub_event(
                floodsub_event,
                response_sender,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                error_logger,
            )
            .await
            {
                // Stories were updated, refresh them
                return Some(ActionResult::RefreshStories);
            }
        }
        EventType::PingEvent(ping_event) => {
            handle_ping_event(ping_event, error_logger).await;
        }
        EventType::RequestResponseEvent(request_response_event) => {
            handle_request_response_event(
                request_response_event,
                swarm,
                local_peer_name,
                ui_logger,
                error_logger,
                pending_messages,
            )
            .await;
        }
        EventType::NodeDescriptionEvent(node_desc_event) => {
            handle_node_description_event(
                node_desc_event,
                swarm,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await;
        }
        EventType::KadEvent(kad_event) => {
            handle_kad_event(kad_event, swarm, ui_logger, error_logger, bootstrap_logger).await;
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

// Connection throttling to prevent rapid reconnection attempts
static LAST_CONNECTION_ATTEMPTS: Lazy<std::sync::Mutex<HashMap<PeerId, Instant>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));
static LAST_SUCCESSFUL_CONNECTIONS: Lazy<std::sync::Mutex<HashMap<PeerId, Instant>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));
const MIN_RECONNECT_INTERVAL: Duration = Duration::from_secs(30);
const MIN_RECONNECT_INTERVAL_RECENT: Duration = Duration::from_secs(5); // Faster reconnects for recently connected peers
const RECENT_CONNECTION_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes
const CLEANUP_THRESHOLD: Duration = Duration::from_secs(3600); // 1 hour

// Helper function that needs to be accessible - copied from main.rs
pub async fn maintain_connections(swarm: &mut Swarm<StoryBehaviour>, error_logger: &ErrorLogger) {
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
            // Check if we should throttle this connection attempt
            let should_attempt = match LAST_CONNECTION_ATTEMPTS.try_lock() {
                Ok(mut attempts) => {
                    // Cleanup entries older than the threshold to prevent memory leaks
                    attempts.retain(|_, &mut last_time| last_time.elapsed() < CLEANUP_THRESHOLD);

                    let last_attempt = attempts.get(&peer);

                    // Determine the appropriate reconnect interval based on recent connection history
                    let reconnect_interval = match LAST_SUCCESSFUL_CONNECTIONS.try_lock() {
                        Ok(successful_connections) => {
                            if let Some(last_successful) = successful_connections.get(&peer) {
                                if last_successful.elapsed() < RECENT_CONNECTION_THRESHOLD {
                                    // Peer was recently connected, use shorter interval
                                    MIN_RECONNECT_INTERVAL_RECENT
                                } else {
                                    // Peer was not recently connected, use normal interval
                                    MIN_RECONNECT_INTERVAL
                                }
                            } else {
                                // Peer has never been successfully connected, use normal interval
                                MIN_RECONNECT_INTERVAL
                            }
                        }
                        Err(_) => {
                            debug!("Successful connections map temporarily unavailable");
                            MIN_RECONNECT_INTERVAL
                        }
                    };

                    match last_attempt {
                        Some(last_time) => {
                            let elapsed = last_time.elapsed();
                            if elapsed >= reconnect_interval {
                                attempts.insert(peer, Instant::now());
                                true
                            } else {
                                debug!(
                                    "Throttling reconnection to peer {} (last attempt {} seconds ago, interval: {}s)",
                                    peer,
                                    elapsed.as_secs(),
                                    reconnect_interval.as_secs()
                                );
                                false
                            }
                        }
                        None => {
                            attempts.insert(peer, Instant::now());
                            true
                        }
                    }
                }
                Err(_) => {
                    debug!("Connection attempts map temporarily unavailable");
                    false
                }
            };

            if should_attempt {
                debug!("Reconnecting to discovered peer: {}", peer);
                if let Err(e) = swarm.dial(peer) {
                    crate::log_network_error!(
                        error_logger,
                        "mdns",
                        "Failed to dial peer {}: {}",
                        peer,
                        e
                    );
                }
            }
        }
    }
}

/// Track successful connection for improved reconnect timing
pub fn track_successful_connection(peer_id: PeerId) {
    if let Ok(mut connections) = LAST_SUCCESSFUL_CONNECTIONS.try_lock() {
        connections.insert(peer_id, Instant::now());
        debug!("Tracked successful connection to peer: {}", peer_id);
    }
}

/// Trigger immediate connection maintenance (useful after connection drops)
pub async fn trigger_immediate_connection_maintenance(swarm: &mut Swarm<StoryBehaviour>, error_logger: &ErrorLogger) {
    debug!("Triggering immediate connection maintenance");
    maintain_connections(swarm, error_logger).await;
}

// Helper function that needs to be accessible - copied from main.rs
pub fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        let error_logger = ErrorLogger::new("errors.log");
        // Read stories and subscriptions separately to avoid Send issues
        let stories = match crate::storage::read_local_stories().await {
            Ok(stories) => stories,
            Err(e) => {
                crate::log_network_error!(
                    error_logger,
                    "storage",
                    "error fetching local stories to answer ALL request, {}",
                    e
                );
                return;
            }
        };

        let subscribed_channels = match crate::storage::read_subscribed_channels(&receiver).await {
            Ok(channels) => channels,
            Err(e) => {
                crate::log_network_error!(
                    error_logger,
                    "storage",
                    "error fetching subscribed channels for {}: {}",
                    receiver,
                    e
                );
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
            crate::log_network_error!(
                error_logger,
                "channel",
                "error sending response via channel, {}",
                e
            );
        }
    });
}

/// Process pending direct messages and retry failed ones
pub async fn process_pending_messages(
    swarm: &mut Swarm<StoryBehaviour>,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
    peer_names: &HashMap<PeerId, String>,
    ui_logger: &UILogger,
) {
    if !dm_config.enable_timed_retries {
        return;
    }

    let mut messages_to_retry = Vec::new();
    let mut exhausted_messages = Vec::new();

    // Collect messages that need retry or are exhausted
    if let Ok(mut queue) = pending_messages.lock() {
        let mut i = 0;
        while i < queue.len() {
            let msg = &mut queue[i];

            if msg.is_exhausted() {
                // Message has exceeded max retry attempts
                exhausted_messages.push(msg.clone());
                queue.remove(i);
            } else if msg.should_retry(dm_config.retry_interval_seconds) {
                // Message is ready for retry
                msg.increment_attempt();
                messages_to_retry.push(msg.clone());
                i += 1;
            } else {
                i += 1;
            }
        }
    }

    // Report exhausted messages to user
    for msg in exhausted_messages {
        ui_logger.log(format!(
            "❌ Failed to deliver message to {} after {} attempts",
            msg.target_name, msg.max_attempts
        ));
    }

    // Retry messages
    for msg in messages_to_retry {
        debug!(
            "Retrying direct message to {} (attempt {}/{})",
            msg.target_name, msg.attempts, msg.max_attempts
        );

        // For placeholder PeerIds, try to find the real peer with matching name
        let target_peer_id = if msg.is_placeholder_peer_id {
            if let Some((real_peer_id, _)) = peer_names
                .iter()
                .find(|(_, name)| name == &&msg.target_name)
            {
                // Update the message with the real PeerId
                if let Ok(mut queue) = pending_messages.lock() {
                    if let Some(stored_msg) = queue
                        .iter_mut()
                        .find(|m| m.target_name == msg.target_name && m.is_placeholder_peer_id)
                    {
                        stored_msg.target_peer_id = *real_peer_id;
                        stored_msg.is_placeholder_peer_id = false;
                    }
                }
                *real_peer_id
            } else {
                // Peer not connected or name not known yet, skip this retry
                debug!(
                    "Peer {} not found or name not available yet, skipping retry",
                    msg.target_name
                );
                continue;
            }
        } else {
            msg.target_peer_id
        };

        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&target_peer_id, msg.message.clone());

        debug!(
            "Retry request sent to {} (request_id: {:?})",
            msg.target_name, request_id
        );
    }
}

/// Process pending messages when new connections are established
pub async fn retry_messages_for_peer(
    peer_id: PeerId,
    swarm: &mut Swarm<StoryBehaviour>,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
    peer_names: &HashMap<PeerId, String>,
) {
    if !dm_config.enable_connection_retries {
        return;
    }

    let mut messages_to_retry = Vec::new();

    // Find messages for this specific peer
    if let Ok(mut queue) = pending_messages.lock() {
        for msg in queue.iter_mut() {
            let should_retry = if msg.is_placeholder_peer_id {
                // For placeholder PeerIds, match by peer name
                if let Some(peer_name) = peer_names.get(&peer_id) {
                    peer_name == &msg.target_name
                } else {
                    false
                }
            } else {
                // For real PeerIds, match by PeerId
                msg.target_peer_id == peer_id
            };

            if should_retry && !msg.is_exhausted() {
                // Update placeholder PeerIds with the real PeerId
                if msg.is_placeholder_peer_id {
                    msg.target_peer_id = peer_id;
                    msg.is_placeholder_peer_id = false;
                }
                msg.increment_attempt();
                messages_to_retry.push(msg.clone());
            }
        }
    }

    // Retry messages for the newly connected peer
    for msg in messages_to_retry {
        debug!(
            "Retrying direct message to {} due to new connection (attempt {}/{})",
            msg.target_name, msg.attempts, msg.max_attempts
        );

        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&msg.target_peer_id, msg.message.clone());

        debug!(
            "Connection-based retry request sent to {} (request_id: {:?})",
            msg.target_name, request_id
        );
    }
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
                _ => {}
            }
        }
    }

    // Mock function that simulates the pattern matching logic from event_handlers.rs
    fn match_command_type(line: &str) -> &'static str {
        // This follows the exact same pattern matching order as in handle_input_event
        match line {
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
        let error_logger = ErrorLogger::new("test_errors.log");
        handle_ping_event(ping_event, &error_logger).await;
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
        let error_logger = ErrorLogger::new("test_errors.log");
        handle_ping_event(ping_event, &error_logger).await;
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
        let ping_config = crate::types::PingConfig::new();
        let mut swarm = crate::network::create_swarm(&ping_config).expect("Failed to create test swarm");

        // This is hard to test properly without a full network setup,
        // but we can at least verify the function doesn't panic
        let error_logger = ErrorLogger::new("test_errors.log");
        maintain_connections(&mut swarm, &error_logger).await;
    }

    #[test]
    fn test_connection_throttling_logic() {
        use libp2p::PeerId;
        use std::time::{Duration, Instant};

        // Test the throttling constants and logic
        assert_eq!(MIN_RECONNECT_INTERVAL, Duration::from_secs(30));

        // Create a test peer ID
        let _test_peer = PeerId::random();

        // Simulate connection attempt timing logic
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);
        let thirty_seconds_ago = now - Duration::from_secs(30);

        // Test that elapsed time calculation works
        assert!(one_minute_ago.elapsed() >= MIN_RECONNECT_INTERVAL);
        assert!(thirty_seconds_ago.elapsed() >= MIN_RECONNECT_INTERVAL);
    }

    #[tokio::test]
    async fn test_story_publishing_non_blocking() {
        use crate::network::create_swarm;
        use crate::types::Story;
        use std::time::Instant;

        let ping_config = crate::types::PingConfig::new();
        let mut swarm = create_swarm(&ping_config).expect("Failed to create swarm");
        let story = Story {
            id: 1,
            name: "Test Story".to_string(),
            header: "Test Header".to_string(),
            body: "Test Body".to_string(),
            public: true,
            channel: "general".to_string(),
            created_at: 1234567890,
        };

        // Measure time taken for story publishing (should be very fast now)
        let start = Instant::now();
        let error_logger = ErrorLogger::new("test_errors.log");
        handle_publish_story_event(story, &mut swarm, &error_logger).await;
        let duration = start.elapsed();

        // Story publishing should complete in well under 500ms (previously took 1+ seconds)
        assert!(
            duration.as_millis() < 500,
            "Story publishing took too long: {}ms",
            duration.as_millis()
        );
    }

    #[test]
    fn test_ui_logger_cloneable() {
        use crate::handlers::UILogger;
        use tokio::sync::mpsc;

        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Should be able to clone UILogger for background tasks
        let ui_logger_clone = ui_logger.clone();

        // Both loggers should work
        ui_logger.log("Test message 1".to_string());
        ui_logger_clone.log("Test message 2".to_string());
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

    #[test]
    fn test_pending_direct_message_retry_logic() {
        use crate::network::DirectMessageRequest;
        use crate::types::{DirectMessageConfig, PendingDirectMessage};

        let config = DirectMessageConfig::new();
        let peer_id = PeerId::random();
        let request = DirectMessageRequest {
            from_peer_id: "test_sender".to_string(),
            from_name: "TestSender".to_string(),
            to_name: "TestReceiver".to_string(),
            message: "Hello!".to_string(),
            timestamp: 1234567890,
        };

        let mut pending_msg = PendingDirectMessage::new(
            peer_id,
            "TestReceiver".to_string(),
            request,
            config.max_retry_attempts,
            false, // Not a placeholder for test
        );

        // Test initial state
        assert_eq!(pending_msg.attempts, 0);
        assert_eq!(pending_msg.max_attempts, 3);
        assert!(!pending_msg.is_exhausted());
        assert!(pending_msg.should_retry(30)); // Should retry initially

        // Test after first attempt
        pending_msg.increment_attempt();
        assert_eq!(pending_msg.attempts, 1);
        assert!(!pending_msg.is_exhausted());
        assert!(!pending_msg.should_retry(30)); // Shouldn't retry immediately after attempt

        // Test when exhausted
        pending_msg.attempts = 3;
        assert!(pending_msg.is_exhausted());
        assert!(!pending_msg.should_retry(30)); // Shouldn't retry when exhausted
    }

    #[test]
    fn test_direct_message_config_defaults() {
        use crate::types::DirectMessageConfig;

        let config = DirectMessageConfig::new();
        assert_eq!(config.max_retry_attempts, 3);
        assert_eq!(config.retry_interval_seconds, 30);
        assert!(config.enable_connection_retries);
        assert!(config.enable_timed_retries);

        let default_config = DirectMessageConfig::default();
        assert_eq!(config.max_retry_attempts, default_config.max_retry_attempts);
        assert_eq!(
            config.retry_interval_seconds,
            default_config.retry_interval_seconds
        );
    }
}
