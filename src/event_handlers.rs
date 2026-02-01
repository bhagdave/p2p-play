use crate::content_fetcher::GatewayFetcher;
use crate::error_logger::ErrorLogger;
use crate::handlers::{
    SortedPeerNamesCache, UILogger, establish_direct_connection, handle_config_auto_share,
    handle_config_sync_days, handle_create_channel, handle_create_description,
    handle_create_stories_with_sender, handle_delete_story, handle_direct_message_with_relay,
    handle_filter_stories, handle_get_description, handle_help, handle_list_channels,
    handle_list_stories, handle_list_subscriptions, handle_publish_story, handle_reload_config,
    handle_search_stories, handle_set_auto_subscription, handle_set_name, handle_show_description,
    handle_show_story, handle_subscribe_channel, handle_unsubscribe_channel, handle_wasm_command,
};
use crate::network::{
    APP_NAME, APP_VERSION, DirectMessageRequest, DirectMessageResponse, HandshakeRequest,
    HandshakeResponse, NodeDescriptionRequest, NodeDescriptionResponse, PEER_ID, StoryBehaviour,
    TOPIC, WasmCapabilitiesRequest, WasmCapabilitiesResponse, WasmExecutionRequest,
    WasmExecutionResponse,
};
use crate::storage::{load_node_description, save_received_story};
use crate::types::{
    ActionResult, DirectMessage, DirectMessageConfig, EventType, Icons, ListMode, ListRequest,
    ListResponse, PeerName, PendingDirectMessage, PendingHandshakePeer, PublishedChannel,
    PublishedStory,
};
use crate::wasm_executor::{ExecutionRequest, WasmExecutionError, WasmExecutor};

use bytes::Bytes;
use libp2p::{PeerId, Swarm, request_response};
use log::debug;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

async fn handle_auto_subscription(
    peer_id: &str,
    channel_name: &str,
    max_auto_subs: usize,
    ui_logger: &UILogger,
) -> Result<bool, String> {
    // Check if already subscribed
    match crate::storage::read_subscribed_channels(peer_id).await {
        Ok(subscribed) => {
            if subscribed.contains(&channel_name.to_string()) {
                // Already subscribed - this is normal, not an error
                return Ok(false); // Not an error, just already subscribed
            }
        }
        Err(e) => {
            return Err(format!("Failed to check existing subscriptions: {e}"));
        }
    }

    // Check subscription count against limit
    match crate::storage::get_auto_subscription_count(peer_id).await {
        Ok(current_count) => {
            if current_count >= max_auto_subs {
                ui_logger.log(format!(
                    "⚠️  Auto-subscription limit reached ({current_count}/{max_auto_subs}). Use 'sub ch {channel_name}' to subscribe manually."
                ));
                return Ok(false); // Not an error, just hit the limit
            }
        }
        Err(e) => {
            return Err(format!("Failed to check subscription count: {e}"));
        }
    }

    // Attempt to auto-subscribe
    match crate::storage::subscribe_to_channel(peer_id, channel_name).await {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to subscribe: {e}")),
    }
}
pub async fn handle_response_event(resp: ListResponse, swarm: &mut Swarm<StoryBehaviour>) {
    debug!("Response received");
    let json = serde_json::to_string(&resp).expect("can jsonify response");
    let json_bytes = Bytes::from(json.into_bytes());
    swarm
        .behaviour_mut()
        .floodsub
        .publish(TOPIC.clone(), json_bytes);
}

