use libp2p::PeerId;
use std::collections::HashMap;

#[test]
fn test_peer_tracking_logic() {
    // Simulate the logic we added for tracking connected peers
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();

    // Simulate a peer connecting
    let peer_id = PeerId::random();

    // Before our fix: peer would not be in peer_names until they send a PeerName message
    // After our fix: peer should be added immediately with a default name

    // Simulate our connection established logic
    peer_names
        .entry(peer_id)
        .or_insert_with(|| format!("Peer_{peer_id}"));

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
    // Use 20 characters instead of 8 to ensure uniqueness between peers with similar prefixes
    let peer_id_display = if peer_id_str.len() >= 20 {
        &peer_id_str[..20]
    } else {
        &peer_id_str
    };

    // Test default name display
    let default_name = format!("Peer_{peer_id}");
    let display_content =
        if default_name.starts_with("Peer_") && default_name.contains(&peer_id.to_string()) {
            format!("Peer_{peer_id_display} [{peer_id_display}]")
        } else {
            format!("{default_name} ({peer_id_display})")
        };

    assert!(display_content.contains("["));
    assert!(display_content.contains("]"));

    // Test real name display
    let real_name = "Alice".to_string();
    let display_content =
        if real_name.starts_with("Peer_") && real_name.contains(&peer_id.to_string()) {
            format!("Peer_{peer_id_display} [{peer_id_display}]")
        } else {
            format!("{real_name} ({peer_id_display})")
        };

    assert!(display_content.contains("("));
    assert!(display_content.contains(")"));

    // Test custom name that starts with "Peer_" but is not a default name
    let custom_peer_name = "Peer_Alice".to_string();
    let display_content = if custom_peer_name.starts_with("Peer_")
        && custom_peer_name.contains(&peer_id.to_string())
    {
        format!("Peer_{peer_id_display} [{peer_id_display}]")
    } else {
        format!("{custom_peer_name} ({peer_id_display})")
    };

    // Should use parentheses for custom names, even if they start with "Peer_"
    assert!(display_content.contains("("));
    assert!(display_content.contains(")"));
    assert_eq!(display_content, format!("Peer_Alice ({peer_id_display})"));
}

