use libp2p::PeerId;
use libp2p::identity::Keypair;
use p2p_play::crypto::CryptoService;
use p2p_play::relay::{RelayAction, RelayService};
use p2p_play::types::{DirectMessage, RelayConfig};

/// Test that relay service can create and process encrypted relay messages
#[tokio::test]
async fn test_relay_message_creation_and_processing() {
    // Create two different keypairs representing Alice and Bob
    let alice_keypair = Keypair::generate_ed25519();
    let bob_keypair = Keypair::generate_ed25519();

    let alice_peer_id = PeerId::from(alice_keypair.public());
    let bob_peer_id = PeerId::from(bob_keypair.public());

    // Create crypto services for Alice and Bob
    let mut alice_crypto = CryptoService::new(alice_keypair.clone());
    let mut bob_crypto = CryptoService::new(bob_keypair.clone());

    // Exchange public keys for encryption
    let alice_public_key = alice_keypair.public().encode_protobuf();
    let bob_public_key = bob_keypair.public().encode_protobuf();

    alice_crypto
        .add_peer_public_key(bob_peer_id, bob_public_key)
        .expect("Alice should add Bob's key");
    bob_crypto
        .add_peer_public_key(alice_peer_id, alice_public_key)
        .expect("Bob should add Alice's key");

    // Create relay services
    let relay_config = RelayConfig::new();
    let mut alice_relay = RelayService::new(relay_config.clone(), alice_crypto);
    let mut bob_relay = RelayService::new(relay_config, bob_crypto);

    // Create a direct message from Alice to Bob
    let direct_message = DirectMessage {
        from_peer_id: alice_peer_id.to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello Bob! This is a secure relay message.".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Alice creates a relay message
    let relay_message = alice_relay
        .create_relay_message(&direct_message, &bob_peer_id)
        .expect("Alice should create relay message");

    // Verify relay message structure
    assert_eq!(relay_message.target_peer_id, bob_peer_id.to_string());
    assert_eq!(relay_message.target_name, "Bob");
    assert_eq!(relay_message.hop_count, 0);
    assert_eq!(relay_message.max_hops, 3);
    assert!(relay_message.relay_attempt);

    // Bob processes the relay message
    let result = bob_relay
        .process_relay_message(&relay_message)
        .expect("Bob should process relay message");

    // Verify that Bob can decrypt and receive the message
    match result {
        RelayAction::DeliverLocally(decrypted_msg) => {
            assert_eq!(decrypted_msg.from_name, "Alice");
            assert_eq!(decrypted_msg.to_name, "Bob");
            assert_eq!(
                decrypted_msg.message,
                "Hello Bob! This is a secure relay message."
            );
            println!("✅ Relay message successfully decrypted and delivered to Bob");
        }
        RelayAction::ForwardMessage(_) => {
            panic!("Message should be delivered locally to Bob, not forwarded");
        }
        RelayAction::DropMessage(reason) => {
            panic!("Message should not be dropped: {reason}");
        }
    }
}

/// Test that relay service correctly forwards messages not intended for the current node
#[tokio::test]
async fn test_relay_message_forwarding() {
    // Create three keypairs: Alice, Bob, and Charlie
    let alice_keypair = Keypair::generate_ed25519();
    let bob_keypair = Keypair::generate_ed25519();
    let charlie_keypair = Keypair::generate_ed25519();

    let alice_peer_id = PeerId::from(alice_keypair.public());
    let bob_peer_id = PeerId::from(bob_keypair.public());
    let charlie_peer_id = PeerId::from(charlie_keypair.public());

    // Create crypto and relay services for each peer
    let alice_crypto = CryptoService::new(alice_keypair.clone());
    let bob_crypto = CryptoService::new(bob_keypair.clone());
    let charlie_crypto = CryptoService::new(charlie_keypair.clone());

    let relay_config = RelayConfig::new();
    let mut alice_relay = RelayService::new(relay_config.clone(), alice_crypto);
    let mut bob_relay = RelayService::new(relay_config.clone(), bob_crypto);
    let mut charlie_relay = RelayService::new(relay_config, charlie_crypto);

    // Add Charlie's public key to Alice's crypto service for encryption
    let charlie_public_key = charlie_keypair.public().encode_protobuf();
    alice_relay
        .crypto_service()
        .add_peer_public_key(charlie_peer_id, charlie_public_key)
        .expect("Alice should add Charlie's key");

    // Create a direct message from Alice to Charlie
    let direct_message = DirectMessage {
        from_peer_id: alice_peer_id.to_string(),
        from_name: "Alice".to_string(),
        to_name: "Charlie".to_string(),
        message: "Hello Charlie via relay!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Alice creates a relay message for Charlie
    let relay_message = alice_relay
        .create_relay_message(&direct_message, &charlie_peer_id)
        .expect("Alice should create relay message");

    // Bob receives the relay message (acting as intermediate node)
    let result = bob_relay
        .process_relay_message(&relay_message)
        .expect("Bob should process relay message");

    // Verify that Bob forwards the message (since it's not for him)
    match result {
        RelayAction::DeliverLocally(_) => {
            panic!("Bob should not be able to decrypt Alice's message to Charlie");
        }
        RelayAction::ForwardMessage(forwarded_msg) => {
            assert_eq!(forwarded_msg.hop_count, 1); // Hop count should be incremented
            assert_eq!(forwarded_msg.target_name, "Charlie");
            println!("✅ Bob correctly forwarded the message to Charlie");

            // Charlie receives the forwarded message
            let charlie_result = charlie_relay
                .process_relay_message(&forwarded_msg)
                .expect("Charlie should process forwarded message");

            match charlie_result {
                RelayAction::DeliverLocally(decrypted_msg) => {
                    assert_eq!(decrypted_msg.from_name, "Alice");
                    assert_eq!(decrypted_msg.to_name, "Charlie");
                    assert_eq!(decrypted_msg.message, "Hello Charlie via relay!");
                    println!("✅ Charlie successfully received the relayed message from Alice");
                }
                _ => {
                    panic!("Charlie should receive the message locally");
                }
            }
        }
        RelayAction::DropMessage(reason) => {
            panic!("Message should not be dropped: {reason}");
        }
    }
}

/// Test that relay service respects maximum hop limits
#[tokio::test]
async fn test_relay_max_hops_limit() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    let crypto = CryptoService::new(keypair);
    let mut relay_config = RelayConfig::new();
    relay_config.max_hops = 2; // Set a low hop limit

    let mut relay_service = RelayService::new(relay_config, crypto);

    // Create a relay message that has already reached the hop limit
    let relay_message = p2p_play::types::RelayMessage {
        message_id: "test-message".to_string(),
        target_peer_id: peer_id.to_string(),
        target_name: "TestPeer".to_string(),
        encrypted_payload: p2p_play::crypto::EncryptedPayload {
            encrypted_data: vec![1, 2, 3],
            nonce: vec![4, 5, 6],
            sender_public_key: vec![7, 8, 9],
        },
        sender_signature: p2p_play::crypto::MessageSignature {
            signature: vec![10, 11, 12],
            public_key: vec![13, 14, 15],
            timestamp: 1000,
        },
        hop_count: 2, // At the maximum
        max_hops: 2,
        timestamp: 1000,
        relay_attempt: true,
    };

    // Processing should drop the message due to hop limit
    let result = relay_service
        .process_relay_message(&relay_message)
        .expect("Should process message even if dropping");

    match result {
        RelayAction::DropMessage(reason) => {
            assert!(reason.contains("Max hops exceeded"));
            println!("✅ Message correctly dropped due to hop limit: {reason}");
        }
        _ => {
            panic!("Message should be dropped due to hop limit");
        }
    }
}

/// Test relay service rate limiting functionality
#[tokio::test]
async fn test_relay_rate_limiting() {
    let alice_keypair = Keypair::generate_ed25519();
    let bob_keypair = Keypair::generate_ed25519();

    let alice_peer_id = PeerId::from(alice_keypair.public());
    let bob_peer_id = PeerId::from(bob_keypair.public());

    let mut alice_crypto = CryptoService::new(alice_keypair);
    let bob_crypto = CryptoService::new(bob_keypair.clone());

    // Exchange keys
    let bob_public_key = bob_keypair.public().encode_protobuf();
    alice_crypto
        .add_peer_public_key(bob_peer_id, bob_public_key)
        .expect("Should add Bob's key");

    let mut relay_config = RelayConfig::new();
    relay_config.rate_limit_per_peer = 1; // Very low limit for testing

    let mut alice_relay = RelayService::new(relay_config, alice_crypto);

    let direct_message = DirectMessage {
        from_peer_id: alice_peer_id.to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Rate limit test message".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // First message should succeed
    let result1 = alice_relay.create_relay_message(&direct_message, &bob_peer_id);
    assert!(result1.is_ok(), "First message should succeed");

    // Second message should fail due to rate limiting
    let result2 = alice_relay.create_relay_message(&direct_message, &bob_peer_id);
    match result2 {
        Err(p2p_play::relay::RelayError::RateLimitExceeded) => {
            println!("✅ Rate limiting working correctly - second message rejected");
        }
        _ => panic!("Second message should be rate limited"),
    }
}
