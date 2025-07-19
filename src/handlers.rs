use crate::error_logger::ErrorLogger;
use crate::network::{DirectMessageRequest, PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{
    create_channel, create_new_story_with_channel, delete_local_story, publish_story, read_channels,
    read_local_stories, read_subscribed_channels, save_local_peer_name, subscribe_to_channel,
    unsubscribe_from_channel,
};
use crate::types::{ListMode, ListRequest, PeerName, Story};
use bytes::Bytes;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use log::info;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

/// Simple UI logger that can be passed around
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

pub async fn handle_list_peers(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    ui_logger: &UILogger,
) {
    ui_logger.log("Discovered Peers:".to_string());
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| {
        let name = peer_names
            .get(p)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        ui_logger.log(format!("{}{}", p, name));
    });
}

pub async fn handle_list_connections(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    ui_logger: &UILogger,
) {
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    ui_logger.log(format!("Connected Peers: {}", connected_peers.len()));
    for peer in connected_peers {
        let name = peer_names
            .get(&peer)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        ui_logger.log(format!("Connected to: {}{}", peer, name));
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
            info!("JSON od request: {}", json);
            let json_bytes = Bytes::from(json.into_bytes());
            info!(
                "Publishing to topic: {:?} from peer:{:?}",
                TOPIC.clone(),
                PEER_ID.clone()
            );
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
            info!("Published request");
        }
        Some(story_peer_id) => {
            ui_logger.log(format!(
                "Requesting all stories from peer: {}",
                story_peer_id
            ));
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            info!("JSON od request: {}", json);
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
                    v.iter().for_each(|r| ui_logger.log(format!("{:?}", r)));
                }
                Err(e) => error_logger.log_error(&format!("Failed to fetch local stories: {}", e)),
            };
        }
    };
}

pub async fn handle_create_stories(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<()> {
    if let Some(rest) = cmd.strip_prefix("create s") {
        let rest = rest.trim();

        // Check if user wants interactive mode (no arguments provided)
        if rest.is_empty() {
            ui_logger.log("Interactive story creation not yet supported in TUI mode.".to_string());
            ui_logger.log(
                "Use format: create s name|header|body or create s name|header|body|channel"
                    .to_string(),
            );
            return None;
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

                if let Err(e) = create_new_story_with_channel(name, header, body, channel).await {
                    error_logger.log_error(&format!("Failed to create story: {}", e));
                } else {
                    ui_logger.log(format!(
                        "Story created successfully in channel '{}'",
                        channel
                    ));
                    return Some(()); // Signal that stories need to be refreshed
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
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_story(id, story_sender).await {
                    error_logger
                        .log_error(&format!("Failed to publish story with id {}: {}", id, e));
                } else {
                    ui_logger.log(format!("Published story with id: {}", id));
                }
            }
            Err(e) => ui_logger.log(format!("invalid id: {}, {}", rest.trim(), e)),
        };
    }
}

pub async fn handle_show_story(cmd: &str, ui_logger: &UILogger) {
    if let Some(rest) = cmd.strip_prefix("show story ") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                // Read local stories to find the story with the given ID
                match read_local_stories().await {
                    Ok(stories) => {
                        if let Some(story) = stories.iter().find(|s| s.id == id) {
                            ui_logger.log(format!("📖 Story {}: {}", story.id, story.name));
                            ui_logger.log(format!("Header: {}", story.header));
                            ui_logger.log(format!("Body: {}", story.body));
                            ui_logger.log(format!(
                                "Public: {}",
                                if story.public { "Yes" } else { "No" }
                            ));
                        } else {
                            ui_logger.log(format!("Story with id {} not found", id));
                        }
                    }
                    Err(e) => {
                        ui_logger.log(format!("Error reading stories: {}", e));
                    }
                }
            }
            Err(e) => {
                ui_logger.log(format!("Invalid story id '{}': {}", rest.trim(), e));
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
) -> Option<()> {
    if let Some(rest) = cmd.strip_prefix("delete s ") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                match delete_local_story(id).await {
                    Ok(deleted) => {
                        if deleted {
                            ui_logger.log(format!("Story with id {} deleted successfully", id));
                            return Some(()); // Signal that stories need to be refreshed
                        } else {
                            ui_logger.log(format!("Story with id {} not found", id));
                        }
                    }
                    Err(e) => {
                        error_logger.log_error(&format!("Failed to delete story with id {}: {}", id, e));
                    }
                }
            }
            Err(e) => {
                ui_logger.log(format!("Invalid story id '{}': {}", rest.trim(), e));
            }
        }
    } else {
        ui_logger.log("Usage: delete s <id>".to_string());
    }
    None
}