#[test]
fn test_peer_name_lookup() {
    // Test that we can find peers by both default and custom names
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_id = PeerId::random();

    // Add peer with default name
    let default_name = format!("Peer_{peer_id}");
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

// ---------------------------------------------------------------------------
// Tests for session-lifetime peer name persistence (HashMap-based cache)
// ---------------------------------------------------------------------------

/// Helper: the same rule used by EventProcessor::is_user_set_peer_name.
/// User-set names are validated to be at most PEER_NAME_MAX (30) characters,
/// while the default placeholder "Peer_<PeerId>" is always longer.
fn is_user_set_peer_name(name: &str) -> bool {
    const PEER_NAME_MAX: usize = 30;
    name.len() <= PEER_NAME_MAX
}

/// Simulates the sync_known_peer_names logic:
///   - learns real aliases from `peer_names` into `known_peer_names`, and
///   - restores known aliases for peers that still have a placeholder name.
fn sync_known_peer_names(
    peer_names: &mut HashMap<PeerId, String>,
    known_peer_names: &mut HashMap<PeerId, String>,
) {
    // Pass 1 – learn updated real aliases.
    for (peer_id, name) in peer_names.iter() {
        if is_user_set_peer_name(name) {
            let known = known_peer_names.get(peer_id);
            if known.map_or(true, |k| k != name) {
                known_peer_names.insert(*peer_id, name.clone());
            }
        }
    }
    // Pass 2 – restore known aliases for peers that only have a placeholder.
    for (peer_id, name) in peer_names.iter_mut() {
        if let Some(known_name) = known_peer_names.get(peer_id) {
            if name != known_name {
                *name = known_name.clone();
            }
        }
    }
}

#[test]
fn test_is_user_set_peer_name_detects_placeholders() {
    // A default name like "Peer_<PeerId>" is always > 30 chars.
    let peer_id = PeerId::random();
    let default_name = format!("Peer_{peer_id}");
    assert!(
        !is_user_set_peer_name(&default_name),
        "default placeholder should NOT be treated as a user-set name"
    );
}

#[test]
fn test_is_user_set_peer_name_accepts_real_aliases() {
    assert!(is_user_set_peer_name("Alice"));
    assert!(is_user_set_peer_name("bob_123"));
    // Verify the exact boundary: a 30-character name should be accepted.
    assert!(is_user_set_peer_name(&"x".repeat(30)));
}

#[test]
fn test_alias_persists_after_disconnect_and_reconnect() {
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut known_peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_id = PeerId::random();

    // --- First connection ---
    // Peer connects with placeholder.
    peer_names.insert(peer_id, format!("Peer_{peer_id}"));
    // Peer broadcasts real alias via floodsub → update peer_names.
    peer_names.insert(peer_id, "Alice".to_string());
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // known_peer_names should now remember "Alice".
    assert_eq!(known_peer_names.get(&peer_id), Some(&"Alice".to_string()));

    // --- Disconnection ---
    if let Some(name) = peer_names.remove(&peer_id) {
        if is_user_set_peer_name(&name) {
            known_peer_names.insert(peer_id, name);
        }
    }
    assert!(peer_names.is_empty(), "peer should be gone from active map");
    assert_eq!(
        known_peer_names.get(&peer_id),
        Some(&"Alice".to_string()),
        "known cache should still hold the alias"
    );

    // --- Reconnection with placeholder ---
    peer_names.insert(peer_id, format!("Peer_{peer_id}"));
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // The alias should be restored immediately without waiting for re-broadcast.
    assert_eq!(
        peer_names.get(&peer_id),
        Some(&"Alice".to_string()),
        "alias should be restored on reconnection"
    );
}

#[test]
fn test_alias_update_propagates_to_known_cache() {
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut known_peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_id = PeerId::random();

    // Peer connects and sets initial alias "Alice".
    peer_names.insert(peer_id, "Alice".to_string());
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);
    assert_eq!(known_peer_names.get(&peer_id), Some(&"Alice".to_string()));

    // Peer changes alias to "Bob" in the same session.
    peer_names.insert(peer_id, "Bob".to_string());
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // known_peer_names should be updated with the newer alias.
    assert_eq!(known_peer_names.get(&peer_id), Some(&"Bob".to_string()));
    assert_eq!(peer_names.get(&peer_id), Some(&"Bob".to_string()));
}

#[test]
fn test_peer_without_alias_stays_as_placeholder() {
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut known_peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_id = PeerId::random();
    let placeholder = format!("Peer_{peer_id}");

    // Peer connects but never broadcasts a real alias.
    peer_names.insert(peer_id, placeholder.clone());
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // No real alias should be stored and the name should remain the placeholder.
    assert!(
        known_peer_names.is_empty(),
        "placeholder should not pollute the known cache"
    );
    assert_eq!(peer_names.get(&peer_id), Some(&placeholder));
}

#[test]
fn test_multiple_peers_independent_caching() {
    let mut peer_names: HashMap<PeerId, String> = HashMap::new();
    let mut known_peer_names: HashMap<PeerId, String> = HashMap::new();
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();

    // Both peers connect and set aliases.
    peer_names.insert(peer_a, "Alice".to_string());
    peer_names.insert(peer_b, "Bob".to_string());
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // Only peer_a disconnects.
    if let Some(name) = peer_names.remove(&peer_a) {
        if is_user_set_peer_name(&name) {
            known_peer_names.insert(peer_a, name);
        }
    }

    // peer_b should still be in peer_names and peer_a's alias still in cache.
    assert_eq!(peer_names.get(&peer_b), Some(&"Bob".to_string()));
    assert_eq!(known_peer_names.get(&peer_a), Some(&"Alice".to_string()));

    // peer_a reconnects with placeholder.
    peer_names.insert(peer_a, format!("Peer_{peer_a}"));
    sync_known_peer_names(&mut peer_names, &mut known_peer_names);

    // peer_a gets "Alice" back; peer_b is unaffected.
    assert_eq!(peer_names.get(&peer_a), Some(&"Alice".to_string()));
    assert_eq!(peer_names.get(&peer_b), Some(&"Bob".to_string()));
}
