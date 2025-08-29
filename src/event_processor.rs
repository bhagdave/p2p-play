use crate::bootstrap::{AutoBootstrap, run_auto_bootstrap_with_retry};
use crate::bootstrap_logger::BootstrapLogger;
use crate::error_logger::ErrorLogger;
use crate::event_handlers::{
    self, handle_event, track_successful_connection, trigger_immediate_connection_maintenance,
};
use crate::handlers::{
    SortedPeerNamesCache, UILogger, mark_story_as_read_for_peer, refresh_unread_counts_for_ui,
};
use crate::network::{
    APP_NAME, APP_VERSION, HandshakeRequest, PEER_ID, StoryBehaviour, StoryBehaviourEvent,
};
use crate::network_circuit_breakers::NetworkCircuitBreakers;
use crate::relay::RelayService;
use crate::storage;
use crate::types::{
    ActionResult, DirectMessageConfig, EventType, NetworkConfig, PendingDirectMessage,
    PendingHandshakePeer,
};
use crate::ui::{App, AppEvent, handle_ui_events};

use libp2p::{PeerId, Swarm, futures::StreamExt, swarm::SwarmEvent};
use log::debug;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

// Configuration constants for event processing intervals
const CONNECTION_MAINTENANCE_INTERVAL_SECS: u64 = 30;
const BOOTSTRAP_RETRY_INTERVAL_SECS: u64 = 5;
const BOOTSTRAP_STATUS_LOG_INTERVAL_SECS: u64 = 60;
const DM_RETRY_INTERVAL_SECS: u64 = 10;

// Handshake timeout - if no response received within this time, disconnect peer
const HANDSHAKE_TIMEOUT_SECS: u64 = 60;

pub struct EventProcessor {
    ui_rcv: mpsc::UnboundedReceiver<AppEvent>,
    ui_log_rcv: mpsc::UnboundedReceiver<String>,
    response_rcv: mpsc::UnboundedReceiver<crate::types::ListResponse>,
    story_rcv: mpsc::UnboundedReceiver<crate::types::Story>,

    response_sender: mpsc::UnboundedSender<crate::types::ListResponse>,
    story_sender: mpsc::UnboundedSender<crate::types::Story>,

    ui_sender: mpsc::UnboundedSender<AppEvent>,

    connection_maintenance_interval: tokio::time::Interval,
    bootstrap_retry_interval: tokio::time::Interval,
    bootstrap_status_log_interval: tokio::time::Interval,
    dm_retry_interval: tokio::time::Interval,
    network_health_update_interval: tokio::time::Interval,
    handshake_timeout_interval: tokio::time::Interval,

    dm_config: DirectMessageConfig,
    pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>>,

    ui_logger: UILogger,
    error_logger: ErrorLogger,
    bootstrap_logger: BootstrapLogger,

    relay_service: Option<RelayService>,

    network_circuit_breakers: NetworkCircuitBreakers,

    pending_handshake_peers: Arc<Mutex<HashMap<PeerId, PendingHandshakePeer>>>,

    // Verified P2P-Play peers (only these are shown in UI)
    verified_p2p_play_peers: Arc<Mutex<HashMap<PeerId, String>>>,
}

