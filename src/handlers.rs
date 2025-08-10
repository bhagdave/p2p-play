use crate::crypto::CryptoError;
use crate::error_logger::ErrorLogger;
use crate::network::{
    DirectMessageRequest, NodeDescriptionRequest, PEER_ID, StoryBehaviour, TOPIC,
};
use crate::relay::{RelayError, RelayService};
use crate::storage::{
    create_channel, create_new_story_with_channel, delete_local_story, load_bootstrap_config,
    load_node_description, mark_story_as_read, publish_story, read_channels, read_local_stories,
    read_subscribed_channels, read_unsubscribed_channels, save_bootstrap_config,
    save_local_peer_name, save_node_description, subscribe_to_channel, unsubscribe_from_channel,
};
use crate::types::{
    ActionResult, DirectMessage, DirectMessageConfig, Icons, ListMode, ListRequest, PeerName,
    PendingDirectMessage, Story,
};
use crate::validation::ContentValidator;
use bytes::Bytes;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use log::debug;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Simple UI logger that can be passed around
#[derive(Clone)]
pub struct UILogger {
    pub sender: mpsc::UnboundedSender<String>,
}

impl UILogger {
    pub fn new(sender: mpsc::UnboundedSender<String>) -> Self {
        Self { sender }
    }

    pub fn log(&self, message: String) {
        let _ = self.sender.send(message);
    }
}

/// Cache for sorted peer names to avoid repeated sorting on every direct message
pub struct SortedPeerNamesCache {
    /// The sorted peer names by length (descending)
    sorted_names: Vec<String>,
    /// Version counter to track changes
    version: u64,
}

impl Default for SortedPeerNamesCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SortedPeerNamesCache {
    pub fn new() -> Self {
        Self {
            sorted_names: Vec::new(),
            version: 0,
        }
    }

    /// Update the cache with new peer names
    pub fn update(&mut self, peer_names: &HashMap<PeerId, String>) {
        let mut names: Vec<String> = peer_names.values().cloned().collect();
        names.sort_by(|a, b| b.len().cmp(&a.len()));
        self.sorted_names = names;
        self.version += 1;
    }

    /// Get the sorted peer names
    pub fn get_sorted_names(&self) -> &[String] {
        &self.sorted_names
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.sorted_names.is_empty()
    }
}

