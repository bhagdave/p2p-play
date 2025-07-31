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