pub async fn handle_help(_cmd: &str, ui_logger: &UILogger) {
    ui_logger.log("ls p to list discovered peers".to_string());
    ui_logger.log("ls c to list connected peers".to_string());
    ui_logger.log("ls s to list stories".to_string());
    ui_logger.log("ls ch to list channels".to_string());
    ui_logger.log("ls sub to list your subscriptions".to_string());
    ui_logger.log("create s name|header|body[|channel] to create story".to_string());
    ui_logger.log("create ch name|description to create channel".to_string());
    ui_logger.log("publish s to publish story".to_string());
    ui_logger.log("show story <id> to show story details".to_string());
    ui_logger.log("delete s <id> to delete a story".to_string());
    ui_logger.log("sub <channel> to subscribe to channel".to_string());
    ui_logger.log("unsub <channel> to unsubscribe from channel".to_string());
    ui_logger.log("name <alias> to set your peer name".to_string());
    ui_logger.log("msg <peer_alias> <message> to send direct message".to_string());
    ui_logger.log("quit to quit".to_string());
}

pub async fn handle_set_name(
    cmd: &str,
    local_peer_name: &mut Option<String>,
    ui_logger: &UILogger,
) -> Option<PeerName> {
    if let Some(name) = cmd.strip_prefix("name ") {
        let name = name.trim();
        if name.is_empty() {
            ui_logger.log("Name cannot be empty".to_string());
            return None;
        }

        *local_peer_name = Some(name.to_string());
        ui_logger.log(format!("Set local peer name to: {}", name));

        // Save the peer name to storage for persistence across restarts
        if let Err(e) = save_local_peer_name(name).await {
            ui_logger.log(format!("Warning: Failed to save peer name: {}", e));
        }

        // Return a PeerName message to broadcast to connected peers
        Some(PeerName::new(PEER_ID.to_string(), name.to_string()))
    } else {
        ui_logger.log("Usage: name <alias>".to_string());
        None
    }
}

/// Parse a direct message command that may contain peer names with spaces
fn parse_direct_message_command(
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

        // Check if the target peer exists and find their PeerId
        let target_peer_id = peer_names
            .iter()
            .find(|(_, name)| name == &&to_name)
            .map(|(peer_id, _)| *peer_id);

        let target_peer_id = match target_peer_id {
            Some(peer_id) => peer_id,
            None => {
                ui_logger.log(format!(
                    "Peer '{}' not found. Use 'ls p' to see available peers.",
                    to_name
                ));
                return;
            }
        };

        // Create a direct message request with validated sender identity
        // The from_peer_id is guaranteed to be our actual peer ID since we control the creation
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

        // Send the direct message using request-response protocol
        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&target_peer_id, direct_msg_request);

        ui_logger.log(format!("Direct message sent to {}: {}", to_name, message));
        info!(
            "Sent direct message to {} from {} (request_id: {:?})",
            to_name, from_name, request_id
        );
    } else {
        ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
    }
}

