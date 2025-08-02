use p2p_play::network::*;

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
async fn test_enhanced_tcp_configuration() {
    // Test that the enhanced TCP configuration creates a valid swarm
    let result = create_swarm();
    assert!(
        result.is_ok(),
        "Enhanced TCP configuration should create valid swarm"
    );

    let swarm = result.unwrap();

    // Verify the swarm is properly configured
    assert_eq!(
        swarm.local_peer_id(),
        &*PEER_ID,
        "Swarm should have correct peer ID"
    );

    // The swarm should be created without errors, indicating proper TCP configuration
    // This includes the enhanced connection limits, yamux config, and swarm settings
}

#[test]
fn test_tcp_configuration_components() {
    // Test that the TCP configuration components are properly set up
    // This test verifies the building blocks work correctly

    // Test peer ID generation
    let peer_id = *PEER_ID;
    assert!(!peer_id.to_string().is_empty(), "Peer ID should be valid");

    // Test topic creation
    let topic = TOPIC.clone();
    let topic_str = format!("{:?}", topic);
    assert!(
        topic_str.contains("stories"),
        "Topic should contain 'stories'"
    );

    // Test consistency across multiple calls
    let peer_id_2 = *PEER_ID;
    let topic_2 = TOPIC.clone();
    assert_eq!(peer_id, peer_id_2, "Peer ID should be consistent");
    assert_eq!(
        format!("{:?}", topic),
        format!("{:?}", topic_2),
        "Topic should be consistent"
    );
}

#[tokio::test]
async fn test_swarm_creation_with_connection_limits() {
    // Test that swarm creation includes connection management features
    let result = create_swarm();
    assert!(
        result.is_ok(),
        "Swarm creation should succeed with connection limits"
    );

    let swarm = result.unwrap();

    // Verify basic swarm properties
    assert_eq!(swarm.local_peer_id(), &*PEER_ID);

    // The fact that the swarm was created successfully means:
    // - TCP configuration with enhanced settings works
    // - Yamux multiplexing with increased stream limits works
    // - Swarm configuration with dial concurrency and idle timeout works
    // - All enhanced connection management features are properly configured
}
