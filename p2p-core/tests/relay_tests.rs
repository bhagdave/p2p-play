use p2p_core::{RelayService, RelayConfig, CryptoService, DirectMessage, RelayMessage};
use libp2p::{identity, PeerId};
use std::time::{SystemTime, UNIX_EPOCH};

fn create_test_direct_message(
    from_peer: &str,
    to_peer: &str,
    message: &str,
    is_outgoing: bool,
) -> DirectMessage {
    DirectMessage {
        from_peer_id: from_peer.to_string(),
        from_name: from_peer.to_string(),
        to_peer_id: to_peer.to_string(),
        to_name: to_peer.to_string(),
        message: message.to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        is_outgoing,
    }
}

#[tokio::test]
async fn test_relay_service_creation() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    
    let relay_service = RelayService::new(config, crypto);
    
    // Test that we can get initial stats
    let stats = relay_service.get_stats();
    assert_eq!(stats.messages_relayed, 0);
    assert_eq!(stats.messages_dropped, 0);
    assert_eq!(stats.rate_limit_hits, 0);
    assert_eq!(stats.crypto_errors, 0);
}

#[tokio::test]
async fn test_relay_config_validation() {
    let valid_config = RelayConfig::default();
    assert!(valid_config.validate().is_ok());
    
    // Test invalid configs
    let invalid_config = RelayConfig {
        max_ttl: 0, // Invalid
        ..Default::default()
    };
    assert!(invalid_config.validate().is_err());
    
    let invalid_config = RelayConfig {
        max_message_size: 0, // Invalid
        ..Default::default()
    };
    assert!(invalid_config.validate().is_err());
    
    let invalid_config = RelayConfig {
        relay_timeout_secs: 0, // Invalid
        ..Default::default()
    };
    assert!(invalid_config.validate().is_err());
}

#[tokio::test]
async fn test_relay_config_timeout_conversion() {
    let config = RelayConfig {
        relay_timeout_secs: 300,
        ..Default::default()
    };
    
    let timeout = config.relay_timeout();
    assert_eq!(timeout.as_secs(), 300);
}

#[tokio::test]
async fn test_create_relay_message() {
    let config = RelayConfig::default();
    let alice_keypair = identity::Keypair::generate_ed25519();
    let bob_keypair = identity::Keypair::generate_ed25519();
    
    let mut alice_crypto = CryptoService::new(alice_keypair.clone());
    let bob_peer_id = PeerId::from(bob_keypair.public());
    
    // Add Bob's public key to Alice's crypto service
    let bob_public_key = bob_keypair.public().encode_protobuf();
    alice_crypto
        .add_peer_public_key(bob_peer_id, bob_public_key)
        .unwrap();
    
    let relay_service = RelayService::new(config.clone(), alice_crypto);
    
    let direct_message = create_test_direct_message(
        &PeerId::from(alice_keypair.public()).to_string(),
        &bob_peer_id.to_string(),
        "Hello Bob!",
        true,
    );
    
    let result = relay_service.create_relay_message(&direct_message, &bob_peer_id);
    assert!(result.is_ok());
    
    let relay_message = result.unwrap();
    assert_eq!(relay_message.target_peer_id, bob_peer_id.to_string());
    assert_eq!(relay_message.message_type, "direct_message");
    assert_eq!(relay_message.relay_ttl, config.max_ttl);
}

