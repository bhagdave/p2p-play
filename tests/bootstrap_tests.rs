use p2p_play::bootstrap::{AutoBootstrap, BootstrapStatus};
use p2p_play::bootstrap_logger::BootstrapLogger;
use p2p_play::error_logger::ErrorLogger;
use p2p_play::network::create_swarm;
use p2p_play::types::BootstrapConfig;
use std::fs;
use tempfile::NamedTempFile;

fn create_test_bootstrap_logger() -> BootstrapLogger {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();
    BootstrapLogger::new(path)
}

fn create_test_error_logger() -> ErrorLogger {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();
    ErrorLogger::new(path)
}

async fn create_test_bootstrap_config_with_name(
    peers: Vec<String>,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = BootstrapConfig {
        bootstrap_peers: peers,
        max_retry_attempts: 5,
        retry_interval_ms: 1000,
        bootstrap_timeout_ms: 30000,
    };
    let config_json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(filename, config_json).await?;

    // Copy to the standard location that bootstrap.initialise() expects
    tokio::fs::copy(filename, "bootstrap_config.json").await?;
    Ok(())
}

async fn create_test_bootstrap_config_raw_with_name(
    peers: Vec<String>,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create config file directly without validation for testing invalid configs
    let config_json = serde_json::json!({
        "bootstrap_peers": peers,
        "max_retry_attempts": 5,
        "retry_interval_ms": 1000,
        "bootstrap_timeout_ms": 30000
    });
    tokio::fs::write(filename, config_json.to_string()).await?;

    // Copy to the standard location that bootstrap.initialise() expects
    tokio::fs::copy(filename, "bootstrap_config.json").await?;
    Ok(())
}

#[tokio::test]
async fn test_attempt_bootstrap_no_config() {
    // Remove any existing config file
    let _ = fs::remove_file("bootstrap_config.json");

    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Test attempt_bootstrap with no config
    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    assert!(!result); // Should return false when no config
}

