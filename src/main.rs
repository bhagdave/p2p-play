mod bootstrap;
mod bootstrap_logger;
mod circuit_breaker;
mod constants;
mod content_fetcher;
mod crypto;
mod data_dir;
mod error_logger;
mod errors;
mod event_handlers;
mod event_processor;
mod file_logger;
mod handlers;
mod migrations;
mod network;
mod network_circuit_breakers;
mod relay;
mod storage;
mod types;
mod ui;
mod validation;
mod wasm_executor;

use bootstrap::AutoBootstrap;
use bootstrap_logger::BootstrapLogger;
use crypto::CryptoService;
use error_logger::ErrorLogger;
use errors::{AppError, AppResult, print_error_chain};
use event_processor::EventProcessor;
use handlers::{PeerState, refresh_unread_counts_for_ui};
use network::{KEYS, PEER_ID, create_swarm};
use network_circuit_breakers::NetworkCircuitBreakers;
use relay::RelayService;
use storage::{
    ensure_general_channel_subscription, ensure_stories_file_exists,
    ensure_unified_network_config_exists, load_local_peer_name, load_unified_network_config,
};
use types::{CommunicationChannels, Loggers, PendingDirectMessage, UnifiedNetworkConfig};
use ui::App;

use clap::Parser;
use data_dir::get_data_path;
use libp2p::Swarm;
use log::error;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

const UNIFIED_CONFIG_FILE: &str = "unified_network_config.json";
use constants::BOOTSTRAP_LOG_FILE;

/// Peer-to-peer story sharing application.
#[derive(Parser, Debug)]
#[command(name = "p2p-play", about = "Peer-to-peer story sharing application")]
struct Cli {
    /// Directory for storing data files (database, logs, config, and peer key).
    /// The directory is created automatically if it does not exist.
    #[arg(long, value_name = "PATH")]
    data_dir: Option<std::path::PathBuf>,
}

// Synchronous entry-point so that the Tokio runtime is started *after*
// the data directory is resolved and the DATA_DIR env var is set.
// This guarantees that the env-var write happens before any worker threads
// are spawned, making the `set_var` call free of data races.
fn main() {
    let cli = Cli::parse();

    if let Some(data_dir) = cli.data_dir {
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            eprintln!(
                "Failed to create data directory '{}': {}",
                data_dir.display(),
                e
            );
            std::process::exit(1);
        }
        // Canonicalize to an absolute path so relative --data-dir values work
        // regardless of later working-directory changes.
        let canonical = match std::fs::canonicalize(&data_dir) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "Failed to resolve data directory '{}': {}",
                    data_dir.display(),
                    e
                );
                std::process::exit(1);
            }
        };
        // SAFETY: No other threads exist at this point — the Tokio runtime
        // has not been started yet, so this call is free of data races.
        unsafe {
            std::env::set_var("DATA_DIR", canonical.to_string_lossy().as_ref());
        }
    }

    // Build the Tokio runtime manually (equivalent to #[tokio::main]).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async {
        if let Err(e) = run_app().await {
            print_error_chain(&e);
            std::process::exit(1);
        }
    });
}

fn initialise_ui() -> AppResult<App> {
    let mut app = App::new().map_err(|e| {
        error!("Failed to initialise UI: {e}");
        e
    })?;
    app.update_local_peer_id(PEER_ID.to_string());
    Ok(app)
}