impl EventProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ui_rcv: mpsc::UnboundedReceiver<AppEvent>,
        ui_log_rcv: mpsc::UnboundedReceiver<String>,
        response_rcv: mpsc::UnboundedReceiver<crate::types::ListResponse>,
        story_rcv: mpsc::UnboundedReceiver<crate::types::Story>,
        response_sender: mpsc::UnboundedSender<crate::types::ListResponse>,
        story_sender: mpsc::UnboundedSender<crate::types::Story>,
        ui_sender: mpsc::UnboundedSender<AppEvent>,
        network_config: &NetworkConfig,
        dm_config: DirectMessageConfig,
        pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>>,
        ui_logger: UILogger,
        error_logger: ErrorLogger,
        bootstrap_logger: BootstrapLogger,
        relay_service: Option<RelayService>,
        network_circuit_breakers: NetworkCircuitBreakers,
    ) -> Self {
        Self {
            ui_rcv,
            ui_log_rcv,
            response_rcv,
            story_rcv,
            response_sender,
            story_sender,
            ui_sender,
            connection_maintenance_interval: interval(Duration::from_secs(
                CONNECTION_MAINTENANCE_INTERVAL_SECS,
            )),
            bootstrap_retry_interval: interval(Duration::from_secs(BOOTSTRAP_RETRY_INTERVAL_SECS)),
            bootstrap_status_log_interval: interval(Duration::from_secs(
                BOOTSTRAP_STATUS_LOG_INTERVAL_SECS,
            )),
            dm_retry_interval: interval(Duration::from_secs(DM_RETRY_INTERVAL_SECS)),
            network_health_update_interval: interval(Duration::from_secs(
                network_config.network_health_update_interval_seconds,
            )),
            handshake_timeout_interval: interval(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS / 2)), // Check every 30s for 60s timeout
            dm_config,
            pending_messages,
            ui_logger,
            error_logger,
            bootstrap_logger,
            relay_service,
            network_circuit_breakers,
            pending_handshake_peers: Arc::new(Mutex::new(HashMap::new())),
            verified_p2p_play_peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Main event loop processing
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &mut self,
        app: &mut App,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &mut HashMap<PeerId, String>,
        local_peer_name: &mut Option<String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
        auto_bootstrap: &mut AutoBootstrap,
    ) {
        loop {
            // Handle UI events first to ensure responsiveness
            if let Err(e) = handle_ui_events(app, self.ui_sender.clone()).await {
                self.error_logger
                    .log_error(&format!("UI event handling error: {e}"));
            }

            if let Err(e) = app.draw() {
                self.error_logger
                    .log_error(&format!("UI drawing error: {e}"));
            }

            if app.should_quit {
                break;
            }

            tokio::task::yield_now().await;

            #[cfg(windows)]
            let main_loop_timeout = std::time::Duration::from_millis(100);

            #[cfg(not(windows))]
            let main_loop_timeout = std::time::Duration::from_millis(50);

            let evt = self
                .select_next_event(
                    main_loop_timeout,
                    app,
                    swarm,
                    peer_names,
                    sorted_peer_names_cache,
                    local_peer_name,
                    auto_bootstrap,
                )
                .await;

            if let Some(event) = evt {
                self.process_event(
                    event,
                    app,
                    swarm,
                    peer_names,
                    local_peer_name,
                    sorted_peer_names_cache,
                )
                .await;

                app.update_peers(peer_names.clone());
                app.update_local_peer_name(local_peer_name.clone());
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn select_next_event(
        &mut self,
        timeout: Duration,
        app: &mut App,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &mut HashMap<PeerId, String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
        local_peer_name: &Option<String>,
        auto_bootstrap: &mut AutoBootstrap,
    ) -> Option<EventType> {
        tokio::select! {
            // Shorter timeout to ensure UI responsiveness
            _ = tokio::time::sleep(timeout) => {
                None
            }
            ui_log_msg = self.ui_log_rcv.recv() => {
                if let Some(msg) = ui_log_msg {
                    app.add_to_log(msg);
                }
                None
            }
            // UI events have higher priority - they are processed immediately
            ui_event = self.ui_rcv.recv() => {
                if let Some(event) = ui_event {
                    match event {
                        AppEvent::Input(line) => Some(EventType::Input(line)),
                        AppEvent::Quit => {
                            debug!("Quit event received in main loop");
                            app.should_quit = true;
                            None
                        }
                        AppEvent::StoryViewed { story_id, channel } => {
                            mark_story_as_read_for_peer(story_id, &PEER_ID.to_string(), &channel).await;
                            refresh_unread_counts_for_ui(app, &PEER_ID.to_string()).await;
                            None
                        }
                        AppEvent::DirectMessage(direct_message) => {
                            app.handle_direct_message(direct_message);
                            None
                        }
                        AppEvent::EnterMessageComposition { target_peer } => {
                            app.input_mode = crate::ui::InputMode::MessageComposition {
                                target_peer,
                                lines: Vec::new(),
                                current_line: String::new(),
                            };
                            app.input.clear();
                            app.add_to_log(format!("{} Entered message composition mode", crate::types::Icons::memo()));
                            None
                        }
                    }
                } else {
                    None
                }
            }
            response = self.response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
            story = self.story_rcv.recv() => Some(EventType::PublishStory(story.expect("story exists"))),
            _ = self.connection_maintenance_interval.tick() => {
                // Periodic connection maintenance - spawn to background to avoid blocking
                event_handlers::maintain_connections(swarm, &self.error_logger).await;
                None
            },
            _ = self.bootstrap_retry_interval.tick() => {
                // Automatic bootstrap retry - only if should retry and time is right
                if auto_bootstrap.should_retry() && auto_bootstrap.is_retry_time() {
                    run_auto_bootstrap_with_retry(auto_bootstrap, swarm, &self.bootstrap_logger, &self.error_logger).await;
                }
                None
            },
            _ = self.bootstrap_status_log_interval.tick() => {
                // Periodically log bootstrap status - use try_lock to avoid blocking
                if let Ok(status) = auto_bootstrap.status.try_lock() {
                    if !matches!(*status, crate::bootstrap::BootstrapStatus::NotStarted) {
                        drop(status); // Release lock before expensive operation
                        let status_msg = auto_bootstrap.get_status_string();
                        self.bootstrap_logger.log_status(&status_msg);
                    }
                }
                None
            },
            _ = self.dm_retry_interval.tick() => {
                event_handlers::process_pending_messages(
                    swarm,
                    &self.dm_config,
                    &self.pending_messages,
                    peer_names,
                    &self.ui_logger,
                ).await;
                None
            },
            _ = self.network_health_update_interval.tick() => {
                let health_summary = self.network_circuit_breakers.health_summary().await;
                app.update_network_health(health_summary);
                None
            },
            _ = self.handshake_timeout_interval.tick() => {
                self.cleanup_timed_out_handshakes(swarm).await;
                None
            },
            // Network events are processed with heavy operations spawned to background
            event = swarm.select_next_some() => {
                self.handle_swarm_event(event, swarm, peer_names, sorted_peer_names_cache, local_peer_name, app, auto_bootstrap).await
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<StoryBehaviourEvent>,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &mut HashMap<PeerId, String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
        local_peer_name: &Option<String>,
        app: &mut App,
        auto_bootstrap: &mut AutoBootstrap,
    ) -> Option<EventType> {
        match event {
            SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => {
                Some(EventType::FloodsubEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => {
                Some(EventType::MdnsEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(event)) => {
                Some(EventType::PingEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(event)) => {
                Some(EventType::RequestResponseEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::NodeDescription(event)) => {
                Some(EventType::NodeDescriptionEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(event)) => {
                Some(EventType::StorySyncEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::Handshake(event)) => {
                Some(EventType::HandshakeEvent(event))
            }
            SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(event)) => {
                // Update bootstrap status based on DHT events
                update_bootstrap_status(&event, auto_bootstrap, swarm);
                Some(EventType::KadEvent(event))
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                app.add_to_log(format!("Local node is listening on {address}"));
                None
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                self.handle_connection_established(
                    peer_id,
                    &endpoint,
                    swarm,
                    peer_names,
                    sorted_peer_names_cache,
                    local_peer_name,
                )
                .await;
                None
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                self.handle_connection_closed(
                    peer_id,
                    cause.as_ref(),
                    swarm,
                    peer_names,
                    sorted_peer_names_cache,
                    app,
                )
                .await;
                None
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id,
                error,
                connection_id,
                ..
            } => {
                self.handle_outgoing_connection_error(peer_id, &error, &connection_id)
                    .await;
                None
            }
            SwarmEvent::IncomingConnectionError {
                local_addr,
                send_back_addr,
                error,
                connection_id,
                ..
            } => {
                self.handle_incoming_connection_error(
                    &local_addr,
                    &send_back_addr,
                    &error,
                    &connection_id,
                )
                .await;
                None
            }
            SwarmEvent::Dialing {
                peer_id,
                connection_id,
                ..
            } => {
                debug!("Dialing peer: {peer_id:?} (connection id: {connection_id:?})");
                None
            }
            _ => {
                debug!("Unhandled Swarm Event: {event:?}");
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_connection_established(
        &mut self,
        peer_id: PeerId,
        endpoint: &libp2p::core::ConnectedPoint,
        swarm: &mut Swarm<StoryBehaviour>,
        _peer_names: &mut HashMap<PeerId, String>,
        _sorted_peer_names_cache: &mut SortedPeerNamesCache,
        _local_peer_name: &Option<String>,
    ) {
        debug!("Connection established to {peer_id} via {endpoint:?}");

        debug!(
            "Successful connection to peer {} - initiating handshake verification",
            peer_id
        );

        let pending_peer = PendingHandshakePeer {
            peer_id,
            connection_time: Instant::now(),
            endpoint: endpoint.clone(),
        };

        {
            let mut pending_peers = self.pending_handshake_peers.lock().unwrap();
            pending_peers.insert(peer_id, pending_peer);
            debug!("Added peer {} to pending handshake list", peer_id);
        }

        debug!(
            "Initiating handshake with newly connected peer: {}",
            peer_id
        );
        let handshake_request = HandshakeRequest {
            app_name: APP_NAME.to_string(),
            app_version: APP_VERSION.to_string(),
            peer_id: PEER_ID.to_string(),
        };

        let request_id = swarm
            .behaviour_mut()
            .handshake
            .send_request(&peer_id, handshake_request);
        debug!(
            "Handshake request sent to {} (request_id: {:?})",
            peer_id, request_id
        );

        // Track successful connection for improved reconnect timing
        track_successful_connection(peer_id);

        debug!(
            "Peer {} awaiting handshake verification before UI display",
            peer_id
        );

    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_connection_closed(
        &self,
        peer_id: PeerId,
        cause: Option<&libp2p::swarm::ConnectionError>,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &mut HashMap<PeerId, String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
        app: &mut App,
    ) {
        debug!("Connection closed to {peer_id}: {cause:?}");

        let error_msg = format!("{:?}", cause);
        if error_msg.contains("Connection refused") || error_msg.contains("os error 111") {
            debug!("Connection refused to {peer_id} - peer may still be starting up, will retry");
        } else {
            debug!("Connection lost to {peer_id}: {cause:?}");
        }

        debug!("Removing peer {peer_id} from floodsub partial view");
        swarm
            .behaviour_mut()
            .floodsub
            .remove_node_from_partial_view(&peer_id);

        {
            let mut pending_peers = self.pending_handshake_peers.lock().unwrap();
            if pending_peers.remove(&peer_id).is_some() {
                debug!(
                    "Removed peer {} from pending handshake list due to disconnection",
                    peer_id
                );
            }
        }

        let was_verified = {
            let mut verified_peers = self.verified_p2p_play_peers.lock().unwrap();
            verified_peers.remove(&peer_id).is_some()
        };

        if let Some(name) = peer_names.remove(&peer_id) {
            debug!("Removed verified peer name '{name}' for disconnected peer {peer_id}");
            sorted_peer_names_cache.update(peer_names);
            app.update_peers(peer_names.clone());
        } else if was_verified {
            debug!(
                "Verified peer {} disconnected but already removed from UI",
                peer_id
            );
        } else {
            debug!("Unverified peer {} disconnected (was not in UI)", peer_id);
        }

        trigger_immediate_connection_maintenance(swarm, &self.error_logger).await;
    }

    async fn handle_outgoing_connection_error(
        &self,
        peer_id: Option<PeerId>,
        error: &libp2p::swarm::DialError,
        connection_id: &libp2p::swarm::ConnectionId,
    ) {
        // Filter out common connection timeout/refused errors to reduce noise
        let should_log_to_ui = match error {
            libp2p::swarm::DialError::Transport(transport_errors) => {
                // Only log to UI for unexpected transport errors, not common connectivity issues
                !transport_errors.iter().any(|(_, e)| {
                    let error_str = e.to_string();
                    error_str.contains("Connection refused")
                        || error_str.contains("timed out")
                        || error_str.contains("No route to host")
                        || error_str.contains("os error 111")
                        || error_str.contains("Network is unreachable")
                        || error_str.contains("Connection reset")
                })
            }
            // Filter out other common dial errors that clutter the UI
            libp2p::swarm::DialError::NoAddresses => false,
            libp2p::swarm::DialError::LocalPeerId { address: _ } => false,
            libp2p::swarm::DialError::WrongPeerId { .. } => false,
            libp2p::swarm::DialError::Aborted => false,
            libp2p::swarm::DialError::Denied { .. } => false,
            _ => {
                // For other dial errors, check if they contain common noisy patterns
                let error_str = error.to_string();
                !(error_str.contains("Multiple dial errors occurred")
                    || error_str.contains("Failed to negotiate transport protocol")
                    || error_str.contains("Unsupported resolved address"))
            }
        };

        crate::log_network_error!(
            self.error_logger,
            "outgoing_connection",
            "Failed to connect to {:?} (connection id: {:?}): {}",
            peer_id,
            connection_id,
            error
        );
    }

    async fn handle_incoming_connection_error(
        &self,
        local_addr: &libp2p::multiaddr::Multiaddr,
        send_back_addr: &libp2p::multiaddr::Multiaddr,
        error: &libp2p::swarm::ListenError,
        connection_id: &libp2p::swarm::ConnectionId,
    ) {
        // Filter out common connection errors to reduce noise
        let should_log_to_ui = {
            let error_str = error.to_string();
            !(error_str.contains("Connection reset")
                || error_str.contains("Broken pipe")
                || error_str.contains("timed out")
                || error_str.contains("Connection refused")
                || error_str.contains("os error 111")
                || error_str.contains("Network is unreachable")
                || error_str.contains("Connection aborted")
                || error_str.contains("Multiple dial errors occurred")
                || error_str.contains("Failed to negotiate transport protocol"))
        };

        crate::log_network_error!(
            self.error_logger,
            "incoming_connection",
            "Failed incoming connection from {} to {} (connection id: {:?}): {}",
            send_back_addr,
            local_addr,
            connection_id,
            error
        );
    }

    async fn process_event(
        &mut self,
        event: EventType,
        app: &mut App,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &mut HashMap<PeerId, String>,
        local_peer_name: &mut Option<String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ) {
        let action_result = handle_event(
            event,
            swarm,
            peer_names,
            self.response_sender.clone(),
            self.story_sender.clone(),
            local_peer_name,
            sorted_peer_names_cache,
            &self.ui_logger,
            &self.error_logger,
            &self.bootstrap_logger,
            &self.dm_config,
            &self.pending_messages,
            &mut self.relay_service,
            &self.network_circuit_breakers,
            &self.pending_handshake_peers,
            &self.verified_p2p_play_peers,
        )
        .await;

        if let Some(action_result) = action_result {
            self.handle_action_result(action_result, app).await;
        }
    }

    async fn cleanup_timed_out_handshakes(&self, swarm: &mut Swarm<StoryBehaviour>) {
        let timeout_duration = Duration::from_secs(HANDSHAKE_TIMEOUT_SECS);
        let mut timed_out_peers = Vec::new();

        {
            let pending_peers = self.pending_handshake_peers.lock().unwrap();
            for (peer_id, pending_peer) in pending_peers.iter() {
                if pending_peer.connection_time.elapsed() > timeout_duration {
                    timed_out_peers.push(*peer_id);
                }
            }
        }

        let timed_out_count = timed_out_peers.len();

        for peer_id in &timed_out_peers {
            // Check if this is a bootstrap peer - be more lenient for bootstrap connections
            let is_bootstrap_peer = {
                let pending_peers = self.pending_handshake_peers.lock().unwrap();
                if let Some(pending_peer) = pending_peers.get(peer_id) {
                    matches!(
                        pending_peer.endpoint,
                        libp2p::core::ConnectedPoint::Dialer { .. }
                    )
                } else {
                    false
                }
            };

            if is_bootstrap_peer {
                debug!(
                    "Bootstrap peer {} handshake timeout after {}s, allowing extra time",
                    peer_id, HANDSHAKE_TIMEOUT_SECS
                );
                // For bootstrap peers, give extra time but log the delay
                self.error_logger.log_error(&format!(
                    "Bootstrap handshake slow with peer {}: {}s elapsed, monitoring...",
                    peer_id, HANDSHAKE_TIMEOUT_SECS
                ));
                continue; // Don't disconnect bootstrap peers on first timeout
            }

            debug!(
                "Handshake with peer {} timed out after {}s, disconnecting",
                peer_id, HANDSHAKE_TIMEOUT_SECS
            );

            let _ = swarm.disconnect_peer_id(*peer_id);

            {
                let mut pending_peers = self.pending_handshake_peers.lock().unwrap();
                pending_peers.remove(peer_id);
            }

            self.error_logger.log_error(&format!(
                "Handshake timeout with peer {}: no response received within {}s",
                peer_id, HANDSHAKE_TIMEOUT_SECS
            ));
        }

        if timed_out_count > 0 {
            debug!("Cleaned up {} timed-out handshakes", timed_out_count);
        }
    }

    async fn handle_action_result(&self, action_result: ActionResult, app: &mut App) {
        match action_result {
            ActionResult::RefreshStories => {
                match storage::read_local_stories().await {
                    Ok(stories) => {
                        app.update_stories(stories);
                        refresh_unread_counts_for_ui(app, &PEER_ID.to_string()).await;
                    }
                    Err(e) => {
                        self.error_logger
                            .log_error(&format!("Failed to refresh stories: {e}"));
                    }
                }
            }
            ActionResult::StartStoryCreation => {
                app.start_story_creation();
            }
            ActionResult::RefreshChannels => {
                match storage::read_subscribed_channels_with_details(&PEER_ID.to_string()).await {
                    Ok(channels) => {
                        app.update_channels(channels);
                    }
                    Err(e) => {
                        self.error_logger
                            .log_error(&format!("Failed to refresh subscribed channels: {e}"));
                    }
                }
            }
            ActionResult::RebroadcastRelayMessage(_) => {
                debug!("Unexpected RebroadcastRelayMessage action result in handle_action_result");
            }
            ActionResult::DirectMessageReceived(direct_message) => {
                if let Err(e) = crate::storage::save_direct_message(&direct_message).await {
                    self.error_logger
                        .log_error(&format!("Failed to save received direct message: {e}"));
                } else {
                    debug!(
                        "Saved received direct message from {}",
                        direct_message.from_peer_id
                    );
                }
                if let Err(e) = self.ui_sender.send(AppEvent::DirectMessage(direct_message)) {
                    self.error_logger
                        .log_error(&format!("Failed to send direct message to UI: {e}"));
                }
            }
            ActionResult::EnterMessageComposition(target_peer) => {
                if let Err(e) = self
                    .ui_sender
                    .send(AppEvent::EnterMessageComposition { target_peer })
                {
                    self.error_logger.log_error(&format!(
                        "Failed to send message composition event to UI: {e}"
                    ));
                }
            }
        }
    }
}

fn update_bootstrap_status(
    kad_event: &libp2p::kad::Event,
    auto_bootstrap: &mut AutoBootstrap,
    swarm: &mut Swarm<StoryBehaviour>,
) {
    match kad_event {
        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => {
            match result {
                libp2p::kad::QueryResult::Bootstrap(Ok(_)) => {
                    // Bootstrap query succeeded - we'll wait for routing table updates to confirm connectivity
                    debug!("Bootstrap query succeeded");
                }
                libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                    auto_bootstrap.mark_failed(format!("Bootstrap query failed: {e:?}"));
                }
                _ => {}
            }
        }
        libp2p::kad::Event::RoutingUpdated {
            is_new_peer: true, ..
        } => {
            let status = auto_bootstrap.status.lock().unwrap();
            let is_in_progress = matches!(
                *status,
                crate::bootstrap::BootstrapStatus::InProgress { .. }
            );
            drop(status); // Release lock before calling mark_connected

            if is_in_progress {
                let peer_count = swarm.behaviour_mut().kad.kbuckets().count();
                debug!("Bootstrap marked as connected with {peer_count} peers in routing table");
                auto_bootstrap.mark_connected(peer_count);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event_processor() -> EventProcessor {
        let (_, ui_rcv) = mpsc::unbounded_channel();
        let (_, ui_log_rcv) = mpsc::unbounded_channel();
        let (_, response_rcv) = mpsc::unbounded_channel();
        let (_, story_rcv) = mpsc::unbounded_channel();
        let (response_sender, _) = mpsc::unbounded_channel();
        let (story_sender, _) = mpsc::unbounded_channel();
        let (ui_sender, _) = mpsc::unbounded_channel();
        let (ui_log_sender, _) = mpsc::unbounded_channel();

        let dm_config = DirectMessageConfig {
            max_retry_attempts: 3,
            retry_interval_seconds: 10,
            enable_connection_retries: true,
            enable_timed_retries: true,
        };

        let pending_messages = Arc::new(Mutex::new(Vec::new()));
        let ui_logger = UILogger::new(ui_log_sender);
        let error_logger = ErrorLogger::new("test_errors.log");
        let bootstrap_logger = BootstrapLogger::new("test_bootstrap.log");

        // Create disabled circuit breakers for testing
        let cb_config = crate::types::NetworkCircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        };
        let network_circuit_breakers =
            crate::network_circuit_breakers::NetworkCircuitBreakers::new(&cb_config);

        // Create default network config for testing
        let network_config = crate::types::NetworkConfig::default();

        EventProcessor::new(
            ui_rcv,
            ui_log_rcv,
            response_rcv,
            story_rcv,
            response_sender,
            story_sender,
            ui_sender,
            &network_config,
            dm_config,
            pending_messages,
            ui_logger,
            error_logger,
            bootstrap_logger,
            None, // No relay service in tests
            network_circuit_breakers,
        )
    }

    #[tokio::test]
    async fn test_event_processor_creation() {
        let event_processor = create_test_event_processor();

        // Test that the EventProcessor can be created without panicking
        assert_eq!(event_processor.dm_config.max_retry_attempts, 3);
        assert_eq!(event_processor.dm_config.retry_interval_seconds, 10);
        assert!(event_processor.dm_config.enable_connection_retries);
        assert!(event_processor.dm_config.enable_timed_retries);
    }
}