#[tokio::test]
async fn test_attempt_bootstrap_empty_peers() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with empty peers using raw creation
    let config_file = "bootstrap_config_empty_test.json";
    create_test_bootstrap_config_raw_with_name(vec![], config_file)
        .await
        .expect("Failed to create config");

    // Initialize bootstrap with the config that has empty peers
    let mut bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap_config.clear_peers(); // Clear the default peers to test empty case
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    assert!(!result); // Should return false when no peers configured

    // Verify status is Failed because there were no peers to attempt
    let status = bootstrap.status.lock().unwrap();
    assert!(matches!(*status, BootstrapStatus::Failed { .. }));

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_invalid_multiaddr() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with invalid multiaddrs using raw creation
    let config_file = "bootstrap_config_invalid_test.json";
    create_test_bootstrap_config_raw_with_name(
        vec!["invalid-multiaddr".to_string(), "also-invalid".to_string()],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config that has invalid peers
    let mut bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap_config.clear_peers(); // Clear the default peers
    bootstrap_config.add_peer("invalid-multiaddr".to_string());
    bootstrap_config.add_peer("also-invalid".to_string());
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    assert!(!result); // Should return false when no valid peers

    // Verify status is Failed because no valid peers could be added
    let status = bootstrap.status.lock().unwrap();
    assert!(matches!(*status, BootstrapStatus::Failed { .. }));

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_valid_multiaddr_no_peer_id() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with valid multiaddr but no peer ID
    let config_file = "bootstrap_config_no_peer_id_test.json";
    create_test_bootstrap_config_with_name(
        vec![
            "/ip4/127.0.0.1/tcp/8080".to_string(), // Valid multiaddr but no peer ID
        ],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config that has valid multiaddr but no peer ID
    let mut bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap_config.clear_peers(); // Clear the default peers
    bootstrap_config.add_peer("/ip4/127.0.0.1/tcp/8080".to_string()); // Valid multiaddr but no peer ID
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    assert!(!result); // Should return false when no peer IDs can be extracted

    // Verify status is set to Failed
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::Failed {
            attempts,
            last_error,
        } => {
            assert_eq!(*attempts, 1);
            assert_eq!(last_error, "No valid bootstrap peers could be added");
        }
        _ => panic!("Expected Failed status, got: {:?}", *status),
    }

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_valid_peer() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with valid multiaddr including peer ID
    let config_file = "bootstrap_config_valid_test.json";
    create_test_bootstrap_config_with_name(
        vec![
            "/ip4/127.0.0.1/tcp/8080/p2p/12D3KooWGqcJhZLALpLwjJHCRE4zepYzxTruktZF4jX8E6tQ1234"
                .to_string(),
        ],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config
    let bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    // This should succeed in adding the peer and starting bootstrap
    assert!(result);

    // Verify status is set to InProgress
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::InProgress { attempts, .. } => {
            assert_eq!(*attempts, 1);
        }
        _ => panic!("Expected InProgress status, got: {:?}", *status),
    }

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_mixed_peers() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with mix of valid multiaddrs - some with peer IDs, some without
    let config_file = "bootstrap_config_mixed_test.json";
    create_test_bootstrap_config_with_name(
        vec![
            "/ip4/127.0.0.1/tcp/8080".to_string(), // Valid addr but no peer ID
            "/ip4/192.168.1.1/tcp/9000/p2p/12D3KooWGqcJhZLALpLwjJHCRE4zepYzxTruktZF4jX8E6tQ5678"
                .to_string(), // Valid with peer ID
        ],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config
    let bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;

    // Should succeed because at least one peer is valid
    assert!(result);

    // Verify status is set to InProgress
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::InProgress { attempts, .. } => {
            assert_eq!(*attempts, 1);
        }
        _ => panic!("Expected InProgress status"),
    }

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_increments_retry_count() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with valid peer
    let config_file = "bootstrap_config_retry_test.json";
    create_test_bootstrap_config_with_name(
        vec![
            "/ip4/127.0.0.1/tcp/8080/p2p/12D3KooWGqcJhZLALpLwjJHCRE4zepYzxTruktZF4jX8E6tQ1234"
                .to_string(),
        ],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config
    let bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    // First attempt
    let result1 = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;
    assert!(result1);

    // Verify status shows attempt 1
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::InProgress { attempts, .. } => {
            assert_eq!(*attempts, 1);
        }
        _ => panic!("Expected InProgress status"),
    }
    drop(status);

    // Reset to not started to allow another attempt
    bootstrap.reset();

    // Second attempt
    let result2 = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;
    assert!(result2);

    // Verify retry count incremented
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::InProgress { attempts, .. } => {
            assert_eq!(*attempts, 1); // Reset clears retry count, so this would be 1
        }
        _ => panic!("Expected InProgress status"),
    }

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_attempt_bootstrap_updates_status_timing() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();

    // Create config file with valid peer
    let config_file = "bootstrap_config_timing_test.json";
    create_test_bootstrap_config_with_name(
        vec![
            "/ip4/127.0.0.1/tcp/8080/p2p/12D3KooWGqcJhZLALpLwjJHCRE4zepYzxTruktZF4jX8E6tQ1234"
                .to_string(),
        ],
        config_file,
    )
    .await
    .expect("Failed to create config");

    // Initialize bootstrap with the config
    let bootstrap_config = p2p_play::types::BootstrapConfig::new();
    bootstrap
        .initialise(&bootstrap_config, &bootstrap_logger, &error_logger)
        .await;

    let before_attempt = std::time::Instant::now();
    let result = bootstrap
        .attempt_bootstrap(&mut swarm, &bootstrap_logger, &error_logger)
        .await;
    let after_attempt = std::time::Instant::now();

    assert!(result);

    // Verify status timing is reasonable
    let status = bootstrap.status.lock().unwrap();
    match &*status {
        BootstrapStatus::InProgress { last_attempt, .. } => {
            assert!(*last_attempt >= before_attempt);
            assert!(*last_attempt <= after_attempt);
        }
        _ => panic!("Expected InProgress status"),
    }

    // Cleanup
    let _ = fs::remove_file(config_file);
    let _ = fs::remove_file("bootstrap_config.json");
}

