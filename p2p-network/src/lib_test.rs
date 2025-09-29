//! Basic test to verify p2p-network compiles and works

use p2p_network::{NetworkConfig, P2PNetwork};

#[tokio::test]
async fn test_network_creation() {
    let config = NetworkConfig::default();
    let result = P2PNetwork::new(config).await;
    assert!(result.is_ok());
}

#[test]
fn test_network_config_default() {
    let config = NetworkConfig::default();
    assert!(config.encryption_enabled);
    assert!(!config.listen_addresses.is_empty());
}