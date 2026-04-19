//! Command handlers for the p2p-play application.
//!
//! This module is split into domain-focused submodules:
//!
//! | Submodule      | Responsibility                                      |
//! |----------------|-----------------------------------------------------|
//! | `stories`      | Story create/list/publish/show/delete/search/export |
//! | `channels`     | Channel create/list/subscribe/auto-subscription     |
//! | `messaging`    | Direct messages, peer name, relay, retry queue      |
//! | `bootstrap`    | DHT bootstrap and peer discovery                    |
//! | `config`       | Help text, config reload, auto-share, descriptions  |
//! | `wasm`         | WASM capability advertisement and execution         |
//!
//! Shared infrastructure (UILogger, PeerState, helpers) lives here in `mod.rs`
//! and is re-used by all submodules via `super::*`.

pub mod bootstrap;
pub mod channels;
pub mod config;
pub mod messaging;
pub mod stories;
pub mod wasm;

// Re-export the public API so callers that import `crate::handlers::foo` continue
// to work without change.
pub use bootstrap::{handle_dht_bootstrap, handle_dht_get_peers};
pub use channels::{
    handle_create_channel, handle_list_channels, handle_list_subscriptions,
    handle_set_auto_subscription, handle_subscribe_channel, handle_unsubscribe_channel,
};
pub use config::{
    handle_config_auto_share, handle_config_sync_days, handle_create_description,
    handle_get_description, handle_help, handle_reload_config, handle_show_description,
};
pub use messaging::{
    handle_direct_message_with_relay,
    handle_set_name,
};
// Used by integration tests (tests/handlers_tests.rs) via the library API.
#[allow(unused_imports)]
pub use messaging::parse_direct_message_command;
pub use stories::{
    handle_create_stories_with_sender, handle_delete_story, handle_export_story,
    handle_filter_stories, handle_list_stories, handle_publish_story, handle_search_stories,
    handle_show_story,
};
pub use wasm::handle_wasm_command;

// ---------------------------------------------------------------------------
// Shared infrastructure
// ---------------------------------------------------------------------------

use crate::error_logger::ErrorLogger;
use libp2p::PeerId;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Sends messages to the terminal UI log pane.
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

/// Caches peer names sorted longest-first for prefix-safe command parsing.
pub struct SortedPeerNamesCache {
    sorted_names: Vec<String>,
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

    pub fn update(&mut self, peer_names: &HashMap<PeerId, String>) {
        let mut names: Vec<String> = peer_names.values().cloned().collect();
        names.sort_by_key(|b| std::cmp::Reverse(b.len()));
        self.sorted_names = names;
        self.version += 1;
    }

    pub fn get_sorted_names(&self) -> &[String] {
        &self.sorted_names
    }
}

/// Groups peer-tracking state threaded through the event loop.
pub struct PeerState {
    pub peer_names: HashMap<PeerId, String>,
    pub local_peer_name: Option<String>,
    pub sorted_peer_names_cache: SortedPeerNamesCache,
}