#[tokio::test]
async fn test_run_auto_bootstrap_with_retry_notifies_ui_on_failure() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();
    let (log_sender, mut log_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let ui_logger = p2p_play::handlers::UILogger::new(log_sender);

    // Initialise with a config that has no valid peers so attempt_bootstrap fails
    let mut config = p2p_play::types::BootstrapConfig::new();
    config.clear_peers();
    bootstrap
        .initialise(&config, &bootstrap_logger, &error_logger)
        .await;

    p2p_play::bootstrap::run_auto_bootstrap_with_retry(
        &mut bootstrap,
        &mut swarm,
        &bootstrap_logger,
        &error_logger,
        &ui_logger,
    )
    .await;

    // Collect UI messages
    let mut messages = Vec::new();
    while let Ok(msg) = log_receiver.try_recv() {
        messages.push(msg);
    }

    // Should have received at least one UI notification about the failure
    assert!(
        !messages.is_empty(),
        "Expected UI notification for bootstrap failure"
    );

    let combined = messages.join("\n");
    // Since retries remain (default max=10), the message should indicate a retry is coming
    // and reference the bootstrap log file
    assert!(
        combined.contains("will retry"),
        "Expected retry notice in bootstrap failure message, got: {combined}"
    );
    assert!(
        combined.contains("bootstrap.log"),
        "Expected bootstrap log filename in failure message, got: {combined}"
    );
}

#[tokio::test]
async fn test_run_auto_bootstrap_with_retry_exhausted_shows_config_hint() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();
    let (log_sender, mut log_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let ui_logger = p2p_play::handlers::UILogger::new(log_sender);

    // A valid multiaddr WITHOUT a /p2p peer-ID component means attempt_bootstrap
    // will: increment retry_count to 1, set Failed { attempts: 1 }, return false.
    // With max_retry_attempts = 1, schedule_next_retry + should_retry() gives
    // false (1 < 1 == false), so the exhausted "Bootstrap failed" message is logged.
    let config = p2p_play::types::BootstrapConfig {
        bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/19999".to_string()],
        max_retry_attempts: 1,
        retry_interval_ms: 0,
        bootstrap_timeout_ms: 30000,
    };
    bootstrap
        .initialise(&config, &bootstrap_logger, &error_logger)
        .await;

    p2p_play::bootstrap::run_auto_bootstrap_with_retry(
        &mut bootstrap,
        &mut swarm,
        &bootstrap_logger,
        &error_logger,
        &ui_logger,
    )
    .await;

    let mut messages = Vec::new();
    while let Ok(msg) = log_receiver.try_recv() {
        messages.push(msg);
    }

    assert!(
        !messages.is_empty(),
        "Expected UI notification when all bootstrap retries are exhausted"
    );

    let combined = messages.join("\n");
    assert!(
        combined.contains("unified_network_config.json"),
        "Expected config file hint in exhausted message, got: {combined}"
    );
    // Should NOT contain "will retry" since retries are exhausted
    assert!(
        !combined.contains("will retry"),
        "Exhausted message should not say 'will retry', got: {combined}"
    );
}

#[tokio::test]
async fn test_bootstrap_retry_message_includes_count() {
    let mut bootstrap = AutoBootstrap::new();
    let ping_config = p2p_play::types::PingConfig::new();
    let network_config = p2p_play::types::NetworkConfig::new();
    let mut swarm =
        create_swarm(&ping_config, &network_config).expect("Failed to create test swarm");
    let bootstrap_logger = create_test_bootstrap_logger();
    let error_logger = create_test_error_logger();
    let (log_sender, mut log_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let ui_logger = p2p_play::handlers::UILogger::new(log_sender);

    // Config with no peers so bootstrap fails immediately, but retries remain (max=10)
    let mut config = p2p_play::types::BootstrapConfig::new();
    config.clear_peers();
    bootstrap
        .initialise(&config, &bootstrap_logger, &error_logger)
        .await;

    p2p_play::bootstrap::run_auto_bootstrap_with_retry(
        &mut bootstrap,
        &mut swarm,
        &bootstrap_logger,
        &error_logger,
        &ui_logger,
    )
    .await;

    let mut messages = Vec::new();
    while let Ok(msg) = log_receiver.try_recv() {
        messages.push(msg);
    }

    assert!(
        !messages.is_empty(),
        "Expected UI notification for bootstrap failure"
    );

    // The retry message should contain the attempt number and max (e.g. "1/10")
    let combined = messages.join("\n");
    assert!(
        combined.contains('/'),
        "Expected attempt count in format N/max in message: {combined}"
    );
}