pub async fn handle_list_stories(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    let rest = cmd.strip_prefix("ls s ");
    match rest {
        Some("all") => {
            ui_logger.log("Requesting all stories from all peers".to_string());
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            debug!("JSON od request: {json}");
            let json_bytes = Bytes::from(json.into_bytes());
            debug!(
                "Publishing to topic: {:?} from peer:{:?}",
                TOPIC.clone(),
                PEER_ID.clone()
            );
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
            debug!("Published request");
        }
        Some(story_peer_id) => {
            ui_logger.log(format!("Requesting all stories from peer: {story_peer_id}"));
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            debug!("JSON od request: {json}");
            let json_bytes = Bytes::from(json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        None => {
            ui_logger.log("Local stories:".to_string());
            match read_local_stories().await {
                Ok(v) => {
                    ui_logger.log(format!("Local stories ({})", v.len()));
                    v.iter().for_each(|r| {
                        let status = if r.public {
                            format!("{} Public", Icons::book())
                        } else {
                            format!("{} Private", Icons::closed_book())
                        };
                        ui_logger.log(format!(
                            "{} | Channel: {} | {}: {}",
                            status, r.channel, r.id, r.name
                        ));
                    });
                }
                Err(e) => error_logger.log_error(&format!("Failed to fetch local stories: {e}")),
            };
        }
    };
}

pub async fn handle_create_stories(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    handle_create_stories_with_sender(cmd, ui_logger, error_logger, None).await
}

pub async fn handle_create_stories_with_sender(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    story_sender: Option<tokio::sync::mpsc::UnboundedSender<crate::types::Story>>,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("create s") {
        let rest = rest.trim();

        // Check if user wants interactive mode (no arguments provided)
        if rest.is_empty() {
            ui_logger.log(format!(
                "{} Starting interactive story creation...",
                Icons::book()
            ));
            ui_logger.log(format!(
                "{} This will guide you through creating a story step by step.",
                Icons::memo()
            ));
            ui_logger.log(format!("{} Use Esc at any time to cancel.", Icons::pin()));
            return Some(ActionResult::StartStoryCreation);
        } else {
            // Parse pipe-separated arguments
            let elements: Vec<&str> = rest.split('|').collect();
            if elements.len() < 3 {
                ui_logger.log(
                    "too few arguments - Format: name|header|body or name|header|body|channel"
                        .to_string(),
                );
            } else {
                let name = elements.first().expect("name is there");
                let header = elements.get(1).expect("header is there");
                let body = elements.get(2).expect("body is there");
                let channel = elements.get(3).unwrap_or(&"general");

                // Validate and sanitize story inputs
                let validated_name = match ContentValidator::validate_story_name(name) {
                    Ok(validated) => validated,
                    Err(e) => {
                        ui_logger.log(format!("Invalid story name: {e}"));
                        return None;
                    }
                };

                let validated_header = match ContentValidator::validate_story_header(header) {
                    Ok(validated) => validated,
                    Err(e) => {
                        ui_logger.log(format!("Invalid story header: {e}"));
                        return None;
                    }
                };

                let validated_body = match ContentValidator::validate_story_body(body) {
                    Ok(validated) => validated,
                    Err(e) => {
                        ui_logger.log(format!("Invalid story body: {e}"));
                        return None;
                    }
                };

                let validated_channel = match ContentValidator::validate_channel_name(channel) {
                    Ok(validated) => validated,
                    Err(e) => {
                        ui_logger.log(format!("Invalid channel name: {e}"));
                        return None;
                    }
                };

                if let Err(e) = create_new_story_with_channel(
                    &validated_name,
                    &validated_header,
                    &validated_body,
                    &validated_channel,
                )
                .await
                {
                    error_logger.log_error(&format!("Failed to create story: {e}"));
                } else {
                    ui_logger.log(format!(
                        "Story created and auto-published to channel '{validated_channel}'"
                    ));

                    // Auto-broadcast the newly created story to connected peers
                    if let Some(sender) = story_sender {
                        // Read the stories to find the one we just created
                        match read_local_stories().await {
                            Ok(stories) => {
                                // Find the most recently created story by name
                                if let Some(created_story) = stories.iter().find(|s| {
                                    s.name == validated_name
                                        && s.header == validated_header
                                        && s.body == validated_body
                                }) {
                                    if let Err(e) = sender.send(created_story.clone()) {
                                        error_logger.log_error(&format!(
                                            "Failed to broadcast newly created story: {e}"
                                        ));
                                    } else {
                                        ui_logger.log(
                                            "Story automatically shared with connected peers"
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error_logger.log_error(&format!(
                                    "Failed to read stories for auto-broadcast: {e}"
                                ));
                            }
                        }
                    }

                    return Some(ActionResult::RefreshStories);
                };
            }
        }
    }
    None
}

pub async fn handle_publish_story(
    cmd: &str,
    story_sender: mpsc::UnboundedSender<Story>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    if let Some(rest) = cmd.strip_prefix("publish s") {
        match ContentValidator::validate_story_id(rest) {
            Ok(id) => {
                if let Err(e) = publish_story(id, story_sender).await {
                    error_logger.log_error(&format!("Failed to publish story with id {id}: {e}"));
                } else {
                    ui_logger.log(format!("Published story with id: {id}"));
                }
            }
            Err(e) => ui_logger.log(format!("Invalid story ID: {e}")),
        };
    }
}

pub async fn handle_show_story(cmd: &str, ui_logger: &UILogger, peer_id: &str) {
    if let Some(rest) = cmd.strip_prefix("show story ") {
        match ContentValidator::validate_story_id(rest) {
            Ok(id) => {
                // Read local stories to find the story with the given ID
                match read_local_stories().await {
                    Ok(stories) => {
                        if let Some(story) = stories.iter().find(|s| s.id == id) {
                            ui_logger.log(format!(
                                "{} Story {}: {}",
                                Icons::book(),
                                story.id,
                                story.name
                            ));
                            ui_logger.log(format!("Channel: {}", story.channel));
                            ui_logger.log(format!("Header: {}", story.header));
                            ui_logger.log(format!("Body: {}", story.body));
                            ui_logger.log(format!(
                                "Public: {}",
                                if story.public { "Yes" } else { "No" }
                            ));

                            // Mark the story as read
                            mark_story_as_read_for_peer(story.id, peer_id, &story.channel).await;
                        } else {
                            ui_logger.log(format!("Story with id {id} not found"));
                        }
                    }
                    Err(e) => {
                        ui_logger.log(format!("Error reading stories: {e}"));
                    }
                }
            }
            Err(e) => {
                ui_logger.log(format!("Invalid story ID: {e}"));
            }
        }
    } else {
        ui_logger.log("Usage: show story <id>".to_string());
    }
}

pub async fn handle_delete_story(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("delete s ") {
        // Split by comma and process each ID
        let id_strings: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        let mut successful_deletions = 0;
        let mut failed_deletions = Vec::new();

        // Skip empty strings that might result from trailing commas or double commas
        let valid_id_strings: Vec<&str> =
            id_strings.into_iter().filter(|s| !s.is_empty()).collect();

        if valid_id_strings.is_empty() {
            ui_logger.log("Usage: delete s <id1>[,<id2>,<id3>...]".to_string());
            return None;
        }

        let is_batch_operation = valid_id_strings.len() > 1;

        for id_str in valid_id_strings {
            match ContentValidator::validate_story_id(id_str) {
                Ok(id) => match delete_local_story(id).await {
                    Ok(deleted) => {
                        if deleted {
                            ui_logger.log(format!("✅ Story {id} deleted successfully"));
                            successful_deletions += 1;
                        } else {
                            let failure_msg = format!("Story {id} not found");
                            ui_logger.log(format!("❌ {failure_msg}"));
                            failed_deletions.push(failure_msg);
                        }
                    }
                    Err(e) => {
                        let failure_msg = format!("Failed to delete story {id}: {e}");
                        ui_logger.log(format!("❌ {failure_msg}"));
                        error_logger.log_error(&failure_msg);
                        failed_deletions.push(failure_msg);
                    }
                },
                Err(e) => {
                    let failure_msg = format!("Invalid story ID: {e}");
                    ui_logger.log(format!("❌ {failure_msg}"));
                    failed_deletions.push(failure_msg);
                }
            }
        }

        // Report batch operation summary only if there were failures and multiple operations
        if is_batch_operation && !failed_deletions.is_empty() {
            let total_failed = failed_deletions.len();
            use crate::errors::StorageError;
            let batch_error = StorageError::batch_operation_failed(
                successful_deletions,
                total_failed,
                failed_deletions,
            );
            ui_logger.log(format!("📊 Batch deletion summary: {batch_error}"));
            error_logger.log_error(&format!(
                "Batch delete operation completed with errors: {batch_error}"
            ));
        }

        // Return RefreshStories if any story was successfully deleted
        if successful_deletions > 0 {
            return Some(ActionResult::RefreshStories);
        }
    } else {
        ui_logger.log("Usage: delete s <id1>[,<id2>,<id3>...]".to_string());
    }
    None
}

pub async fn handle_peer_id(_cmd: &str, ui_logger: &UILogger) {
    ui_logger.log(format!("Local Peer ID: {}", PEER_ID.to_string()));
}

pub async fn handle_help(_cmd: &str, ui_logger: &UILogger) {
    ui_logger.log("ls s to list stories".to_string());
    ui_logger.log("ls ch [available|unsubscribed] to list channels".to_string());
    ui_logger.log("ls sub to list your subscriptions".to_string());
    ui_logger
        .log("create s name|header|body[|channel] to create and auto-publish story".to_string());
    ui_logger.log("create ch name|description to create channel".to_string());
    ui_logger.log("create desc <description> to create node description".to_string());
    ui_logger.log("publish s to manually publish/re-publish story".to_string());
    ui_logger.log("show story <id> to show story details".to_string());
    ui_logger.log("show desc to show your node description".to_string());
    ui_logger.log("get desc <peer_alias> to get description from peer".to_string());
    ui_logger.log("set auto-sub [on|off|status] to manage auto-subscription".to_string());
    ui_logger
        .log("config auto-share [on|off|status] to control automatic story sharing".to_string());
    ui_logger.log("config sync-days <N> to set story sync timeframe (days)".to_string());
    ui_logger.log("delete s <id1>[,<id2>,<id3>...] to delete one or more stories".to_string());
    ui_logger.log("sub <channel> to subscribe to channel".to_string());
    ui_logger.log("unsub <channel> to unsubscribe from channel".to_string());
    ui_logger.log("name <alias> to set your peer name".to_string());
    ui_logger.log("peer id to show your full peer ID".to_string());
    ui_logger.log("msg <peer_alias> <message> to send direct message".to_string());
    ui_logger.log("dht bootstrap add/remove/list/clear/retry - manage bootstrap peers".to_string());
    ui_logger.log("dht bootstrap <multiaddr> to bootstrap directly with peer".to_string());
    ui_logger.log("dht peers to find closest peers in DHT".to_string());
    ui_logger.log("reload config to reload network configuration".to_string());
    ui_logger.log("quit to quit".to_string());
}

pub async fn handle_reload_config(_cmd: &str, ui_logger: &UILogger) {
    use crate::storage::load_unified_network_config;

    match load_unified_network_config().await {
        Ok(config) => {
            // For now, just validate and log success
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

pub async fn handle_config_auto_share(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    use crate::storage::{load_unified_network_config, save_unified_network_config};

    if let Some(setting) = cmd.strip_prefix("config auto-share ").map(|s| s.trim()) {
        match setting {
            "on" => match load_unified_network_config().await {
                Ok(mut config) => {
                    config.auto_share.global_auto_share = true;
                    match save_unified_network_config(&config).await {
                        Ok(_) => {
                            ui_logger.log(format!(
                                "{} Auto-share enabled - new stories will be shared automatically",
                                Icons::check()
                            ));
                        }
                        Err(e) => {
                            error_logger
                                .log_error(&format!("Failed to save auto-share config: {e}"));
                            ui_logger.log(format!(
                                "{} Failed to save auto-share configuration",
                                Icons::cross()
                            ));
                        }
                    }
                }
                Err(e) => {
                    error_logger
                        .log_error(&format!("Failed to load config for auto-share update: {e}"));
                    ui_logger.log(format!("{} Failed to load configuration", Icons::cross()));
                }
            },
            "off" => match load_unified_network_config().await {
                Ok(mut config) => {
                    config.auto_share.global_auto_share = false;
                    match save_unified_network_config(&config).await {
                        Ok(_) => {
                            ui_logger.log(format!(
                                "{} Auto-share disabled - stories will not be shared automatically",
                                Icons::check()
                            ));
                        }
                        Err(e) => {
                            error_logger
                                .log_error(&format!("Failed to save auto-share config: {e}"));
                            ui_logger.log(format!(
                                "{} Failed to save auto-share configuration",
                                Icons::cross()
                            ));
                        }
                    }
                }
                Err(e) => {
                    error_logger
                        .log_error(&format!("Failed to load config for auto-share update: {e}"));
                    ui_logger.log(format!("{} Failed to load configuration", Icons::cross()));
                }
            },
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

pub async fn handle_config_sync_days(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    use crate::storage::{load_unified_network_config, save_unified_network_config};

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

                match load_unified_network_config().await {
                    Ok(mut config) => {
                        config.auto_share.sync_days = days;
                        match save_unified_network_config(&config).await {
                            Ok(_) => {
                                ui_logger.log(format!(
                                    "{} Story sync timeframe set to {} days",
                                    Icons::check(),
                                    days
                                ));
                            }
                            Err(e) => {
                                error_logger
                                    .log_error(&format!("Failed to save sync-days config: {e}"));
                                ui_logger.log(format!(
                                    "{} Failed to save sync days configuration",
                                    Icons::cross()
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        error_logger
                            .log_error(&format!("Failed to load config for sync-days update: {e}"));
                        ui_logger.log(format!("{} Failed to load configuration", Icons::cross()));
                    }
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

pub async fn handle_set_name(
    cmd: &str,
    local_peer_name: &mut Option<String>,
    ui_logger: &UILogger,
) -> Option<PeerName> {
    if let Some(name) = cmd.strip_prefix("name ") {
        let name = name.trim();

        // Validate and sanitize peer name
        let validated_name = match ContentValidator::validate_peer_name(name) {
            Ok(validated) => validated,
            Err(e) => {
                ui_logger.log(format!("Invalid peer name: {e}"));
                return None;
            }
        };

        *local_peer_name = Some(validated_name.clone());

        // Save the peer name to storage for persistence across restarts
        if let Err(e) = save_local_peer_name(&validated_name).await {
            ui_logger.log(format!("Warning: Failed to save peer name: {e}"));
        }

        // Return a PeerName message to broadcast to connected peers
        Some(PeerName::new(PEER_ID.to_string(), validated_name))
    } else {
        ui_logger.log("Usage: name <alias>".to_string());
        None
    }
}

/// Parse a direct message command that may contain peer names with spaces
pub fn parse_direct_message_command(
    rest: &str,
    sorted_peer_names: &[String],
) -> Option<(String, String)> {
    // Try to match against sorted peer names first (handles names with spaces)
    // Names are already sorted by length in descending order to prioritize longer names
    for peer_name in sorted_peer_names {
        // Check if the rest starts with this peer name
        if rest.starts_with(peer_name) {
            let remaining = &rest[peer_name.len()..];

            // If we have an exact match (peer name with no message)
            if remaining.is_empty() {
                return None; // No message provided
            }

            // If the peer name is followed by a space
            if remaining.starts_with(' ') {
                let message = remaining[1..].trim();
                if !message.is_empty() {
                    return Some((peer_name.clone(), message.to_string()));
                } else {
                    return None; // Empty message after space
                }
            }

            // If it's not followed by a space, this is an invalid command
            // because the peer name should be followed by a space and then a message
            return None;
        }
    }

    // Fallback to original parsing for backward compatibility
    // This handles simple names without spaces that are not in the known peer list
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() >= 2 {
        let to_name = parts[0].trim();
        let message = parts[1].trim();

        if !to_name.is_empty() && !message.is_empty() {
            return Some((to_name.to_string(), message.to_string()));
        }
    }

    None
}

pub async fn handle_direct_message(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    sorted_peer_names_cache: &SortedPeerNamesCache,
    ui_logger: &UILogger,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) {
    if let Some(rest) = cmd.strip_prefix("msg ") {
        let (to_name, message) =
            match parse_direct_message_command(rest, sorted_peer_names_cache.get_sorted_names()) {
                Some((name, msg)) => (name, msg),
                None => {
                    ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
                    return;
                }
            };

        // Validate and sanitize peer name
        let validated_to_name = match ContentValidator::validate_peer_name(&to_name) {
            Ok(validated) => validated,
            Err(e) => {
                ui_logger.log(format!("Invalid peer name: {e}"));
                return;
            }
        };

        // Validate message content
        let validated_message = match ContentValidator::validate_direct_message(&message) {
            Ok(validated) => validated,
            Err(e) => {
                ui_logger.log(format!("Invalid message: {e}"));
                return;
            }
        };

        let from_name = match local_peer_name {
            Some(name) => name.clone(),
            None => {
                ui_logger.log("You must set your name first using 'name <alias>'".to_string());
                return;
            }
        };

        // Try to find current peer ID, but don't require it
        let (target_peer_id, is_placeholder) = peer_names
            .iter()
            .find(|(_, name)| name == &&validated_to_name)
            .map(|(peer_id, _)| (*peer_id, false))
            .unwrap_or_else(|| {
                // Generate a placeholder PeerId for queueing - this will be resolved when peer connects
                // For now, we'll use a hash of the peer name as a temporary PeerId
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                validated_to_name.hash(&mut hasher);
                let placeholder_id =
                    PeerId::from_bytes(&hasher.finish().to_be_bytes()).unwrap_or(PeerId::random());
                (placeholder_id, true)
            });

        // Create a direct message request with validated sender identity
        let direct_msg_request = DirectMessageRequest {
            from_peer_id: PEER_ID.to_string(),
            from_name: from_name.clone(),
            to_name: validated_to_name.clone(),
            message: validated_message.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // Add message to retry queue instead of sending immediately
        let pending_msg = PendingDirectMessage::new(
            target_peer_id,
            validated_to_name.clone(),
            direct_msg_request.clone(),
            dm_config.max_retry_attempts,
            is_placeholder,
        );

        // Try to send immediately, but queue for retry if it fails
        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&target_peer_id, direct_msg_request);

        // Add to pending queue regardless - will be removed on successful delivery
        if let Ok(mut queue) = pending_messages.lock() {
            queue.push(pending_msg);
        }

        ui_logger.log(format!(
            "Direct message queued for {validated_to_name}: {validated_message}"
        ));
        debug!(
            "Queued direct message to {validated_to_name} from {from_name} (request_id: {request_id:?})"
        );
    } else {
        ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
    }
}

/// Enhanced direct message handler with relay support as fallback
pub async fn handle_direct_message_with_relay(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    sorted_peer_names_cache: &SortedPeerNamesCache,
    ui_logger: &UILogger,
    dm_config: &DirectMessageConfig,
    relay_service: &mut Option<RelayService>,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) {
    if let Some(rest) = cmd.strip_prefix("msg ") {
        let (to_name, message) =
            match parse_direct_message_command(rest, sorted_peer_names_cache.get_sorted_names()) {
                Some((name, msg)) => (name, msg),
                None => {
                    ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
                    return;
                }
            };

        if to_name.is_empty() || message.is_empty() {
            ui_logger.log("Both peer alias and message must be non-empty".to_string());
            return;
        }

        let from_name = match local_peer_name {
            Some(name) => name.clone(),
            None => {
                ui_logger.log("You must set your name first using 'name <alias>'".to_string());
                return;
            }
        };

        if to_name.trim().is_empty() {
            ui_logger.log("Peer name cannot be empty".to_string());
            return;
        }

        if to_name.len() > 50 {
            ui_logger.log("Peer name too long (max 50 characters)".to_string());
            return;
        }

        // Try to find current peer ID
        let target_peer_info = peer_names
            .iter()
            .find(|(_, name)| name == &&to_name)
            .map(|(peer_id, _)| (*peer_id, false));

        if let Some((target_peer_id, _)) = target_peer_info {
            // Check relay configuration to see if we should prefer direct or always use relay
            let prefer_direct = relay_service
                .as_ref()
                .map(|rs| rs.config().prefer_direct)
                .unwrap_or(true);

            if prefer_direct {
                // 1. Try direct connection first if peer is known and connected
                ui_logger.log(format!("⏳ Attempting direct message to {to_name}..."));

                let direct_msg_request = DirectMessageRequest {
                    from_peer_id: PEER_ID.to_string(),
                    from_name: from_name.clone(),
                    to_name: to_name.clone(),
                    message: message.clone(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };

                // Attempt direct send
                let request_id = swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&target_peer_id, direct_msg_request.clone());

                ui_logger.log(format!(
                    "📨 Direct message sent to {to_name} (request_id: {request_id:?})"
                ));
                debug!("Direct message sent to {to_name} with request_id {request_id:?}");

                // When prefer_direct=true, don't attempt relay backup for successful direct sends
                return;
            }
        }

        // 2. Try relay delivery if relay service is available and enabled
        if let Some(relay_svc) = relay_service {
            if relay_svc.config().enable_relay {
                let relay_target_peer_id = if let Some((peer_id, _)) = target_peer_info {
                    peer_id
                } else {
                    // For unknown peers, we can't encrypt directly to them yet
                    ui_logger.log(format!(
                        "❌ Cannot relay to unknown peer '{to_name}' - peer not in network"
                    ));
                    // Fall through to queuing system
                    ui_logger.log(format!(
                        "📥 Queueing message for {to_name} - will retry when peer connects"
                    ));
                    queue_message_for_retry(
                        &from_name,
                        &to_name,
                        &message,
                        &target_peer_info,
                        dm_config,
                        pending_messages,
                        ui_logger,
                    );
                    return;
                };

                // Try relay delivery
                if try_relay_delivery(
                    swarm,
                    relay_svc,
                    &from_name,
                    &to_name,
                    &message,
                    &relay_target_peer_id,
                    ui_logger,
                )
                .await
                {
                    return; // Successfully sent via relay
                }
            }
        }

        // 3. Fall back to traditional queuing system for retry
        ui_logger.log(format!(
            "📥 Queueing message for {to_name} - will retry when peer connects"
        ));
        queue_message_for_retry(
            &from_name,
            &to_name,
            &message,
            &target_peer_info,
            dm_config,
            pending_messages,
            ui_logger,
        );
    } else {
        ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
    }
}

/// Helper function to attempt relay delivery
async fn try_relay_delivery(
    swarm: &mut Swarm<StoryBehaviour>,
    relay_service: &mut RelayService,
    from_name: &str,
    to_name: &str,
    message: &str,
    target_peer_id: &PeerId,
    ui_logger: &UILogger,
) -> bool {
    ui_logger.log(format!("📡 Trying relay delivery to {to_name}..."));

    // Create DirectMessage struct for relay
    let direct_msg = DirectMessage {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        to_name: to_name.to_string(),
        message: message.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // Create and broadcast relay message
    match relay_service.create_relay_message(&direct_msg, target_peer_id) {
        Ok(relay_msg) => {
            // Broadcast the relay message via floodsub
            match crate::event_handlers::broadcast_relay_message(swarm, &relay_msg).await {
                Ok(()) => {
                    ui_logger.log(format!("✅ Message sent to {to_name} via relay network"));
                    debug!("Relay message broadcasted successfully for {to_name}");
                    true
                }
                Err(e) => {
                    ui_logger.log(format!("❌ Failed to broadcast relay message: {e}"));
                    debug!("Relay broadcast failed: {e}");
                    false
                }
            }
        }
        Err(e) => {
            // Check if this is a missing public key error (offline peer scenario)
            if let RelayError::CryptoError(CryptoError::EncryptionFailed(msg)) = &e {
                if msg.contains("Public key not found") {
                    // User-friendly message for offline peer without public key
                    ui_logger.log(format!(
                        "{} Cannot send secure message to offline peer '{to_name}'",
                        Icons::warning()
                    ));
                    ui_logger.log(format!(
                        "{} Message queued - will be delivered when {to_name} comes online and security keys are exchanged",
                        Icons::envelope()
                    ));
                    ui_logger.log(format!(
                        "{}  Tip: Both peers must be online simultaneously for secure messaging setup",
                        Icons::memo()
                    ));
                    debug!(
                        "Relay message creation failed due to missing public key for {to_name}: {e}"
                    );
                    return false;
                }
            }

            // Fallback to technical error for other issues
            ui_logger.log(format!("❌ Failed to create relay message: {e}"));
            debug!("Relay message creation failed: {e}");
            false
        }
    }
}

/// Helper function to queue message for retry
fn queue_message_for_retry(
    from_name: &str,
    to_name: &str,
    message: &str,
    target_peer_info: &Option<(PeerId, bool)>,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
    ui_logger: &UILogger,
) {
    let (target_peer_id, is_placeholder) = target_peer_info.unwrap_or_else(|| {
        // Generate a placeholder PeerId for queueing
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        to_name.hash(&mut hasher);
        let placeholder_id =
            PeerId::from_bytes(&hasher.finish().to_be_bytes()).unwrap_or(PeerId::random());
        (placeholder_id, true)
    });

    let direct_msg_request = DirectMessageRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        to_name: to_name.to_string(),
        message: message.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let pending_msg = PendingDirectMessage::new(
        target_peer_id,
        to_name.to_string(),
        direct_msg_request,
        dm_config.max_retry_attempts,
        is_placeholder,
    );

    if let Ok(mut queue) = pending_messages.lock() {
        queue.push(pending_msg);
        ui_logger.log(format!("Message for {to_name} added to retry queue"));
        debug!("Message queued for {to_name} with fallback to retry system");
    } else {
        ui_logger.log("Failed to queue message - retry system unavailable".to_string());
    }
}

pub async fn handle_create_channel(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("create ch") {
        let rest = rest.trim();
        let elements: Vec<&str> = rest.split('|').collect();

        if elements.len() < 2 {
            ui_logger.log("Format: create ch name|description".to_string());
            return None;
        }

        let name = elements[0].trim();
        let description = elements[1].trim();

        // Validate and sanitize channel inputs
        let validated_name = match ContentValidator::validate_channel_name(name) {
            Ok(validated) => validated,
            Err(e) => {
                ui_logger.log(format!("Invalid channel name: {e}"));
                return None;
            }
        };

        let validated_description =
            match ContentValidator::validate_channel_description(description) {
                Ok(validated) => validated,
                Err(e) => {
                    ui_logger.log(format!("Invalid channel description: {e}"));
                    return None;
                }
            };

        let creator = match local_peer_name {
            Some(peer_name) => peer_name.clone(),
            None => PEER_ID.to_string(),
        };

        if let Err(e) = create_channel(&validated_name, &validated_description, &creator).await {
            error_logger.log_error(&format!("Failed to create channel: {e}"));
        } else {
            ui_logger.log(format!("Channel '{validated_name}' created successfully"));

            // Auto-subscribe to the channel we created
            if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), &validated_name).await {
                error_logger
                    .log_error(&format!("Failed to auto-subscribe to created channel: {e}"));
            }

            // Broadcast the channel to other peers
            let channel = crate::types::Channel::new(
                validated_name.clone(),
                validated_description,
                creator.clone(),
            );
            let published_channel = crate::types::PublishedChannel::new(channel.clone(), creator);

            // Broadcast both formats for backward compatibility
            // 1. Broadcast PublishedChannel for new nodes (preferred format)
            let published_json = match serde_json::to_string(&published_channel) {
                Ok(json) => json,
                Err(e) => {
                    error_logger.log_error(&format!("Failed to serialize published channel: {e}"));
                    return Some(ActionResult::RefreshStories);
                }
            };
            let published_json_bytes = Bytes::from(published_json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), published_json_bytes);
            debug!("Broadcasted published channel '{validated_name}' to connected peers");

            // 2. Broadcast legacy Channel format for backward compatibility with older nodes
            let legacy_json = match serde_json::to_string(&channel) {
                Ok(json) => json,
                Err(e) => {
                    error_logger.log_error(&format!("Failed to serialize legacy channel: {e}"));
                    return Some(ActionResult::RefreshStories);
                }
            };
            let legacy_json_bytes = Bytes::from(legacy_json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), legacy_json_bytes);
            debug!("Broadcasted legacy channel '{validated_name}' for backward compatibility");

            ui_logger.log(format!("Channel '{validated_name}' shared with network"));

            return Some(ActionResult::RefreshStories);
        }
    }
    None
}

pub async fn handle_list_channels(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    let rest = cmd.strip_prefix("ls ch");
    match rest {
        Some(" available") => {
            // List all discovered channels (subscribed + unsubscribed)
            match read_channels().await {
                Ok(channels) => {
                    ui_logger.log("Available channels:".to_string());
                    if channels.is_empty() {
                        ui_logger.log("  (no channels discovered)".to_string());
                    } else {
                        for channel in channels {
                            ui_logger.log(format!("  {} - {}", channel.name, channel.description));
                        }
                    }
                }
                Err(e) => {
                    error_logger.log_error(&format!("Failed to read available channels: {e}"))
                }
            }
        }
        Some(" unsubscribed") => {
            // List channels available but not subscribed to
            match read_unsubscribed_channels(&PEER_ID.to_string()).await {
                Ok(channels) => {
                    ui_logger.log("Unsubscribed channels:".to_string());
                    if channels.is_empty() {
                        ui_logger.log("  (no unsubscribed channels)".to_string());
                    } else {
                        for channel in channels {
                            ui_logger.log(format!("  {} - {}", channel.name, channel.description));
                        }
                    }
                }
                Err(e) => {
                    error_logger.log_error(&format!("Failed to read unsubscribed channels: {e}"))
                }
            }
        }
        Some("") | None => {
            // Default behavior - list all channels
            match read_channels().await {
                Ok(channels) => {
                    ui_logger.log("Available channels:".to_string());
                    if channels.is_empty() {
                        ui_logger.log("  (no channels discovered)".to_string());
                    } else {
                        for channel in channels {
                            ui_logger.log(format!("  {} - {}", channel.name, channel.description));
                        }
                    }
                }
                Err(e) => error_logger.log_error(&format!("Failed to read channels: {e}")),
            }
        }
        _ => {
            ui_logger.log("Usage: ls ch [available|unsubscribed]".to_string());
        }
    }
}

pub async fn handle_subscribe_channel(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<crate::types::ActionResult> {
    let channel_name = if let Some(name) = cmd.strip_prefix("sub ch ") {
        name.trim()
    } else if let Some(name) = cmd.strip_prefix("sub ") {
        name.trim()
    } else {
        ui_logger.log("Usage: sub ch <channel_name> or sub <channel_name>".to_string());
        return None;
    };

    if channel_name.is_empty() {
        ui_logger.log("Usage: sub ch <channel_name> or sub <channel_name>".to_string());
        return None;
    }

    // First check if the channel exists in the channels table
    match read_channels().await {
        Ok(channels) => {
            let channel_exists = channels.iter().any(|c| c.name == channel_name);
            if !channel_exists {
                ui_logger.log(format!("❌ Channel '{channel_name}' not found in available channels. Use 'ls ch available' to see all discovered channels."));
                return None;
            }
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to check available channels: {e}"));
            ui_logger.log("❌ Could not verify channel exists. Please try again.".to_string());
            return None;
        }
    }

    if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), channel_name).await {
        error_logger.log_error(&format!("Failed to subscribe to channel: {e}"));
        ui_logger.log(format!(
            "❌ Failed to subscribe to channel '{channel_name}': {e}"
        ));
        None
    } else {
        ui_logger.log(format!("✅ Subscribed to channel '{channel_name}'"));
        Some(crate::types::ActionResult::RefreshChannels)
    }
}

pub async fn handle_unsubscribe_channel(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<crate::types::ActionResult> {
    let channel_name = if let Some(name) = cmd.strip_prefix("unsub ch ") {
        name.trim()
    } else if let Some(name) = cmd.strip_prefix("unsub ") {
        name.trim()
    } else {
        ui_logger.log("Usage: unsub ch <channel_name> or unsub <channel_name>".to_string());
        return None;
    };

    if channel_name.is_empty() {
        ui_logger.log("Usage: unsub ch <channel_name> or unsub <channel_name>".to_string());
        return None;
    }

    if let Err(e) = unsubscribe_from_channel(&PEER_ID.to_string(), channel_name).await {
        error_logger.log_error(&format!("Failed to unsubscribe from channel: {e}"));
        ui_logger.log(format!(
            "❌ Failed to unsubscribe from channel '{channel_name}': {e}"
        ));
        None
    } else {
        ui_logger.log(format!("✅ Unsubscribed from channel '{channel_name}'"));
        Some(crate::types::ActionResult::RefreshChannels)
    }
}

pub async fn handle_list_subscriptions(ui_logger: &UILogger, error_logger: &ErrorLogger) {
    match read_subscribed_channels(&PEER_ID.to_string()).await {
        Ok(channels) => {
            ui_logger.log("Your subscribed channels:".to_string());
            if channels.is_empty() {
                ui_logger.log("  (no subscriptions)".to_string());
            } else {
                for channel in channels {
                    ui_logger.log(format!("  {channel}"));
                }
            }
        }
        Err(e) => error_logger.log_error(&format!("Failed to read subscriptions: {e}")),
    }
}

/// Handle setting auto-subscription configuration
pub async fn handle_set_auto_subscription(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    let rest = cmd.strip_prefix("set auto-sub ");
    match rest {
        Some("on") => {
            // Enable auto-subscription
            match crate::storage::load_unified_network_config().await {
                Ok(mut config) => {
                    config
                        .channel_auto_subscription
                        .auto_subscribe_to_new_channels = true;
                    match crate::storage::save_unified_network_config(&config).await {
                        Ok(_) => ui_logger.log("✅ Auto-subscription enabled".to_string()),
                        Err(e) => error_logger.log_error(&format!("Failed to save config: {e}")),
                    }
                }
                Err(e) => error_logger.log_error(&format!("Failed to load config: {e}")),
            }
        }
        Some("off") => {
            // Disable auto-subscription
            match crate::storage::load_unified_network_config().await {
                Ok(mut config) => {
                    config
                        .channel_auto_subscription
                        .auto_subscribe_to_new_channels = false;
                    match crate::storage::save_unified_network_config(&config).await {
                        Ok(_) => ui_logger.log("❌ Auto-subscription disabled".to_string()),
                        Err(e) => error_logger.log_error(&format!("Failed to save config: {e}")),
                    }
                }
                Err(e) => error_logger.log_error(&format!("Failed to load config: {e}")),
            }
        }
        Some("status") | None => {
            // Show current status
            match crate::storage::load_unified_network_config().await {
                Ok(config) => {
                    let status = if config
                        .channel_auto_subscription
                        .auto_subscribe_to_new_channels
                    {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    ui_logger.log(format!("Auto-subscription is currently {status}"));
                    ui_logger.log(format!(
                        "Notifications: {}",
                        if config.channel_auto_subscription.notify_new_channels {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));
                    ui_logger.log(format!(
                        "Max auto-subscriptions: {}",
                        config.channel_auto_subscription.max_auto_subscriptions
                    ));
                }
                Err(e) => error_logger.log_error(&format!("Failed to load config: {e}")),
            }
        }
        _ => {
            ui_logger.log("Usage: set auto-sub [on|off|status]".to_string());
        }
    }
}

/// Mark a story as read (should be called after displaying it)
pub async fn mark_story_as_read_for_peer(story_id: usize, peer_id: &str, channel_name: &str) {
    if let Err(e) = mark_story_as_read(story_id, peer_id, channel_name).await {
        debug!("Failed to mark story {story_id} as read: {e}");
    }
}

/// Helper function to refresh unread counts and update UI
pub async fn refresh_unread_counts_for_ui(app: &mut crate::ui::App, peer_id: &str) {
    match crate::storage::get_unread_counts_by_channel(peer_id).await {
        Ok(unread_counts) => {
            debug!(
                "Refreshed unread counts for {} channels",
                unread_counts.len()
            );
            app.update_unread_counts(unread_counts);
        }
        Err(e) => {
            debug!("Failed to refresh unread counts: {e}");
        }
    }
}

pub async fn establish_direct_connection(
    swarm: &mut Swarm<StoryBehaviour>,
    addr_str: &str,
    ui_logger: &UILogger,
) {
    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            ui_logger.log(format!("Manually dialing address: {addr}"));
            match swarm.dial(addr) {
                Ok(_) => {
                    ui_logger.log("Dialing initiated successfully".to_string());

                    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
                    debug!("Number of connected peers: {}", connected_peers.len());

                    // Add existing connected peers to floodsub immediately
                    for peer in connected_peers {
                        debug!("Connected to peer: {peer}");
                        debug!("Adding peer to floodsub: {peer}");
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                Err(e) => ui_logger.log(format!("Failed to dial: {e}")),
            }
        }
        Err(e) => ui_logger.log(format!("Failed to parse address: {e}")),
    }
}

/// Handle creating node description
pub async fn handle_create_description(cmd: &str, ui_logger: &UILogger) {
    let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
    if parts.len() < 3 {
        ui_logger.log("Usage: create desc <description>".to_string());
        return;
    }

    let description = parts[2].trim();

    // Validate and sanitize node description
    let validated_description = match ContentValidator::validate_node_description(description) {
        Ok(validated) => validated,
        Err(e) => {
            ui_logger.log(format!("Invalid node description: {e}"));
            return;
        }
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

/// Handle requesting node description from a peer
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

    // Find the peer by their alias
    let target_peer = peer_names
        .iter()
        .find(|(_, name)| name.as_str() == peer_alias)
        .map(|(peer_id, _)| *peer_id);

    let target_peer = match target_peer {
        Some(peer) => peer,
        None => {
            ui_logger.log(format!("Peer '{peer_alias}' not found in connected peers."));
            return;
        }
    };

    // Check if we're connected to this peer
    if !swarm.is_connected(&target_peer) {
        ui_logger.log(format!(
            "Not connected to peer '{peer_alias}'. Use 'connect' to establish connection."
        ));
        return;
    }

    // Send a node description request using the dedicated protocol
    let from_name = local_peer_name.as_deref().unwrap_or("Unknown");

    let description_request = NodeDescriptionRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let _request_id = swarm
        .behaviour_mut()
        .node_description
        .send_request(&target_peer, description_request);

    ui_logger.log(format!("Requesting description from '{peer_alias}'..."));
}

/// Handle showing local node description
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

/// Handle DHT bootstrap command with subcommands
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
        // Legacy support: if third part looks like a multiaddr, treat as direct bootstrap
        addr if addr.starts_with("/") => {
            let addr_str = parts[2..].join(" ");
            handle_direct_bootstrap(&addr_str, swarm, ui_logger).await;
        }
        _ => show_bootstrap_usage(ui_logger),
    }
}

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

    // Validate the multiaddr format
    if let Err(e) = multiaddr.parse::<libp2p::Multiaddr>() {
        ui_logger.log(format!("Invalid multiaddr '{multiaddr}': {e}"));
        return;
    }

    // Load current config, add peer, and save
    match load_bootstrap_config().await {
        Ok(mut config) => {
            if config.add_peer(multiaddr.clone()) {
                match save_bootstrap_config(&config).await {
                    Ok(_) => {
                        ui_logger.log(format!("Added bootstrap peer: {multiaddr}"));
                        ui_logger.log(format!(
                            "Total bootstrap peers: {}",
                            config.bootstrap_peers.len()
                        ));
                    }
                    Err(e) => ui_logger.log(format!("Failed to save bootstrap config: {e}")),
                }
            } else {
                ui_logger.log(format!("Bootstrap peer already exists: {multiaddr}"));
            }
        }
        Err(e) => ui_logger.log(format!("Failed to load bootstrap config: {e}")),
    }
}

async fn handle_bootstrap_remove(args: &[&str], ui_logger: &UILogger) {
    if args.is_empty() {
        ui_logger.log("Usage: dht bootstrap remove <multiaddr>".to_string());
        return;
    }

    let multiaddr = args.join(" ");

    // Load current config, remove peer, and save
    match load_bootstrap_config().await {
        Ok(mut config) => {
            // Check if this would remove the last bootstrap peer
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
    match load_bootstrap_config().await {
        Ok(mut config) => {
            let peer_count = config.bootstrap_peers.len();
            config.clear_peers();
            match save_bootstrap_config(&config).await {
                Ok(_) => {
                    ui_logger.log(format!("Cleared {peer_count} bootstrap peers"));
                    ui_logger.log("Warning: No bootstrap peers configured. Add peers to enable DHT connectivity.".to_string());
                }
                Err(e) => ui_logger.log(format!("Failed to save bootstrap config: {e}")),
            }
        }
        Err(e) => ui_logger.log(format!("Failed to load bootstrap config: {e}")),
    }
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

            // Start bootstrap process
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

            // Add the address as a bootstrap peer in the DHT
            if let Some(peer_id) = extract_peer_id_from_multiaddr(&addr) {
                swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, addr.clone());

                // Start bootstrap process (this will handle dialing the peer internally)
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

/// Handle DHT get closest peers command
pub async fn handle_dht_get_peers(
    _cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
) {
    ui_logger.log("Searching for closest peers in DHT...".to_string());

    // Get closest peers to our own peer ID
    let _query_id = swarm.behaviour_mut().kad.get_closest_peers(*PEER_ID);
    ui_logger.log("DHT peer search started (results will appear in events)".to_string());
}

/// Extract peer ID from a multiaddr if it contains one
pub fn extract_peer_id_from_multiaddr(addr: &libp2p::Multiaddr) -> Option<PeerId> {
    for protocol in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer_id) = protocol {
            return Some(peer_id);
        }
    }
    None
}
