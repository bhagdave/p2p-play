use p2p_play::storage::{
    create_channel, ensure_general_channel_subscription, read_subscribed_channels,
    subscribe_to_channel,
};
use std::env;
use std::sync::Mutex;
use tempfile::TempDir;

static TEST_DB_MUTEX: Mutex<()> = Mutex::new(());

async fn setup_test_db() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_subscriptions.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }
    p2p_play::storage::init_test_database()
        .await
        .expect("Failed to initialize test database");

    // init_test_database wipes all rows including channels; re-create "general"
    // so the foreign key constraint on channel_subscriptions is satisfied.
    create_channel("general", "General discussion", "system")
        .await
        .expect("Failed to create general channel");

    temp_dir
}

#[tokio::test]
async fn test_ensure_general_subscribes_when_not_subscribed() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let peer_id = "peer_not_yet_subscribed";

    let result = ensure_general_channel_subscription(peer_id).await;
    assert!(result.is_ok(), "Should succeed when subscribing to general");

    let subscriptions = read_subscribed_channels(peer_id)
        .await
        .expect("Failed to read subscriptions");
    assert!(
        subscriptions.contains(&"general".to_string()),
        "Should be subscribed to general after calling ensure"
    );
}

#[tokio::test]
async fn test_ensure_general_is_idempotent() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let peer_id = "peer_already_subscribed";

    // Subscribe first
    subscribe_to_channel(peer_id, "general")
        .await
        .expect("Failed to subscribe");

    // Call again — should not error
    let result = ensure_general_channel_subscription(peer_id).await;
    assert!(result.is_ok(), "Should succeed when already subscribed");

    let subscriptions = read_subscribed_channels(peer_id)
        .await
        .expect("Failed to read subscriptions");
    let general_count = subscriptions
        .iter()
        .filter(|s| *s == "general")
        .count();
    assert_eq!(general_count, 1, "Should only have one general subscription");
}

#[tokio::test]
async fn test_ensure_general_does_not_affect_other_subscriptions() {
    let _lock = TEST_DB_MUTEX.lock().unwrap();
    let _dir = setup_test_db().await;

    let peer_id = "peer_with_other_channels";

    create_channel("rust", "Rust programming", "system")
        .await
        .expect("Failed to create rust channel");
    subscribe_to_channel(peer_id, "rust")
        .await
        .expect("Failed to subscribe to rust");

    ensure_general_channel_subscription(peer_id)
        .await
        .expect("Failed to ensure general subscription");

    let subscriptions = read_subscribed_channels(peer_id)
        .await
        .expect("Failed to read subscriptions");

    assert!(subscriptions.contains(&"general".to_string()), "general should be present");
    assert!(subscriptions.contains(&"rust".to_string()), "rust should still be present");
}
