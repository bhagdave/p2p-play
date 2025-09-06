use p2p_core::{NetworkConfig, PingConfig};

#[tokio::test]
async fn test_network_config_creation() {
    let config = NetworkConfig::default();
    assert_eq!(config.request_timeout_seconds, 30);
    assert_eq!(config.max_concurrent_streams, 128);
    assert_eq!(config.max_connections_per_peer, 5);
    assert_eq!(config.max_established_total, 100);
    assert_eq!(config.max_pending_outgoing, 8);
}

#[tokio::test]
async fn test_network_config_validation_success() {
    let config = NetworkConfig::default();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_network_config_validation_invalid_timeout() {
    let config = NetworkConfig {
        request_timeout_seconds: 0, // Invalid: must be > 0
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Request timeout must be greater than 0"));
}

#[tokio::test]
async fn test_network_config_validation_invalid_streams() {
    let config = NetworkConfig {
        max_concurrent_streams: 0, // Invalid: must be > 0
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Max concurrent streams must be greater than 0"));
}

#[tokio::test]
async fn test_network_config_validation_invalid_connections() {
    let config = NetworkConfig {
        max_connections_per_peer: 0, // Invalid: must be > 0
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Max connections per peer must be greater than 0"));
}

#[tokio::test]
async fn test_network_config_validation_invalid_total() {
    let config = NetworkConfig {
        max_established_total: 0, // Invalid: must be > 0
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Max established total must be greater than 0"));
}

#[tokio::test]
async fn test_ping_config_creation() {
    let config = PingConfig::default();
    assert_eq!(config.interval_secs, 30);
    assert_eq!(config.timeout_secs, 10);
}

#[tokio::test]
async fn test_ping_config_validation_success() {
    let config = PingConfig::default();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_ping_config_validation_invalid_interval() {
    let config = PingConfig {
        interval_secs: 0, // Invalid: must be > 0
        timeout_secs: 10,
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Ping interval must be greater than 0"));
}

#[tokio::test]
async fn test_ping_config_validation_invalid_timeout() {
    let config = PingConfig {
        interval_secs: 30,
        timeout_secs: 0, // Invalid: must be > 0
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Ping timeout must be greater than 0"));
}

#[tokio::test]
async fn test_ping_config_validation_timeout_too_long() {
    let config = PingConfig {
        interval_secs: 30,
        timeout_secs: 35, // Invalid: must be < interval
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Ping timeout must be less than interval"));
}

#[tokio::test]
async fn test_ping_config_duration_conversion() {
    let config = PingConfig {
        interval_secs: 60,
        timeout_secs: 30,
    };

    let interval_duration = config.interval_duration();
    let timeout_duration = config.timeout_duration();

    assert_eq!(interval_duration.as_secs(), 60);
    assert_eq!(timeout_duration.as_secs(), 30);
}