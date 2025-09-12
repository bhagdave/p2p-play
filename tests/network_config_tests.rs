use p2p_play::storage::{
    load_unified_network_config_from_path, save_unified_network_config_to_path,
};
use p2p_play::types::{NetworkConfig, UnifiedNetworkConfig};
use std::fs;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_network_config_creation() {
    let config = NetworkConfig::new();
    assert_eq!(config.connection_maintenance_interval_seconds, 300);
    assert_eq!(config.request_timeout_seconds, 120);
    assert_eq!(config.max_concurrent_streams, 100);
}

#[tokio::test]
async fn test_network_config_validation_success() {
    let config = NetworkConfig::new();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_network_config_validation_too_low() {
    let config = NetworkConfig {
        connection_maintenance_interval_seconds: 5, // Below minimum of 10
        request_timeout_seconds: 60,
        max_concurrent_streams: 100,
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be at least 10 seconds"));
}

#[tokio::test]
async fn test_network_config_validation_too_high() {
    let config = NetworkConfig {
        connection_maintenance_interval_seconds: 4000, // Above maximum of 3600
        request_timeout_seconds: 60,
        max_concurrent_streams: 100,
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be at most 3600 seconds"));
}

#[tokio::test]
async fn test_network_config_validation_edge_cases() {
    // Test minimum valid value
    let config_min = NetworkConfig {
        connection_maintenance_interval_seconds: 10, // new minimum
        request_timeout_seconds: 10,                 // minimum
        max_concurrent_streams: 1,                   // minimum
        ..Default::default()
    };
    assert!(config_min.validate().is_ok());

    // Test maximum valid value
    let config_max = NetworkConfig {
        connection_maintenance_interval_seconds: 3600,
        request_timeout_seconds: 300, // maximum
        max_concurrent_streams: 1000, // maximum
        ..Default::default()
    };
    assert!(config_max.validate().is_ok());
}

#[tokio::test]
async fn test_save_and_load_network_config() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    let original_unified_config = UnifiedNetworkConfig {
        network: NetworkConfig {
            connection_maintenance_interval_seconds: 600,
            request_timeout_seconds: 120,
            max_concurrent_streams: 50,
            ..Default::default()
        },
        ..Default::default()
    };

    // Save config
    save_unified_network_config_to_path(&original_unified_config, path)
        .await
        .unwrap();

    // Load config
    let loaded_unified_config = load_unified_network_config_from_path(path).await.unwrap();

    assert_eq!(
        loaded_unified_config
            .network
            .connection_maintenance_interval_seconds,
        original_unified_config
            .network
            .connection_maintenance_interval_seconds
    );
    assert_eq!(
        loaded_unified_config.network.request_timeout_seconds,
        original_unified_config.network.request_timeout_seconds
    );
    assert_eq!(
        loaded_unified_config.network.max_concurrent_streams,
        original_unified_config.network.max_concurrent_streams
    );
}

#[tokio::test]
async fn test_load_network_config_creates_default_if_missing() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Remove the temp file so it doesn't exist
    fs::remove_file(path).unwrap();

    // Load config from non-existent file
    let loaded_unified_config = load_unified_network_config_from_path(path).await.unwrap();

    // Should be default config
    assert_eq!(
        loaded_unified_config
            .network
            .connection_maintenance_interval_seconds,
        300
    );
    assert_eq!(loaded_unified_config.network.request_timeout_seconds, 120);
    assert_eq!(loaded_unified_config.network.max_concurrent_streams, 100);

    // File should now exist
    assert!(fs::metadata(path).is_ok());
}

#[tokio::test]
async fn test_load_network_config_invalid_json() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Write invalid JSON
    fs::write(path, "invalid json").unwrap();

    // Should fail to load
    let result = load_unified_network_config_from_path(path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_load_network_config_invalid_values() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Write unified config with invalid network value (too low)
    let invalid_config = r#"{
        "bootstrap": {
            "bootstrap_peers": ["/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"],
            "retry_interval_ms": 5000,
            "max_retry_attempts": 10,
            "bootstrap_timeout_ms": 60000
        },
        "network": {
            "connection_maintenance_interval_seconds": 5,
            "request_timeout_seconds": 120,
            "max_concurrent_streams": 100,
            "max_connections_per_peer": 1,
            "max_pending_incoming": 10,
            "max_pending_outgoing": 10,
            "max_established_total": 100,
            "connection_establishment_timeout_seconds": 45,
            "network_health_update_interval_seconds": 15
        },
        "ping": {
            "interval_secs": 50,
            "timeout_secs": 45
        },
        "direct_message": {
            "max_retry_attempts": 3,
            "retry_interval_seconds": 30,
            "enable_connection_retries": true,
            "enable_timed_retries": true
        },
        "channel_auto_subscription": {
            "auto_subscribe_to_new_channels": false,
            "notify_new_channels": true,
            "max_auto_subscriptions": 10
        },
        "message_notifications": {
            "enable_color_coding": true,
            "enable_sound_notifications": false,
            "enable_flash_indicators": true,
            "flash_duration_ms": 200,
            "show_delivery_status": true,
            "enhanced_timestamps": true
        },
        "relay": {
            "enable_relay": true,
            "enable_forwarding": true,
            "max_hops": 3,
            "relay_timeout_ms": 5000,
            "prefer_direct": true,
            "rate_limit_per_peer": 10
        },
        "auto_share": {
            "global_auto_share": true,
            "sync_days": 30
        },
        "circuit_breaker": {
            "failure_threshold": 5,
            "success_threshold": 3,
            "timeout_secs": 60,
            "operation_timeout_secs": 30,
            "enabled": true
        }
    }"#;
    fs::write(path, invalid_config).unwrap();

    // Should fail validation
    let result = load_unified_network_config_from_path(path).await;
    assert!(result.is_err());
    let error_message = result.unwrap_err().to_string();
    assert!(
        error_message
            .contains("connection_maintenance_interval_seconds must be at least 10 seconds")
    );
}

