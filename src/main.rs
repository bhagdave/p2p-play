mod bootstrap;
mod bootstrap_logger;
mod circuit_breaker;
mod crypto;
mod error_logger;
mod errors;
mod event_handlers;
mod event_processor;
mod handlers;
mod migrations;
mod network;
mod network_circuit_breakers;
mod relay;
mod storage;
mod types;
mod ui;
mod validation;

use bootstrap::AutoBootstrap;
use bootstrap_logger::BootstrapLogger;
use crypto::CryptoService;
use error_logger::ErrorLogger;
use errors::{AppError, AppResult};
use event_processor::EventProcessor;
use handlers::{SortedPeerNamesCache, refresh_unread_counts_for_ui};
use network::{KEYS, PEER_ID, create_swarm};
use network_circuit_breakers::NetworkCircuitBreakers;
use relay::RelayService;
use storage::{
    ensure_stories_file_exists, ensure_unified_network_config_exists, load_local_peer_name,
    load_unified_network_config,
};
use types::{CommunicationChannels, Loggers, PendingDirectMessage, UnifiedNetworkConfig};
use ui::App;

use libp2p::{PeerId, Swarm};
use log::{debug, error};
use std::collections::HashMap;
use std::error::Error;
use std::process;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    if let Err(e) = run_app().await {
        eprintln!("Application error: {e}");
        // Log the error chain for debugging
        let mut source = e.source();
        let mut indent = 1;
        while let Some(err) = source {
            eprintln!("{:indent$}Caused by: {err}", "", indent = indent * 2);
            source = err.source();
            indent += 1;
        }
        std::process::exit(1);
    }
}

