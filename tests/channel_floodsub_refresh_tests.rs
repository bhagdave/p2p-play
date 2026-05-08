/// Integration tests verifying that floodsub Channel / PublishedChannel messages
/// cause `handle_floodsub_event` to return `ActionResult::RefreshChannels`, so the
/// TUI is refreshed whenever a new channel is discovered from the network.
use libp2p::PeerId;
use libp2p::floodsub::{FloodsubMessage, Topic};
use p2p_play::error_logger::ErrorLogger;
use p2p_play::event_handlers::handle_floodsub_event;
use p2p_play::handlers::{SortedPeerNamesCache, UILogger};
use p2p_play::network::PEER_ID;
use p2p_play::storage::{clear_database_for_testing, read_subscribed_channels};
use p2p_play::types::{ActionResult, Channel, PublishedChannel};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::mpsc;

// Serialize database-touching tests within this file to avoid TEST_DATABASE_PATH races.
static TEST_DB_MUTEX: Mutex<()> = Mutex::new(());

async fn setup_test_db() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_channel_floodsub_refresh.db");
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }
    clear_database_for_testing()
        .await
        .expect("Failed to reset test database");
    temp_dir
}

/// Build a verified-peers map that contains `source` so the handler does not
/// drop the floodsub message as coming from an unverified peer.
fn make_verified_peers(source: PeerId) -> Arc<Mutex<HashMap<PeerId, String>>> {
    let map = Arc::new(Mutex::new(HashMap::new()));
    map.lock()
        .unwrap()
        .insert(source, "test-sender".to_string());
    map
}

/// Drive `handle_floodsub_event` with a minimal environment and return its result.
async fn call_handler(
    source: PeerId,
    data: Vec<u8>,
    verified_peers: &Arc<Mutex<HashMap<PeerId, String>>>,
) -> Option<ActionResult> {
    let floodsub_event = libp2p::floodsub::Event::Message(FloodsubMessage {
        source,
        data: data.into(),
        sequence_number: vec![],
        topics: vec![Topic::new("stories")],
    });

    let (response_sender, _) = mpsc::unbounded_channel();
    let (ui_sender, _) = mpsc::unbounded_channel();
    let ui_logger = UILogger::new(ui_sender);
    let error_logger = ErrorLogger::new("/tmp/test_channel_floodsub_refresh_errors.log");
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut sorted_cache = SortedPeerNamesCache::new();
    let mut relay_service = None;

    handle_floodsub_event(
        floodsub_event,
        response_sender,
        &mut peer_names,
        &None,
        &mut sorted_cache,
        &ui_logger,
        &error_logger,
        &mut relay_service,
        verified_peers,
    )
    .await
}

// ── Channel (raw) path ────────────────────────────────────────────────────────

/// A new `Channel` message received from a verified peer must return
/// `RefreshChannels` so the TUI reloads the channel list.
#[tokio::test]
async fn test_floodsub_channel_message_returns_refresh_channels() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    let channel = Channel::new(
        "floodsub-channel".to_string(),
        "A channel received over floodsub".to_string(),
        source.to_string(),
    );
    let data = serde_json::to_vec(&channel).unwrap();

    let result = call_handler(source, data, &verified_peers).await;
    assert_eq!(result, Some(ActionResult::RefreshChannels));
}

/// Receiving the same `Channel` twice (triggering a UNIQUE constraint) should
/// still return `RefreshChannels` — the channel already exists, but the TUI
/// must still be notified to remain consistent.
#[tokio::test]
async fn test_floodsub_duplicate_channel_still_returns_refresh_channels() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    let channel = Channel::new(
        "dup-channel".to_string(),
        "A duplicate channel".to_string(),
        source.to_string(),
    );
    let channel_data = serde_json::to_vec(&channel).unwrap();

    // First arrival — channel is new.
    let first_result = call_handler(source, channel_data.clone(), &verified_peers).await;
    assert_eq!(first_result, Some(ActionResult::RefreshChannels));

    // Second arrival — UNIQUE constraint fires, but must still refresh.
    let second_result = call_handler(source, channel_data, &verified_peers).await;
    assert_eq!(second_result, Some(ActionResult::RefreshChannels));
}