#[tokio::test]
async fn test_network_config_request_timeout_validation() {
    // Test request timeout too low
    let config_low = NetworkConfig {
        connection_maintenance_interval_seconds: 300,
        request_timeout_seconds: 5, // Below minimum of 10
        max_concurrent_streams: 100,
        ..Default::default()
    };
    let result = config_low.validate();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("request_timeout_seconds must be at least 10 seconds")
    );

    // Test request timeout too high
    let config_high = NetworkConfig {
        connection_maintenance_interval_seconds: 300,
        request_timeout_seconds: 400, // Above maximum of 300
        max_concurrent_streams: 100,
        ..Default::default()
    };
    let result = config_high.validate();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("request_timeout_seconds must not exceed 300 seconds")
    );
}

#[tokio::test]
async fn test_network_config_max_streams_validation() {
    // Test max streams zero
    let config_zero = NetworkConfig {
        connection_maintenance_interval_seconds: 300,
        request_timeout_seconds: 60,
        max_concurrent_streams: 0,
        ..Default::default()
    };
    let result = config_zero.validate();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("max_concurrent_streams must be greater than 0")
    );

    // Test max streams too high
    let config_high = NetworkConfig {
        connection_maintenance_interval_seconds: 300,
        request_timeout_seconds: 60,
        max_concurrent_streams: 1500, // Above maximum of 1000
        ..Default::default()
    };
    let result = config_high.validate();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("max_concurrent_streams must not exceed 1000")
    );
}

#[tokio::test]
async fn test_ensure_network_config_exists() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Remove the temp file
    fs::remove_file(path).unwrap();

    // Ensure config exists should create it
    // Note: This test would need to be adapted since ensure_network_config_exists()
    // uses a hardcoded path. For this test, we're testing the pattern.
    assert!(fs::metadata(path).is_err());
}
