use p2p_play::network::*;
use p2p_play::types::NetworkConfig;
use std::fs;

#[tokio::test]
async fn test_create_swarm() {
    let result = create_swarm();
    assert!(result.is_ok());

    let swarm = result.unwrap();
    assert_eq!(swarm.local_peer_id(), &*PEER_ID);
}

#[test]
fn test_peer_id_consistency() {
    let peer_id_1 = *PEER_ID;
    let peer_id_2 = *PEER_ID;
    assert_eq!(peer_id_1, peer_id_2);
}

#[test]
fn test_topic_creation() {
    let topic = TOPIC.clone();
    let topic_str = format!("{:?}", topic);
    assert!(topic_str.contains("stories"));
}

#[test]
fn test_network_constants() {
    // Test that network constants are accessible and have expected properties
    let peer_id = *PEER_ID;
    let topic = TOPIC.clone();

    // PeerID should be valid
    assert!(!peer_id.to_string().is_empty());

    // Topic should contain expected content
    let topic_str = format!("{:?}", topic);
    assert!(topic_str.len() > 0);
}

#[test]
fn test_network_configuration() {
    // Test that network configuration is consistent
    let peer_id = *PEER_ID;
    let topic = TOPIC.clone();

    // Verify peer ID is valid and consistent
    assert!(!peer_id.to_string().is_empty());
    assert_eq!(peer_id, *PEER_ID);

    // Verify topic is valid
    let topic_str = format!("{:?}", topic);
    assert!(!topic_str.is_empty());
    assert!(topic_str.contains("stories"));
}

#[tokio::test]
async fn test_network_config_integration() {
    // Test that the network config can be loaded and used in create_swarm
    let config = NetworkConfig::new();
    assert_eq!(config.request_timeout_seconds, 60);
    assert_eq!(config.max_concurrent_streams, 100);
    
    // Ensure create_swarm still works (it should use the config internally)
    let result = create_swarm();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_network_config_file_integration() {
    use tempfile::NamedTempFile;
    
    // Create a custom network config file
    let custom_config = NetworkConfig {
        request_timeout_seconds: 120,
        max_concurrent_streams: 50,
    };
    
    let temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    custom_config.save_to_file(temp_path).unwrap();
    
    // Verify the config can be loaded
    let loaded_config = NetworkConfig::load_from_file(temp_path).unwrap();
    assert_eq!(loaded_config.request_timeout_seconds, 120);
    assert_eq!(loaded_config.max_concurrent_streams, 50);
    
    // Clean up
    fs::remove_file(temp_path).ok();
}
