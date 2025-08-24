use crate::error_logger::ErrorLogger;
use crate::handlers::{
    SortedPeerNamesCache, UILogger, establish_direct_connection, handle_config_auto_share,
    handle_config_sync_days, handle_create_channel, handle_create_description,
    handle_create_stories_with_sender, handle_delete_story, handle_direct_message_with_relay,
    handle_filter_stories, handle_get_description, handle_help, handle_list_channels,
    handle_list_stories, handle_list_subscriptions, handle_peer_id, handle_publish_story,
    handle_reload_config, handle_search_stories, handle_set_auto_subscription, handle_set_name,
    handle_show_description, handle_show_story, handle_subscribe_channel,
    handle_unsubscribe_channel,
};
use crate::network::{
    APP_NAME, APP_VERSION, DirectMessageRequest, DirectMessageResponse, HandshakeRequest,
    HandshakeResponse, NodeDescriptionRequest, NodeDescriptionResponse, PEER_ID, StoryBehaviour,
    TOPIC,
};
use crate::storage::{load_node_description, save_received_story};
use crate::types::{
    ActionResult, DirectMessage, DirectMessageConfig, EventType, Icons, ListMode, ListRequest,
    ListResponse, PeerName, PendingDirectMessage, PendingHandshakePeer, PublishedChannel,
    PublishedStory,
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

async fn handle_auto_subscription(
    peer_id: &str,
    channel_name: &str,
    max_auto_subs: usize,
    ui_logger: &UILogger,
) -> Result<bool, String> {
    match crate::storage::read_subscribed_channels(peer_id).await {
        Ok(subscribed) => {
            if subscribed.contains(&channel_name.to_string()) {
                return Ok(false); // Not an error, just already subscribed
            }
        }
        Err(e) => {
            return Err(format!("Failed to check existing subscriptions: {e}"));
        }
    }

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

    match crate::storage::subscribe_to_channel(peer_id, channel_name).await {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to subscribe: {e}")),
    }
}
pub async fn handle_response_event(resp: ListResponse, swarm: &mut Swarm<StoryBehaviour>) {
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
    maintain_connections(swarm, error_logger).await;

    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();

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

    let publish_result = network_circuit_breakers
        .execute("story_publish", || async {
            let json = serde_json::to_string(&published_story)
                .map_err(|e| format!("Failed to serialize story: {e}"))?;
            let json_bytes = Bytes::from(json.into_bytes());

            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);

            Ok::<(), String>(())
        })
        .await;

    match publish_result {
        Ok(_) => {}
        Err(e) => {
            error_logger.log_error(&format!("Story publishing failed: {e:?}"));
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
        cmd if cmd.starts_with("peer id") => handle_peer_id(cmd, ui_logger).await,
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
            match local_peer_name {
                Some(name) => ui_logger.log(format!("Current alias: {name}")),
                None => ui_logger.log("No alias set. Use 'name <alias>' to set one.".to_string()),
            }
        }
        cmd if cmd.starts_with("name ") => {
            if let Some(peer_name) = handle_set_name(cmd, local_peer_name, ui_logger).await {
                let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
                let json_bytes = Bytes::from(json.into_bytes());
                swarm
                    .behaviour_mut()
                    .floodsub
                    .publish(TOPIC.clone(), json_bytes);
            }
        }
        cmd if cmd.starts_with("connect ") => {
            if let Some(addr) = cmd.strip_prefix("connect ") {
                establish_direct_connection(swarm, addr, ui_logger).await;
            }
        }
        cmd if cmd.starts_with("compose ") => {
            if let Some(peer_name) = cmd.strip_prefix("compose ") {
                let peer_name = peer_name.trim();
                if peer_name.is_empty() {
                    ui_logger.log("Usage: compose <peer_alias>".to_string());
                } else {
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
            for (peer, _addr) in discovered_list {
                if !swarm.is_connected(&peer) {
                    if let Err(e) = swarm.dial(peer) {
                        crate::log_network_error!(
                            error_logger,
                            "mdns",
                            "Failed to initiate dial to {}: {}",
                            peer,
                            e
                        );
                    }
                }
            }
        }
        libp2p::mdns::Event::Expired(expired_list) => {
            for (peer, _addr) in expired_list {
                let discovered_nodes: Vec<_> = swarm.behaviour().mdns.discovered_nodes().collect();
                if !discovered_nodes.contains(&(&peer)) {
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
            let source_peer = msg.source;
            let is_verified = {
                let verified_peers = verified_p2p_play_peers.lock().unwrap();
                verified_peers.contains_key(&source_peer)
            };

            if !is_verified {
                return None;
            }
            if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    resp.data.iter().for_each(|r| debug!("{r:?}"));
                }
            } else if let Ok(published) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                if published.publisher != PEER_ID.to_string() {
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
                        ui_logger.log(format!(
                            "📖 Received story '{}' from {} in channel '{}'",
                            published.story.name, msg.source, published.story.channel
                        ));

                        if let Err(e) = save_received_story(published.story.clone()).await {
                            crate::log_network_error!(
                                error_logger,
                                "storage",
                                "Failed to save received story: {}",
                                e
                            );
                            ui_logger.log(format!("Warning: Failed to save received story: {e}"));
                        } else {
                            return Some(crate::types::ActionResult::RefreshStories);
                        }
                    }
                }
            } else if let Ok(peer_name) = serde_json::from_slice::<PeerName>(&msg.data) {
                if let Ok(peer_id) = peer_name.peer_id.parse::<PeerId>() {
                    if peer_id != *PEER_ID {
                        let mut names_changed = false;
                        peer_names
                            .entry(peer_id)
                            .and_modify(|existing_name| {
                                if existing_name != &peer_name.name {
                                    *existing_name = peer_name.name.clone();
                                    names_changed = true;
                                }
                            })
                            .or_insert_with(|| {
                                names_changed = true;
                                peer_name.name.clone()
                            });

                        if names_changed {
                            sorted_peer_names_cache.update(peer_names);
                        }
                    }
                }
            } else if let Ok(published_channel) =
                serde_json::from_slice::<PublishedChannel>(&msg.data)
            {
                if published_channel.publisher != PEER_ID.to_string() {

                    let auto_sub_config = match crate::storage::load_unified_network_config().await
                    {
                        Ok(config) => config.channel_auto_subscription,
                        Err(_) => {
                            crate::types::ChannelAutoSubscriptionConfig::new() // Use defaults
                        }
                    };

                    if auto_sub_config.notify_new_channels {
                        ui_logger.log(format!(
                            "📺 New channel discovered: '{}' - {} from {}",
                            published_channel.channel.name,
                            published_channel.channel.description,
                            published_channel.publisher
                        ));
                    }

                    let should_auto_subscribe = auto_sub_config.auto_subscribe_to_new_channels;
                    let max_auto_subs = auto_sub_config.max_auto_subscriptions;

                    let channel_to_save = &published_channel.channel;
                    let peer_id_str = PEER_ID.to_string();

                    if channel_to_save.name.is_empty() || channel_to_save.description.is_empty() {
                        debug!("Ignoring invalid published channel with empty name or description");
                    } else {
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
                                true
                            }
                            Err(e) if e.to_string().contains("UNIQUE constraint") => {
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
                ui_logger.log(format!(
                    "📺 Received channel '{}' - {} from network",
                    channel.name, channel.description
                ));

                let channel_to_save = channel.clone();
                let ui_logger_clone = ui_logger.clone();
                tokio::spawn(async move {
                    if channel_to_save.name.is_empty() || channel_to_save.description.is_empty() {
                        return;
                    }

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
                        }
                        Ok(crate::relay::RelayAction::ForwardMessage(forward_msg)) => {
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
                ui_logger.log(format!(
                    "✅ Message delivery confirmed: {} (path length: {})",
                    &relay_confirmation.message_id[..8],
                    relay_confirmation.relay_path_length
                ));
            } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                match req.mode {
                    ListMode::ALL => {
                        respond_with_public_stories(
                            response_sender.clone(),
                            msg.source.to_string(),
                        );
                    }
                    ListMode::One(ref peer_id) => {
                        if peer_id == &PEER_ID.to_string() {
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
            }
            libp2p::kad::QueryResult::GetClosestPeers(Ok(get_closest_peers_ok)) => {
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
    debug!(
        "Received PeerName event: {} -> {}",
        peer_name.peer_id, peer_name.name
    );
}

pub async fn handle_request_response_event(
    event: request_response::Event<DirectMessageRequest, DirectMessageResponse>,
    swarm: &mut Swarm<StoryBehaviour>,
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

                    let should_process = if let Some(local_name) = local_peer_name {
                        &request.to_name == local_name
                    } else {
                        false
                    };

                    let direct_message = if should_process {
                        Some(DirectMessage {
                            from_peer_id: request.from_peer_id.clone(),
                            from_name: request.from_name.clone(),
                            to_name: request.to_name.clone(),
                            message: request.message.clone(),
                            timestamp: request.timestamp,
                        })
                    } else {
                        None
                    };

                    let response = DirectMessageResponse {
                        received: should_process,
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
                        if let Ok(mut queue) = pending_messages.lock() {
                            queue.retain(|msg| msg.target_peer_id != peer);
                        }

                        ui_logger.log(format!("{} Message delivered to {}", Icons::check(), peer));
                    } else {
                        crate::log_network_error!(
                            error_logger,
                            "direct_message",
                            "Direct message was rejected by peer {}",
                            peer
                        );

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
            error_logger.log_network_error(
                "direct_message",
                &format!("Failed to send direct message to {peer}: {error:?}"),
            );
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
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
                    ui_logger.log(format!(
                        "{} Description request from {}",
                        Icons::clipboard(),
                        request.from_name
                    ));

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

                            let channels = match crate::storage::read_channels().await {
                                Ok(all_channels) => {
                                    all_channels
                                }
                                Err(_) => {
                                    match crate::storage::get_channels_for_stories(
                                        &filtered_stories,
                                    )
                                    .await
                                    {
                                        Ok(story_channels) => {
                                            story_channels
                                        }
                                        Err(_) => {
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

                    let mut discovered_channels_count = 0;

                    if !response.channels.is_empty() {
                        match crate::storage::process_discovered_channels(
                            &response.channels,
                            &response.from_name,
                        )
                        .await
                        {
                            Ok(count) => {
                                discovered_channels_count = count;
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

                    let mut saved_count = 0;
                    for story in response.stories {
                        match crate::storage::save_received_story(story.clone()).await {
                            Ok(_) => {
                                saved_count += 1;
                            }
                            Err(e) => {
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

/// Initiate story synchronization with a newly connected peer
pub async fn initiate_story_sync_with_peer(
    peer_id: PeerId,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    _error_logger: &ErrorLogger,
) {

    let sync_days = match crate::storage::load_unified_network_config().await {
        Ok(config) => config.auto_share.sync_days,
        Err(_) => 30, // Default to 30 days if config can't be loaded
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sync_timeframe_seconds = (sync_days as u64) * 24 * 60 * 60; // Convert days to seconds
    let last_sync_timestamp = now.saturating_sub(sync_timeframe_seconds);

    let subscribed_channels =
        match crate::storage::read_subscribed_channels(&PEER_ID.to_string()).await {
            Ok(channels) => channels,
            Err(_) => {
                Vec::new() // Send empty list as fallback
            }
        };

    let request = crate::network::StorySyncRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: local_peer_name.as_deref().unwrap_or("Unknown").to_string(),
        last_sync_timestamp, // Use calculated timestamp based on sync_days
        subscribed_channels,
        timestamp: now,
    };

    let _request_id = swarm
        .behaviour_mut()
        .story_sync
        .send_request(&peer_id, request.clone());

    {
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
    if let Some(name) = local_peer_name {
        let peer_name = PeerName::new(PEER_ID.to_string(), name.clone());
        let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
        let json_bytes = Bytes::from(json.into_bytes());
        swarm
            .behaviour_mut()
            .floodsub
            .publish(TOPIC.clone(), json_bytes);
    }

    retry_messages_for_peer(peer_id, swarm, dm_config, pending_messages, peer_names).await;

    initiate_story_sync_with_peer(peer_id, swarm, local_peer_name, ui_logger, error_logger).await;
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
                    let accepted = request.app_name == APP_NAME;

                    let response = HandshakeResponse {
                        accepted,
                        app_name: APP_NAME.to_string(),
                        app_version: APP_VERSION.to_string(),
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

                    if !accepted {
                        let _ = swarm.disconnect_peer_id(peer);
                    }
                }
                request_response::Message::Response { response, .. } => {
                    if response.accepted && response.app_name == APP_NAME {
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);

                        let peer_name = format!("Peer_{peer}");
                        {
                            let mut verified_peers = verified_p2p_play_peers.lock().unwrap();
                            verified_peers.insert(peer, peer_name.clone());
                        }
                        peer_names.insert(peer, peer_name);
                        sorted_peer_names_cache.update(peer_names);

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

                        {
                            let mut pending_peers = pending_handshake_peers.lock().unwrap();
                            if pending_peers.remove(&peer).is_some() {
                                debug!(
                                    "Removed peer {} from pending handshake list after successful handshake",
                                    peer
                                );
                            }
                        }

                        ui_logger.log(format!("✅ Verified P2P-Play peer: {}", peer));
                    } else {
                        let _ = swarm.disconnect_peer_id(peer);

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
            let _ = swarm.disconnect_peer_id(peer);

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

    for peer in discovered_peers {
        if !swarm.is_connected(&peer) {
            let should_attempt = match LAST_CONNECTION_ATTEMPTS.try_lock() {
                Ok(mut attempts) => {
                    attempts.retain(|_, &mut last_time| last_time.elapsed() < CLEANUP_THRESHOLD);

                    let last_attempt = attempts.get(&peer);

                    let reconnect_interval = match LAST_SUCCESSFUL_CONNECTIONS.try_lock() {
                        Ok(successful_connections) => {
                            if let Some(last_successful) = successful_connections.get(&peer) {
                                if last_successful.elapsed() < RECENT_CONNECTION_THRESHOLD {
                                    MIN_RECONNECT_INTERVAL_RECENT
                                } else {
                                    MIN_RECONNECT_INTERVAL
                                }
                            } else {
                                MIN_RECONNECT_INTERVAL
                            }
                        }
                        Err(_) => {
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
                    false
                }
            };

            if should_attempt {
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
    }
}

pub async fn trigger_immediate_connection_maintenance(
    swarm: &mut Swarm<StoryBehaviour>,
    error_logger: &ErrorLogger,
) {
    maintain_connections(swarm, error_logger).await;
}

pub fn respond_with_public_stories(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        let error_logger = ErrorLogger::new("errors.log");
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

        let filtered_stories: Vec<_> = stories
            .into_iter()
            .filter(|story| {
                story.public
                    && (subscribed_channels.contains(&story.channel) || story.channel == "general")
            })
            .collect();

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

    if let Ok(mut queue) = pending_messages.lock() {
        let mut i = 0;
        while i < queue.len() {
            let msg = &mut queue[i];

            if msg.is_exhausted() {
                exhausted_messages.push(msg.clone());
                queue.remove(i);
            } else if msg.should_retry(dm_config.retry_interval_seconds) {
                msg.increment_attempt();
                messages_to_retry.push(msg.clone());
                i += 1;
            } else {
                i += 1;
            }
        }
    }

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
        // For placeholder PeerIds, try to find the real peer with matching name
        let target_peer_id = if msg.is_placeholder_peer_id {
            if let Some((real_peer_id, _)) = peer_names
                .iter()
                .find(|(_, name)| name == &&msg.target_name)
            {
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
                continue;
            }
        } else {
            msg.target_peer_id
        };

        let _request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&target_peer_id, msg.message.clone());
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
                msg.target_peer_id == peer_id
            };

            if should_retry && !msg.is_exhausted() {
                if msg.is_placeholder_peer_id {
                    msg.target_peer_id = peer_id;
                    msg.is_placeholder_peer_id = false;
                }
                msg.increment_attempt();
                messages_to_retry.push(msg.clone());
            }
        }
    }

    for msg in messages_to_retry {

        let _request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&msg.target_peer_id, msg.message.clone());

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
        let mut swarm =
            crate::network::create_swarm(&ping_config).expect("Failed to create test swarm");

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
