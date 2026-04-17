use crate::bootstrap::{AutoBootstrap, run_auto_bootstrap_with_retry};
use crate::bootstrap_logger::BootstrapLogger;
use crate::error_logger::ErrorLogger;
use crate::event_handlers::{
    self, handle_event, track_successful_connection, trigger_immediate_connection_maintenance,
};
use crate::handlers::{PeerState, SortedPeerNamesCache, UILogger, refresh_unread_counts_for_ui};
use crate::network::{
    APP_NAME, APP_VERSION, HandshakeRequest, PEER_ID, StoryBehaviour, StoryBehaviourEvent,
};
use crate::network_circuit_breakers::NetworkCircuitBreakers;
use crate::relay::RelayService;
use crate::storage;
use crate::storage::mark_story_as_read;
use crate::types::{
    ActionResult, DirectMessageConfig, EventType, NetworkConfig, PendingDirectMessage,
    PendingHandshakePeer,
};
use crate::ui::{App, AppEvent, handle_ui_events};

use libp2p::{PeerId, Swarm, futures::StreamExt, swarm::SwarmEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

const BOOTSTRAP_RETRY_INTERVAL_SECS: u64 = 5;
const BOOTSTRAP_STATUS_LOG_INTERVAL_SECS: u64 = 60;
const DM_RETRY_INTERVAL_SECS: u64 = 10;

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

    // Session-lifetime cache of user-set peer aliases keyed by PeerId.
    // Survives peer disconnections so that the alias is restored immediately
    // on the next connection without waiting for the peer to re-broadcast it.
    known_peer_names: HashMap<PeerId, String>,
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
                network_config.connection_maintenance_interval_seconds,
            )),
            bootstrap_retry_interval: interval(Duration::from_secs(BOOTSTRAP_RETRY_INTERVAL_SECS)),
            bootstrap_status_log_interval: interval(Duration::from_secs(
                BOOTSTRAP_STATUS_LOG_INTERVAL_SECS,
            )),
            dm_retry_interval: interval(Duration::from_secs(DM_RETRY_INTERVAL_SECS)),
            network_health_update_interval: interval(Duration::from_secs(
                network_config.network_health_update_interval_seconds,
            )),
            handshake_timeout_interval: interval(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS / 2)),
            dm_config,
            pending_messages,
            ui_logger,
            error_logger,
            bootstrap_logger,
            relay_service,
            network_circuit_breakers,
            pending_handshake_peers: Arc::new(Mutex::new(HashMap::new())),
            verified_p2p_play_peers: Arc::new(Mutex::new(HashMap::new())),
            known_peer_names: HashMap::new(),
        }
    }

    /// Main event loop
    pub async fn run(
        &mut self,
        app: &mut App,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_state: &mut PeerState,
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
                .select_next_event(main_loop_timeout, app, swarm, peer_state, auto_bootstrap)
                .await;

            if let Some(event) = evt {
                self.process_event(event, app, swarm, peer_state).await;

                // Restore any previously learned aliases for reconnecting peers and
                // learn new aliases from peers that just broadcast their name.
                self.sync_known_peer_names(
                    &mut peer_state.peer_names,
                    &mut peer_state.sorted_peer_names_cache,
                );

                app.update_peers(peer_state.peer_names.clone());
                app.update_local_peer_name(peer_state.local_peer_name.clone());
            }

            // Keep bootstrap status display up-to-date every loop iteration
            app.update_bootstrap_status_display(auto_bootstrap.get_bootstrap_short_status());
        }
    }

    async fn select_next_event(
        &mut self,
        timeout: Duration,
        app: &mut App,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_state: &mut PeerState,
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
            ui_event = self.ui_rcv.recv() => {
                if let Some(event) = ui_event {
                    match event {
                        AppEvent::Input(line) => Some(EventType::Input(line)),
                        AppEvent::Quit => {
                            app.should_quit = true;
                            None
                        }
                        AppEvent::StoryViewed { story_id, channel } => {
                            let _result = mark_story_as_read(story_id, &PEER_ID.to_string(), &channel).await;
                            refresh_unread_counts_for_ui(app, &PEER_ID.to_string()).await;
                            None
                        }
                        AppEvent::DirectMessage(direct_message) => {
                            app.handle_direct_message(direct_message);
                            app.refresh_conversations().await;
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
                        AppEvent::ConversationViewed { peer_id } => {
                            if let Err(e) = crate::storage::mark_conversation_messages_as_read(&peer_id).await {
                                self.error_logger.log_error(&format!("Failed to mark messages as read: {e}"));
                            }
                            app.display_conversation(&peer_id).await;
                            app.refresh_conversations().await;
                            None
                        }
                    }
                } else {
                    None
                }
            }
            response = self.response_rcv.recv() => response.map(EventType::Response),
            story = self.story_rcv.recv() => story.map(EventType::PublishStory),
            _ = self.connection_maintenance_interval.tick() => {
                self.on_connection_maintenance_tick(swarm).await;
                None
            },
            _ = self.bootstrap_retry_interval.tick() => {
                self.on_bootstrap_retry_tick(swarm, auto_bootstrap).await;
                None
            },
            _ = self.bootstrap_status_log_interval.tick() => {
                self.on_bootstrap_status_tick(auto_bootstrap);
                None
            },
            _ = self.dm_retry_interval.tick() => {
                self.on_dm_retry_tick(swarm, &peer_state.peer_names).await;
                None
            },
            _ = self.network_health_update_interval.tick() => {
                self.on_network_health_tick(app).await;
                None
            },
            _ = self.handshake_timeout_interval.tick() => {
                self.cleanup_timed_out_handshakes(swarm).await;
                None
            },
            event = swarm.select_next_some() => {
                self.handle_swarm_event(event, swarm, peer_state, app, auto_bootstrap).await
            },
        }
    }

    // -------------------------------------------------------------------------
    // Timer-tick handlers (extracted from select_next_event for clarity)
    // -------------------------------------------------------------------------

    async fn on_connection_maintenance_tick(&self, swarm: &mut Swarm<StoryBehaviour>) {
        event_handlers::maintain_connections(swarm, &self.error_logger).await;
    }

    async fn on_bootstrap_retry_tick(
        &self,
        swarm: &mut Swarm<StoryBehaviour>,
        auto_bootstrap: &mut AutoBootstrap,
    ) {
        if auto_bootstrap.should_retry() && auto_bootstrap.is_retry_time() {
            run_auto_bootstrap_with_retry(
                auto_bootstrap,
                swarm,
                &self.bootstrap_logger,
                &self.error_logger,
                &self.ui_logger,
            )
            .await;
        }
    }

    fn on_bootstrap_status_tick(&self, auto_bootstrap: &AutoBootstrap) {
        if auto_bootstrap.try_has_started().unwrap_or(false) {
            let status_msg = auto_bootstrap.get_status_string();
            self.bootstrap_logger.log_status(&status_msg);
        }
    }

    async fn on_dm_retry_tick(
        &self,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_names: &HashMap<PeerId, String>,
    ) {
        event_handlers::process_pending_messages(
            swarm,
            &self.dm_config,
            &self.pending_messages,
            peer_names,
            &self.ui_logger,
        )
        .await;
    }

    async fn on_network_health_tick(&self, app: &mut App) {
        let health_summary = self.network_circuit_breakers.health_summary().await;
        app.update_network_health(health_summary);
    }

    // -------------------------------------------------------------------------

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<StoryBehaviourEvent>,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_state: &mut PeerState,
        app: &mut App,
        auto_bootstrap: &mut AutoBootstrap,
    ) -> Option<EventType> {
        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                // Handle special side-effects before mapping to EventType.
                match &behaviour_event {
                    StoryBehaviourEvent::Mdns(_) => {
                        // Track mDNS discovery status for the TUI status bar
                        app.mdns_active =
                            swarm.behaviour().mdns.discovered_nodes().next().is_some();
                    }
                    StoryBehaviourEvent::Kad(kad_event) => {
                        // Update bootstrap status based on DHT events
                        update_bootstrap_status(kad_event, auto_bootstrap, swarm);
                    }
                    _ => {}
                }
                Some(map_behaviour_to_event(behaviour_event))
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                app.add_to_log(format!("Local node is listening on {address}"));
                None
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                self.handle_connection_established(peer_id, &endpoint, swarm)
                    .await;
                None
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                self.handle_connection_closed(peer_id, cause.as_ref(), swarm, peer_state, app)
                    .await;
                None
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id,
                error,
                connection_id,
                ..
            } => {
                self.handle_outgoing_connection_error(peer_id, &error, &connection_id);
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
                );
                None
            }
            SwarmEvent::Dialing { .. } => None,
            _ => None,
        }
    }

    async fn handle_connection_established(
        &mut self,
        peer_id: PeerId,
        endpoint: &libp2p::core::ConnectedPoint,
        swarm: &mut Swarm<StoryBehaviour>,
    ) {
        let pending_peer = PendingHandshakePeer {
            peer_id,
            connection_time: Instant::now(),
            endpoint: endpoint.clone(),
        };

        {
            let mut pending_peers = self.pending_handshake_peers.lock().unwrap();
            pending_peers.insert(peer_id, pending_peer);
        }

        let handshake_request = HandshakeRequest {
            app_name: APP_NAME.to_string(),
            app_version: APP_VERSION.to_string(),
            peer_id: PEER_ID.to_string(),
            wasm_capable: true, // This node supports WASM capability advertisement
        };

        let _ = swarm
            .behaviour_mut()
            .handshake
            .send_request(&peer_id, handshake_request);

        track_successful_connection(peer_id);

        // Persist peer connection to database.
        // Only store the multiaddr for outbound (Dialer) connections so that
        // on the next startup we only attempt to reconnect to peers we
        // explicitly dialled, not to arbitrary inbound connections.
        let multiaddr = match endpoint {
            libp2p::core::ConnectedPoint::Dialer { address, .. } => Some(address.to_string()),
            libp2p::core::ConnectedPoint::Listener { .. } => None,
        };
        if let Err(e) =
            storage::upsert_peer(&peer_id.to_string(), None, multiaddr.as_deref(), true).await
        {
            self.error_logger
                .log_error(&format!("Failed to persist peer connection {peer_id}: {e}"));
        }
    }

    async fn handle_connection_closed(
        &mut self,
        peer_id: PeerId,
        _cause: Option<&libp2p::swarm::ConnectionError>,
        swarm: &mut Swarm<StoryBehaviour>,
        peer_state: &mut PeerState,
        app: &mut App,
    ) {
        swarm
            .behaviour_mut()
            .floodsub
            .remove_node_from_partial_view(&peer_id);

        {
            let mut pending_peers = self.pending_handshake_peers.lock().unwrap();
            pending_peers.remove(&peer_id);
        }

        {
            let mut verified_peers = self.verified_p2p_play_peers.lock().unwrap();
            verified_peers.remove(&peer_id);
        }

        if let Some(name) = peer_state.peer_names.remove(&peer_id) {
            // Persist user-set aliases so they are restored on the next connection
            // without waiting for the peer to re-broadcast its name.
            self.remember_alias_on_disconnect(peer_id, name);
            peer_state
                .sorted_peer_names_cache
                .update(&peer_state.peer_names);
            app.update_peers(peer_state.peer_names.clone());
        }

        trigger_immediate_connection_maintenance(swarm, &self.error_logger).await;

        // Mark peer as disconnected in database
        if let Err(e) = storage::mark_peer_disconnected(&peer_id.to_string()).await {
            self.error_logger.log_error(&format!(
                "Failed to mark peer {peer_id} as disconnected: {e}"
            ));
        }
    }

    /// Returns `true` when `name` is a user-set alias rather than the auto-generated
    /// `"Peer_<peer_id>"` placeholder.  User-set names are validated to be at most
    /// `PEER_NAME_MAX` characters (currently 30), while the placeholder is always
    /// longer (5 + ~52 chars for the base58-encoded PeerId).
    fn is_user_set_peer_name(name: &str) -> bool {
        name.len() <= crate::validation::ContentLimits::PEER_NAME_MAX
    }

    /// Saves the peer's alias into `known_peer_names` when the alias is a user-set
    /// value (not a placeholder).  Called on disconnection so the alias survives
    /// across reconnections within the same session.
    fn remember_alias_on_disconnect(&mut self, peer_id: PeerId, name: String) {
        if Self::is_user_set_peer_name(&name) {
            self.known_peer_names.insert(peer_id, name);
        }
    }

    /// Overwrites placeholder names in `peer_names` with the previously cached alias
    /// for each peer.  Returns `true` if at least one entry was updated.
    fn restore_aliases_for_connected_peers(
        &self,
        peer_names: &mut HashMap<PeerId, String>,
    ) -> bool {
        let mut names_updated = false;
        for (peer_id, name) in peer_names.iter_mut() {
            if let Some(known_name) = self.known_peer_names.get(peer_id)
                && name != known_name
            {
                *name = known_name.clone();
                names_updated = true;
            }
        }
        names_updated
    }

    /// Keeps `known_peer_names` and `peer_names` in sync so that user-set aliases
    /// survive peer disconnections within a session.
    ///
    /// * **Learn**: any real alias present in `peer_names` is saved into
    ///   `known_peer_names` so it is available if the peer disconnects and reconnects.
    /// * **Restore**: any peer in `peer_names` that still has the default placeholder
    ///   gets its previously learned alias applied immediately, without waiting for
    ///   the peer to re-broadcast its name.
    fn sync_known_peer_names(
        &mut self,
        peer_names: &mut HashMap<PeerId, String>,
        sorted_peer_names_cache: &mut SortedPeerNamesCache,
    ) {
        // Learn new / updated real aliases from the active peer map.
        for (peer_id, name) in peer_names.iter() {
            if Self::is_user_set_peer_name(name) {
                let known = self.known_peer_names.get(peer_id);
                // Insert when the peer has no cached name or when their name has changed.
                if known.is_none_or(|k| k != name) {
                    self.known_peer_names.insert(*peer_id, name.clone());
                }
            }
        }

        // Restore known aliases for peers that joined with a placeholder.
        if self.restore_aliases_for_connected_peers(peer_names) {
            sorted_peer_names_cache.update(peer_names);
        }
    }

    fn handle_outgoing_connection_error(
        &self,
        peer_id: Option<PeerId>,
        error: &libp2p::swarm::DialError,
        connection_id: &libp2p::swarm::ConnectionId,
    ) {
        crate::log_network_error!(
            self.error_logger,
            "outgoing_connection",
            "Failed to connect to {:?} (connection id: {:?}): {}",
            peer_id,
            connection_id,
            error
        );
    }

    fn handle_incoming_connection_error(
        &self,
        local_addr: &libp2p::multiaddr::Multiaddr,
        send_back_addr: &libp2p::multiaddr::Multiaddr,
        error: &libp2p::swarm::ListenError,
        connection_id: &libp2p::swarm::ConnectionId,
    ) {
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
        peer_state: &mut PeerState,
    ) {
        let action_result = handle_event(
            event,
            swarm,
            &mut peer_state.peer_names,
            self.response_sender.clone(),
            self.story_sender.clone(),
            &mut peer_state.local_peer_name,
            &mut peer_state.sorted_peer_names_cache,
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
            self.handle_action_result(action_result, app, &peer_state.peer_names)
                .await;
        }
    }

    async fn cleanup_timed_out_handshakes(&self, swarm: &mut Swarm<StoryBehaviour>) {
        let timeout_duration = Duration::from_secs(HANDSHAKE_TIMEOUT_SECS);

        // Collect all timed-out peers and their metadata in a single lock pass.
        let timed_out: Vec<(PeerId, bool)> = {
            let pending_peers = self.pending_handshake_peers.lock().unwrap();
            pending_peers
                .iter()
                .filter(|(_, p)| p.connection_time.elapsed() > timeout_duration)
                .map(|(id, p)| {
                    let is_bootstrap_peer =
                        matches!(p.endpoint, libp2p::core::ConnectedPoint::Dialer { .. });
                    (*id, is_bootstrap_peer)
                })
                .collect()
        };

        for (peer_id, is_bootstrap_peer) in timed_out {
            if is_bootstrap_peer {
                self.error_logger.log_error(&format!(
                    "Bootstrap handshake slow with peer {}: {}s elapsed, monitoring...",
                    peer_id, HANDSHAKE_TIMEOUT_SECS
                ));
                continue; // Don't disconnect bootstrap peers on first timeout
            }

            let _ = swarm.disconnect_peer_id(peer_id);
            self.pending_handshake_peers
                .lock()
                .unwrap()
                .remove(&peer_id);

            self.error_logger.log_error(&format!(
                "Handshake timeout with peer {}: no response received within {}s",
                peer_id, HANDSHAKE_TIMEOUT_SECS
            ));
        }
    }

    async fn handle_action_result(
        &self,
        action_result: ActionResult,
        app: &mut App,
        peer_names: &HashMap<PeerId, String>,
    ) {
        match action_result {
            ActionResult::RefreshStories => match storage::read_local_stories().await {
                Ok(stories) => {
                    app.update_stories(stories);
                    refresh_unread_counts_for_ui(app, &PEER_ID.to_string()).await;
                }
                Err(e) => {
                    self.error_logger
                        .log_error(&format!("Failed to refresh stories: {e}"));
                }
            },
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
            ActionResult::RebroadcastRelayMessage(_) => {}
            ActionResult::DirectMessageReceived(direct_message) => {
                if let Err(e) =
                    crate::storage::save_direct_message(&direct_message, Some(peer_names)).await
                {
                    self.error_logger
                        .log_error(&format!("Failed to save received direct message: {e}"));
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

/// Maps a `StoryBehaviourEvent` to its corresponding `EventType`.
/// Special-case side effects (mDNS active tracking, Kademlia bootstrap status) are
/// handled by the caller before invoking this function.
fn map_behaviour_to_event(event: StoryBehaviourEvent) -> EventType {
    match event {
        StoryBehaviourEvent::Floodsub(e) => EventType::FloodsubEvent(e),
        StoryBehaviourEvent::Mdns(e) => EventType::MdnsEvent(e),
        StoryBehaviourEvent::Ping(e) => EventType::PingEvent(e),
        StoryBehaviourEvent::RequestResponse(e) => EventType::RequestResponseEvent(e),
        StoryBehaviourEvent::NodeDescription(e) => EventType::NodeDescriptionEvent(e),
        StoryBehaviourEvent::StorySync(e) => EventType::StorySyncEvent(e),
        StoryBehaviourEvent::Handshake(e) => EventType::HandshakeEvent(e),
        StoryBehaviourEvent::Kad(e) => EventType::KadEvent(e),
        StoryBehaviourEvent::WasmCapabilities(e) => EventType::WasmCapabilitiesEvent(e),
        StoryBehaviourEvent::WasmExecution(e) => EventType::WasmExecutionEvent(e),
    }
}

fn update_bootstrap_status(
    kad_event: &libp2p::kad::Event,
    auto_bootstrap: &mut AutoBootstrap,
    swarm: &mut Swarm<StoryBehaviour>,
) {
    match kad_event {
        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => match result {
            libp2p::kad::QueryResult::Bootstrap(Ok(_)) => {}
            libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                auto_bootstrap.mark_failed(format!("Bootstrap query failed: {e:?}"));
            }
            _ => {}
        },
        libp2p::kad::Event::RoutingUpdated {
            is_new_peer: true, ..
        } => {
            if auto_bootstrap.is_in_progress() {
                let peer_count = swarm.behaviour_mut().kad.kbuckets().count();
                auto_bootstrap.mark_connected(peer_count);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Shared test fixture
    // -----------------------------------------------------------------------

    struct TestEventProcessorBuilder {
        network_config: crate::types::NetworkConfig,
        dm_config: DirectMessageConfig,
    }

    impl Default for TestEventProcessorBuilder {
        fn default() -> Self {
            Self {
                network_config: crate::types::NetworkConfig::default(),
                dm_config: DirectMessageConfig {
                    max_retry_attempts: 3,
                    retry_interval_seconds: 10,
                    enable_connection_retries: true,
                    enable_timed_retries: true,
                },
            }
        }
    }

    impl TestEventProcessorBuilder {
        fn with_network_config(mut self, config: crate::types::NetworkConfig) -> Self {
            self.network_config = config;
            self
        }

        fn build(self) -> EventProcessor {
            let (_, ui_rcv) = mpsc::unbounded_channel();
            let (_, ui_log_rcv) = mpsc::unbounded_channel();
            let (_, response_rcv) = mpsc::unbounded_channel();
            let (_, story_rcv) = mpsc::unbounded_channel();
            let (response_sender, _) = mpsc::unbounded_channel();
            let (story_sender, _) = mpsc::unbounded_channel();
            let (ui_sender, _) = mpsc::unbounded_channel();
            let (ui_log_sender, _) = mpsc::unbounded_channel();

            let pending_messages = Arc::new(Mutex::new(Vec::new()));
            let ui_logger = UILogger::new(ui_log_sender);
            let error_logger = ErrorLogger::new("test_errors.log");
            let bootstrap_logger = BootstrapLogger::new("test_bootstrap.log");

            let cb_config = crate::types::NetworkCircuitBreakerConfig {
                enabled: false,
                ..Default::default()
            };
            let network_circuit_breakers =
                crate::network_circuit_breakers::NetworkCircuitBreakers::new(&cb_config);

            EventProcessor::new(
                ui_rcv,
                ui_log_rcv,
                response_rcv,
                story_rcv,
                response_sender,
                story_sender,
                ui_sender,
                &self.network_config,
                self.dm_config,
                pending_messages,
                ui_logger,
                error_logger,
                bootstrap_logger,
                None, // No relay service in tests
                network_circuit_breakers,
            )
        }
    }

    fn create_test_event_processor() -> EventProcessor {
        TestEventProcessorBuilder::default().build()
    }

    // -----------------------------------------------------------------------
    // Basic creation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_event_processor_creation() {
        let event_processor = create_test_event_processor();

        // Test that the EventProcessor can be created without panicking
        assert_eq!(event_processor.dm_config.max_retry_attempts, 3);
        assert_eq!(event_processor.dm_config.retry_interval_seconds, 10);
        assert!(event_processor.dm_config.enable_connection_retries);
        assert!(event_processor.dm_config.enable_timed_retries);
    }

    // -----------------------------------------------------------------------
    // Tests for session-lifetime peer alias persistence
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_user_set_peer_name_detects_placeholders() {
        let peer_id = PeerId::random();
        let default_name = format!("Peer_{peer_id}");
        assert!(
            !EventProcessor::is_user_set_peer_name(&default_name),
            "default placeholder should NOT be treated as a user-set name"
        );
    }

    #[test]
    fn test_is_user_set_peer_name_accepts_real_aliases() {
        assert!(EventProcessor::is_user_set_peer_name("Alice"));
        assert!(EventProcessor::is_user_set_peer_name("bob_123"));
        // Exact boundary: a 30-character name should be accepted.
        assert!(EventProcessor::is_user_set_peer_name(&"x".repeat(30)));
        // One over the boundary should be rejected.
        assert!(!EventProcessor::is_user_set_peer_name(&"x".repeat(31)));
    }

    #[tokio::test]
    async fn test_alias_persists_after_disconnect_and_reconnect() {
        let mut ep = create_test_event_processor();
        let mut peer_names: HashMap<PeerId, String> = HashMap::new();
        let mut cache = SortedPeerNamesCache::new();
        let peer_id = PeerId::random();

        // First connection: peer broadcasts real alias.
        peer_names.insert(peer_id, "Alice".to_string());
        ep.sync_known_peer_names(&mut peer_names, &mut cache);
        assert_eq!(
            ep.known_peer_names.get(&peer_id),
            Some(&"Alice".to_string())
        );

        // Disconnection: save alias via the helper and remove peer.
        if let Some(name) = peer_names.remove(&peer_id) {
            ep.remember_alias_on_disconnect(peer_id, name);
        }
        assert!(peer_names.is_empty(), "peer should be gone from active map");
        assert_eq!(
            ep.known_peer_names.get(&peer_id),
            Some(&"Alice".to_string()),
            "known cache should still hold the alias after disconnect"
        );

        // Reconnection with placeholder: alias is restored by sync.
        peer_names.insert(peer_id, format!("Peer_{peer_id}"));
        ep.sync_known_peer_names(&mut peer_names, &mut cache);
        assert_eq!(
            peer_names.get(&peer_id),
            Some(&"Alice".to_string()),
            "alias should be restored on reconnection without waiting for re-broadcast"
        );
    }

    #[tokio::test]
    async fn test_alias_update_propagates_to_known_cache() {
        let mut ep = create_test_event_processor();
        let mut peer_names: HashMap<PeerId, String> = HashMap::new();
        let mut cache = SortedPeerNamesCache::new();
        let peer_id = PeerId::random();

        // Peer connects and sets initial alias.
        peer_names.insert(peer_id, "Alice".to_string());
        ep.sync_known_peer_names(&mut peer_names, &mut cache);
        assert_eq!(
            ep.known_peer_names.get(&peer_id),
            Some(&"Alice".to_string())
        );

        // Peer changes alias to "Bob" in the same session.
        peer_names.insert(peer_id, "Bob".to_string());
        ep.sync_known_peer_names(&mut peer_names, &mut cache);

        // Both the active map and the cache should show the updated alias.
        assert_eq!(ep.known_peer_names.get(&peer_id), Some(&"Bob".to_string()));
        assert_eq!(peer_names.get(&peer_id), Some(&"Bob".to_string()));
    }

    #[tokio::test]
    async fn test_peer_without_alias_stays_as_placeholder() {
        let mut ep = create_test_event_processor();
        let mut peer_names: HashMap<PeerId, String> = HashMap::new();
        let mut cache = SortedPeerNamesCache::new();
        let peer_id = PeerId::random();
        let placeholder = format!("Peer_{peer_id}");

        // Peer connects but never broadcasts a real alias.
        peer_names.insert(peer_id, placeholder.clone());
        ep.sync_known_peer_names(&mut peer_names, &mut cache);

        assert!(
            ep.known_peer_names.is_empty(),
            "placeholder should not pollute the known cache"
        );
        assert_eq!(peer_names.get(&peer_id), Some(&placeholder));
    }

    #[tokio::test]
    async fn test_multiple_peers_independent_caching() {
        let mut ep = create_test_event_processor();
        let mut peer_names: HashMap<PeerId, String> = HashMap::new();
        let mut cache = SortedPeerNamesCache::new();
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();

        // Both peers connect and set aliases.
        peer_names.insert(peer_a, "Alice".to_string());
        peer_names.insert(peer_b, "Bob".to_string());
        ep.sync_known_peer_names(&mut peer_names, &mut cache);

        // Only peer_a disconnects.
        if let Some(name) = peer_names.remove(&peer_a) {
            ep.remember_alias_on_disconnect(peer_a, name);
        }

        // peer_b is still active; peer_a's alias is in the cache.
        assert_eq!(peer_names.get(&peer_b), Some(&"Bob".to_string()));
        assert_eq!(ep.known_peer_names.get(&peer_a), Some(&"Alice".to_string()));

        // peer_a reconnects with placeholder.
        peer_names.insert(peer_a, format!("Peer_{peer_a}"));
        ep.sync_known_peer_names(&mut peer_names, &mut cache);

        // peer_a gets their alias back; peer_b is unaffected.
        assert_eq!(peer_names.get(&peer_a), Some(&"Alice".to_string()));
        assert_eq!(peer_names.get(&peer_b), Some(&"Bob".to_string()));
    }

    #[tokio::test]
    async fn test_connection_maintenance_interval_uses_config() {
        // Use a distinctive non-default value to prove the config is wired, not hardcoded.
        let network_config = crate::types::NetworkConfig {
            connection_maintenance_interval_seconds: 120,
            ..Default::default()
        };
        let ep = TestEventProcessorBuilder::default()
            .with_network_config(network_config)
            .build();

        assert_eq!(
            ep.connection_maintenance_interval.period(),
            Duration::from_secs(120),
            "connection_maintenance_interval should reflect network_config value, not a hardcoded constant"
        );
    }
}