impl PeerState {
    pub fn new(local_peer_name: Option<String>) -> Self {
        Self {
            peer_names: HashMap::new(),
            local_peer_name,
            sorted_peer_names_cache: SortedPeerNamesCache::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Module-internal shared helpers
// (pub(super) so submodules can call them via `super::`)
// ---------------------------------------------------------------------------

/// Validates input and logs a user-facing error if validation fails.
///
/// # Error policy
/// User input errors are shown in the UI but not written to the error log,
/// because they are expected and should not clutter the operational log.
pub(super) fn validate_and_log<T>(
    validator_result: Result<T, crate::validation::ValidationError>,
    error_type: &str,
    ui_logger: &UILogger,
) -> Option<T> {
    match validator_result {
        Ok(validated) => Some(validated),
        Err(e) => {
            ui_logger.log(format!("Invalid {error_type}: {e}"));
            None
        }
    }
}

/// Loads the unified config, applies `modifier`, and saves it.  
/// On failure, logs to both `error_logger` (operational) and `ui_logger` (user-facing).
///
/// Returns `true` on success, `false` on any error.
pub(super) async fn modify_config<F>(
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    operation_name: &str,
    modifier: F,
) -> bool
where
    F: FnOnce(&mut crate::types::UnifiedNetworkConfig),
{
    use crate::storage::{load_unified_network_config, save_unified_network_config};

    match load_unified_network_config().await {
        Ok(mut config) => {
            modifier(&mut config);
            match save_unified_network_config(&config).await {
                Ok(_) => true,
                Err(e) => {
                    error_logger.log_error(&format!("Failed to save {operation_name} config: {e}"));
                    ui_logger.log(format!(
                        "{} Failed to save {operation_name} configuration",
                        crate::types::Icons::cross()
                    ));
                    false
                }
            }
        }
        Err(e) => {
            error_logger.log_error(&format!(
                "Failed to load config for {operation_name} update: {e}"
            ));
            ui_logger.log(format!(
                "{} Failed to load configuration",
                crate::types::Icons::cross()
            ));
            false
        }
    }
}

/// Loads the bootstrap config, applies `modifier`, and saves it.
///
/// `modifier` returns `false` to indicate that no save is needed (e.g. the peer
/// already existed).  Returns `true` only when the save succeeded.
pub(super) async fn modify_bootstrap_config<F>(
    ui_logger: &UILogger,
    operation_name: &str,
    modifier: F,
) -> bool
where
    F: FnOnce(&mut crate::types::BootstrapConfig) -> bool,
{
    use crate::storage::{load_bootstrap_config, save_bootstrap_config};

    match load_bootstrap_config().await {
        Ok(mut config) => {
            if modifier(&mut config) {
                match save_bootstrap_config(&config).await {
                    Ok(_) => true,
                    Err(e) => {
                        ui_logger.log(format!(
                            "Failed to save {operation_name} bootstrap config: {e}"
                        ));
                        false
                    }
                }
            } else {
                false
            }
        }
        Err(e) => {
            ui_logger.log(format!(
                "Failed to load bootstrap config for {operation_name}: {e}"
            ));
            false
        }
    }
}

/// Loads the unified network config and returns it.
///
/// On failure, logs to both `error_logger` (operational) and `ui_logger` (user-facing) and
/// returns `None`.  Callers can use `?`-style early-return on `None` instead of repeating the
/// three-line error block every time a status sub-command needs the config.
pub(super) async fn load_config_or_log(
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    operation_context: &str,
) -> Option<crate::types::UnifiedNetworkConfig> {
    match crate::storage::load_unified_network_config().await {
        Ok(config) => Some(config),
        Err(e) => {
            error_logger
                .log_error(&format!("Failed to load config for {operation_context}: {e}"));
            ui_logger.log(format!(
                "{} Failed to load configuration",
                crate::types::Icons::cross()
            ));
            None
        }
    }
}

/// Returns the current Unix timestamp in seconds.
pub(super) fn current_unix_timestamp() -> u64 {
    crate::current_unix_timestamp()
}

/// Looks up a connected peer by display alias.  Returns `None` when not found.
pub(super) fn resolve_peer_by_alias(
    alias: &str,
    peer_names: &HashMap<PeerId, String>,
) -> Option<PeerId> {
    peer_names
        .iter()
        .find(|(_, name)| name.as_str() == alias)
        .map(|(peer_id, _)| *peer_id)
}

/// Extracts the `PeerId` from a `/p2p/<peer_id>` component of a multiaddr.
pub fn extract_peer_id_from_multiaddr(addr: &libp2p::Multiaddr) -> Option<PeerId> {
    for protocol in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer_id) = protocol {
            return Some(peer_id);
        }
    }
    None
}

/// Updates the unread-story counts displayed in the TUI.
pub async fn refresh_unread_counts_for_ui(app: &mut crate::ui::App, peer_id: &str) {
    if let Ok(unread_counts) = crate::storage::get_unread_counts_by_channel(peer_id).await {
        app.update_unread_counts(unread_counts);
    }
}

/// Dials a multiaddr and adds all already-connected peers to the floodsub view.
pub async fn establish_direct_connection(
    swarm: &mut libp2p::Swarm<crate::network::StoryBehaviour>,
    addr_str: &str,
    ui_logger: &UILogger,
) {
    establish_direct_connection_impl(swarm, addr_str, ui_logger, |s, a| s.dial(a)).await;
}

/// Inner implementation that accepts an injectable dial function for testability.
pub async fn establish_direct_connection_impl<F>(
    swarm: &mut libp2p::Swarm<crate::network::StoryBehaviour>,
    addr_str: &str,
    ui_logger: &UILogger,
    dial_fn: F,
) where
    F: FnOnce(
        &mut libp2p::Swarm<crate::network::StoryBehaviour>,
        libp2p::Multiaddr,
    ) -> Result<(), libp2p::swarm::DialError>,
{
    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            ui_logger.log(format!("Manually dialing address: {addr}"));
            match dial_fn(swarm, addr) {
                Ok(_) => {
                    ui_logger.log("Dialing initiated successfully".to_string());

                    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();

                    for peer in connected_peers {
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                Err(e) => ui_logger.log(format!(
                    "Failed to dial {addr_str}: {e} — check the peer is online and reachable"
                )),
            }
        }
        Err(e) => ui_logger.log(format!(
            "Invalid address '{addr_str}': {e} — expected a multiaddr, e.g. /ip4/HOST/tcp/PORT/p2p/PEER_ID"
        )),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_state_new_with_no_name() {
        let state = PeerState::new(None);
        assert!(state.local_peer_name.is_none());
        assert!(state.peer_names.is_empty());
    }

    #[test]
    fn test_peer_state_new_with_name() {
        let state = PeerState::new(Some("alice".to_string()));
        assert_eq!(state.local_peer_name, Some("alice".to_string()));
        assert!(state.peer_names.is_empty());
    }

    #[test]
    fn test_peer_state_fields_are_mutable() {
        let mut state = PeerState::new(None);
        let peer_id = PeerId::random();
        state.peer_names.insert(peer_id, "bob".to_string());
        state.local_peer_name = Some("alice".to_string());

        assert_eq!(state.peer_names.get(&peer_id), Some(&"bob".to_string()));
        assert_eq!(state.local_peer_name, Some("alice".to_string()));
    }

    #[test]
    fn test_sorted_peer_names_cache_sorts_longest_first() {
        let mut cache = SortedPeerNamesCache::new();
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let mut names = HashMap::new();
        names.insert(peer_a, "alice".to_string());
        names.insert(peer_b, "alice bob".to_string());
        cache.update(&names);
        let sorted = cache.get_sorted_names();
        assert_eq!(sorted[0], "alice bob");
        assert_eq!(sorted[1], "alice");
    }

    #[test]
    fn test_resolve_peer_by_alias_found() {
        let peer_id = PeerId::random();
        let mut peer_names = HashMap::new();
        peer_names.insert(peer_id, "alice".to_string());
        assert_eq!(resolve_peer_by_alias("alice", &peer_names), Some(peer_id));
    }

    #[test]
    fn test_resolve_peer_by_alias_not_found() {
        let peer_names: HashMap<PeerId, String> = HashMap::new();
        assert_eq!(resolve_peer_by_alias("unknown", &peer_names), None);
    }

    #[test]
    fn test_current_unix_timestamp_nonzero() {
        assert!(current_unix_timestamp() > 0);
    }

    #[test]
    fn test_extract_peer_id_from_multiaddr_present() {
        use std::str::FromStr;
        let addr = libp2p::Multiaddr::from_str(
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGSoqZYTRsUUJtPdR9RRKX8vZuFyFX5JqFR1p2tKQ3oJf",
        );
        if let Ok(addr) = addr {
            let result = extract_peer_id_from_multiaddr(&addr);
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_extract_peer_id_from_multiaddr_absent() {
        use std::str::FromStr;
        let addr =
            libp2p::Multiaddr::from_str("/ip4/127.0.0.1/tcp/4001").expect("valid multiaddr");
        assert!(extract_peer_id_from_multiaddr(&addr).is_none());
    }
}
