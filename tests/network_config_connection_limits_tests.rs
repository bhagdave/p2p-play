use p2p_play::types::NetworkConfig;
use tempfile::tempdir;
use std::fs;

#[tokio::test]
async fn test_network_config_connection_limits_defaults() {
    let config = NetworkConfig::new();
    
    assert_eq!(config.max_connections_per_peer, 1);
    assert_eq!(config.max_pending_incoming, 10);
    assert_eq!(config.max_pending_outgoing, 10);
    assert_eq!(config.max_established_total, 100);
    assert_eq!(config.connection_establishment_timeout_seconds, 30);
}

#[tokio::test]
async fn test_network_config_validation_connection_limits() {
    let mut config = NetworkConfig::new();
    
    // Test valid configuration
    assert!(config.validate().is_ok());
    
    // Test invalid max_connections_per_peer (zero)
    config.max_connections_per_peer = 0;
    assert!(config.validate().is_err());
    
    // Test invalid max_connections_per_peer (too high)
    config.max_connections_per_peer = 11;
    assert!(config.validate().is_err());
    
    // Reset to valid value
    config.max_connections_per_peer = 1;
    assert!(config.validate().is_ok());
    
    // Test invalid max_pending_incoming (zero)
    config.max_pending_incoming = 0;
    assert!(config.validate().is_err());
    
    // Reset to valid value
    config.max_pending_incoming = 10;
    assert!(config.validate().is_ok());
    
    // Test invalid max_pending_outgoing (zero)
    config.max_pending_outgoing = 0;
    assert!(config.validate().is_err());
    
    // Reset to valid value
    config.max_pending_outgoing = 10;
    assert!(config.validate().is_ok());
    
    // Test invalid max_established_total (zero)
    config.max_established_total = 0;
    assert!(config.validate().is_err());
    
    // Reset to valid value
    config.max_established_total = 100;
    assert!(config.validate().is_ok());
    
    // Test invalid connection_establishment_timeout_seconds (too low)
    config.connection_establishment_timeout_seconds = 4;
    assert!(config.validate().is_err());
    
    // Test invalid connection_establishment_timeout_seconds (too high)
    config.connection_establishment_timeout_seconds = 301;
    assert!(config.validate().is_err());
    
    // Reset to valid value
    config.connection_establishment_timeout_seconds = 30;
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_network_config_load_and_save_with_connection_limits() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_network_config.json");
    let config_path_str = config_path.to_str().unwrap();
    
    // Create a config with custom connection limits
    let original_config = NetworkConfig {
        connection_maintenance_interval_seconds: 300,
        request_timeout_seconds: 60,
        max_concurrent_streams: 100,
        max_connections_per_peer: 2,
        max_pending_incoming: 15,
        max_pending_outgoing: 20,
        max_established_total: 150,
        connection_establishment_timeout_seconds: 45,
    };
    
    // Save the config
    original_config.save_to_file(config_path_str).expect("Failed to save config");
    
    // Load the config back
    let loaded_config = NetworkConfig::load_from_file(config_path_str).expect("Failed to load config");
    
    // Verify all connection limit fields are preserved
    assert_eq!(loaded_config.max_connections_per_peer, 2);
    assert_eq!(loaded_config.max_pending_incoming, 15);
    assert_eq!(loaded_config.max_pending_outgoing, 20);
    assert_eq!(loaded_config.max_established_total, 150);
    assert_eq!(loaded_config.connection_establishment_timeout_seconds, 45);
    
    // Verify other fields too
    assert_eq!(loaded_config.connection_maintenance_interval_seconds, 300);
    assert_eq!(loaded_config.request_timeout_seconds, 60);
    assert_eq!(loaded_config.max_concurrent_streams, 100);
}

#[tokio::test]
async fn test_network_config_backwards_compatibility() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("legacy_config.json");
    let config_path_str = config_path.to_str().unwrap();
    
    // Write a legacy config file without the new connection limit fields
    let legacy_config = r#"{
        "connection_maintenance_interval_seconds": 300,
        "request_timeout_seconds": 60,
        "max_concurrent_streams": 100
    }"#;
    
    fs::write(config_path_str, legacy_config).expect("Failed to write legacy config");
    
    // Try to load - should fail gracefully and create a new config with defaults
    let result = NetworkConfig::load_from_file(config_path_str);
    
    // The load should either succeed with defaults or create a new file with defaults
    // This tests backward compatibility
    assert!(result.is_ok());
    let config = result.unwrap();
    
    // Should have default connection limit values
    assert_eq!(config.max_connections_per_peer, 1);
    assert_eq!(config.max_pending_incoming, 10);
    assert_eq!(config.max_pending_outgoing, 10);
    assert_eq!(config.max_established_total, 100);
    assert_eq!(config.connection_establishment_timeout_seconds, 30);
}

#[tokio::test] 
async fn test_network_config_serialization_format() {
    let config = NetworkConfig::new();
    let json = serde_json::to_string_pretty(&config).expect("Failed to serialize config");
    
    // Verify that all connection limit fields are present in the JSON
    assert!(json.contains("max_connections_per_peer"));
    assert!(json.contains("max_pending_incoming"));
    assert!(json.contains("max_pending_outgoing"));
    assert!(json.contains("max_established_total"));
    assert!(json.contains("connection_establishment_timeout_seconds"));
    
    // Verify the JSON can be deserialized back
    let deserialized: NetworkConfig = serde_json::from_str(&json).expect("Failed to deserialize config");
    assert_eq!(deserialized.max_connections_per_peer, config.max_connections_per_peer);
    assert_eq!(deserialized.max_pending_incoming, config.max_pending_incoming);
    assert_eq!(deserialized.max_pending_outgoing, config.max_pending_outgoing);
    assert_eq!(deserialized.max_established_total, config.max_established_total);
    assert_eq!(deserialized.connection_establishment_timeout_seconds, config.connection_establishment_timeout_seconds);
}