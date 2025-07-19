mod error_logger;
mod event_handlers;
mod handlers;
mod migrations;
mod network;
mod storage;
mod types;
mod ui;

use error_logger::ErrorLogger;
use event_handlers::handle_event;
use handlers::SortedPeerNamesCache;
use network::{PEER_ID, StoryBehaviourEvent, TOPIC, create_swarm};
use storage::{ensure_stories_file_exists, load_local_peer_name};
use types::{EventType, PeerName};
use ui::{App, AppEvent, handle_ui_events};

use bytes::Bytes;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, futures::StreamExt};
use log::{error, info};
use std::collections::HashMap;
use std::process;
use tokio::sync::mpsc;

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

    info!("Peer Id: {}", PEER_ID.clone());

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
                info!("Loaded saved peer name: {}", name);
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

    // Auto-subscribe to general channel if not already subscribed
    match storage::read_subscribed_channels(&PEER_ID.to_string()).await {
        Ok(subscriptions) => {
            if !subscriptions.contains(&"general".to_string()) {
                if let Err(e) = storage::subscribe_to_channel(&PEER_ID.to_string(), "general").await
                {
                    error!("Failed to auto-subscribe to general channel: {}", e);
                } else {
                    info!("Auto-subscribed to general channel");
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
            info!("Loaded {} local stories", stories.len());
            app.update_local_stories(stories);
        }
        Err(e) => {
            error!("Failed to load local stories: {}", e);
            app.add_to_log(format!("Failed to load local stories: {}", e));
        }
    }

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can get a local socket"),
    )
    .expect("swarm can be started");

    // Main application loop
    loop {
        // Handle UI events
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

        // Add a small yield to prevent blocking
        tokio::task::yield_now().await;

        let evt = {
            tokio::select! {
                // Add a timeout to ensure the loop doesn't get stuck
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    None
                }
                ui_log_msg = ui_log_rcv.recv() => {
                    if let Some(msg) = ui_log_msg {
                        app.add_to_log(msg);
                    }
                    None
                }
                ui_event = ui_rcv.recv() => {
                    if let Some(event) = ui_event {
                        match event {
                            AppEvent::Input(line) => Some(EventType::Input(line)),
                            AppEvent::Quit => {
                                info!("Quit event received in main loop");
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
                    // Periodic connection maintenance
                    event_handlers::maintain_connections(&mut swarm).await;
                    None
                },
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(event)) => Some(EventType::FloodsubEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Mdns(event)) => Some(EventType::MdnsEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(event)) => Some(EventType::PingEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(event)) => Some(EventType::RequestResponseEvent(event)),
                        SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(event)) => Some(EventType::KadEvent(event)),
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Local node is listening on {}", address);
                            app.add_to_log(format!("Local node is listening on {}", address));
                            None
                        },
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            info!("Connection established to {} via {:?}", peer_id, endpoint);
                            // Connection status is now visible in the Connected Peers section
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
                            // Connection status is now visible in the Connected Peers section
                            info!("Removing peer {} from floodsub partial view", peer_id);
                            swarm.behaviour_mut().floodsub.remove_node_from_partial_view(&peer_id);

                            // Remove the peer name when connection is closed
                            if let Some(name) = peer_names.remove(&peer_id) {
                                info!("Removed peer name '{}' for disconnected peer {}", name, peer_id);
                                // Update the cache since peer names changed
                                sorted_peer_names_cache.update(&peer_names);
                                app.update_peers(peer_names.clone());
                            }

                            None
                        },
                        SwarmEvent::OutgoingConnectionError { peer_id, error, connection_id, .. } => {
                            // Log to file instead of console to avoid UI spam
                            log_network_error!(error_logger, "outgoing_connection", "Failed to connect to {:?} (connection id: {:?}): {}", peer_id, connection_id, error);
                            // Only log connection errors that matter to the user
                            if let Some(peer_id) = peer_id {
                                app.add_to_log(format!("Failed to connect to {}: {}", peer_id, error));
                            }
                            None
                        },
                        SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, connection_id, .. } => {
                            // Log to file instead of console to avoid UI spam
                            log_network_error!(error_logger, "incoming_connection", "Failed incoming connection from {} to {} (connection id: {:?}): {}", send_back_addr, local_addr, connection_id, error);
                            // Don't log incoming connection errors to reduce noise
                            None
                        },
                        SwarmEvent::Dialing { peer_id, connection_id, .. } => {
                            info!("Dialing peer: {:?} (connection id: {:?})", peer_id, connection_id);
                            // Don't log dialing attempts to reduce noise
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
            if let Some(()) = handle_event(
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
                // Stories were updated, refresh them
                match storage::read_local_stories().await {
                    Ok(stories) => {
                        info!("Refreshed {} stories", stories.len());
                        app.update_local_stories(stories);
                    }
                    Err(e) => {
                        error!("Failed to refresh stories: {}", e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ListResponse, Story};
    use tokio::sync::mpsc;

    #[test]
    fn test_respond_with_public_stories() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let (sender, mut receiver) = mpsc::unbounded_channel::<ListResponse>();
            let receiver_name = "test_receiver".to_string();

            // Test the function (it will read from default stories file)
            event_handlers::respond_with_public_stories(sender, receiver_name.clone());

            // Try to receive a response (may timeout if no stories file exists)
            // This mainly tests that the function doesn't panic
            tokio::time::timeout(tokio::time::Duration::from_millis(100), receiver.recv())
                .await
                .ok();
        });
    }

    #[test]
    fn test_event_type_variants() {
        use crate::types::{EventType, ListMode, ListResponse, PeerName};
        use bytes::Bytes;
        use libp2p::floodsub::Event;
        use libp2p::mdns::Event as MdnsEvent;
        use libp2p::ping::Event as PingEvent;
        use std::time::Duration;

        // Test all EventType variants can be created
        let _input_event = EventType::Input("test input".to_string());

        let list_response = ListResponse::new(ListMode::ALL, "test".to_string(), vec![]);
        let _response_event = EventType::Response(list_response);

        let story = Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            true,
        );
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
        let floodsub_event = Event::Message(mock_message);
        let _floodsub_event_type = EventType::FloodsubEvent(floodsub_event);

        // Test mDNS event
        let mdns_event = MdnsEvent::Discovered(
            std::iter::once((*PEER_ID, "/ip4/127.0.0.1/tcp/8080".parse().unwrap())).collect(),
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