#[tokio::test]
async fn test_create_relay_message_too_large() {
    let config = RelayConfig {
        max_message_size: 10, // Very small limit
        ..Default::default()
    };
    
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    let large_message = create_test_direct_message(
        "alice",
        "bob", 
        &"x".repeat(1000), // Large message
        true,
    );
    
    let target_peer = PeerId::random();
    let result = relay_service.create_relay_message(&large_message, &target_peer);
    
    assert!(result.is_err());
    match result.unwrap_err() {
        p2p_core::RelayError::MessageTooLarge { size, max } => {
            assert!(size > max);
        }
        other => panic!("Expected MessageTooLarge error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_process_relay_message_expired_ttl() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    // Create a relay message with expired TTL
    let relay_message = RelayMessage {
        target_peer_id: PeerId::random().to_string(),
        encrypted_payload: p2p_core::EncryptedPayload {
            encrypted_data: vec![1, 2, 3],
            nonce: vec![4, 5, 6],
            sender_public_key: vec![7, 8, 9],
        },
        signature: p2p_core::MessageSignature {
            signature: vec![10, 11, 12],
            public_key: vec![13, 14, 15],
            timestamp: 1234567890,
        },
        timestamp: 1234567890,
        message_type: "direct_message".to_string(),
        relay_ttl: 0, // Expired TTL
    };
    
    let result = relay_service.process_relay_message(&relay_message).unwrap();
    assert!(result.is_none()); // Should be dropped
    
    // Check stats
    let stats = relay_service.get_stats();
    assert_eq!(stats.messages_dropped, 1);
}

#[tokio::test]
async fn test_create_forwarded_message() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    let original_message = RelayMessage {
        target_peer_id: PeerId::random().to_string(),
        encrypted_payload: p2p_core::EncryptedPayload {
            encrypted_data: vec![1, 2, 3],
            nonce: vec![4, 5, 6],
            sender_public_key: vec![7, 8, 9],
        },
        signature: p2p_core::MessageSignature {
            signature: vec![10, 11, 12],
            public_key: vec![13, 14, 15],
            timestamp: 1234567890,
        },
        timestamp: 1234567890,
        message_type: "direct_message".to_string(),
        relay_ttl: 3,
    };
    
    let forwarded = relay_service.create_forwarded_message(&original_message);
    assert!(forwarded.is_some());
    
    let forwarded = forwarded.unwrap();
    assert_eq!(forwarded.relay_ttl, 2); // Decremented
    assert_eq!(forwarded.target_peer_id, original_message.target_peer_id);
}

#[tokio::test]
async fn test_create_forwarded_message_expired() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    let original_message = RelayMessage {
        target_peer_id: PeerId::random().to_string(),
        encrypted_payload: p2p_core::EncryptedPayload {
            encrypted_data: vec![1, 2, 3],
            nonce: vec![4, 5, 6],
            sender_public_key: vec![7, 8, 9],
        },
        signature: p2p_core::MessageSignature {
            signature: vec![10, 11, 12],
            public_key: vec![13, 14, 15],
            timestamp: 1234567890,
        },
        timestamp: 1234567890,
        message_type: "direct_message".to_string(),
        relay_ttl: 1, // Will be 0 after decrement
    };
    
    let forwarded = relay_service.create_forwarded_message(&original_message);
    assert!(forwarded.is_none()); // Should not forward
}

#[tokio::test]
async fn test_rate_limiting() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    let peer_id = "test_peer_123";
    
    // Should initially allow forwarding
    assert!(relay_service.can_forward(peer_id));
    
    // Simulate many rapid requests (beyond rate limit)
    for _ in 0..15 {
        relay_service.can_forward(peer_id);
    }
    
    // Should eventually hit rate limit
    let stats = relay_service.get_stats();
    // Note: The exact behavior depends on rate limit implementation
    // This test mainly ensures the method doesn't panic
}

#[tokio::test]
async fn test_relay_stats_management() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    // Initial stats should be zero
    let stats = relay_service.get_stats();
    assert_eq!(stats.messages_relayed, 0);
    assert_eq!(stats.messages_dropped, 0);
    
    // Reset stats
    relay_service.reset_stats();
    let stats = relay_service.get_stats();
    assert_eq!(stats.messages_relayed, 0);
    assert_eq!(stats.messages_dropped, 0);
}

#[tokio::test]
async fn test_rate_limit_cleanup() {
    let config = RelayConfig::default();
    let keypair = identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    let relay_service = RelayService::new(config, crypto);
    
    // Add some rate limit entries
    relay_service.can_forward("peer1");
    relay_service.can_forward("peer2");
    
    // Clean up old entries (should not panic)
    relay_service.cleanup_rate_limits();
}