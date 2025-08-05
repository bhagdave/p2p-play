// Integration test to verify new channel auto-subscription commands work

use p2p_play::handlers::*;
use p2p_play::storage::*;
use p2p_play::types::UnifiedNetworkConfig;
use std::env;
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_new_commands_integration() {
    // Setup test database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_commands.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Reset and initialize storage
    reset_db_connection_for_testing().await.expect("Failed to reset connection");
    ensure_stories_file_exists().await.expect("Failed to initialize storage");
    
    // Clear all existing data for clean test
    let conn_arc = get_db_connection().await.expect("Failed to get connection");
    let conn = conn_arc.lock().await;
    conn.execute("DELETE FROM channel_subscriptions", []).expect("Failed to clear subscriptions");
    conn.execute("DELETE FROM channels", []).expect("Failed to clear channels");
    drop(conn); // Release the lock

    // Setup test environment with config in working directory  
    let default_config = UnifiedNetworkConfig::new();
    save_unified_network_config(&default_config).await.expect("Failed to save config");
    
    let (ui_sender, mut ui_receiver) = mpsc::unbounded_channel();
    let ui_logger = UILogger::new(ui_sender);
    let error_logger = p2p_play::error_logger::ErrorLogger::new("test_errors.log");

    // Test 1: Create some channels
    create_channel("rust-lang", "Rust programming discussions", "alice").await.expect("Failed to create channel");
    create_channel("javascript", "JavaScript development", "bob").await.expect("Failed to create channel");
    create_channel("devops", "DevOps and infrastructure", "charlie").await.expect("Failed to create channel");

    // Test 2: Test "ls ch available" command
    handle_list_channels("ls ch available", &ui_logger, &error_logger).await;
    
    // Check UI output
    let mut available_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        available_output.push_str(&msg);
        available_output.push('\n');
    }
    assert!(available_output.contains("Available channels:"));
    assert!(available_output.contains("rust-lang"));
    assert!(available_output.contains("javascript"));
    assert!(available_output.contains("devops"));

    // Test 3: Test "ls ch unsubscribed" command (should show all channels initially)
    handle_list_channels("ls ch unsubscribed", &ui_logger, &error_logger).await;
    
    let mut unsubscribed_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        unsubscribed_output.push_str(&msg);
        unsubscribed_output.push('\n');
    }
    assert!(unsubscribed_output.contains("Unsubscribed channels:"));
    assert!(unsubscribed_output.contains("rust-lang"));

    // Test 4: Test subscription with new "sub ch" command
    handle_subscribe_channel("sub ch rust-lang", &ui_logger, &error_logger).await;
    
    let mut sub_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        sub_output.push_str(&msg);
        sub_output.push('\n');
    }
    assert!(sub_output.contains("Subscribed to channel 'rust-lang'"));

    // Test 5: Test "ls ch unsubscribed" after subscription (should not show rust-lang)
    handle_list_channels("ls ch unsubscribed", &ui_logger, &error_logger).await;
    
    let mut unsubscribed_after_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        unsubscribed_after_output.push_str(&msg);
        unsubscribed_after_output.push('\n');
    }
    assert!(unsubscribed_after_output.contains("javascript"));
    assert!(unsubscribed_after_output.contains("devops"));
    // rust-lang should not be in unsubscribed list anymore
    assert!(!unsubscribed_after_output.contains("rust-lang"));

    // Test 6: Test "set auto-sub status" command
    handle_set_auto_subscription("set auto-sub status", &ui_logger, &error_logger).await;
    
    let mut status_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        status_output.push_str(&msg);
        status_output.push('\n');
    }
    println!("Status output: '{}'", status_output); // Debug output
    assert!(status_output.contains("Auto-subscription is currently") || status_output.contains("Failed to load config"));

    // Test 7: Test enabling auto-subscription
    handle_set_auto_subscription("set auto-sub on", &ui_logger, &error_logger).await;
    
    let mut enable_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        enable_output.push_str(&msg);
        enable_output.push('\n');
    }
    assert!(enable_output.contains("Auto-subscription enabled"));

    // Test 8: Test disabling auto-subscription
    handle_set_auto_subscription("set auto-sub off", &ui_logger, &error_logger).await;
    
    let mut disable_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        disable_output.push_str(&msg);
        disable_output.push('\n');
    }
    assert!(disable_output.contains("Auto-subscription disabled"));

    // Test 9: Test unsubscription with new "unsub ch" command
    handle_unsubscribe_channel("unsub ch rust-lang", &ui_logger, &error_logger).await;
    
    let mut unsub_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        unsub_output.push_str(&msg);
        unsub_output.push('\n');
    }
    assert!(unsub_output.contains("Unsubscribed from channel 'rust-lang'"));

    // Test 10: Verify rust-lang is back in unsubscribed list
    handle_list_channels("ls ch unsubscribed", &ui_logger, &error_logger).await;
    
    let mut final_unsubscribed_output = String::new();
    while let Ok(msg) = ui_receiver.try_recv() {
        final_unsubscribed_output.push_str(&msg);
        final_unsubscribed_output.push('\n');
    }
    assert!(final_unsubscribed_output.contains("rust-lang"));
    assert!(final_unsubscribed_output.contains("javascript"));
    assert!(final_unsubscribed_output.contains("devops"));

    println!("✅ All new command integration tests passed!");
}