pub async fn handle_create_channel(
    cmd: &str,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<()> {
    if let Some(rest) = cmd.strip_prefix("create ch") {
        let rest = rest.trim();
        let elements: Vec<&str> = rest.split('|').collect();

        if elements.len() < 2 {
            ui_logger.log("Format: create ch name|description".to_string());
            return None;
        }

        let name = elements[0].trim();
        let description = elements[1].trim();

        if name.is_empty() || description.is_empty() {
            ui_logger.log("Channel name and description cannot be empty".to_string());
            return None;
        }

        let creator = match local_peer_name {
            Some(peer_name) => peer_name.clone(),
            None => PEER_ID.to_string(),
        };

        if let Err(e) = create_channel(name, description, &creator).await {
            error_logger.log_error(&format!("Failed to create channel: {}", e));
        } else {
            ui_logger.log(format!("Channel '{}' created successfully", name));
            // Auto-subscribe to the channel we created
            if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), name).await {
                error_logger.log_error(&format!(
                    "Failed to auto-subscribe to created channel: {}",
                    e
                ));
            }
            return Some(());
        }
    }
    None
}

pub async fn handle_list_channels(ui_logger: &UILogger, error_logger: &ErrorLogger) {
    match read_channels().await {
        Ok(channels) => {
            ui_logger.log("Available channels:".to_string());
            for channel in channels {
                ui_logger.log(format!("  {} - {}", channel.name, channel.description));
            }
        }
        Err(e) => error_logger.log_error(&format!("Failed to read channels: {}", e)),
    }
}

pub async fn handle_subscribe_channel(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if let Some(channel_name) = cmd.strip_prefix("sub ") {
        let channel_name = channel_name.trim();

        if channel_name.is_empty() {
            ui_logger.log("Usage: sub <channel_name>".to_string());
            return;
        }

        if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), channel_name).await {
            error_logger.log_error(&format!("Failed to subscribe to channel: {}", e));
        } else {
            ui_logger.log(format!("Subscribed to channel '{}'", channel_name));
        }
    } else {
        ui_logger.log("Usage: sub <channel_name>".to_string());
    }
}

pub async fn handle_unsubscribe_channel(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    if let Some(channel_name) = cmd.strip_prefix("unsub ") {
        let channel_name = channel_name.trim();

        if channel_name.is_empty() {
            ui_logger.log("Usage: unsub <channel_name>".to_string());
            return;
        }

        if let Err(e) = unsubscribe_from_channel(&PEER_ID.to_string(), channel_name).await {
            error_logger.log_error(&format!("Failed to unsubscribe from channel: {}", e));
        } else {
            ui_logger.log(format!("Unsubscribed from channel '{}'", channel_name));
        }
    } else {
        ui_logger.log("Usage: unsub <channel_name>".to_string());
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
                    ui_logger.log(format!("  {}", channel));
                }
            }
        }
        Err(e) => error_logger.log_error(&format!("Failed to read subscriptions: {}", e)),
    }
}

