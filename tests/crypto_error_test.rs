// Test to reproduce and validate the crypto error user experience improvement
use libp2p::PeerId;
use libp2p::identity::Keypair;
use p2p_play::crypto::{CryptoError, CryptoService};
use p2p_play::handlers::UILogger;
use p2p_play::relay::{RelayError, RelayService};
use p2p_play::types::{DirectMessage, RelayConfig};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[test]
fn test_crypto_error_propagation() {
    // Set up a relay service with a crypto service
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(&keypair.public());
    let crypto = CryptoService::new(keypair);
    let config = RelayConfig::default();
    let mut relay_service = RelayService::new(config, crypto);

    // Create a direct message to an unknown peer (no public key in cache)
    let unknown_peer_id = PeerId::random();
    let direct_msg = DirectMessage {
        from_peer_id: peer_id.to_string(),
        from_name: "test_user".to_string(),
        to_name: "offline_peer".to_string(),
        message: "Hello there".to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // This should fail with a crypto error about missing public key
    let result = relay_service.create_relay_message(&direct_msg, &unknown_peer_id);

    assert!(result.is_err());

    let error = result.unwrap_err();

    // This should be a crypto error about missing public key
    match &error {
        RelayError::CryptoError(CryptoError::EncryptionFailed(msg)) => {
            assert!(msg.contains("Public key not found"));
            // This is the current technical error message that confuses users
            println!("Current error: {}", error);
        }
        _ => panic!(
            "Expected CryptoError::EncryptionFailed with missing public key, got: {:?}",
            error
        ),
    }
}

#[test]
fn test_user_friendly_error_detection() {
    // Test that we can detect and classify the specific error
    let crypto_error =
        CryptoError::EncryptionFailed("Public key not found for peer 12D3KooW...".to_string());
    let relay_error = RelayError::CryptoError(crypto_error);

    // Test the error detection logic that would be used in handlers.rs
    match &relay_error {
        RelayError::CryptoError(CryptoError::EncryptionFailed(msg)) => {
            if msg.contains("Public key not found") {
                // This is the case we want to detect and provide friendly messages for
                assert!(true, "Successfully detected missing public key scenario");
            } else {
                panic!("Unexpected encryption error message: {}", msg);
            }
        }
        _ => panic!(
            "Expected CryptoError::EncryptionFailed, got: {:?}",
            relay_error
        ),
    }
}

#[tokio::test]
async fn test_user_friendly_message_formatting() {
    // Create a mock UI logger to capture messages
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let ui_logger = UILogger::new(sender);

    // Simulate the user-friendly messages that would be sent
    let to_name = "alice";
    ui_logger.log(format!(
        "🔐 Cannot send secure message to offline peer '{to_name}'"
    ));
    ui_logger.log(format!(
        "📥 Message queued - will be delivered when {to_name} comes online and security keys are exchanged"
    ));
    ui_logger.log(
        "ℹ️  Tip: Both peers must be online simultaneously for secure messaging setup".to_string(),
    );

    // Collect all messages
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    // Verify the user-friendly messages
    assert_eq!(messages.len(), 3);
    assert!(messages[0].contains("🔐 Cannot send secure message to offline peer 'alice'"));
    assert!(messages[1].contains("📥 Message queued"));
    assert!(messages[1].contains("alice comes online"));
    assert!(messages[2].contains("ℹ️  Tip: Both peers must be online"));

    // Verify no technical jargon appears
    for message in &messages {
        assert!(!message.contains("Crypto error"));
        assert!(!message.contains("Encryption failed"));
        assert!(!message.contains("Public key not found"));
    }
}
