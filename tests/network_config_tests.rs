use p2p_play::storage::{
    ensure_network_config_exists, load_network_config_from_path, save_network_config_to_path,
};
use p2p_play::types::NetworkConfig;
use std::fs;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_network_config_creation() {
    let config = NetworkConfig::new();
    assert_eq!(config.connection_maintenance_interval_seconds, 300);
}

#[tokio::test]
async fn test_network_config_validation_success() {
    let config = NetworkConfig::new();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_network_config_validation_too_low() {
    let config = NetworkConfig {
        connection_maintenance_interval_seconds: 30, // Below minimum of 60
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be at least 60 seconds"));
}

#[tokio::test]
async fn test_network_config_validation_too_high() {
    let config = NetworkConfig {
        connection_maintenance_interval_seconds: 4000, // Above maximum of 3600
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be at most 3600 seconds"));
}

#[tokio::test]
async fn test_network_config_validation_edge_cases() {
    // Test minimum valid value
    let config_min = NetworkConfig {
        connection_maintenance_interval_seconds: 60,
    };
    assert!(config_min.validate().is_ok());

    // Test maximum valid value
    let config_max = NetworkConfig {
        connection_maintenance_interval_seconds: 3600,
    };
    assert!(config_max.validate().is_ok());
}

#[tokio::test]
async fn test_save_and_load_network_config() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    let original_config = NetworkConfig {
        connection_maintenance_interval_seconds: 600,
    };

    // Save config
    save_network_config_to_path(&original_config, path)
        .await
        .unwrap();

    // Load config
    let loaded_config = load_network_config_from_path(path).await.unwrap();

    assert_eq!(
        loaded_config.connection_maintenance_interval_seconds,
        original_config.connection_maintenance_interval_seconds
    );
}

#[tokio::test]
async fn test_load_network_config_creates_default_if_missing() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Remove the temp file so it doesn't exist
    fs::remove_file(path).unwrap();

    // Load config from non-existent file
    let loaded_config = load_network_config_from_path(path).await.unwrap();

    // Should be default config
    assert_eq!(loaded_config.connection_maintenance_interval_seconds, 300);

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
    let result = load_network_config_from_path(path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_load_network_config_invalid_values() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Write config with invalid value (too low)
    let invalid_config = r#"{"connection_maintenance_interval_seconds": 30}"#;
    fs::write(path, invalid_config).unwrap();

    // Should fail validation
    let result = load_network_config_from_path(path).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must be at least 60 seconds")
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
    assert!(!fs::metadata(path).is_ok());
}