pub async fn establish_direct_connection(
    swarm: &mut Swarm<StoryBehaviour>,
    addr_str: &str,
    ui_logger: &UILogger,
) {
    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            ui_logger.log(format!("Manually dialing address: {}", addr));
            match swarm.dial(addr) {
                Ok(_) => {
                    ui_logger.log("Dialing initiated successfully".to_string());

                    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!("Number of connected peers: {}", connected_peers.len());

                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let connected_peers_after: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!(
                        "Number of connected peers after 2 seconds: {}",
                        connected_peers_after.len()
                    );
                    for peer in connected_peers {
                        info!("Connected to peer: {}", peer);

                        info!("Adding peer to floodsub: {}", peer);
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                Err(e) => ui_logger.log(format!("Failed to dial: {}", e)),
            }
        }
        Err(e) => ui_logger.log(format!("Failed to parse address: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_logger::ErrorLogger;
    use crate::types::Story;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_handle_set_name_valid() {
        let mut local_peer_name = None;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Test setting a valid name
        let result = handle_set_name("name Alice", &mut local_peer_name, &ui_logger).await;

        assert!(result.is_some());
        assert_eq!(local_peer_name, Some("Alice".to_string()));

        let peer_name = result.unwrap();
        assert_eq!(peer_name.name, "Alice");
        assert_eq!(peer_name.peer_id, PEER_ID.to_string());
    }

    #[tokio::test]
    async fn test_handle_set_name_empty() {
        let mut local_peer_name = None;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Test setting an empty name
        let result = handle_set_name("name ", &mut local_peer_name, &ui_logger).await;

        assert!(result.is_none());
        assert_eq!(local_peer_name, None);
    }

    #[tokio::test]
    async fn test_handle_set_name_invalid_format() {
        let mut local_peer_name = None;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Test invalid command format
        let result = handle_set_name("invalid command", &mut local_peer_name, &ui_logger).await;

        assert!(result.is_none());
        assert_eq!(local_peer_name, None);
    }

    #[tokio::test]
    async fn test_handle_set_name_with_spaces() {
        let mut local_peer_name = None;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Test name with spaces
        let result = handle_set_name("name Alice Smith", &mut local_peer_name, &ui_logger).await;

        assert!(result.is_some());
        assert_eq!(local_peer_name, Some("Alice Smith".to_string()));
    }

    #[tokio::test]
    async fn test_handle_help() {
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // This function just prints help text, we'll test it doesn't panic
        handle_help("help", &ui_logger).await;

        // Verify help messages are sent to the logger
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }

        assert!(!messages.is_empty());
        assert!(messages.iter().any(|m| m.contains("ls p")));
        assert!(messages.iter().any(|m| m.contains("create s")));
        assert!(messages.iter().any(|m| m.contains("show story")));
        assert!(messages.iter().any(|m| m.contains("quit")));
    }

    #[tokio::test]
    async fn test_handle_show_story() {
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        // Test invalid command format
        handle_show_story("show story", &ui_logger).await;
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Usage: show story <id>"))
        );

        // Test invalid story ID
        handle_show_story("show story abc", &ui_logger).await;
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }
        assert!(messages.iter().any(|m| m.contains("Invalid story id")));

        // Test non-existent story ID
        handle_show_story("show story 999", &ui_logger).await;
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }
        assert!(
            messages
                .iter()
                .any(|m| m.contains("not found") || m.contains("Error reading stories"))
        );
    }

    #[test]
    fn test_handle_create_stories_valid() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);
        let error_logger = ErrorLogger::new("test_errors.log");

        // Note: This will try to create actual files, but we're testing the parsing logic
        rt.block_on(async {
            // Test valid create story command format
            handle_create_stories(
                "create sTest Story|Test Header|Test Body",
                &ui_logger,
                &error_logger,
            )
            .await;
            // The function will try to create a story but may fail due to file system issues
            // We're mainly testing that the parsing doesn't panic
        });
    }

    #[test]
    fn test_handle_create_stories_invalid() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);
        let error_logger = ErrorLogger::new("test_errors.log");

        rt.block_on(async {
            // Test invalid format (too few arguments)
            handle_create_stories("create sTest|Header", &ui_logger, &error_logger).await;

            // Test completely invalid format
            handle_create_stories("invalid command", &ui_logger, &error_logger).await;
        });
    }

    #[test]
    fn test_handle_publish_story_valid_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);
        let error_logger = ErrorLogger::new("test_errors.log");

        rt.block_on(async {
            let (story_sender, _story_receiver) = mpsc::unbounded_channel::<Story>();

            // Test with valid ID format
            handle_publish_story("publish s123", story_sender, &ui_logger, &error_logger).await;
            // The function will try to publish but may fail due to file system issues
            // We're testing the parsing logic
        });
    }

    #[test]
    fn test_handle_publish_story_invalid_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);
        let error_logger = ErrorLogger::new("test_errors.log");

        rt.block_on(async {
            let (story_sender, _story_receiver) = mpsc::unbounded_channel::<Story>();

            // Test with invalid ID format
            handle_publish_story("publish sabc", story_sender, &ui_logger, &error_logger).await;

            // Test with invalid command format - use a new sender
            let (story_sender2, _story_receiver2) = mpsc::unbounded_channel::<Story>();
            handle_publish_story("invalid command", story_sender2, &ui_logger, &error_logger).await;
        });
    }

    #[test]
    fn test_command_parsing_edge_cases() {
        // Test various edge cases in command parsing

        // Test commands with extra whitespace
        assert_eq!("ls s all".strip_prefix("ls s "), Some("all"));
        assert_eq!("create s".strip_prefix("create s"), Some(""));
        assert_eq!("name   Alice   ".strip_prefix("name "), Some("  Alice   "));

        // Test commands that don't match expected prefixes
        assert_eq!("invalid".strip_prefix("ls s "), None);
        assert_eq!("list stories".strip_prefix("ls s "), None);
    }

    #[test]
    fn test_multiaddr_parsing() {
        // Test address parsing logic used in establish_direct_connection
        let valid_addr = "/ip4/127.0.0.1/tcp/8080";
        let parsed = valid_addr.parse::<libp2p::Multiaddr>();
        assert!(parsed.is_ok());

        let invalid_addr = "not-a-valid-address";
        let parsed_invalid = invalid_addr.parse::<libp2p::Multiaddr>();
        assert!(parsed_invalid.is_err());
    }

    #[test]
    fn test_direct_message_command_parsing() {
        // Test parsing of direct message commands
        let valid_cmd = "msg Alice Hello there!";
        assert_eq!(valid_cmd.strip_prefix("msg "), Some("Alice Hello there!"));

        let parts: Vec<&str> = "Alice Hello there!".splitn(2, ' ').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "Alice");
        assert_eq!(parts[1], "Hello there!");

        // Test edge cases
        let _no_message = "msg Alice";
        let parts: Vec<&str> = "Alice".splitn(2, ' ').collect();
        assert_eq!(parts.len(), 1);

        let no_space = "msgAlice";
        assert_eq!(no_space.strip_prefix("msg "), None);
    }

    #[test]
    fn test_parse_direct_message_command() {
        use libp2p::PeerId;
        use std::collections::HashMap;

        // Create a mock peer names map
        let mut peer_names = HashMap::new();
        let peer_id1 = PeerId::random();
        let peer_id2 = PeerId::random();
        let peer_id3 = PeerId::random();

        peer_names.insert(peer_id1, "Alice".to_string());
        peer_names.insert(peer_id2, "Alice Smith".to_string());
        peer_names.insert(peer_id3, "Bob Jones Jr".to_string());

        // Create a cache and update it with peer names
        let mut cache = SortedPeerNamesCache::new();
        cache.update(&peer_names);

        // Test simple name without spaces
        let result = parse_direct_message_command("Alice Hello there!", cache.get_sorted_names());
        assert_eq!(
            result,
            Some(("Alice".to_string(), "Hello there!".to_string()))
        );

        // Test name with spaces
        let result =
            parse_direct_message_command("Alice Smith Hello world", cache.get_sorted_names());
        assert_eq!(
            result,
            Some(("Alice Smith".to_string(), "Hello world".to_string()))
        );

        // Test name with multiple spaces
        let result =
            parse_direct_message_command("Bob Jones Jr How are you?", cache.get_sorted_names());
        assert_eq!(
            result,
            Some(("Bob Jones Jr".to_string(), "How are you?".to_string()))
        );

        // Test edge case - no message
        let result = parse_direct_message_command("Alice Smith", cache.get_sorted_names());
        assert_eq!(result, None);

        // Test edge case - no space after name
        let result = parse_direct_message_command("Alice SmithHello", cache.get_sorted_names());
        assert_eq!(result, None);

        // Test fallback to original parsing for simple names not in known peers
        let result = parse_direct_message_command("Charlie Hello there", cache.get_sorted_names());
        assert_eq!(
            result,
            Some(("Charlie".to_string(), "Hello there".to_string()))
        );
    }

    #[tokio::test]
    async fn test_handle_direct_message_no_local_name() {
        use crate::network::create_swarm;
        use std::collections::HashMap;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let peer_names = HashMap::new();
        let local_peer_name = None;
        let mut cache = SortedPeerNamesCache::new();
        cache.update(&peer_names);

        // This should print an error message about needing to set name first
        handle_direct_message(
            "msg Alice Hello",
            &mut swarm,
            &peer_names,
            &local_peer_name,
            &cache,
            &ui_logger,
        )
        .await;
        // Test passes if it doesn't panic
    }

    #[tokio::test]
    async fn test_handle_direct_message_invalid_format() {
        use crate::network::create_swarm;
        use std::collections::HashMap;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let peer_names = HashMap::new();
        let local_peer_name = Some("Bob".to_string());
        let mut cache = SortedPeerNamesCache::new();
        cache.update(&peer_names);

        // Test invalid command formats
        handle_direct_message(
            "msg Alice",
            &mut swarm,
            &peer_names,
            &local_peer_name,
            &cache,
            &ui_logger,
        )
        .await;
        handle_direct_message(
            "msg",
            &mut swarm,
            &peer_names,
            &local_peer_name,
            &cache,
            &ui_logger,
        )
        .await;
        handle_direct_message(
            "invalid command",
            &mut swarm,
            &peer_names,
            &local_peer_name,
            &cache,
            &ui_logger,
        )
        .await;
        // Test passes if it doesn't panic
    }

    #[tokio::test]
    async fn test_handle_direct_message_with_spaces_in_names() {
        use crate::network::create_swarm;
        use libp2p::PeerId;
        use std::collections::HashMap;
        let (sender, _receiver) = mpsc::unbounded_channel::<String>();
        let ui_logger = UILogger::new(sender);

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let mut peer_names = HashMap::new();
        let peer_id = PeerId::random();
        peer_names.insert(peer_id, "Alice Smith".to_string());

        let local_peer_name = Some("Bob".to_string());
        let mut cache = SortedPeerNamesCache::new();
        cache.update(&peer_names);

        // Test message to peer with spaces in name
        handle_direct_message(
            "msg Alice Smith Hello world",
            &mut swarm,
            &peer_names,
            &local_peer_name,
            &cache,
            &ui_logger,
        )
        .await;
        // Test passes if it doesn't panic and correctly parses the name
    }

    #[test]
    fn test_sorted_peer_names_cache() {
        use libp2p::PeerId;
        use std::collections::HashMap;

        let mut cache = SortedPeerNamesCache::new();
        assert!(cache.get_sorted_names().is_empty());

        // Create test peer names
        let mut peer_names = HashMap::new();
        let peer_id1 = PeerId::random();
        let peer_id2 = PeerId::random();
        let peer_id3 = PeerId::random();

        peer_names.insert(peer_id1, "Alice".to_string());
        peer_names.insert(peer_id2, "Alice Smith".to_string());
        peer_names.insert(peer_id3, "Bob".to_string());

        // Update cache
        cache.update(&peer_names);

        // Verify names are sorted by length (descending)
        let sorted_names = cache.get_sorted_names();
        assert_eq!(sorted_names.len(), 3);
        assert_eq!(sorted_names[0], "Alice Smith"); // Longest first
        assert_eq!(sorted_names[1], "Alice");
        assert_eq!(sorted_names[2], "Bob");

        // Test that parsing still works correctly with the sorted cache
        let result = parse_direct_message_command("Alice Smith Hello world", sorted_names);
        assert_eq!(
            result,
            Some(("Alice Smith".to_string(), "Hello world".to_string()))
        );

        // Test that longer names are preferred (should match "Alice Smith", not "Alice")
        let result = parse_direct_message_command("Alice Smith test", sorted_names);
        assert_eq!(
            result,
            Some(("Alice Smith".to_string(), "test".to_string()))
        );
    }
}
