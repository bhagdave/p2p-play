//! Configuration, description, and help handlers.

use crate::error_logger::ErrorLogger;
use crate::network::{NodeDescriptionRequest, PEER_ID, StoryBehaviour};
use crate::storage::{load_node_description, save_node_description};
use crate::types::Icons;
use crate::validation::ContentValidator;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use std::collections::HashMap;

use super::{UILogger, current_unix_timestamp, modify_config, resolve_peer_by_alias, validate_and_log};

// ---------------------------------------------------------------------------
// Data-driven help text
// ---------------------------------------------------------------------------

/// Each entry is `(text, optional_example)`.
static HELP_ENTRIES: &[(&str, Option<&str>)] = &[
    ("ls s to list stories", None),
    ("  Example: ls s", None),
    ("  Example: ls s all", None),
    (
        "search <query> [channel:<channel>] [author:<peer>] [recent:<days>] [public|private] to search stories",
        None,
    ),
    (
        "  Example: search rust channel:tech author:alice recent:7 public",
        None,
    ),
    (
        "filter channel <name> | filter recent <days> to filter stories",
        None,
    ),
    ("  Example: filter channel general", None),
    ("ls ch [available|unsubscribed] to list channels", None),
    ("  Example: ls ch available", None),
    ("ls sub to list your subscriptions", None),
    ("  Example: ls sub", None),
    (
        "create s name|header|body[|channel] to create and auto-publish story",
        None,
    ),
    (
        "  Example: create s My Story|An intro|Story body here|general",
        None,
    ),
    ("create ch name|description to create channel", None),
    (
        "  Example: create ch tech|A channel for technology stories",
        None,
    ),
    ("create desc <description> to create node description", None),
    (
        "  Example: create desc A peer interested in open source",
        None,
    ),
    ("publish s <id> to manually publish/re-publish story", None),
    ("  Example: publish s 3", None),
    ("show story <id> to show story details", None),
    ("  Example: show story 5", None),
    ("show desc to show your node description", None),
    ("  Example: show desc", None),
    ("get desc <peer_alias> to get description from peer", None),
    ("  Example: get desc alice", None),
    (
        "set auto-sub [on|off|status] to manage auto-subscription",
        None,
    ),
    ("  Example: set auto-sub on", None),
    (
        "config auto-share [on|off|status] to control automatic story sharing",
        None,
    ),
    ("  Example: config auto-share on", None),
    ("config sync-days <N> to set story sync timeframe (days)", None),
    ("  Example: config sync-days 30", None),
    (
        "delete s <id1>[,<id2>,<id3>...] to delete one or more stories",
        None,
    ),
    ("  Example: delete s 1,2,5", None),
    (
        "export s <id|all> <md|json> to export stories to ./exports/",
        None,
    ),
    ("  Example: export s 3 md", None),
    ("  Example: export s all json", None),
    ("sub <channel> to subscribe to channel", None),
    ("  Example: sub tech", None),
    ("unsub <channel> to unsubscribe from channel", None),
    ("  Example: unsub tech", None),
    ("name <alias> to set your peer name", None),
    ("  Example: name alice", None),
    ("peer id to show your full peer ID", None),
    ("  Example: peer id", None),
    ("msg <peer_alias> <message> to send direct message", None),
    ("  Example: msg alice Hello, are you there?", None),
    (
        "compose <peer_alias> to enter multi-line message composition mode",
        None,
    ),
    ("  Example: compose alice", None),
    (
        "Enhanced messaging: 'r' for quick reply, 'm' for message compose, Tab for auto-complete",
        None,
    ),
    (
        "dht bootstrap add/remove/list/clear/retry - manage bootstrap peers",
        None,
    ),
    ("  Example: dht bootstrap list", None),
    ("dht bootstrap <multiaddr> to bootstrap directly with peer", None),
    (
        "  Example: dht bootstrap /ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
        None,
    ),
    ("dht peers to find closest peers in DHT", None),
    ("  Example: dht peers", None),
    ("reload config to reload network configuration", None),
    ("  Example: reload config", None),
    ("--- WASM Capabilities ---", None),
    (
        "wasm create <name>|<desc>|<ipfs_cid>|<version> to create WASM offering",
        None,
    ),
    (
        "  Example: wasm create MyPlugin|A useful plugin|Qm.../v0.1.0|1.0.0",
        None,
    ),
    ("wasm ls [local|remote|all] to list WASM offerings", None),
    ("  Example: wasm ls all", None),
    ("wasm show <id> to show WASM offering details", None),
    ("  Example: wasm show 2", None),
    ("wasm toggle <id> to enable/disable WASM offering", None),
    ("  Example: wasm toggle 2", None),
    ("wasm delete <id> to delete WASM offering", None),
    ("  Example: wasm delete 2", None),
    (
        "wasm query <peer_alias> to query peer's WASM capabilities",
        None,
    ),
    ("  Example: wasm query alice", None),
    (
        "wasm run <peer_alias> <offering_id> [args...] to execute remote WASM",
        None,
    ),
    ("  Example: wasm run alice 3 hello world", None),
    ("wasm config to show WASM configuration", None),
    ("  Example: wasm config", None),
    ("quit to quit", None),
    ("  Example: quit", None),
];

