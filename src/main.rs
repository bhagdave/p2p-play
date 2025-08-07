mod bootstrap;
mod bootstrap_logger;
mod crypto;
mod error_logger;
mod event_handlers;
mod event_processor;
mod handlers;
mod migrations;
mod network;
mod relay;
mod storage;
mod types;
mod ui;

use bootstrap::AutoBootstrap;
use bootstrap_logger::BootstrapLogger;
use error_logger::ErrorLogger;
use event_processor::EventProcessor;
use handlers::{SortedPeerNamesCache, refresh_unread_counts_for_ui};
use network::{PEER_ID, create_swarm};
use storage::{
    ensure_stories_file_exists, ensure_unified_network_config_exists, load_local_peer_name,
    load_unified_network_config,
};
use types::{PendingDirectMessage, UnifiedNetworkConfig};
use ui::App;

use libp2p::{PeerId, Swarm};
use log::{debug, error};
use std::collections::HashMap;
use std::process;
use std::sync::{Arc, Mutex};
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

    let (response_sender, response_rcv) = mpsc::unbounded_channel();
    let (story_sender, story_rcv) = mpsc::unbounded_channel();
    let (ui_sender, ui_rcv) = mpsc::unbounded_channel();
    let (ui_log_sender, ui_log_rcv) = mpsc::unbounded_channel();

    // Create UI logger
    let ui_logger = handlers::UILogger::new(ui_log_sender);

    // Create error logger that writes to file
    let error_logger = ErrorLogger::new("errors.log");

    // Create bootstrap logger that writes to file
    let bootstrap_logger = BootstrapLogger::new("bootstrap.log");

    // Load unified network configuration
    if let Err(e) = ensure_unified_network_config_exists().await {
        error!("Failed to initialize unified network config: {}", e);
        app.add_to_log(format!(
            "Failed to initialize unified network config: {}",
            e
        ));
    }

    let unified_config = match load_unified_network_config().await {
        Ok(config) => {
            debug!(
                "Loaded unified network config: connection_maintenance_interval_seconds={}",
                config.network.connection_maintenance_interval_seconds
            );
            app.add_to_log(format!("Loaded unified network config from file"));
            config
        }
        Err(e) => {
            error!("Failed to load unified network config: {}", e);
            app.add_to_log(format!(
                "Failed to load unified network config: {}, using defaults",
                e
            ));
            UnifiedNetworkConfig::new()
        }
    };

    // Extract individual configs for convenience
    let network_config = &unified_config.network;
    let dm_config = &unified_config.direct_message;

    let mut swarm = create_swarm(&unified_config.ping).expect("Failed to create swarm");

    // Storage for peer names (peer_id -> alias)
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();

    // Cache for sorted peer names to avoid repeated sorting on every direct message
    let mut sorted_peer_names_cache = SortedPeerNamesCache::new();

    // Initialize direct message retry queue using config from unified_config
    let pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>> = Arc::new(Mutex::new(Vec::new()));

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

    // Initialize automatic bootstrap
    let mut auto_bootstrap = AutoBootstrap::new();
    auto_bootstrap
        .initialize(&unified_config.bootstrap, &bootstrap_logger, &error_logger)
        .await;

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
            app.update_stories(stories);
            // Refresh unread counts
            refresh_unread_counts_for_ui(&mut app, &PEER_ID.to_string()).await;
            // Note: Unread counts are loaded separately after this in the init phase
        }
        Err(e) => {
            error!("Failed to load local stories: {}", e);
            app.add_to_log(format!("Failed to load local stories: {}", e));
        }
    }

    // Load initial subscribed channels and update UI
    match storage::read_subscribed_channels_with_details(&PEER_ID.to_string()).await {
        Ok(channels) => {
            debug!("Loaded {} subscribed channels", channels.len());
            app.update_channels(channels);
        }
        Err(e) => {
            error!("Failed to load subscribed channels: {}", e);
            app.add_to_log(format!("Failed to load subscribed channels: {}", e));
        }
    }

    // Load initial unread counts and update UI
    refresh_unread_counts_for_ui(&mut app, &PEER_ID.to_string()).await;
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

    // Create event processor
    let mut event_processor = EventProcessor::new(
        ui_rcv,
        ui_log_rcv,
        response_rcv,
        story_rcv,
        response_sender,
        story_sender,
        ui_sender,
        dm_config.clone(),
        pending_messages,
        ui_logger,
        error_logger,
        bootstrap_logger,
    );

    // Run the main event loop
    event_processor.run(
        &mut app,
        &mut swarm,
        &mut peer_names,
        &mut local_peer_name,
        &mut sorted_peer_names_cache,
        &mut auto_bootstrap,
    ).await;

    // Cleanup
    if let Err(e) = app.cleanup() {
        error!("Error during cleanup: {}", e);
    }
}