/// A `Channel` message with an empty name is invalid and must be silently
/// dropped — no `RefreshChannels` action should be emitted.
#[tokio::test]
async fn test_floodsub_invalid_channel_empty_name_is_ignored() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    let channel = Channel {
        name: "".to_string(),
        description: "Some description".to_string(),
        created_by: source.to_string(),
        created_at: 0,
    };
    let data = serde_json::to_vec(&channel).unwrap();

    let result = call_handler(source, data, &verified_peers).await;
    assert_eq!(result, None, "Empty channel name must be ignored");
}

// ── PublishedChannel path ─────────────────────────────────────────────────────

/// A `PublishedChannel` from a remote (non-local) peer must return
/// `RefreshChannels` so the TUI picks up the newly discovered channel.
#[tokio::test]
async fn test_floodsub_published_channel_returns_refresh_channels() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    // The source peer must differ from the local PEER_ID so the handler does
    // not treat the message as its own echo.
    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    let published_channel = PublishedChannel {
        channel: Channel::new(
            "published-channel".to_string(),
            "Channel received via PublishedChannel".to_string(),
            source.to_string(),
        ),
        publisher: source.to_string(),
    };
    let data = serde_json::to_vec(&published_channel).unwrap();

    let result = call_handler(source, data, &verified_peers).await;
    assert_eq!(result, Some(ActionResult::RefreshChannels));
}

/// A `PublishedChannel` whose `publisher` field matches the local `PEER_ID`
/// must be filtered out (own echo), and `None` should be returned.
#[tokio::test]
async fn test_floodsub_own_published_channel_echo_is_ignored() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    // Set the publisher to the local node's ID — this simulates receiving back
    // a channel that this node published itself.
    let local_peer_id_str = PEER_ID.to_string();
    let published_channel = PublishedChannel {
        channel: Channel::new(
            "own-channel".to_string(),
            "Should be filtered as own echo".to_string(),
            local_peer_id_str.clone(),
        ),
        publisher: local_peer_id_str,
    };
    let data = serde_json::to_vec(&published_channel).unwrap();

    let result = call_handler(source, data, &verified_peers).await;
    assert_eq!(
        result, None,
        "Own-echo PublishedChannel must be silently dropped"
    );
}

// ── Auto-subscribe path ───────────────────────────────────────────────────────

/// When auto-subscribe is enabled in the network config, receiving a
/// `PublishedChannel` should subscribe the local peer to that channel AND
/// still return `RefreshChannels`.
#[tokio::test]
async fn test_floodsub_published_channel_auto_subscribe_enabled() {
    use p2p_play::storage::save_unified_network_config;
    use p2p_play::types::{ChannelAutoSubscriptionConfig, UnifiedNetworkConfig};

    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    // Persist a config that enables auto-subscription.
    let mut config = UnifiedNetworkConfig::new();
    config.channel_auto_subscription = ChannelAutoSubscriptionConfig {
        auto_subscribe_to_new_channels: true,
        notify_new_channels: true,
        max_auto_subscriptions: 10,
    };
    save_unified_network_config(&config)
        .await
        .expect("Failed to save unified network config");

    let source = PeerId::random();
    let verified_peers = make_verified_peers(source);

    let published_channel = PublishedChannel {
        channel: Channel::new(
            "auto-sub-channel".to_string(),
            "Channel that triggers auto-subscription".to_string(),
            source.to_string(),
        ),
        publisher: source.to_string(),
    };
    let data = serde_json::to_vec(&published_channel).unwrap();

    let result = call_handler(source, data, &verified_peers).await;

    // TUI refresh must always be triggered.
    assert_eq!(result, Some(ActionResult::RefreshChannels));

    // With auto-subscribe enabled the local peer should now be subscribed to
    // the newly received channel.
    let local_peer_id = PEER_ID.to_string();
    let subscriptions = read_subscribed_channels(&local_peer_id)
        .await
        .expect("Failed to read subscriptions");
    assert!(
        subscriptions.contains(&"auto-sub-channel".to_string()),
        "Local peer should be auto-subscribed to the received channel; got: {subscriptions:?}"
    );
}
