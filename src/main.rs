mod bootstrap;
mod bootstrap_logger;
mod error_logger;
mod event_handlers;
mod handlers;
mod migrations;
mod network;
mod storage;
mod types;
mod ui;

use bootstrap::{AutoBootstrap, run_auto_bootstrap_with_retry};
use bootstrap_logger::BootstrapLogger;
use error_logger::ErrorLogger;
use event_handlers::handle_event;
use handlers::SortedPeerNamesCache;
use network::{PEER_ID, StoryBehaviourEvent, TOPIC, create_swarm};
use storage::{
    ensure_bootstrap_config_exists, ensure_stories_file_exists, load_bootstrap_config,
    load_local_peer_name,
};
use types::{ActionResult, EventType, PeerName};
use ui::{App, AppEvent, handle_ui_events};

use bytes::Bytes;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, futures::StreamExt};
use log::{debug, error};
use std::collections::HashMap;
use std::process;
use tokio::sync::mpsc;

/// Update bootstrap status based on DHT events
fn update_bootstrap_status(
    kad_event: &libp2p::kad::Event,
    auto_bootstrap: &mut AutoBootstrap,
    swarm: &mut Swarm<network::StoryBehaviour>,
) {
    match kad_event {
        libp2p::kad::Event::OutboundQueryProgressed { result, .. } => {
            match result {
                libp2p::kad::QueryResult::Bootstrap(Ok(_)) => {
                    // Bootstrap query succeeded - we'll wait for routing table updates to confirm connectivity
                    debug!("Bootstrap query succeeded");
                }
                libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                    auto_bootstrap.mark_failed(format!("Bootstrap query failed: {:?}", e));
                }
                _ => {}
            }
        }
        libp2p::kad::Event::RoutingUpdated {
            is_new_peer: true, ..
        } => {
            // New peer added to routing table - this indicates successful DHT connectivity
            // We'll count peers by checking the current status and updating if needed
            let status = auto_bootstrap.status.lock().unwrap();
            let is_in_progress = matches!(*status, bootstrap::BootstrapStatus::InProgress { .. });
            drop(status); // Release lock before calling mark_connected

            if is_in_progress {
                // Get the actual number of peers in the routing table
                let peer_count = swarm.behaviour_mut().kad.kbuckets().count();
                debug!(
                    "Bootstrap marked as connected with {} peers in routing table",
                    peer_count
                );
                auto_bootstrap.mark_connected(peer_count);
            }
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    // Set up custom logger that filters libp2p errors from console but logs them to file
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .filter_module("p2p_play", log::LevelFilter::Info) // Allow our app's info messages
        .filter_module("libp2p", log::LevelFilter::Warn)
        .filter_module("libp2p_swarm", log::LevelFilter::Error)
        .filter_module("libp2p_tcp", log::LevelFilter::Error)
        .filter_module("libp2p_noise", log::LevelFilter::Error)
        .filter_module("libp2p_yamux", log::LevelFilter::Error)
        .filter_module("multistream_select", log::LevelFilter::Error)
        .init();

    debug!("Peer Id: {}", PEER_ID.clone());

    // Initialize the UI
    let mut app = match App::new() {
        Ok(app) => app,
        Err(e) => {
            error!("Failed to initialize UI: {}", e);
            process::exit(1);
        }
    };

    // Ensure stories.json file exists
    if let Err(e) = ensure_stories_file_exists().await {
        error!("Failed to initialize stories file: {}", e);
        let _ = app.cleanup();
        process::exit(1);
    }

    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();
    let (story_sender, mut story_rcv) = mpsc::unbounded_channel();
    let (ui_sender, mut ui_rcv) = mpsc::unbounded_channel();
    let (ui_log_sender, mut ui_log_rcv) = mpsc::unbounded_channel();

    // Create UI logger
    let ui_logger = handlers::UILogger::new(ui_log_sender);

    // Create error logger that writes to file
    let error_logger = ErrorLogger::new("errors.log");

    // Create bootstrap logger that writes to file
    let bootstrap_logger = BootstrapLogger::new("bootstrap.log");

    // Create a timer for periodic connection maintenance
    let mut connection_maintenance_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(30));

    let mut swarm = create_swarm().expect("Failed to create swarm");

    // Storage for peer names (peer_id -> alias)
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();

    // Cache for sorted peer names to avoid repeated sorting on every direct message
    let mut sorted_peer_names_cache = SortedPeerNamesCache::new();

    // Load saved peer name if it exists
    let mut local_peer_name: Option<String> = match load_local_peer_name().await {
        Ok(saved_name) => {
            if let Some(ref name) = saved_name {
                debug!("Loaded saved peer name: {}", name);
                app.add_to_log(format!("Loaded saved peer name: {}", name));
                app.update_local_peer_name(saved_name.clone());
            }
            saved_name
        }
        Err(e) => {
            error!("Failed to load saved peer name: {}", e);
            app.add_to_log(format!("Failed to load saved peer name: {}", e));
            None
        }
    };

    // Ensure bootstrap config file exists and load it
    if let Err(e) = ensure_bootstrap_config_exists().await {
        error!("Failed to initialize bootstrap config: {}", e);
        bootstrap_logger.log_error(&format!("Failed to initialize bootstrap config: {}", e));
    }

    // Load bootstrap configuration
    let _bootstrap_config = match load_bootstrap_config().await {
        Ok(config) => {
            debug!(
                "Loaded bootstrap config with {} peers",
                config.bootstrap_peers.len()
            );
            bootstrap_logger.log_init(&format!(
                "Loaded bootstrap config with {} peers",
                config.bootstrap_peers.len()
            ));
            Some(config)
        }
        Err(e) => {
            error!("Failed to load bootstrap config: {}", e);
            bootstrap_logger.log_error(&format!("Failed to load bootstrap config: {}", e));
            None
        }
    };

    // Initialize automatic bootstrap
    let mut auto_bootstrap = AutoBootstrap::new();
    auto_bootstrap.initialize(&bootstrap_logger).await;

    // Create a timer for automatic bootstrap retry
    let mut bootstrap_retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    // Create a timer for periodic bootstrap status logging
    let mut bootstrap_status_log_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(30));

    // Auto-subscribe to general channel if not already subscribed
    match storage::read_subscribed_channels(&PEER_ID.to_string()).await {
        Ok(subscriptions) => {
            if !subscriptions.contains(&"general".to_string()) {
                if let Err(e) = storage::subscribe_to_channel(&PEER_ID.to_string(), "general").await
                {
                    error!("Failed to auto-subscribe to general channel: {}", e);
                } else {
                    debug!("Auto-subscribed to general channel");
                }
            }
        }
        Err(e) => {
            error!("Failed to check subscriptions: {}", e);
            // Try to subscribe to general anyway
            if let Err(e) = storage::subscribe_to_channel(&PEER_ID.to_string(), "general").await {
                error!("Failed to auto-subscribe to general channel: {}", e);
            }
        }
    }

    // Load initial stories and update UI
    match storage::read_local_stories().await {
        Ok(stories) => {
            debug!("Loaded {} local stories", stories.len());
            app.update_local_stories(stories);
        }
        Err(e) => {
            error!("Failed to load local stories: {}", e);
            app.add_to_log(format!("Failed to load local stories: {}", e));
        }
    }
    // Windows fix for port in use
    #[cfg(windows)]
    let listen_addr = "/ip4/127.0.0.1/tcp/0"; // Bind to localhost only on Windows to reduce conflicts

    #[cfg(not(windows))]
    let listen_addr = "/ip4/0.0.0.0/tcp/0"; // Bind to all interfaces on Unix

    Swarm::listen_on(
        &mut swarm,
        listen_addr.parse().expect("can get a local socket"),
    )
    .expect("swarm can be started");

    // Main application loop
    loop {
        // Handle UI events first to ensure responsiveness
        if let Err(e) = handle_ui_events(&mut app, ui_sender.clone()).await {
            error!("UI event handling error: {}", e);
        }

        // Draw the UI
        if let Err(e) = app.draw() {
            error!("UI drawing error: {}", e);
        }

        // Check if we should quit
        if app.should_quit {
            break;
        }

        // Yield control to allow other tasks to run
        tokio::task::yield_now().await;

        #[cfg(windows)]
        let main_loop_timeout = std::time::Duration::from_millis(100); // Slower on Windows

        #[cfg(not(windows))]
        let main_loop_timeout = std::time::Duration::from_millis(50); // Keep existing on Unix

        let evt = {
            tokio::select! {
                // Shorter timeout to ensure UI responsiveness
                _ = tokio::time::sleep(main_loop_timeout) => {
                    None
                }
                ui_log_msg = ui_log_rcv.recv() => {
                    if let Some(msg) = ui_log_msg {
                        app.add_to_log(msg);
                    }
                    None
                }
                // UI events have higher priority - they are processed immediately
                ui_event = ui_rcv.recv() => {
                    if let Some(event) = ui_event {
                        match event {
                            AppEvent::Input(line) => Some(EventType::Input(line)),
                            AppEvent::Quit => {
                                debug!("Quit event received in main loop");
                                app.should_quit = true;
                                break;
                            }
                            AppEvent::Log(msg) => {
                                app.add_to_log(msg);
                                None
                            }
                            AppEvent::PeerUpdate(peers) => {
                                app.update_peers(peers);
                                None
                            }
                            AppEvent::StoriesUpdate(stories) => {
                                app.update_local_stories(stories);
                                None
                            }
                            AppEvent::ReceivedStoriesUpdate(stories) => {
                                app.update_received_stories(stories);
                                None
                            }
                            AppEvent::PeerNameUpdate(name) => {
                                app.update_local_peer_name(name);
                                None
                            }
                            AppEvent::DirectMessage(dm) => {
                                app.handle_direct_message(dm);
                                None
                            }
                        }
                    } else {
                        None
                    }
                }
                response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
                story = story_rcv.recv() => Some(EventType::PublishStory(story.expect("story exists"))),
                _ = connection_maintenance_interval.tick() => {
                    // Periodic connection maintenance - spawn to background to avoid blocking
                    event_handlers::maintain_connections(&mut swarm).await;
                    None
                },
                _ = bootstrap_retry_interval.tick() => {
                    // Automatic bootstrap retry - only if should retry and time is right
                    if auto_bootstrap.should_retry() && auto_bootstrap.is_retry_time() {
                        run_auto_bootstrap_with_retry(&mut auto_bootstrap, &mut swarm, &bootstrap_logger).await;
                    }
                    None
                },
                _ = bootstrap_status_log_interval.tick() => {
                    // Periodically log bootstrap status - use try_lock to avoid blocking
                    if let Ok(status) = auto_bootstrap.status.try_lock() {
                        if !matches!(*status, bootstrap::BootstrapStatus::NotStarted) {
                            drop(status); // Release lock before expensive operation
                            let status_msg = auto_bootstrap.get_status_string();
                            bootstrap_logger.log_status(&status_msg);
                        }
                    }
                    None
                },
                // Network events are processed but heavy operations are spawned to background
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => Some(EventType::FloodsubEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => Some(EventType::MdnsEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(event)) => Some(EventType::PingEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(event)) => Some(EventType::RequestResponseEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::NodeDescription(event)) => Some(EventType::NodeDescriptionEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(event)) => {
                            // Update bootstrap status based on DHT events
                            update_bootstrap_status(&event, &mut auto_bootstrap, &mut swarm);
                            Some(EventType::KadEvent(event))
                        },
                        SwarmEvent::NewListenAddr { address, .. } => {
                            debug!("Local node is listening on {}", address);
                            app.add_to_log(format!("Local node is listening on {}", address));
                            None
                        },
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            debug!("Connection established to {} via {:?}", peer_id, endpoint);
                            // Connection status is now visible in the Connected Peers section
                            debug!("Adding peer {} to floodsub partial view", peer_id);
                            swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer_id);

                            // If we have a local peer name set, broadcast it to the newly connected peer
                            if let Some(ref name) = local_peer_name {
                                let peer_name = PeerName::new(PEER_ID.to_string(), name.clone());
                                let json = serde_json::to_string(&peer_name).expect("can jsonify peer name");
                                let json_bytes = Bytes::from(json.into_bytes());
                                swarm.behaviour_mut().floodsub.publish(TOPIC.clone(), json_bytes);
                                debug!("Sent local peer name '{}' to newly connected peer {}", name, peer_id);
                            }

                            None
                        },
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            debug!("Connection closed to {}: {:?}", peer_id, cause);
                            // Connection status is now visible in the Connected Peers section
                            debug!("Removing peer {} from floodsub partial view", peer_id);
                            swarm.behaviour_mut().floodsub.remove_node_from_partial_view(&peer_id);

                            // Remove the peer name when connection is closed
                            if let Some(name) = peer_names.remove(&peer_id) {
                                debug!("Removed peer name '{}' for disconnected peer {}", name, peer_id);
                                // Update the cache since peer names changed
                                sorted_peer_names_cache.update(&peer_names);
                                app.update_peers(peer_names.clone());
                            }

                            None
                        },
                        SwarmEvent::OutgoingConnectionError { peer_id, error, connection_id, .. } => {
                            // Log to file instead of console to avoid UI spam
                            log_network_error!(error_logger, "outgoing_connection", "Failed to connect to {:?} (connection id: {:?}): {}", peer_id, connection_id, error);
                            None
                        },
                        SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, connection_id, .. } => {
                            // Log to file instead of console to avoid UI spam
                            log_network_error!(error_logger, "incoming_connection", "Failed incoming connection from {} to {} (connection id: {:?}): {}", send_back_addr, local_addr, connection_id, error);
                            // Don't log incoming connection errors to reduce noise
                            None
                        },
                        SwarmEvent::Dialing { peer_id, connection_id, .. } => {
                            debug!("Dialing peer: {:?} (connection id: {:?})", peer_id, connection_id);
                            // Don't log dialing attempts to reduce noise
                            None
                        },
                        _ => {
                            debug!("Unhandled Swarm Event: {:?}", event);
                            None
                        }
                    }
                },
            }
        };

        if let Some(event) = evt {
            // Process events with different priorities
            match &event {
                // Input events are always processed immediately for UI responsiveness
                EventType::Input(_) => {
                    if let Some(action_result) = handle_event(
                        event,
                        &mut swarm,
                        &mut peer_names,
                        response_sender.clone(),
                        story_sender.clone(),
                        &mut local_peer_name,
                        &mut sorted_peer_names_cache,
                        &ui_logger,
                        &error_logger,
                    )
                    .await
                    {
                        match action_result {
                            ActionResult::RefreshStories => {
                                // Stories were updated, refresh them
                                match storage::read_local_stories().await {
                                    Ok(stories) => {
                                        debug!("Refreshed {} stories", stories.len());
                                        app.update_local_stories(stories);
                                    }
                                    Err(e) => {
                                        error!("Failed to refresh stories: {}", e);
                                    }
                                }
                            }
                            ActionResult::StartStoryCreation => {
                                // Start interactive story creation mode
                                app.start_story_creation();
                            }
                        }
                    }
                }
                // Heavy network operations are spawned to background tasks where possible
                EventType::PublishStory(_) => {
                    // Story publishing can be heavy, but needs swarm access
                    // Process immediately but the sleep has been removed from the handler
                    if let Some(action_result) = handle_event(
                        event,
                        &mut swarm,
                        &mut peer_names,
                        response_sender.clone(),
                        story_sender.clone(),
                        &mut local_peer_name,
                        &mut sorted_peer_names_cache,
                        &ui_logger,
                        &error_logger,
                    )
                    .await
                    {
                        match action_result {
                            ActionResult::RefreshStories => {
                                // Stories were updated, refresh them
                                match storage::read_local_stories().await {
                                    Ok(stories) => {
                                        debug!("Refreshed {} stories", stories.len());
                                        app.update_local_stories(stories);
                                    }
                                    Err(e) => {
                                        error!("Failed to refresh stories: {}", e);
                                    }
                                }
                            }
                            ActionResult::StartStoryCreation => {
                                // Start interactive story creation mode
                                app.start_story_creation();
                            }
                        }
                    }
                }
                // All other events are processed normally
                _ => {
                    if let Some(action_result) = handle_event(
                        event,
                        &mut swarm,
                        &mut peer_names,
                        response_sender.clone(),
                        story_sender.clone(),
                        &mut local_peer_name,
                        &mut sorted_peer_names_cache,
                        &ui_logger,
                        &error_logger,
                    )
                    .await
                    {
                        match action_result {
                            ActionResult::RefreshStories => {
                                // Stories were updated, refresh them
                                match storage::read_local_stories().await {
                                    Ok(stories) => {
                                        debug!("Refreshed {} stories", stories.len());
                                        app.update_local_stories(stories);
                                    }
                                    Err(e) => {
                                        error!("Failed to refresh stories: {}", e);
                                    }
                                }
                            }
                            ActionResult::StartStoryCreation => {
                                // Start interactive story creation mode
                                app.start_story_creation();
                            }
                        }
                    }
                }
            }

            // Update UI with the latest peer names
            app.update_peers(peer_names.clone());
            app.update_local_peer_name(local_peer_name.clone());
        }
    }

    // Cleanup
    if let Err(e) = app.cleanup() {
        error!("Error during cleanup: {}", e);
    }
}