pub async fn handle_help(_cmd: &str, ui_logger: &UILogger) {
    for (text, _example) in HELP_ENTRIES {
        ui_logger.log(text.to_string());
    }
}

// ---------------------------------------------------------------------------
// Config handlers
// ---------------------------------------------------------------------------

pub async fn handle_reload_config(_cmd: &str, ui_logger: &UILogger) {
    use crate::storage::load_unified_network_config;

    match load_unified_network_config().await {
        Ok(config) => {
            if let Err(e) = config.validate() {
                ui_logger.log(format!(
                    "{} Configuration validation failed: {}",
                    Icons::cross(),
                    e
                ));
            } else {
                ui_logger.log(format!(
                    "{} Network configuration reloaded successfully",
                    Icons::check()
                ));
                ui_logger.log(format!(
                    "{} Bootstrap peers: {}",
                    Icons::chart(),
                    config.bootstrap.bootstrap_peers.len()
                ));
                ui_logger.log(format!(
                    "{} Connection maintenance interval: {}s",
                    Icons::wrench(),
                    config.network.connection_maintenance_interval_seconds
                ));
                ui_logger.log(format!(
                    "{} Ping interval: {}s",
                    Icons::ping(),
                    config.ping.interval_secs
                ));
                ui_logger.log(format!(
                    "{} DM max retry attempts: {}",
                    Icons::speech(),
                    config.direct_message.max_retry_attempts
                ));
                ui_logger.log(format!(
                    "{}Note: Some configuration changes require application restart to take effect",
                    Icons::warning()
                ));
            }
        }
        Err(e) => {
            ui_logger.log(format!(
                "{} Failed to reload configuration: {}",
                Icons::cross(),
                e
            ));
        }
    }
}

