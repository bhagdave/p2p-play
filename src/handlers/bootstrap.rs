//! DHT bootstrap and peer-discovery handlers.

use crate::network::StoryBehaviour;
use crate::storage::{load_bootstrap_config, save_bootstrap_config};
use libp2p::swarm::Swarm;

use super::{UILogger, extract_peer_id_from_multiaddr, modify_bootstrap_config};

pub async fn handle_dht_bootstrap(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();

    if parts.len() < 3 {
        show_bootstrap_usage(ui_logger);
        return;
    }

    match parts[2] {
        "add" => handle_bootstrap_add(&parts[3..], ui_logger).await,
        "remove" => handle_bootstrap_remove(&parts[3..], ui_logger).await,
        "list" => handle_bootstrap_list(ui_logger).await,
        "clear" => handle_bootstrap_clear(ui_logger).await,
        "retry" => handle_bootstrap_retry(swarm, ui_logger).await,
        // Legacy support: if the third part looks like a multiaddr, treat it as a
        // direct bootstrap address (backward compatibility with old command syntax).
        addr if addr.starts_with('/') => {
            let addr_str = parts[2..].join(" ");
            handle_direct_bootstrap(&addr_str, swarm, ui_logger).await;
        }
        _ => show_bootstrap_usage(ui_logger),
    }
}

