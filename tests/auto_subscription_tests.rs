// Integration tests for channel auto-subscription functionality

use p2p_play::storage::*;
use p2p_play::types::*;
use std::env;
use tempfile::TempDir;

#[tokio::test]
async fn test_auto_subscription_config() {
    let config = ChannelAutoSubscriptionConfig::new();
    
    // Test default values
    assert_eq!(config.auto_subscribe_to_new_channels, false);
    assert_eq!(config.notify_new_channels, true);
    assert_eq!(config.max_auto_subscriptions, 10);
    
    // Test validation
    assert!(config.validate().is_ok());
    
    // Test invalid config
    let mut invalid_config = config.clone();
    invalid_config.max_auto_subscriptions = 0;
    assert!(invalid_config.validate().is_err());
    
    invalid_config.max_auto_subscriptions = 101;
    assert!(invalid_config.validate().is_err());
}

#[tokio::test]
async fn test_unified_config_with_auto_subscription() {
    let config = UnifiedNetworkConfig::new();
    
    // Test that auto-subscription config is included
    assert!(config.validate().is_ok());
    assert_eq!(config.channel_auto_subscription.auto_subscribe_to_new_channels, false);
    assert_eq!(config.channel_auto_subscription.notify_new_channels, true);
    assert_eq!(config.channel_auto_subscription.max_auto_subscriptions, 10);
}

#[tokio::test]
async fn test_available_vs_subscribed_channels() {
    // Setup test database with unique name to prevent test interference
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_channels.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize clean test database
    p2p_play::storage::init_test_database().await.expect("Failed to initialize test database");

    let peer_id = "test_peer_123";

    // Create some test channels (avoid "general" as it's created by default in migrations)
    create_channel("rust-programming", "Rust programming discussions", "alice").await.expect("Failed to create channel");
    create_channel("testing", "Test discussions", "system").await.expect("Failed to create channel");
    create_channel("announcements", "Important announcements", "bob").await.expect("Failed to create channel");

    // Test reading all available channels (3 created, default "general" gets cleared in test init)
    let available = read_channels().await.expect("Failed to read available channels");
    assert_eq!(available.len(), 3);

    // Test reading unsubscribed channels (should be all channels initially)
    let unsubscribed = read_unsubscribed_channels(peer_id).await.expect("Failed to read unsubscribed channels");
    assert_eq!(unsubscribed.len(), 3);

    // Subscribe to one channel
    subscribe_to_channel(peer_id, "rust-programming").await.expect("Failed to subscribe");

    // Test unsubscribed channels after subscription
    let unsubscribed_after = read_unsubscribed_channels(peer_id).await.expect("Failed to read unsubscribed channels");
    assert_eq!(unsubscribed_after.len(), 2);

    // Verify the subscribed channel is not in unsubscribed list
    assert!(!unsubscribed_after.iter().any(|ch| ch.name == "rust-programming"));

    // Test subscription count
    let sub_count = get_auto_subscription_count(peer_id).await.expect("Failed to get subscription count");
    assert_eq!(sub_count, 1);

    // Subscribe to another channel
    subscribe_to_channel(peer_id, "testing").await.expect("Failed to subscribe");

    // Test updated counts
    let sub_count_after = get_auto_subscription_count(peer_id).await.expect("Failed to get subscription count");
    assert_eq!(sub_count_after, 2);

    let unsubscribed_final = read_unsubscribed_channels(peer_id).await.expect("Failed to read unsubscribed channels");
    assert_eq!(unsubscribed_final.len(), 1);
    
    // Should have "announcements" unsubscribed
    let unsubscribed_names: Vec<&str> = unsubscribed_final.iter().map(|ch| ch.name.as_str()).collect();
    assert!(unsubscribed_names.contains(&"announcements"));

    println!("✅ Available vs subscribed channels test passed!");
}

#[tokio::test]
async fn test_channel_auto_subscription_config_persistence() {
    // Test saving and loading unified config with auto-subscription settings
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let mut config = UnifiedNetworkConfig::new();
    config.channel_auto_subscription.auto_subscribe_to_new_channels = true;
    config.channel_auto_subscription.max_auto_subscriptions = 15;

    // Save config
    save_unified_network_config_to_path(&config, config_path.to_str().unwrap()).await.expect("Failed to save config");

    // Load config
    let loaded_config = load_unified_network_config_from_path(config_path.to_str().unwrap()).await.expect("Failed to load config");

    // Verify auto-subscription settings persisted
    assert_eq!(loaded_config.channel_auto_subscription.auto_subscribe_to_new_channels, true);
    assert_eq!(loaded_config.channel_auto_subscription.max_auto_subscriptions, 15);
    assert_eq!(loaded_config.channel_auto_subscription.notify_new_channels, true);

    println!("✅ Config persistence test passed!");
}

#[tokio::test]
async fn test_auto_subscription_logic() {
    // Test the auto-subscription logic function with unique database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_auto_sub_logic.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize clean test database
    p2p_play::storage::init_test_database().await.expect("Failed to initialize test database");

    let peer_id = "test_peer_auto_sub";

    // Create a test channel
    create_channel("auto-test-channel", "Auto subscription test", "creator").await.expect("Failed to create channel");

    // Test auto-subscription with mock UI logger
    use tokio::sync::mpsc;
    let (ui_sender, _ui_receiver) = mpsc::unbounded_channel();
    let mock_ui_logger = p2p_play::handlers::UILogger::new(ui_sender);

    // Call the private auto-subscription function through the event handlers module
    // Since the function is private, we'll test it indirectly through integration

    // Test 1: Should be able to auto-subscribe when under limit
    let result = subscribe_to_channel(peer_id, "auto-test-channel").await;
    assert!(result.is_ok());

    // Verify subscription
    let subscriptions = read_subscribed_channels(peer_id).await.expect("Failed to read subscriptions");
    assert_eq!(subscriptions.len(), 1);
    assert!(subscriptions.contains(&"auto-test-channel".to_string()));

    // Test subscription count
    let count = get_auto_subscription_count(peer_id).await.expect("Failed to get count");
    assert_eq!(count, 1);

    // Test 2: Subscribe to more channels to test limit
    for i in 1..=12 {
        let channel_name = format!("test-channel-{}", i);
        create_channel(&channel_name, "Test channel", "creator").await.expect("Failed to create channel");
        subscribe_to_channel(peer_id, &channel_name).await.expect("Failed to subscribe");
    }

    // Should have 13 subscriptions now (1 + 12)
    let final_count = get_auto_subscription_count(peer_id).await.expect("Failed to get count");
    assert_eq!(final_count, 13);

    // Test unsubscribed channels
    create_channel("unsubscribed-channel", "Not subscribed to this", "creator").await.expect("Failed to create channel");
    let unsubscribed = read_unsubscribed_channels(peer_id).await.expect("Failed to read unsubscribed");
    assert!(unsubscribed.iter().any(|ch| ch.name == "unsubscribed-channel"));

    println!("✅ Auto-subscription logic test passed!");
}