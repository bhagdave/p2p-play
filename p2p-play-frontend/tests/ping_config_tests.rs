use p2p_play::types::PingConfig;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_ping_config_defaults() {
    let config = PingConfig::new();
    assert_eq!(config.interval_secs, 30);
    assert_eq!(config.timeout_secs, 20);
}

#[tokio::test]
async fn test_ping_config_validation() {
    // Valid config
    let valid_config = PingConfig {
        interval_secs: 30,
        timeout_secs: 20,
    };
    assert!(valid_config.validate().is_ok());

    // Invalid: interval is 0
    let invalid_config1 = PingConfig {
        interval_secs: 0,
        timeout_secs: 20,
    };
    assert!(invalid_config1.validate().is_err());

    // Invalid: timeout is 0
    let invalid_config2 = PingConfig {
        interval_secs: 30,
        timeout_secs: 0,
    };
    assert!(invalid_config2.validate().is_err());

    // Invalid: timeout >= interval
    let invalid_config3 = PingConfig {
        interval_secs: 20,
        timeout_secs: 25,
    };
    assert!(invalid_config3.validate().is_err());

    let invalid_config4 = PingConfig {
        interval_secs: 20,
        timeout_secs: 20,
    };
    assert!(invalid_config4.validate().is_err());
}

#[tokio::test]
async fn test_ping_config_load_from_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_ping_config.json");

    // Test loading valid config
    let test_config = r#"{
        "interval_secs": 45,
        "timeout_secs": 15
    }"#;
    fs::write(&config_path, test_config).unwrap();

    let config = PingConfig::load_from_file(config_path.to_str().unwrap()).unwrap();
    assert_eq!(config.interval_secs, 45);
    assert_eq!(config.timeout_secs, 15);
}

#[tokio::test]
async fn test_ping_config_load_from_nonexistent_file() {
    // Should return default config when file doesn't exist
    let config = PingConfig::load_from_file("nonexistent_file.json").unwrap();
    assert_eq!(config.interval_secs, 30);
    assert_eq!(config.timeout_secs, 20);
}

#[tokio::test]
async fn test_ping_config_load_invalid_json() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("invalid_ping_config.json");

    // Test loading invalid JSON
    let invalid_json = r#"{
        "interval_secs": 45,
        "timeout_secs": 
    }"#;
    fs::write(&config_path, invalid_json).unwrap();

    let result = PingConfig::load_from_file(config_path.to_str().unwrap());
    assert!(result.is_err());
}

#[tokio::test]
async fn test_ping_config_load_invalid_values() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("invalid_values_ping_config.json");

    // Test loading config with invalid values
    let invalid_values = r#"{
        "interval_secs": 10,
        "timeout_secs": 15
    }"#;
    fs::write(&config_path, invalid_values).unwrap();

    let result = PingConfig::load_from_file(config_path.to_str().unwrap());
    assert!(result.is_err());
}

#[tokio::test]
async fn test_ping_config_duration_conversion() {
    let config = PingConfig {
        interval_secs: 45,
        timeout_secs: 15,
    };

    assert_eq!(config.interval_duration().as_secs(), 45);
    assert_eq!(config.timeout_duration().as_secs(), 15);
}

#[tokio::test]
async fn test_ping_config_default_trait() {
    let config = PingConfig::default();
    assert_eq!(config.interval_secs, 30);
    assert_eq!(config.timeout_secs, 20);
}
