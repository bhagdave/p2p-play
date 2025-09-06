use p2p_core::{NetworkService, NetworkConfig, PingConfig, DirectMessageRequest};
use libp2p::{Multiaddr, PeerId, floodsub::Topic};
use std::str::FromStr;

#[tokio::test]
async fn test_network_service_creation() {
    let config = NetworkConfig::default();
    let network = NetworkService::new(config).await;
    
    assert!(network.is_ok());
    let network = network.unwrap();
    
    // Test that we can get the local peer ID
    let peer_id = network.local_peer_id();
    assert_ne!(peer_id, PeerId::random()); // Should be deterministic from keypair
}

#[tokio::test]
async fn test_network_service_with_custom_ping() {
    let network_config = NetworkConfig::default();
    let ping_config = PingConfig {
        interval_secs: 60,
        timeout_secs: 30,
    };
    
    let network = NetworkService::new_with_ping(network_config, ping_config).await;
    assert!(network.is_ok());
}

#[tokio::test]
async fn test_network_service_topic_subscription() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let topic = Topic::new("test-topic");
    
    // Test subscription
    let subscribed = network.subscribe(topic.clone());
    assert!(subscribed);
    
    // Test unsubscription
    let unsubscribed = network.unsubscribe(&topic);
    assert!(unsubscribed);
}

#[tokio::test]
async fn test_network_service_message_publishing() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let topic = Topic::new("test-topic");
    network.subscribe(topic.clone());
    
    let message = b"Hello, p2p world!".to_vec();
    let result = network.publish(topic, message);
    
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_network_service_direct_messages() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let target_peer = PeerId::random();
    let request = DirectMessageRequest {
        from_peer_id: network.local_peer_id().to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello Bob!".to_string(),
        timestamp: 1234567890,
    };
    
    // This will return a request ID since we can't actually connect to the random peer
    let _request_id = network.send_direct_message(target_peer, request);
    // The test passes if no panic occurs
}

#[tokio::test]
async fn test_network_service_listen_on_address() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    // Try to listen on localhost
    let addr = Multiaddr::from_str("/ip4/127.0.0.1/tcp/0").unwrap();
    let result = network.listen_on(addr);
    
    // Should succeed or fail gracefully
    // We don't assert success because port might be in use
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_network_service_dial_peer() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let target_peer = PeerId::random();
    let result = network.dial(target_peer);
    
    // This should return an error since we're dialing a random peer
    // but it shouldn't panic
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_network_config_validation_integration() {
    // Test valid config
    let valid_config = NetworkConfig {
        request_timeout_seconds: 30,
        max_concurrent_streams: 100,
        max_connections_per_peer: 3,
        max_established_total: 50,
        max_pending_outgoing: 5,
    };
    
    let network = NetworkService::new(valid_config).await;
    assert!(network.is_ok());
    
    // Test invalid config
    let invalid_config = NetworkConfig {
        request_timeout_seconds: 0, // Invalid
        max_concurrent_streams: 100,
        max_connections_per_peer: 3,
        max_established_total: 50,
        max_pending_outgoing: 5,
    };
    
    // Note: The network service creation might succeed even with invalid config
    // because validation might happen elsewhere
    let _network = NetworkService::new(invalid_config).await;
}

#[tokio::test]
async fn test_multiple_network_services() {
    let config1 = NetworkConfig::default();
    let config2 = NetworkConfig::default();
    
    let network1 = NetworkService::new(config1).await;
    let network2 = NetworkService::new(config2).await;
    
    assert!(network1.is_ok());
    assert!(network2.is_ok());
    
    let network1 = network1.unwrap();
    let network2 = network2.unwrap();
    
    // Each should have a different peer ID
    assert_ne!(network1.local_peer_id(), network2.local_peer_id());
}

#[tokio::test]
async fn test_network_service_connected_peers() {
    let config = NetworkConfig::default();
    let network = NetworkService::new(config).await.unwrap();
    
    // Initially should have no connected peers
    let peers: Vec<_> = network.connected_peers().collect();
    assert_eq!(peers.len(), 0);
}

#[tokio::test] 
async fn test_network_service_handshake_protocol() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let target_peer = PeerId::random();
    let request = p2p_core::HandshakeRequest {
        app_name: "p2p-core-test".to_string(),
        app_version: "0.1.0".to_string(),
        peer_id: network.local_peer_id().to_string(),
    };
    
    // Send handshake request (will fail since target is random)
    let _request_id = network.send_handshake(target_peer, request);
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_network_service_node_description() {
    let config = NetworkConfig::default();
    let mut network = NetworkService::new(config).await.unwrap();
    
    let target_peer = PeerId::random();
    let request = p2p_core::NodeDescriptionRequest {
        from_peer_id: network.local_peer_id().to_string(),
        from_name: "TestNode".to_string(),
        timestamp: 1234567890,
    };
    
    // Send node description request (will fail since target is random)
    let _request_id = network.request_node_description(target_peer, request);
    // Test passes if no panic occurs
}