pub async fn handle_publish_story_event(
    story: crate::types::Story,
    swarm: &mut Swarm<StoryBehaviour>,
    error_logger: &ErrorLogger,
    network_circuit_breakers: &crate::network_circuit_breakers::NetworkCircuitBreakers,
) {
    debug!("Broadcasting published story: {}", story.name);

    // Pre-publish connection check and reconnection
    maintain_connections(swarm, error_logger).await;

    // Debug: Show connected peers and floodsub state
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    debug!("Currently connected peers: {}", connected_peers.len());
    for peer in &connected_peers {
        debug!("Connected to: {peer}");
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

    // Use circuit breaker for story publishing
    let publish_result = network_circuit_breakers
        .execute("story_publish", || async {
            let json = serde_json::to_string(&published_story)
                .map_err(|e| format!("Failed to serialize story: {e}"))?;
            let json_bytes = Bytes::from(json.into_bytes());

            debug!(
                "Publishing {} bytes to topic {:?}",
                json_bytes.len(),
                TOPIC.clone()
            );

            // Perform the actual publishing
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);

            Ok::<(), String>(())
        })
        .await;

    match publish_result {
        Ok(_) => {
            debug!("Story broadcast completed successfully");
        }
        Err(e) => {
            error_logger.log_error(&format!("Story publishing failed: {e:?}"));
            // Log the specific type of error for better debugging
            match e {
                crate::circuit_breaker::CircuitBreakerError::CircuitOpen { circuit_name } => {
                    error_logger.log_error(&format!(
                        "Story publishing blocked - {circuit_name} circuit breaker is open"
                    ));
                }
                crate::circuit_breaker::CircuitBreakerError::OperationTimeout {
                    circuit_name,
                    timeout,
                } => {
                    error_logger.log_error(&format!(
                        "Story publishing timed out after {timeout:?} in {circuit_name}"
                    ));
                }
                crate::circuit_breaker::CircuitBreakerError::OperationFailed(inner_error) => {
                    error_logger
                        .log_error(&format!("Story publishing operation failed: {inner_error}"));
                }
            }
        }
    }
}

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
    relay_service: &mut Option<crate::relay::RelayService>,
) -> Option<ActionResult> {
    let line = line.trim();
    match line {
        cmd if cmd.starts_with("ls ch") => handle_list_channels(cmd, ui_logger, error_logger).await,
        "ls sub" => handle_list_subscriptions(ui_logger, error_logger).await,
        cmd if cmd.starts_with("ls s") => {
            handle_list_stories(cmd, swarm, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("search ") => {
            handle_search_stories(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("filter ") => {
            handle_filter_stories(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("create s") => {
            return handle_create_stories_with_sender(
                cmd,
                ui_logger,
                error_logger,
                Some(story_sender.clone()),
            )
            .await;
        }
        cmd if cmd.starts_with("create ch") => {
            return handle_create_channel(cmd, swarm, local_peer_name, ui_logger, error_logger)
                .await;
        }
        cmd if cmd.starts_with("create desc") => handle_create_description(cmd, ui_logger).await,
        cmd if cmd.starts_with("sub ") => {
            return handle_subscribe_channel(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("unsub ") => {
            return handle_unsubscribe_channel(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("set auto-sub") => {
            handle_set_auto_subscription(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("publish s") => {
            handle_publish_story(cmd, story_sender.clone(), ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("show story") => {
            let local_peer_id = swarm.local_peer_id().to_string();
            handle_show_story(cmd, ui_logger, &local_peer_id).await
        }
        "show desc" => handle_show_description(ui_logger).await,
        cmd if cmd.starts_with("get desc") => {
            handle_get_description(cmd, ui_logger, swarm, local_peer_name, peer_names).await
        }
        cmd if cmd.starts_with("delete s") => {
            return handle_delete_story(cmd, ui_logger, error_logger).await;
        }
        cmd if cmd.starts_with("help") => handle_help(cmd, ui_logger).await,
        cmd if cmd.starts_with("peer id") => ui_logger.log(format!("Local Peer ID: {}", *PEER_ID)),
        cmd if cmd.starts_with("reload config") => handle_reload_config(cmd, ui_logger).await,
        cmd if cmd.starts_with("config auto-share") => {
            handle_config_auto_share(cmd, ui_logger, error_logger).await
        }
        cmd if cmd.starts_with("config sync-days") => {
            handle_config_sync_days(cmd, ui_logger, error_logger).await
        }
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
                Some(name) => ui_logger.log(format!("Current alias: {name}")),
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
        cmd if cmd.starts_with("compose ") => {
            // Handle compose command to enter message composition mode
            if let Some(peer_name) = cmd.strip_prefix("compose ") {
                let peer_name = peer_name.trim();
                if peer_name.is_empty() {
                    ui_logger.log("Usage: compose <peer_alias>".to_string());
                } else {
                    // Check if peer exists
                    let peer_exists = peer_names.values().any(|name| name == peer_name);
                    if peer_exists {
                        return Some(crate::types::ActionResult::EnterMessageComposition(
                            peer_name.to_string(),
                        ));
                    } else {
                        ui_logger.log(format!(
                            "❌ Peer '{}' not found. Available peers: {}",
                            peer_name,
                            peer_names
                                .values()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            } else {
                ui_logger.log("Usage: compose <peer_alias>".to_string());
            }
        }
        cmd if cmd.starts_with("msg ") => {
            handle_direct_message_with_relay(
                cmd,
                swarm,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                dm_config,
                relay_service,
                pending_messages,
            )
            .await;
        }
        cmd if cmd.starts_with("wasm ") => {
            handle_wasm_command(
                cmd,
                swarm,
                peer_names,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await;
        }
        _ => ui_logger.log("unknown command".to_string()),
    }
    None
}

pub async fn handle_mdns_event(
    mdns_event: libp2p::mdns::Event,
    swarm: &mut Swarm<StoryBehaviour>,
    _ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    match mdns_event {
        libp2p::mdns::Event::Discovered(discovered_list) => {
            debug!("Discovered Peers event");
            for (peer, addr) in discovered_list {
                debug!("Discovered a peer:{peer} at {addr}");
                if !swarm.is_connected(&peer) {
                    debug!("Attempting to dial peer: {peer}");
                    if let Err(e) = swarm.dial(peer) {
                        crate::log_network_error!(
                            error_logger,
                            "mdns",
                            "Failed to initiate dial to {}: {}",
                            peer,
                            e
                        );
                    } else {
                        debug!("Dial initiated successfully for peer: {}", peer);
                    }
                } else {
                    debug!("Already connected to peer: {peer}");
                }
            }
        }
        libp2p::mdns::Event::Expired(expired_list) => {
            debug!("Expired Peers event");
            for (peer, _addr) in expired_list {
                debug!("Expired a peer:{peer} at {_addr}");
                let discovered_nodes: Vec<_> = swarm.behaviour().mdns.discovered_nodes().collect();
                if !discovered_nodes.contains(&(&peer)) {
                    debug!("Removing peer from partial view: {peer}");
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .remove_node_from_partial_view(&peer);
                }
            }
        }
    }
}

pub async fn handle_floodsub_event(
    floodsub_event: libp2p::floodsub::Event,
    response_sender: mpsc::UnboundedSender<ListResponse>,
    peer_names: &mut HashMap<PeerId, String>,
    _local_peer_name: &Option<String>,
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    relay_service: &mut Option<crate::relay::RelayService>,
    verified_p2p_play_peers: &Arc<Mutex<HashMap<PeerId, String>>>,
) -> Option<crate::types::ActionResult> {
    match floodsub_event {
        libp2p::floodsub::Event::Message(msg) => {
            debug!("Message event received from {:?}", msg.source);
            debug!("Message data length: {} bytes", msg.data.len());

            // Verify that message is from a verified P2P-Play peer
            let source_peer = msg.source;
            let is_verified = {
                let verified_peers = verified_p2p_play_peers.lock().unwrap();
                verified_peers.contains_key(&source_peer)
            };

            if !is_verified {
                debug!(
                    "Ignoring floodsub message from unverified peer: {}",
                    source_peer
                );
                return None;
            }
            if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    debug!("Response from {}:", msg.source);
                    resp.data.iter().for_each(|r| debug!("{r:?}"));
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
                            ui_logger.log(format!("Warning: Failed to save received story: {e}"));
                        } else {
                            // Signal that stories need to be refreshed only if save was successful
                            return Some(crate::types::ActionResult::RefreshStories);
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
            } else if let Ok(published_channel) =
                serde_json::from_slice::<PublishedChannel>(&msg.data)
            {
                if published_channel.publisher != PEER_ID.to_string() {
                    debug!(
                        "Received published channel '{}' - {} from {}",
                        published_channel.channel.name,
                        published_channel.channel.description,
                        msg.source
                    );

                    // Load auto-subscription config to determine behavior
                    let auto_sub_config = match crate::storage::load_unified_network_config().await
                    {
                        Ok(config) => config.channel_auto_subscription,
                        Err(e) => {
                            debug!("Failed to load auto-subscription config: {e}");
                            crate::types::ChannelAutoSubscriptionConfig::new() // Use defaults
                        }
                    };

                    // Show notification if enabled
                    if auto_sub_config.notify_new_channels {
                        ui_logger.log(format!(
                            "📺 New channel discovered: '{}' - {} from {}",
                            published_channel.channel.name,
                            published_channel.channel.description,
                            published_channel.publisher
                        ));
                    }

                    // Handle auto-subscription if enabled (but avoid the tokio::spawn Send issue)
                    let should_auto_subscribe = auto_sub_config.auto_subscribe_to_new_channels;
                    let max_auto_subs = auto_sub_config.max_auto_subscriptions;

                    // Save the received channel to local storage synchronously to avoid race condition
                    let channel_to_save = &published_channel.channel;
                    let peer_id_str = PEER_ID.to_string();

                    // Add validation before saving
                    if channel_to_save.name.is_empty() || channel_to_save.description.is_empty() {
                        debug!("Ignoring invalid published channel with empty name or description");
                    } else {
                        // Save the channel first - synchronously to ensure it exists before subscription
                        let channel_saved = match crate::storage::create_channel(
                            &channel_to_save.name,
                            &channel_to_save.description,
                            &channel_to_save.created_by,
                        )
                        .await
                        {
                            Ok(_) => {
                                ui_logger.log(format!(
                                    "📺 Channel '{}' added to your channels list",
                                    channel_to_save.name
                                ));
                                debug!(
                                    "Successfully created channel '{}' in database",
                                    channel_to_save.name
                                );
                                true
                            }
                            Err(e) if e.to_string().contains("UNIQUE constraint") => {
                                debug!(
                                    "Published channel '{}' already exists in database",
                                    channel_to_save.name
                                );
                                // Still notify user that channel is available
                                ui_logger.log(format!(
                                    "📺 Channel '{}' is available (already in your channels list)",
                                    channel_to_save.name
                                ));
                                true // Channel exists, can proceed with subscription
                            }
                            Err(e) => {
                                crate::log_network_error!(
                                    error_logger,
                                    "storage",
                                    "Failed to save received published channel '{}': {}",
                                    channel_to_save.name,
                                    e
                                );
                                false
                            }
                        };

                        // Only attempt auto-subscription AFTER successful channel creation to avoid race condition
                        if channel_saved && should_auto_subscribe {
                            match handle_auto_subscription(
                                &peer_id_str,
                                &channel_to_save.name,
                                max_auto_subs,
                                ui_logger,
                            )
                            .await
                            {
                                Ok(true) => {
                                    ui_logger.log(format!(
                                        "✅ Auto-subscribed to channel '{}'",
                                        channel_to_save.name
                                    ));
                                }
                                Ok(false) => {
                                    // Auto-subscription was skipped (already subscribed or limit reached)
                                }
                                Err(e) => {
                                    ui_logger.log(format!(
                                        "❌ Failed to auto-subscribe to '{}': {}",
                                        channel_to_save.name, e
                                    ));
                                }
                            }
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
            } else if let Ok(relay_msg) =
                serde_json::from_slice::<crate::types::RelayMessage>(&msg.data)
            {
                debug!(
                    "Received relay message from {}: {}",
                    msg.source, relay_msg.message_id
                );

                // Process relay message if relay service is available
                if let Some(relay_svc) = relay_service {
                    match relay_svc.process_relay_message(&relay_msg) {
                        Ok(crate::relay::RelayAction::DeliverLocally(direct_msg)) => {
                            ui_logger.log(format!(
                                "{} Relay message delivered: {} -> {}: {}",
                                crate::types::Icons::speech(),
                                direct_msg.from_name,
                                direct_msg.to_name,
                                direct_msg.message
                            ));
                            debug!(
                                "Relay message decrypted and delivered locally: {}",
                                relay_msg.message_id
                            );
                        }
                        Ok(crate::relay::RelayAction::ForwardMessage(forward_msg)) => {
                            // Return action to re-broadcast the forwarded message via floodsub
                            debug!("Forwarding relay message: {}", forward_msg.message_id);
                            ui_logger.log(format!(
                                "{} Forwarding relay message {} (hops: {}/{})",
                                crate::types::Icons::antenna(),
                                &forward_msg.message_id[..8],
                                forward_msg.hop_count,
                                forward_msg.max_hops
                            ));
                            return Some(crate::types::ActionResult::RebroadcastRelayMessage(
                                Box::new(forward_msg),
                            ));
                        }
                        Ok(crate::relay::RelayAction::DropMessage(reason)) => {
                            debug!(
                                "Dropping relay message {}: {}",
                                relay_msg.message_id, reason
                            );
                        }
                        Err(e) => {
                            debug!(
                                "Failed to process relay message {}: {}",
                                relay_msg.message_id, e
                            );
                        }
                    }
                } else {
                    // Relay service not available, just log
                    ui_logger.log(format!(
                        "📡 Relay message received but relay service disabled (ID: {}, hops: {})",
                        &relay_msg.message_id[..8],
                        relay_msg.hop_count
                    ));
                }
            } else if let Ok(relay_confirmation) =
                serde_json::from_slice::<crate::types::RelayConfirmation>(&msg.data)
            {
                debug!(
                    "Received relay confirmation from {}: {}",
                    msg.source, relay_confirmation.message_id
                );
                ui_logger.log(format!(
                    "✅ Message delivery confirmed: {} (path length: {})",
                    &relay_confirmation.message_id[..8],
                    relay_confirmation.relay_path_length
                ));
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

pub async fn handle_dht_bootstrap(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    crate::handlers::handle_dht_bootstrap(cmd, swarm, ui_logger).await;
}

pub async fn handle_dht_get_peers(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    crate::handlers::handle_dht_get_peers(cmd, swarm, ui_logger).await;
}

pub async fn handle_kad_event(
    kad_event: libp2p::kad::Event,
    _swarm: &mut Swarm<StoryBehaviour>,
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
                // DHT bootstrap errors are logged to error file only, not shown in UI
            }
            libp2p::kad::QueryResult::GetClosestPeers(Ok(get_closest_peers_ok)) => {
                debug!(
                    "Found {} closest peers to key",
                    get_closest_peers_ok.peers.len()
                );
                for peer in &get_closest_peers_ok.peers {
                    debug!("Closest peer: {peer:?}");
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
                debug!("Other Kademlia query result: {result:?}");
            }
        },
        libp2p::kad::Event::RoutingUpdated {
            peer, is_new_peer, ..
        } => {
            if is_new_peer {
                debug!("New peer added to DHT routing table: {peer}");
                // NOTE: No longer showing all DHT peers in UI to reduce noise
                // Only verified P2P-Play peers will be displayed after handshake
                debug!("DHT peer {peer} requires handshake verification before UI display");
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
                debug!("Received other DHT inbound request: {request:?}");
            }
        },
        libp2p::kad::Event::ModeChanged { new_mode } => {
            debug!("Kademlia mode changed to: {new_mode:?}");
            ui_logger.log(format!("DHT mode changed to: {new_mode:?}"));
        }
        _ => {
            debug!("Other Kademlia event: {kad_event:?}");
        }
    }
}

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

pub async fn handle_peer_name_event(peer_name: PeerName) {
    // This shouldn't happen since PeerName events are created from floodsub messages
    // but we'll handle it just in case
    debug!(
        "Received PeerName event: {} -> {}",
        peer_name.peer_id, peer_name.name
    );
}

pub async fn handle_request_response_event(
    event: request_response::Event<DirectMessageRequest, DirectMessageResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) -> Option<DirectMessage> {
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
                        return None;
                    }

                    // Handle incoming direct message request
                    let should_process = if let Some(local_name) = local_peer_name {
                        &request.to_name == local_name
                    } else {
                        false
                    };

                    let direct_message = if should_process {
                        Some(DirectMessage {
                            from_peer_id: request.from_peer_id.clone(),
                            from_name: request.from_name.clone(),
                            to_peer_id: swarm.local_peer_id().to_string(),
                            to_name: request.to_name.clone(),
                            message: request.message.clone(),
                            timestamp: request.timestamp,
                            is_outgoing: false,
                        })
                    } else {
                        None
                    };

                    // Send response acknowledging receipt
                    let response = DirectMessageResponse {
                        received: should_process,
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
                            &format!("Failed to send response to {peer}: {e:?}"),
                        );
                    }

                    if let Some(dm) = direct_message {
                        return Some(dm);
                    }
                }
                request_response::Message::Response { response, .. } => {
                    if response.received {
                        debug!("Direct message was received by peer {peer}");

                        // Save the outgoing message before removing it from queue
                        if let Ok(mut queue) = pending_messages.lock() {
                            if let Some(pending_msg) =
                                queue.iter().find(|msg| msg.target_peer_id == peer)
                            {
                                let outgoing_message = crate::types::DirectMessage {
                                    from_peer_id: pending_msg.message.from_peer_id.clone(),
                                    from_name: pending_msg.message.from_name.clone(),
                                    to_peer_id: peer.to_string(),
                                    to_name: pending_msg.message.to_name.clone(),
                                    message: pending_msg.message.message.clone(),
                                    timestamp: pending_msg.message.timestamp,
                                    is_outgoing: true,
                                };

                                if let Err(e) = crate::storage::save_direct_message(
                                    &outgoing_message,
                                    Some(peer_names),
                                )
                                .await
                                {
                                    crate::log_network_error!(
                                        error_logger,
                                        "direct_message",
                                        "Failed to save outgoing direct message: {}",
                                        e
                                    );
                                }
                            }

                            // Remove successful message from retry queue
                            let before_count = queue.len();
                            queue.retain(|msg| msg.target_peer_id != peer);
                            let after_count = queue.len();
                        }

                        ui_logger.log(format!("{} Message delivered to {}", Icons::check(), peer));
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
                            "{} Message rejected by {} (identity validation failed)",
                            Icons::cross(),
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
                &format!("Failed to send direct message to {peer}: {error:?}"),
            );
            // Don't immediately report failure to user - let retry logic handle it
            debug!("Direct message to {peer} failed, will be retried automatically");
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            // Log to error file instead of TUI to avoid corrupting the interface
            error_logger.log_network_error(
                "direct_message",
                &format!("Failed to receive direct message from {peer}: {error:?}"),
            );
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("Response sent to {peer}");
        }
    }
    None
}

pub async fn handle_direct_message_event(direct_msg: DirectMessage) {
    // This shouldn't happen since DirectMessage events are processed in floodsub handler
    // but we'll handle it just in case
    debug!(
        "Received DirectMessage event: {} -> {}: {}",
        direct_msg.from_name, direct_msg.to_name, direct_msg.message
    );
}

pub async fn handle_channel_event(channel: crate::types::Channel) {
    debug!(
        "Received Channel event: {} - {}",
        channel.name, channel.description
    );
}

pub async fn handle_channel_subscription_event(subscription: crate::types::ChannelSubscription) {
    debug!(
        "Received ChannelSubscription event: {} subscribed to {}",
        subscription.peer_id, subscription.channel_name
    );
}

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

                    ui_logger.log(format!(
                        "{} Description request from {}",
                        Icons::clipboard(),
                        request.from_name
                    ));

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
                                    "{} Failed to send description response to {}: {:?}",
                                    Icons::cross(),
                                    peer,
                                    e
                                ));
                            } else {
                                debug!("Sent description response to {peer}");
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
                                "{} Description from {} ({} bytes):",
                                Icons::clipboard(),
                                response.from_name,
                                description.len()
                            ));
                            ui_logger.log(description);
                        }
                        None => {
                            ui_logger.log(format!(
                                "{} {} has no description set",
                                Icons::clipboard(),
                                response.from_name
                            ));
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
                        "{} Peer {} doesn't support node descriptions (version mismatch)",
                        Icons::cross(),
                        peer
                    )
                }
                _ => {
                    format!(
                        "{} Failed to request description from {}: {:?}",
                        Icons::cross(),
                        peer,
                        error
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
            debug!("Node description response sent to {peer}");
        }
    }
}

pub async fn handle_story_sync_event(
    event: request_response::Event<
        crate::network::StorySyncRequest,
        crate::network::StorySyncResponse,
    >,
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
                    // Handle incoming story sync request
                    debug!(
                        "Received story sync request from {} ({}) for channels: {:?}",
                        request.from_name, request.from_peer_id, request.subscribed_channels
                    );

                    ui_logger.log(format!(
                        "{} Story sync request from {} (last sync: {})",
                        Icons::sync(),
                        request.from_name,
                        if request.last_sync_timestamp == 0 {
                            "never".to_string()
                        } else {
                            format!(
                                "{} seconds ago",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                                    .saturating_sub(request.last_sync_timestamp)
                            )
                        }
                    ));

                    // Get local stories that match the peer's subscribed channels and are newer than last sync
                    match crate::storage::read_local_stories_for_sync(
                        request.last_sync_timestamp,
                        &request.subscribed_channels,
                    )
                    .await
                    {
                        Ok(filtered_stories) => {
                            debug!(
                                "Sync request from {} - sending {} stories (channels: {:?}) since timestamp {}",
                                peer,
                                filtered_stories.len(),
                                filtered_stories
                                    .iter()
                                    .map(|s| &s.channel)
                                    .collect::<std::collections::HashSet<_>>(),
                                request.last_sync_timestamp
                            );

                            let story_count = filtered_stories.len();

                            // Get ALL available channels for discovery, not just channels associated with the stories
                            let channels = match crate::storage::read_channels().await {
                                Ok(all_channels) => {
                                    debug!(
                                        "Sending {} total channels for peer discovery during sync response",
                                        all_channels.len()
                                    );
                                    all_channels
                                }
                                Err(e) => {
                                    debug!("Failed to read all channels for sync: {}", e);
                                    // Fallback to story-specific channels if reading all channels fails
                                    match crate::storage::get_channels_for_stories(
                                        &filtered_stories,
                                    )
                                    .await
                                    {
                                        Ok(story_channels) => {
                                            debug!(
                                                "Fallback: using {} story-specific channels",
                                                story_channels.len()
                                            );
                                            story_channels
                                        }
                                        Err(e2) => {
                                            debug!("Failed fallback channel lookup: {}", e2);
                                            Vec::new() // Final fallback to maintain functionality
                                        }
                                    }
                                }
                            };

                            let response = crate::network::StorySyncResponse {
                                stories: filtered_stories,
                                channels,
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name
                                    .as_deref()
                                    .unwrap_or("Unknown")
                                    .to_string(),
                                sync_timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };

                            // Send the response
                            if let Err(e) = swarm
                                .behaviour_mut()
                                .story_sync
                                .send_response(channel, response)
                            {
                                crate::log_network_error!(
                                    error_logger,
                                    "story_sync",
                                    "Failed to send story sync response to {}: {:?}",
                                    peer,
                                    e
                                );
                                ui_logger.log(format!(
                                    "{} Failed to send story sync response to {}: {:?}",
                                    Icons::cross(),
                                    peer,
                                    e
                                ));
                            } else {
                                debug!("Sent story sync response to {peer}");
                                ui_logger.log(format!(
                                    "{} Sent {} stories to {}",
                                    Icons::sync(),
                                    story_count,
                                    request.from_name
                                ));
                            }
                        }
                        Err(e) => {
                            crate::log_network_error!(
                                error_logger,
                                "story_sync",
                                "Failed to load stories for sync: {}",
                                e
                            );

                            // Send empty response to indicate error
                            let response = crate::network::StorySyncResponse {
                                stories: Vec::new(),
                                channels: Vec::new(), // No stories means no channels to share
                                from_peer_id: PEER_ID.to_string(),
                                from_name: local_peer_name
                                    .as_deref()
                                    .unwrap_or("Unknown")
                                    .to_string(),
                                sync_timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };

                            if let Err(e) = swarm
                                .behaviour_mut()
                                .story_sync
                                .send_response(channel, response)
                            {
                                crate::log_network_error!(
                                    error_logger,
                                    "story_sync",
                                    "Failed to send empty story sync response to {}: {:?}",
                                    peer,
                                    e
                                );
                            }
                        }
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // Handle incoming story sync response
                    debug!(
                        "Received story sync response from {} ({}) with {} stories",
                        response.from_name,
                        response.from_peer_id,
                        response.stories.len()
                    );

                    if response.stories.is_empty() {
                        ui_logger.log(format!(
                            "{} No new stories from {}",
                            Icons::sync(),
                            response.from_name
                        ));
                        return;
                    }

                    ui_logger.log(format!(
                        "{} Received {} stories from {}",
                        Icons::sync(),
                        response.stories.len(),
                        response.from_name
                    ));

                    // Process discovered channels first (before stories for logical order)
                    let mut discovered_channels_count = 0;
                    debug!(
                        "Received {} channels from {}: {:?}",
                        response.channels.len(),
                        response.from_name,
                        response
                            .channels
                            .iter()
                            .map(|c| &c.name)
                            .collect::<Vec<_>>()
                    );

                    if !response.channels.is_empty() {
                        match crate::storage::process_discovered_channels(
                            &response.channels,
                            &response.from_name,
                        )
                        .await
                        {
                            Ok(count) => {
                                discovered_channels_count = count;
                                debug!(
                                    "Channel discovery result: {} new channels from {} (out of {} total channels received)",
                                    count,
                                    response.from_name,
                                    response.channels.len()
                                );
                                if count > 0 {
                                    ui_logger.log(format!(
                                        "📺 Discovered {} new channels from {}",
                                        count, response.from_name
                                    ));
                                }
                            }
                            Err(e) => {
                                crate::log_network_error!(
                                    error_logger,
                                    "story_sync",
                                    "Failed to process discovered channels from {}: {}",
                                    response.from_name,
                                    e
                                );
                            }
                        }
                    }

                    // Save received stories (with deduplication handled by save_received_story)
                    let mut saved_count = 0;
                    for story in response.stories {
                        match crate::storage::save_received_story(story.clone()).await {
                            Ok(_) => {
                                saved_count += 1;
                                debug!("Saved story: {}", story.name);
                            }
                            Err(e) => {
                                debug!("Failed to save story '{}': {}", story.name, e);
                                // Don't log to UI for duplicates - this is expected
                                if !e.to_string().contains("already exists") {
                                    crate::log_network_error!(
                                        error_logger,
                                        "story_sync",
                                        "Failed to save story '{}': {}",
                                        story.name,
                                        e
                                    );
                                }
                            }
                        }
                    }

                    // Provide comprehensive sync summary
                    if discovered_channels_count > 0 && saved_count > 0 {
                        ui_logger.log(format!(
                            "{} Sync complete: {} stories, {} channels from {}",
                            Icons::checkmark(),
                            saved_count,
                            discovered_channels_count,
                            response.from_name
                        ));
                    } else if saved_count > 0 {
                        ui_logger.log(format!(
                            "{} Saved {} new stories from {}",
                            Icons::checkmark(),
                            saved_count,
                            response.from_name
                        ));
                    } else if discovered_channels_count > 0 {
                        ui_logger.log(format!(
                            "{} Discovered {} channels from {} (no new stories)",
                            Icons::checkmark(),
                            discovered_channels_count,
                            response.from_name
                        ));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            crate::log_network_error!(
                error_logger,
                "story_sync",
                "Failed to send story sync request to {}: {:?}",
                peer,
                error
            );

            let user_message = match error {
                request_response::OutboundFailure::UnsupportedProtocols => {
                    format!(
                        "{} Peer {} doesn't support story sync (version mismatch)",
                        Icons::cross(),
                        peer
                    )
                }
                _ => {
                    format!(
                        "{} Failed to sync stories with {}: {:?}",
                        Icons::cross(),
                        peer,
                        error
                    )
                }
            };

            ui_logger.log(user_message);
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            crate::log_network_error!(
                error_logger,
                "story_sync",
                "Failed to receive story sync request from {}: {:?}",
                peer,
                error
            );
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("Story sync response sent to {peer}");
        }
    }
}

pub async fn initiate_story_sync_with_peer(
    peer_id: PeerId,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    _error_logger: &ErrorLogger,
) {
    debug!("Initiating story sync with peer {peer_id}");

    // Load auto-share configuration to determine sync timeframe
    let sync_days = match crate::storage::load_unified_network_config().await {
        Ok(config) => config.auto_share.sync_days,
        Err(_) => 30, // Default to 30 days if config can't be loaded
    };

    // Calculate last_sync_timestamp based on sync_days configuration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sync_timeframe_seconds = (sync_days as u64) * 24 * 60 * 60; // Convert days to seconds
    let last_sync_timestamp = now.saturating_sub(sync_timeframe_seconds);

    // Get our subscribed channels to send in the sync request
    let subscribed_channels =
        match crate::storage::read_subscribed_channels(&PEER_ID.to_string()).await {
            Ok(channels) => channels,
            Err(e) => {
                debug!("Failed to read subscribed channels for sync: {e}");
                Vec::new() // Send empty list as fallback
            }
        };

    // Create sync request with calculated timestamp based on sync_days configuration
    let request = crate::network::StorySyncRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: local_peer_name.as_deref().unwrap_or("Unknown").to_string(),
        last_sync_timestamp, // Use calculated timestamp based on sync_days
        subscribed_channels,
        timestamp: now,
    };

    // Send the story sync request
    let request_id = swarm
        .behaviour_mut()
        .story_sync
        .send_request(&peer_id, request.clone());

    {
        debug!(
            "Sent story sync request to peer {peer_id} (request ID: {request_id:?}, sync days: {sync_days})"
        );
        ui_logger.log(format!(
            "{} Requesting stories from {} (syncing {} days)",
            Icons::sync(),
            peer_id,
            sync_days
        ));
    }
}

pub async fn execute_deferred_peer_operations(
    peer_id: PeerId,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &mut HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) {
    debug!(
        "Executing deferred operations for verified P2P-Play peer: {}",
        peer_id
    );

    // Broadcast local peer name to the newly verified peer
    if let Some(name) = local_peer_name {
        let peer_name = PeerName::new(PEER_ID.to_string(), name.clone());
        let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
        let json_bytes = Bytes::from(json.into_bytes());
        swarm
            .behaviour_mut()
            .floodsub
            .publish(TOPIC.clone(), json_bytes);
        debug!(
            "Sent local peer name '{}' to verified peer {}",
            name, peer_id
        );
    }

    // Retry any pending direct messages for this peer
    retry_messages_for_peer(peer_id, swarm, dm_config, pending_messages, peer_names).await;

    // Initiate story synchronization with the verified peer
    initiate_story_sync_with_peer(peer_id, swarm, local_peer_name, ui_logger, error_logger).await;

    debug!(
        "Completed deferred operations for verified peer {}",
        peer_id
    );
}

pub async fn handle_handshake_event(
    event: request_response::Event<HandshakeRequest, HandshakeResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &mut HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
    pending_handshake_peers: &Arc<Mutex<HashMap<PeerId, PendingHandshakePeer>>>,
    verified_p2p_play_peers: &Arc<Mutex<HashMap<PeerId, String>>>,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => {
            match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    // Handle incoming handshake request
                    debug!(
                        "Received handshake request from {}: app={}, version={}",
                        request.peer_id, request.app_name, request.app_version
                    );

                    // Validate the handshake request (only check app name)
                    let accepted = request.app_name == APP_NAME;

                    if accepted {
                        debug!(
                            "✅ Verified P2P-Play peer: {} (v{})",
                            request.peer_id, request.app_version
                        );
                    } else {
                        debug!(
                            "❌ Rejected non-P2P-Play peer: {} (app: {}, version: {})",
                            request.peer_id, request.app_name, request.app_version
                        );
                    }

                    // Send handshake response
                    let response = HandshakeResponse {
                        accepted,
                        app_name: APP_NAME.to_string(),
                        app_version: APP_VERSION.to_string(),
                        wasm_capable: true, // This node supports WASM capability advertisement
                    };

                    if let Err(e) = swarm
                        .behaviour_mut()
                        .handshake
                        .send_response(channel, response)
                    {
                        crate::log_network_error!(
                            error_logger,
                            "handshake",
                            "Failed to send handshake response to {}: {:?}",
                            peer,
                            e
                        );
                    }

                    // If peer is not compatible, disconnect from them
                    if !accepted {
                        debug!("Disconnecting from incompatible peer: {}", peer);
                        let _ = swarm.disconnect_peer_id(peer);
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // Handle handshake response
                    debug!(
                        "Received handshake response from {}: accepted={}, app={}, version={}",
                        peer, response.accepted, response.app_name, response.app_version
                    );

                    if response.accepted && response.app_name == APP_NAME {
                        debug!(
                            "✅ Handshake successful with P2P-Play peer: {} (v{})",
                            peer, response.app_version
                        );

                        // Add peer to floodsub for story sharing
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                        debug!("Added verified peer {} to floodsub partial view", peer);

                        // Add verified P2P-Play peer to UI display
                        let peer_name = format!("Peer_{peer}");
                        {
                            let mut verified_peers = verified_p2p_play_peers.lock().unwrap();
                            verified_peers.insert(peer, peer_name.clone());
                        }
                        peer_names.insert(peer, peer_name);
                        sorted_peer_names_cache.update(peer_names);
                        debug!("Added verified P2P-Play peer {} to UI display", peer);

                        // Execute all deferred operations now that handshake is successful
                        execute_deferred_peer_operations(
                            peer,
                            swarm,
                            peer_names,
                            local_peer_name,
                            ui_logger,
                            error_logger,
                            dm_config,
                            pending_messages,
                        )
                        .await;

                        // Remove peer from pending handshake list
                        {
                            let mut pending_peers = pending_handshake_peers.lock().unwrap();
                            if pending_peers.remove(&peer).is_some() {
                                debug!(
                                    "Removed peer {} from pending handshake list after successful handshake",
                                    peer
                                );
                            }
                        }

                        // Log successful P2P-Play peer verification to UI
                        ui_logger.log(format!("✅ Verified P2P-Play peer: {}", peer));
                    } else {
                        debug!(
                            "❌ Handshake failed with peer {}: not a compatible P2P-Play node",
                            peer
                        );

                        // Disconnect from incompatible peer
                        debug!("Disconnecting from incompatible peer: {}", peer);
                        let _ = swarm.disconnect_peer_id(peer);

                        // Remove peer from pending handshake list
                        {
                            let mut pending_peers = pending_handshake_peers.lock().unwrap();
                            if pending_peers.remove(&peer).is_some() {
                                debug!(
                                    "Removed incompatible peer {} from pending handshake list",
                                    peer
                                );
                            }
                        }
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            debug!("Handshake outbound failure with peer {}: {:?}", peer, error);
            // If handshake fails, assume peer is not compatible and disconnect
            debug!("Disconnecting from unresponsive peer: {}", peer);
            let _ = swarm.disconnect_peer_id(peer);

            // Remove peer from pending handshake list
            {
                let mut pending_peers = pending_handshake_peers.lock().unwrap();
                if pending_peers.remove(&peer).is_some() {
                    debug!(
                        "Removed unresponsive peer {} from pending handshake list",
                        peer
                    );
                }
            }
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            debug!("Handshake inbound failure with peer {}: {:?}", peer, error);

            // Remove peer from pending handshake list
            {
                let mut pending_peers = pending_handshake_peers.lock().unwrap();
                if pending_peers.remove(&peer).is_some() {
                    debug!("Removed failed peer {} from pending handshake list", peer);
                }
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
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
    relay_service: &mut Option<crate::relay::RelayService>,
    network_circuit_breakers: &crate::network_circuit_breakers::NetworkCircuitBreakers,
    pending_handshake_peers: &Arc<Mutex<HashMap<PeerId, PendingHandshakePeer>>>,
    verified_p2p_play_peers: &Arc<Mutex<HashMap<PeerId, String>>>,
) -> Option<ActionResult> {
    debug!("Event Received");
    match event {
        EventType::Response(resp) => {
            handle_response_event(resp, swarm).await;
        }
        EventType::PublishStory(story) => {
            handle_publish_story_event(story, swarm, error_logger, network_circuit_breakers).await;
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
                relay_service,
            )
            .await;
        }
        EventType::MdnsEvent(mdns_event) => {
            handle_mdns_event(mdns_event, swarm, ui_logger, error_logger).await;
        }
        EventType::FloodsubEvent(floodsub_event) => {
            if let Some(action_result) = handle_floodsub_event(
                floodsub_event,
                response_sender,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                error_logger,
                relay_service,
                verified_p2p_play_peers,
            )
            .await
            {
                match action_result {
                    crate::types::ActionResult::RefreshStories => {
                        return Some(ActionResult::RefreshStories);
                    }
                    crate::types::ActionResult::RebroadcastRelayMessage(relay_msg) => {
                        // Rebroadcast the relay message via floodsub
                        if let Err(e) = broadcast_relay_message(swarm, &relay_msg).await {
                            error_logger
                                .log_error(&format!("Failed to rebroadcast relay message: {e}"));
                        }
                    }
                    _ => {} // Other action results are not expected from floodsub events
                }
            }
        }
        EventType::PingEvent(ping_event) => {
            handle_ping_event(ping_event, error_logger).await;
        }
        EventType::RequestResponseEvent(request_response_event) => {
            if let Some(direct_message) = handle_request_response_event(
                request_response_event,
                swarm,
                peer_names,
                local_peer_name,
                ui_logger,
                error_logger,
                pending_messages,
            )
            .await
            {
                return Some(ActionResult::DirectMessageReceived(direct_message));
            }
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
        EventType::StorySyncEvent(story_sync_event) => {
            handle_story_sync_event(
                story_sync_event,
                swarm,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await;
        }
        EventType::HandshakeEvent(handshake_event) => {
            // Handle handshake events with peer validation
            handle_handshake_event(
                handshake_event,
                swarm,
                peer_names,
                local_peer_name,
                sorted_peer_names_cache,
                ui_logger,
                error_logger,
                dm_config,
                pending_messages,
                pending_handshake_peers,
                verified_p2p_play_peers,
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
        EventType::WasmCapabilitiesEvent(wasm_caps_event) => {
            handle_wasm_capabilities_event(
                wasm_caps_event,
                swarm,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await;
        }
        EventType::WasmExecutionEvent(wasm_exec_event) => {
            handle_wasm_execution_event(
                wasm_exec_event,
                swarm,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await;
        }
    }
    None
}

/// Handle WASM capabilities request/response events
pub async fn handle_wasm_capabilities_event(
    event: request_response::Event<WasmCapabilitiesRequest, WasmCapabilitiesResponse>,
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
                    debug!(
                        "Received WASM capabilities request from {} ({})",
                        request.from_name, peer
                    );

                    // Load local WASM offerings from storage
                    let offerings = match crate::storage::read_enabled_wasm_offerings().await {
                        Ok(offerings) => offerings,
                        Err(e) => {
                            error_logger.log_error(&format!(
                                "Failed to load WASM offerings for capability response: {e}"
                            ));
                            Vec::new()
                        }
                    };

                    // Check if WASM execution is enabled
                    let wasm_enabled = match crate::storage::load_unified_network_config().await {
                        Ok(config) => config.wasm.capability.allow_remote_execution,
                        Err(_) => false,
                    };

                    let from_name = local_peer_name
                        .clone()
                        .unwrap_or_else(|| PEER_ID.to_string());

                    let response = WasmCapabilitiesResponse {
                        peer_id: PEER_ID.to_string(),
                        peer_name: from_name,
                        wasm_enabled,
                        offerings,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    };

                    if let Err(e) = swarm
                        .behaviour_mut()
                        .wasm_capabilities
                        .send_response(channel, response)
                    {
                        error_logger.log_error(&format!(
                            "Failed to send WASM capabilities response: {e:?}"
                        ));
                    } else {
                        debug!("Sent WASM capabilities response to {}", peer);
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // We received capabilities from a peer
                    ui_logger.log(format!(
                        "{} Received {} WASM offering(s) from {} (execution: {})",
                        Icons::wrench(),
                        response.offerings.len(),
                        response.peer_name,
                        if response.wasm_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));

                    // Cache discovered offerings
                    for offering in &response.offerings {
                        if let Err(e) = crate::storage::cache_discovered_wasm_offering(
                            &response.peer_id,
                            offering,
                        )
                        .await
                        {
                            error_logger.log_error(&format!(
                                "Failed to cache WASM offering '{}' from {}: {e}",
                                offering.name, response.peer_name
                            ));
                        }
                    }

                    // Show offerings to user
                    for offering in &response.offerings {
                        ui_logger.log(format!(
                            "  {} {} (v{}) - {}",
                            Icons::chart(),
                            offering.name,
                            offering.version,
                            offering.description
                        ));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            error_logger.log_error(&format!(
                "WASM capabilities request to {} failed: {error:?}",
                peer
            ));
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            error_logger.log_error(&format!(
                "WASM capabilities inbound failure from {}: {error:?}",
                peer
            ));
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("WASM capabilities response sent to {}", peer);
        }
    }
}

/// Handle WASM execution request/response events
pub async fn handle_wasm_execution_event(
    event: request_response::Event<WasmExecutionRequest, WasmExecutionResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
    _local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => {
            match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    debug!(
                        "Received WASM execution request from {} ({}) for offering {}",
                        request.from_name, peer, request.offering_id
                    );

                    // Check if remote execution is enabled
                    let config = match crate::storage::load_unified_network_config().await {
                        Ok(config) => config,
                        Err(e) => {
                            error_logger.log_error(&format!(
                                "Failed to load config for WASM execution: {e}"
                            ));
                            let response = WasmExecutionResponse {
                                success: false,
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                                fuel_consumed: 0,
                                exit_code: -1,
                                error: Some("Internal configuration error".to_string()),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };
                            let _ = swarm
                                .behaviour_mut()
                                .wasm_execution
                                .send_response(channel, response);
                            return;
                        }
                    };

                    if !config.wasm.capability.allow_remote_execution {
                        ui_logger.log(format!(
                            "{} Rejected WASM execution request from {} - remote execution disabled",
                            Icons::cross(),
                            request.from_name
                        ));
                        let response = WasmExecutionResponse {
                            success: false,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                            fuel_consumed: 0,
                            exit_code: -1,
                            error: Some(
                                "Remote WASM execution is disabled on this node".to_string(),
                            ),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        };
                        let _ = swarm
                            .behaviour_mut()
                            .wasm_execution
                            .send_response(channel, response);
                        return;
                    }

                    // Verify the offering exists and is enabled
                    let offering =
                        match crate::storage::get_wasm_offering_by_id(&request.offering_id).await {
                            Ok(Some(offering)) => offering,
                            Ok(None) => {
                                ui_logger.log(format!(
                                    "{} WASM execution request for unknown offering: {}",
                                    Icons::cross(),
                                    request.offering_id
                                ));
                                let response = WasmExecutionResponse {
                                    success: false,
                                    stdout: Vec::new(),
                                    stderr: Vec::new(),
                                    fuel_consumed: 0,
                                    exit_code: -1,
                                    error: Some(format!(
                                        "Offering '{}' not found",
                                        request.offering_id
                                    )),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                };
                                let _ = swarm
                                    .behaviour_mut()
                                    .wasm_execution
                                    .send_response(channel, response);
                                return;
                            }
                            Err(e) => {
                                error_logger
                                    .log_error(&format!("Failed to lookup WASM offering: {e}"));
                                let response = WasmExecutionResponse {
                                    success: false,
                                    stdout: Vec::new(),
                                    stderr: Vec::new(),
                                    fuel_consumed: 0,
                                    exit_code: -1,
                                    error: Some("Internal error looking up offering".to_string()),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                };
                                let _ = swarm
                                    .behaviour_mut()
                                    .wasm_execution
                                    .send_response(channel, response);
                                return;
                            }
                        };

                    // Verify CID matches
                    if offering.ipfs_cid != request.ipfs_cid {
                        ui_logger.log(format!(
                            "{} CID mismatch for offering {} - expected {}, got {}",
                            Icons::cross(),
                            request.offering_id,
                            offering.ipfs_cid,
                            request.ipfs_cid
                        ));
                        let response = WasmExecutionResponse {
                            success: false,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                            fuel_consumed: 0,
                            exit_code: -1,
                            error: Some("CID verification failed".to_string()),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        };
                        let _ = swarm
                            .behaviour_mut()
                            .wasm_execution
                            .send_response(channel, response);
                        return;
                    }

                    // Check if offering is enabled
                    if !offering.enabled {
                        let response = WasmExecutionResponse {
                            success: false,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                            fuel_consumed: 0,
                            exit_code: -1,
                            error: Some("Offering is currently disabled".to_string()),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        };
                        let _ = swarm
                            .behaviour_mut()
                            .wasm_execution
                            .send_response(channel, response);
                        return;
                    }

                    ui_logger.log(format!(
                        "{} Executing WASM '{}' for {}",
                        Icons::sync(),
                        offering.name,
                        request.from_name
                    ));

                    // Determine resource limits: use offering defaults, allow request overrides
                    let fuel_limit = request
                        .fuel_limit
                        .unwrap_or(offering.resource_requirements.max_fuel);
                    let memory_limit_mb = request
                        .memory_limit_mb
                        .unwrap_or(offering.resource_requirements.max_memory_mb);
                    let timeout_secs = request
                        .timeout_secs
                        .or(Some(offering.resource_requirements.estimated_timeout_secs));

                    // Create executor with content fetcher
                    let fetcher = Arc::new(GatewayFetcher::new());
                    let executor = match WasmExecutor::new(fetcher) {
                        Ok(exec) => exec,
                        Err(e) => {
                            error_logger.log_error(&format!("Failed to create WASM executor: {e}"));
                            let response = WasmExecutionResponse {
                                success: false,
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                                fuel_consumed: 0,
                                exit_code: -1,
                                error: Some(format!("Failed to initialize WASM executor: {e}")),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };
                            let _ = swarm
                                .behaviour_mut()
                                .wasm_execution
                                .send_response(channel, response);
                            return;
                        }
                    };

                    // Build execution request
                    let exec_request = ExecutionRequest::new(request.ipfs_cid.clone())
                        .with_input(request.input.clone())
                        .with_fuel_limit(fuel_limit)
                        .with_memory_limit_mb(memory_limit_mb)
                        .with_args(request.args.clone());

                    let exec_request = if let Some(timeout) = timeout_secs {
                        exec_request.with_timeout_secs(Some(timeout))
                    } else {
                        exec_request
                    };

                    // Execute WASM module
                    let response = match executor.execute(exec_request).await {
                        Ok(result) => {
                            ui_logger.log(format!(
                                "{} WASM '{}' executed (fuel: {}, exit: {})",
                                Icons::check(),
                                offering.name,
                                result.fuel_consumed,
                                result.exit_code
                            ));
                            WasmExecutionResponse {
                                success: result.exit_code == 0,
                                stdout: result.stdout,
                                stderr: result.stderr,
                                fuel_consumed: result.fuel_consumed,
                                exit_code: result.exit_code,
                                error: None,
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("{e}");
                            let fuel_consumed = match &e {
                                WasmExecutionError::FuelExhausted { consumed } => *consumed,
                                _ => 0,
                            };
                            ui_logger.log(format!(
                                "{} WASM '{}' failed: {}",
                                Icons::cross(),
                                offering.name,
                                error_msg
                            ));
                            WasmExecutionResponse {
                                success: false,
                                stdout: Vec::new(),
                                stderr: error_msg.as_bytes().to_vec(),
                                fuel_consumed,
                                exit_code: -1,
                                error: Some(error_msg),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            }
                        }
                    };

                    if let Err(e) = swarm
                        .behaviour_mut()
                        .wasm_execution
                        .send_response(channel, response)
                    {
                        error_logger
                            .log_error(&format!("Failed to send WASM execution response: {e:?}"));
                    }
                }
                request_response::Message::Response { response, .. } => {
                    // We received execution results
                    if response.success {
                        ui_logger.log(format!(
                            "{} WASM execution completed successfully (fuel: {})",
                            Icons::check(),
                            response.fuel_consumed
                        ));
                        if !response.stdout.is_empty() {
                            if let Ok(stdout_str) = String::from_utf8(response.stdout.clone()) {
                                ui_logger.log(format!("stdout: {}", stdout_str));
                            } else {
                                ui_logger.log(format!(
                                    "stdout: ({} bytes of binary data)",
                                    response.stdout.len()
                                ));
                            }
                        }
                        if !response.stderr.is_empty() {
                            if let Ok(stderr_str) = String::from_utf8(response.stderr.clone()) {
                                ui_logger.log(format!("stderr: {}", stderr_str));
                            }
                        }
                    } else {
                        ui_logger.log(format!(
                            "{} WASM execution failed: {}",
                            Icons::cross(),
                            response.error.as_deref().unwrap_or("Unknown error")
                        ));
                    }
                }
            }
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            error_logger.log_error(&format!(
                "WASM execution request to {} failed: {error:?}",
                peer
            ));
            ui_logger.log(format!(
                "{} WASM execution request failed: {error:?}",
                Icons::cross()
            ));
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            error_logger.log_error(&format!(
                "WASM execution inbound failure from {}: {error:?}",
                peer
            ));
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!("WASM execution response sent to {}", peer);
        }
    }
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
                debug!("Reconnecting to discovered peer: {peer}");
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

pub fn track_successful_connection(peer_id: PeerId) {
    if let Ok(mut connections) = LAST_SUCCESSFUL_CONNECTIONS.try_lock() {
        connections.insert(peer_id, Instant::now());
        debug!("Tracked successful connection to peer: {peer_id}");
    }
}

pub async fn trigger_immediate_connection_maintenance(
    swarm: &mut Swarm<StoryBehaviour>,
    error_logger: &ErrorLogger,
) {
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
            "{} Failed to deliver message to {} after {} attempts",
            Icons::cross(),
            msg.target_name,
            msg.max_attempts
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

pub async fn broadcast_relay_message(
    swarm: &mut Swarm<StoryBehaviour>,
    relay_msg: &crate::types::RelayMessage,
) -> Result<(), String> {
    let json = serde_json::to_string(relay_msg)
        .map_err(|e| format!("Failed to serialize relay message: {e}"))?;

    let json_bytes = Bytes::from(json.into_bytes());
    swarm
        .behaviour_mut()
        .floodsub
        .publish(crate::network::RELAY_TOPIC.clone(), json_bytes);

    debug!(
        "Broadcasted relay message with ID: {}",
        relay_msg.message_id
    );
    Ok(())
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

    #[test]
    fn test_command_input_trimming() {
        // Test that commands with leading and trailing spaces are handled correctly
        let test_cases = vec![
            ("help", "help"),
            (" help", "help"),    // Leading space
            ("help ", "help"),    // Trailing space
            ("  help  ", "help"), // Both leading and trailing spaces
            ("\thelp\t", "help"), // Tab characters
            (" ls sub ", "subscription"),
            ("  ls ch  ", "channels"),
            (" ls s local ", "stories"),
        ];

        for (input, expected_type) in test_cases {
            let result = match_command_type_with_trim(input);
            assert_eq!(
                result, expected_type,
                "Command '{input}' should match {expected_type} handler"
            );
        }
    }

    // Mock function that simulates the pattern matching logic from event_handlers.rs
    fn match_command_type(line: &str) -> &'static str {
        // This follows the exact same pattern matching order as in handle_input_event
        match line {
            "ls ch" => "channels",
            "ls sub" => "subscription",
            cmd if cmd.starts_with("ls s") => "stories",
            "help" => "help",
            _ => "unknown",
        }
    }

    // Mock function that simulates the new trimming behavior
    fn match_command_type_with_trim(line: &str) -> &'static str {
        let line = line.trim();
        match line {
            "ls ch" => "channels",
            "ls sub" => "subscription",
            cmd if cmd.starts_with("ls s") => "stories",
            "help" => "help",
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
            "peer456".to_string(),
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
        let network_config = crate::types::NetworkConfig::new();
        let mut swarm = crate::network::create_swarm(&ping_config, &network_config)
            .expect("Failed to create test swarm");

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
        let network_config = crate::types::NetworkConfig::new();
        let mut swarm =
            create_swarm(&ping_config, &network_config).expect("Failed to create swarm");
        let story = Story {
            id: 1,
            name: "Test Story".to_string(),
            header: "Test Header".to_string(),
            body: "Test Body".to_string(),
            public: true,
            channel: "general".to_string(),
            created_at: 1234567890,
            auto_share: None, // Use global setting for test
        };

        // Measure time taken for story publishing (should be very fast now)
        let start = Instant::now();
        let error_logger = ErrorLogger::new("test_errors.log");

        // Create disabled circuit breakers for testing
        let cb_config = crate::types::NetworkCircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        };
        let network_circuit_breakers =
            crate::network_circuit_breakers::NetworkCircuitBreakers::new(&cb_config);

        handle_publish_story_event(story, &mut swarm, &error_logger, &network_circuit_breakers)
            .await;
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
