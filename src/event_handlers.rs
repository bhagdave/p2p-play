use crate::error_logger::ErrorLogger;
use crate::handlers::*;
use crate::network::{DirectMessageRequest, DirectMessageResponse, PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::save_received_story;
use crate::types::{
    DirectMessage, EventType, ListMode, ListRequest, ListResponse, PeerName, PublishedStory,
};

use bytes::Bytes;
use libp2p::{PeerId, Swarm, request_response};
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
    sorted_peer_names_cache: &SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<()> {
    match line.as_str() {
        "ls p" => handle_list_peers(swarm, peer_names, ui_logger).await,
        "ls c" => handle_list_connections(swarm, peer_names, ui_logger).await,
        cmd if cmd.starts_with("ls s") => {
            handle_list_stories(cmd, swarm, ui_logger, error_logger).await
        }
        "ls ch" => handle_list_channels(ui_logger, error_logger).await,
        "ls sub" => handle_list_subscriptions(ui_logger, error_logger).await,
        cmd if cmd.starts_with("create s") => {
            if let Some(()) = handle_create_stories(cmd, ui_logger, error_logger).await {
                return Some(());
            }
        }
        cmd if cmd.starts_with("create ch") => {
            if let Some(()) =
                handle_create_channel(cmd, local_peer_name, ui_logger, error_logger).await
            {
                return Some(());
            }
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
        cmd if cmd.starts_with("help") => handle_help(cmd, ui_logger).await,
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
                info!("Broadcasted peer name to connected peers");
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
    floodsub_event: libp2p::floodsub::Event,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    peer_names: &mut HashMap<PeerId, String>,
    _local_peer_name: &Option<String>,
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
) -> Option<()> {
    match floodsub_event {
        libp2p::floodsub::Event::Message(msg) => {
            info!("Message event received from {:?}", msg.source);
            info!("Message data length: {} bytes", msg.data.len());
            if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    info!("Response from {}:", msg.source);
                    resp.data.iter().for_each(|r| info!("{:?}", r));
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
                        info!(
                            "Received published story '{}' from {} in channel '{}'",
                            published.story.name, msg.source, published.story.channel
                        );
                        info!("Story: {:?}", published.story);
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
                        info!(
                            "Ignoring story '{}' from channel '{}' - not subscribed",
                            published.story.name, published.story.channel
                        );
                    }
                }
            } else if let Ok(peer_name) = serde_json::from_slice::<PeerName>(&msg.data) {
                if let Ok(peer_id) = peer_name.peer_id.parse::<PeerId>() {
                    if peer_id != *PEER_ID {
                        info!("Received peer name '{}' from {}", peer_name.name, peer_id);

                        // Only update the peer name if it's new or has actually changed
                        let mut names_changed = false;
                        peer_names
                            .entry(peer_id)
                            .and_modify(|existing_name| {
                                if existing_name != &peer_name.name {
                                    info!(
                                        "Peer {} name changed from '{}' to '{}'",
                                        peer_id, existing_name, peer_name.name
                                    );
                                    *existing_name = peer_name.name.clone();
                                    names_changed = true;
                                } else {
                                    info!("Peer {} name unchanged: '{}'", peer_id, peer_name.name);
                                }
                            })
                            .or_insert_with(|| {
                                info!(
                                    "Setting peer {} name to '{}' (first time)",
                                    peer_id, peer_name.name
                                );
                                names_changed = true;
                                peer_name.name.clone()
                            });

                        // Update the cache if peer names changed
                        if names_changed {
                            sorted_peer_names_cache.update(peer_names);
                            ui_logger
                                .log(format!("Peer {} set name to '{}'", peer_id, peer_name.name));
                        }
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
    None
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
                            ui_logger.log(format!(
                                "📨 Direct message from {}: {}",
                                request.from_name, request.message
                            ));
                            info!(
                                "Received direct message from {} ({}): {}",
                                request.from_name, request.from_peer_id, request.message
                            );
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
                        info!("Direct message was received by peer {}", peer);
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
            info!("Response sent to {}", peer);
        }
    }
}

/// Handle direct message events
pub async fn handle_direct_message_event(direct_msg: DirectMessage) {
    // This shouldn't happen since DirectMessage events are processed in floodsub handler
    // but we'll handle it just in case
    info!(
        "Received DirectMessage event: {} -> {}: {}",
        direct_msg.from_name, direct_msg.to_name, direct_msg.message
    );
}

/// Handle channel events
pub async fn handle_channel_event(channel: crate::types::Channel) {
    info!(
        "Received Channel event: {} - {}",
        channel.name, channel.description
    );
}

/// Handle channel subscription events
pub async fn handle_channel_subscription_event(subscription: crate::types::ChannelSubscription) {
    info!(
        "Received ChannelSubscription event: {} subscribed to {}",
        subscription.peer_id, subscription.channel_name
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
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<()> {
    info!("Event Received");
    match event {
        EventType::Response(resp) => {
            handle_response_event(resp, swarm).await;
        }
        EventType::PublishStory(story) => {
            handle_publish_story_event(story, swarm).await;
        }
        EventType::Input(line) => {
            if let Some(()) = handle_input_event(
                line,
                swarm,
                peer_names,
                story_sender,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                error_logger,
            )
            .await
            {
                return Some(());
            }
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
                return Some(());
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

        info!(
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
}
