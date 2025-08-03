use std::collections::HashMap;
use libp2p::PeerId;

#[test]
fn test_peer_tracking_logic() {
    // Simulate the logic we added for tracking connected peers
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    
    // Simulate a peer connecting
    let peer_id = PeerId::random();
    
    // Before our fix: peer would not be in peer_names until they send a PeerName message
    // After our fix: peer should be added immediately with a default name
    
    // Simulate our connection established logic
    if !peer_names.contains_key(&peer_id) {
        peer_names.insert(peer_id, format!("Peer_{}", &peer_id.to_string()[..8]));
    }
    
    // Verify the peer is now tracked
    assert!(peer_names.contains_key(&peer_id));
    let name = peer_names.get(&peer_id).unwrap();
    assert!(name.starts_with("Peer_"));
    
    // Simulate receiving a PeerName message with a real name
    let real_name = "Alice".to_string();
    peer_names.entry(peer_id).and_modify(|existing_name| {
        *existing_name = real_name.clone();
    });
    
    // Verify the name was updated
    assert_eq!(peer_names.get(&peer_id).unwrap(), &real_name);
}

#[test]
fn test_ui_display_logic() {
    // Test the UI display logic for different types of peer names
    let peer_id = PeerId::random();
    let peer_id_str = peer_id.to_string();
    
    // Test default name display
    let default_name = format!("Peer_{}", &peer_id_str[..8]);
    let display_content = if default_name.starts_with("Peer_") {
        format!("{} [{}]", default_name, &peer_id_str[..8])
    } else {
        format!("{} ({})", default_name, &peer_id_str[..8])
    };
    
    assert!(display_content.contains("["));
    assert!(display_content.contains("]"));
    
    // Test real name display
    let real_name = "Alice".to_string();
    let display_content = if real_name.starts_with("Peer_") {
        format!("{} [{}]", real_name, &peer_id_str[..8])
    } else {
        format!("{} ({})", real_name, &peer_id_str[..8])
    };
    
    assert!(display_content.contains("("));
    assert!(display_content.contains(")"));
}

#[test]
fn test_peer_name_lookup() {
    // Test that we can find peers by both default and custom names
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_id = PeerId::random();
    
    // Add peer with default name
    let default_name = format!("Peer_{}", &peer_id.to_string()[..8]);
    peer_names.insert(peer_id, default_name.clone());
    
    // Test lookup by default name
    let found = peer_names
        .iter()
        .find(|(_, name)| name == &&default_name)
        .map(|(peer_id, _)| *peer_id);
    
    assert!(found.is_some());
    assert_eq!(found.unwrap(), peer_id);
    
    // Update to custom name
    let custom_name = "Bob".to_string();
    peer_names.entry(peer_id).and_modify(|existing_name| {
        *existing_name = custom_name.clone();
    });
    
    // Test lookup by custom name
    let found = peer_names
        .iter()
        .find(|(_, name)| name == &&custom_name)
        .map(|(peer_id, _)| *peer_id);
    
    assert!(found.is_some());
    assert_eq!(found.unwrap(), peer_id);
    
    // Default name should no longer work
    let not_found = peer_names
        .iter()
        .find(|(_, name)| name == &&default_name)
        .map(|(peer_id, _)| *peer_id);
    
    assert!(not_found.is_none());
}