pub async fn handle_dht_get_peers(
    _cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    use crate::network::PEER_ID;

    ui_logger.log("Searching for closest peers in DHT...".to_string());
    let _query_id = swarm.behaviour_mut().kad.get_closest_peers(*PEER_ID);
    ui_logger.log("DHT peer search started (results will appear in events)".to_string());
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn show_bootstrap_usage(ui_logger: &UILogger) {
    ui_logger.log("DHT Bootstrap Commands:".to_string());
    ui_logger.log("  dht bootstrap add <multiaddr>    - Add bootstrap peer to config".to_string());
    ui_logger
        .log("  dht bootstrap remove <multiaddr> - Remove bootstrap peer from config".to_string());
    ui_logger
        .log("  dht bootstrap list               - Show configured bootstrap peers".to_string());
    ui_logger.log("  dht bootstrap clear              - Clear all bootstrap peers".to_string());
    ui_logger
        .log("  dht bootstrap retry              - Retry bootstrap with config peers".to_string());
    ui_logger.log("  dht bootstrap <multiaddr>        - Bootstrap directly with peer".to_string());
    ui_logger.log("Example: dht bootstrap add /dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN".to_string());
}

async fn handle_bootstrap_add(args: &[&str], ui_logger: &UILogger) {
    if args.is_empty() {
        ui_logger.log("Usage: dht bootstrap add <multiaddr>".to_string());
        return;
    }

    let multiaddr = args.join(" ");

    if let Err(e) = multiaddr.parse::<libp2p::Multiaddr>() {
        ui_logger.log(format!("Invalid multiaddr '{multiaddr}': {e}"));
        return;
    }

    if modify_bootstrap_config(ui_logger, "bootstrap add", |config| {
        if config.add_peer(multiaddr.clone()) {
            ui_logger.log(format!("Added bootstrap peer: {multiaddr}"));
            ui_logger.log(format!(
                "Total bootstrap peers: {}",
                config.bootstrap_peers.len()
            ));
            true
        } else {
            ui_logger.log(format!("Bootstrap peer already exists: {multiaddr}"));
            false
        }
    })
    .await
    {}
}

async fn handle_bootstrap_remove(args: &[&str], ui_logger: &UILogger) {
    if args.is_empty() {
        ui_logger.log("Usage: dht bootstrap remove <multiaddr>".to_string());
        return;
    }

    let multiaddr = args.join(" ");

    match load_bootstrap_config().await {
        Ok(mut config) => {
            if config.bootstrap_peers.len() <= 1 && config.bootstrap_peers.contains(&multiaddr) {
                ui_logger.log("Warning: Cannot remove the last bootstrap peer. At least one peer is required for DHT connectivity.".to_string());
                ui_logger.log("Use 'dht bootstrap add <multiaddr>' to add another peer first, or 'dht bootstrap clear' to remove all peers.".to_string());
                return;
            }

            if config.remove_peer(&multiaddr) {
                match save_bootstrap_config(&config).await {
                    Ok(_) => {
                        ui_logger.log(format!("Removed bootstrap peer: {multiaddr}"));
                        ui_logger.log(format!(
                            "Total bootstrap peers: {}",
                            config.bootstrap_peers.len()
                        ));
                    }
                    Err(e) => ui_logger.log(format!("Failed to save bootstrap config: {e}")),
                }
            } else {
                ui_logger.log(format!("Bootstrap peer not found: {multiaddr}"));
            }
        }
        Err(e) => ui_logger.log(format!("Failed to load bootstrap config: {e}")),
    }
}

async fn handle_bootstrap_list(ui_logger: &UILogger) {
    match load_bootstrap_config().await {
        Ok(config) => {
            ui_logger.log(format!(
                "Bootstrap Configuration ({} peers):",
                config.bootstrap_peers.len()
            ));
            for (i, peer) in config.bootstrap_peers.iter().enumerate() {
                ui_logger.log(format!("  {}. {}", i + 1, peer));
            }
            ui_logger.log(format!("Retry Interval: {}ms", config.retry_interval_ms));
            ui_logger.log(format!("Max Retry Attempts: {}", config.max_retry_attempts));
            ui_logger.log(format!(
                "Bootstrap Timeout: {}ms",
                config.bootstrap_timeout_ms
            ));
        }
        Err(e) => ui_logger.log(format!("Failed to load bootstrap config: {e}")),
    }
}

async fn handle_bootstrap_clear(ui_logger: &UILogger) {
    if modify_bootstrap_config(ui_logger, "bootstrap clear", |config| {
        let peer_count = config.bootstrap_peers.len();
        config.clear_peers();
        ui_logger.log(format!("Cleared {peer_count} bootstrap peers"));
        ui_logger.log(
            "Warning: No bootstrap peers configured. Add peers to enable DHT connectivity."
                .to_string(),
        );
        true
    })
    .await
    {}
}

async fn handle_bootstrap_retry(swarm: &mut Swarm<StoryBehaviour>, ui_logger: &UILogger) {
    match load_bootstrap_config().await {
        Ok(config) => {
            if config.bootstrap_peers.is_empty() {
                ui_logger.log("No bootstrap peers configured. Use 'dht bootstrap add <multiaddr>' to add peers.".to_string());
                return;
            }

            ui_logger.log(format!(
                "Retrying bootstrap with {} configured peers...",
                config.bootstrap_peers.len()
            ));

            for peer_addr in &config.bootstrap_peers {
                match peer_addr.parse::<libp2p::Multiaddr>() {
                    Ok(addr) => {
                        if let Some(peer_id) = extract_peer_id_from_multiaddr(&addr) {
                            swarm
                                .behaviour_mut()
                                .kad
                                .add_address(&peer_id, addr.clone());
                            ui_logger.log(format!("Added bootstrap peer to DHT: {peer_addr}"));
                        } else {
                            ui_logger.log(format!("Failed to extract peer ID from: {peer_addr}"));
                        }
                    }
                    Err(e) => {
                        ui_logger.log(format!("Invalid multiaddr in config '{peer_addr}': {e}"))
                    }
                }
            }

            if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
                ui_logger.log(format!("Failed to start DHT bootstrap: {e:?}"));
            } else {
                ui_logger.log("DHT bootstrap retry started successfully".to_string());
            }
        }
        Err(e) => ui_logger.log(format!("Failed to load bootstrap config: {e}")),
    }
}

async fn handle_direct_bootstrap(
    addr_str: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    let addr_str = addr_str.trim();

    if addr_str.is_empty() {
        show_bootstrap_usage(ui_logger);
        return;
    }

    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            ui_logger.log(format!("Attempting to bootstrap DHT with peer at: {addr}"));

            if let Some(peer_id) = extract_peer_id_from_multiaddr(&addr) {
                swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, addr.clone());

                if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
                    ui_logger.log(format!("Failed to start DHT bootstrap: {e:?}"));
                } else {
                    ui_logger.log("DHT bootstrap started successfully".to_string());
                }
            } else {
                ui_logger.log("Failed to extract peer ID from multiaddr".to_string());
            }
        }
        Err(e) => ui_logger.log(format!("Failed to parse multiaddr: {e}")),
    }
}