async fn run_app() -> AppResult<()> {
    initialize_logging();

    // Initialize the UI
    let mut app = match App::new() {
        Ok(mut app) => {
            app.update_local_peer_id(PEER_ID.to_string());
            app.refresh_conversations().await;
            app
        }
        Err(e) => {
            error!("Failed to initialize UI: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = ensure_stories_file_exists().await {
        error!("Failed to initialize stories file: {e}");
        let _ = app.cleanup();
        process::exit(1);
    }

    let (channels, loggers) = setup_communication_channels();

    let unified_config = load_configuration(&mut app).await;

    let network_config = &unified_config.network;
    let dm_config = &unified_config.direct_message;

    let mut swarm = create_swarm(&unified_config.ping).expect("Failed to create swarm");

    let mut peer_names: HashMap<PeerId, String> = HashMap::new();

    let mut sorted_peer_names_cache = SortedPeerNamesCache::new();

    // Initialize direct message retry queue using config from unified_config
    let pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>> = Arc::new(Mutex::new(Vec::new()));

    // Load saved peer name if it exists
    let mut local_peer_name: Option<String> = match load_local_peer_name().await {
        Ok(saved_name) => {
            if let Some(ref name) = saved_name {
                app.add_to_log(format!("Loaded saved peer name: {name}"));
                app.update_local_peer_name(saved_name.clone());
            }
            saved_name
        }
        Err(e) => {
            error!("Failed to load saved peer name: {e}");
            app.add_to_log(format!("Failed to load saved peer name: {e}"));
            None
        }
    };

    // Initialize automatic bootstrap
    let mut auto_bootstrap = AutoBootstrap::new();
    auto_bootstrap
        .initialize(
            &unified_config.bootstrap,
            &loggers.bootstrap_logger,
            &loggers.error_logger,
        )
        .await;

    // Auto-subscribe to general channel if not already subscribed
    match storage::read_subscribed_channels(&PEER_ID.to_string()).await {
        Ok(subscriptions) => {
            if !subscriptions.contains(&"general".to_string()) {
                if let Err(e) = storage::subscribe_to_channel(&PEER_ID.to_string(), "general").await
                {
                    error!("Failed to auto-subscribe to general channel: {e}");
                }
            }
        }
        Err(e) => {
            error!("Failed to check subscriptions: {e}");
            // Try to subscribe to general anyway
            if let Err(e) = storage::subscribe_to_channel(&PEER_ID.to_string(), "general").await {
                error!("Failed to auto-subscribe to general channel: {e}");
            }
        }
    }

    match storage::read_local_stories().await {
        Ok(stories) => {
            app.update_stories(stories);
            refresh_unread_counts_for_ui(&mut app, &PEER_ID.to_string()).await;
        }
        Err(e) => {
            error!("Failed to load local stories: {e}");
            app.add_to_log(format!("Failed to load local stories: {e}"));
        }
    }

    match storage::read_subscribed_channels_with_details(&PEER_ID.to_string()).await {
        Ok(channels) => {
            app.update_channels(channels);
        }
        Err(e) => {
            error!("Failed to load subscribed channels: {e}");
            app.add_to_log(format!("Failed to load subscribed channels: {e}"));
        }
    }

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

    let crypto_service = CryptoService::new(KEYS.clone());

    let network_circuit_breakers = NetworkCircuitBreakers::new(&unified_config.circuit_breaker);

    let relay_service = if unified_config.relay.enable_relay {
        Some(RelayService::new(
            unified_config.relay.clone(),
            crypto_service,
        ))
    } else {
        None
    };

    // Create event processor
    let mut event_processor = EventProcessor::new(
        channels.ui_rcv,
        channels.ui_log_rcv,
        channels.response_rcv,
        channels.story_rcv,
        channels.response_sender,
        channels.story_sender,
        channels.ui_sender,
        network_config,
        dm_config.clone(),
        pending_messages,
        loggers.ui_logger,
        loggers.error_logger,
        loggers.bootstrap_logger,
        relay_service,
        network_circuit_breakers,
    );

    // Run the main event loop
    event_processor
        .run(
            &mut app,
            &mut swarm,
            &mut peer_names,
            &mut local_peer_name,
            &mut sorted_peer_names_cache,
            &mut auto_bootstrap,
        )
        .await;

    app.cleanup().map_err(AppError::from).map_err(|e| {
        error!("Error during cleanup: {e}");
        e
    })?;

    Ok(())
}

fn initialize_logging() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .filter_module("p2p_play", log::LevelFilter::Info)
        .filter_module("libp2p", log::LevelFilter::Warn)
        .filter_module("libp2p_swarm", log::LevelFilter::Error)
        .filter_module("libp2p_tcp", log::LevelFilter::Error)
        .filter_module("libp2p_noise", log::LevelFilter::Error)
        .filter_module("libp2p_yamux", log::LevelFilter::Error)
        .filter_module("multistream_select", log::LevelFilter::Error)
        .init();
}

fn setup_communication_channels() -> (CommunicationChannels, Loggers) {
    let (response_sender, response_rcv) = mpsc::unbounded_channel();
    let (story_sender, story_rcv) = mpsc::unbounded_channel();
    let (ui_sender, ui_rcv) = mpsc::unbounded_channel();
    let (ui_log_sender, ui_log_rcv) = mpsc::unbounded_channel();

    let channels = CommunicationChannels {
        response_sender,
        response_rcv,
        story_sender,
        story_rcv,
        ui_sender,
        ui_rcv,
        ui_log_rcv,
    };

    let loggers = Loggers {
        ui_logger: handlers::UILogger::new(ui_log_sender),
        error_logger: ErrorLogger::new("errors.log"),
        bootstrap_logger: BootstrapLogger::new("bootstrap.log"),
    };

    (channels, loggers)
}

async fn load_configuration(app: &mut App) -> UnifiedNetworkConfig {
    if let Err(e) = ensure_unified_network_config_exists().await {
        error!("Failed to initialize unified network config: {e}");
        app.add_to_log(format!("Failed to initialize unified network config: {e}"));
    }

    match load_unified_network_config().await {
        Ok(config) => {
            app.add_to_log("Loaded unified network config from file".to_string());
            config
        }
        Err(e) => {
            error!("Failed to load unified network config: {e}");
            app.add_to_log(format!(
                "Failed to load unified network config: {e}, using defaults"
            ));
            UnifiedNetworkConfig::new()
        }
    }
}