async fn run_app() -> AppResult<()> {
    initialise_logging();

    let mut app = initialise_ui()?;
    app.refresh_conversations().await;

    initialise_database(&mut app).await?;

    // Load saved peer name if it exists
    let local_peer_name: Option<String> = match load_local_peer_name().await {
        Ok(None) => {
            app.add_to_log(
                "No saved peer name found. Type 'name <alias>' to set a human-readable name."
                    .to_string(),
            );
            None
        }
        Ok(Some(name)) => {
            app.add_to_log(format!("Loaded saved peer name: {name}"));
            app.update_local_peer_name(Some(name.clone()));
            Some(name)
        }
        Err(e) => {
            error!("Failed to load saved peer name: {e}");
            app.add_to_log(format!("Failed to load saved peer name: {e}"));
            None
        }
    };
    let mut peer_state = PeerState::new(local_peer_name);

    let errors_log_is_new = !std::path::Path::new(&get_data_path("errors.log")).exists();
    let bootstrap_log_is_new = !std::path::Path::new(&get_data_path(BOOTSTRAP_LOG_FILE)).exists();
    let (channels, loggers) = setup_communication_channels();
    if errors_log_is_new {
        app.add_to_log(format!(
            "Logging errors to: {}",
            get_data_path("errors.log")
        ));
    }
    if bootstrap_log_is_new {
        app.add_to_log(format!(
            "Logging bootstrap activity to: {}",
            get_data_path(BOOTSTRAP_LOG_FILE)
        ));
    }

    let unified_config = load_configuration(&mut app).await;

    let network_config = &unified_config.network;
    let dm_config = &unified_config.direct_message;

    let mut swarm = create_swarm(&unified_config.ping, &unified_config.network)
        .expect("Failed to create swarm");

    // Initialise direct message retry queue using config from unified_config
    let pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>> = Arc::new(Mutex::new(Vec::new()));

    let mut auto_bootstrap = AutoBootstrap::new();
    auto_bootstrap.initialise(
        &unified_config.bootstrap,
        &loggers.bootstrap_logger,
        &loggers.error_logger,
    );

    if let Err(e) = ensure_general_channel_subscription(&PEER_ID.to_string()).await {
        error!("Failed to ensure general channel subscription: {e}");
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

    reconnect_stored_peers(&mut swarm, &mut app).await;

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
        .run(&mut app, &mut swarm, &mut peer_state, &mut auto_bootstrap)
        .await;

    app.cleanup().map_err(AppError::from).map_err(|e| {
        error!("Error during cleanup: {e}");
        e
    })?;

    Ok(())
}

fn initialise_logging() {
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
        error_logger: ErrorLogger::new(&get_data_path("errors.log")),
        bootstrap_logger: BootstrapLogger::new(&get_data_path(BOOTSTRAP_LOG_FILE)),
    };

    (channels, loggers)
}

async fn reconnect_stored_peers(
    swarm: &mut Swarm<impl libp2p::swarm::NetworkBehaviour>,
    app: &mut App,
) {
    // Reconnect to peers that were previously dialled (outbound connections stored in DB).
    // We silently skip peers whose multiaddr cannot be parsed or dialled — failures here
    // are non-fatal and will be retried naturally via mDNS/Kademlia discovery.
    match storage::get_outbound_peers(10).await {
        Ok(addrs) => {
            for addr_str in addrs {
                match addr_str.parse::<libp2p::Multiaddr>() {
                    Ok(addr) => {
                        if let Err(e) = swarm.dial(addr.clone()) {
                            app.add_to_log(format!("Startup reconnect failed for {addr}: {e}"));
                        } else {
                            app.add_to_log(format!("Reconnecting to known peer at {addr}"));
                        }
                    }
                    Err(e) => {
                        error!("Invalid multiaddr in peer database '{addr_str}': {e}");
                        app.add_to_log(format!("Skipping invalid peer address '{addr_str}': {e}"));
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to load outbound peers for startup reconnect: {e}");
            app.add_to_log(format!("Failed to load stored peers for reconnect: {e}"));
        }
    }
}

async fn initialise_database(app: &mut App) -> AppResult<()> {
    let db_path = std::env::var("TEST_DATABASE_PATH")
        .or_else(|_| std::env::var("DATABASE_PATH"))
        .unwrap_or_else(|_| get_data_path("stories.db"));
    let db_is_new = !std::path::Path::new(&db_path).exists();
    if let Err(e) = ensure_stories_file_exists().await {
        error!("Failed to initialise stories file: {e}");
        let _ = app.cleanup();
        return Err(e.into());
    } else if db_is_new {
        app.add_to_log(format!("Created database: {}", db_path));
    }
    Ok(())
}

async fn load_configuration(app: &mut App) -> UnifiedNetworkConfig {
    let config_path = get_data_path(UNIFIED_CONFIG_FILE);
    let config_is_new = tokio::fs::metadata(&config_path).await.is_err();
    if let Err(e) = ensure_unified_network_config_exists().await {
        error!("Failed to initialise unified network config: {e}");
        app.add_to_log(format!("Failed to initialise unified network config: {e}"));
    } else if config_is_new {
        app.add_to_log(format!(
            "Created config: {} — edit to customise bootstrap peers",
            config_path
        ));
    }

    match load_unified_network_config().await {
        Ok(config) => {
            app.add_to_log("Loaded unified network config from file".to_string());
            config
        }
        Err(e) => {
            error!("Failed to load unified network config: {e}");
            app.add_to_log(format!(
                "Config error: {e} — Edit {config_path} to fix. Using defaults for all settings."
            ));
            UnifiedNetworkConfig::new()
        }
    }
}