pub async fn handle_config_auto_share(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    use crate::storage::load_unified_network_config;

    if let Some(setting) = cmd.strip_prefix("config auto-share ").map(|s| s.trim()) {
        match setting {
            "on" => {
                if modify_config(ui_logger, error_logger, "auto-share", |config| {
                    config.auto_share.global_auto_share = true;
                })
                .await
                {
                    ui_logger.log(format!(
                        "{} Auto-share enabled - new stories will be shared automatically",
                        Icons::check()
                    ));
                }
            }
            "off" => {
                if modify_config(ui_logger, error_logger, "auto-share", |config| {
                    config.auto_share.global_auto_share = false;
                })
                .await
                {
                    ui_logger.log(format!(
                        "{} Auto-share disabled - stories will not be shared automatically",
                        Icons::check()
                    ));
                }
            }
            "status" => match load_unified_network_config().await {
                Ok(config) => {
                    let status = if config.auto_share.global_auto_share {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    ui_logger.log(format!(
                        "{} Auto-share is currently {} (sync {} days)",
                        Icons::chart(),
                        status,
                        config.auto_share.sync_days
                    ));
                }
                Err(e) => {
                    error_logger.log_error(&format!("Failed to load auto-share config: {e}"));
                    ui_logger.log(format!(
                        "{} Failed to load auto-share status",
                        Icons::cross()
                    ));
                }
            },
            _ => {
                ui_logger.log("Usage: config auto-share [on|off|status]".to_string());
            }
        }
    } else {
        ui_logger.log("Usage: config auto-share [on|off|status]".to_string());
    }
}

pub async fn handle_config_sync_days(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    if let Some(days_str) = cmd.strip_prefix("config sync-days ").map(|s| s.trim()) {
        match days_str.parse::<u32>() {
            Ok(days) => {
                if days == 0 {
                    ui_logger.log("Sync days must be greater than 0".to_string());
                    return;
                }
                if days > 365 {
                    ui_logger.log(
                        "Sync days should not exceed 365 to avoid excessive data transfer"
                            .to_string(),
                    );
                    return;
                }

                if modify_config(ui_logger, error_logger, "sync-days", |config| {
                    config.auto_share.sync_days = days;
                })
                .await
                {
                    ui_logger.log(format!(
                        "{} Story sync timeframe set to {} days",
                        Icons::check(),
                        days
                    ));
                }
            }
            Err(_) => {
                ui_logger.log(format!(
                    "Invalid number: '{days_str}'. Please provide a valid number of days.",
                ));
            }
        }
    } else {
        ui_logger.log("Usage: config sync-days <number>".to_string());
    }
}

// ---------------------------------------------------------------------------
// Description handlers
// ---------------------------------------------------------------------------

pub async fn handle_create_description(cmd: &str, ui_logger: &UILogger) {
    let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
    if parts.len() < 3 {
        ui_logger.log("Usage: create desc <description>".to_string());
        return;
    }

    let description = parts[2].trim();

    let validated_description = match validate_and_log(
        ContentValidator::validate_node_description(description),
        "node description",
        ui_logger,
    ) {
        Some(desc) => desc,
        None => return,
    };

    match save_node_description(&validated_description).await {
        Ok(()) => {
            ui_logger.log(format!(
                "Node description saved: {} bytes",
                validated_description.len()
            ));
        }
        Err(e) => {
            ui_logger.log(format!("Failed to save description: {e}"));
        }
    }
}

pub async fn handle_get_description(
    cmd: &str,
    ui_logger: &UILogger,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    peer_names: &HashMap<PeerId, String>,
) {
    let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
    if parts.len() < 3 {
        ui_logger.log("Usage: get desc <peer_alias>".to_string());
        return;
    }

    let peer_alias = parts[2];

    let target_peer = match resolve_peer_by_alias(peer_alias, peer_names) {
        Some(peer) => peer,
        None => {
            ui_logger.log(format!("Peer '{peer_alias}' not found in connected peers."));
            return;
        }
    };

    if !swarm.is_connected(&target_peer) {
        ui_logger.log(format!(
            "Not connected to peer '{peer_alias}'. Use 'connect' to establish connection."
        ));
        return;
    }

    let from_name = local_peer_name.as_deref().unwrap_or("Unknown");

    let description_request = NodeDescriptionRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        timestamp: current_unix_timestamp(),
    };

    let _request_id = swarm
        .behaviour_mut()
        .node_description
        .send_request(&target_peer, description_request);

    ui_logger.log(format!("Requesting description from '{peer_alias}'..."));
}

pub async fn handle_show_description(ui_logger: &UILogger) {
    match load_node_description().await {
        Ok(Some(description)) => {
            ui_logger.log(format!(
                "Your node description ({} bytes):",
                description.len()
            ));
            ui_logger.log(description);
        }
        Ok(None) => {
            ui_logger.log(
                "No node description set. Use 'create desc <description>' to create one."
                    .to_string(),
            );
        }
        Err(e) => {
            ui_logger.log(format!("Failed to load description: {e}"));
        }
    }
}
