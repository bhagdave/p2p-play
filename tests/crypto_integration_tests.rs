use libp2p::identity;
use p2p_play::crypto::{CryptoError, CryptoService};

#[tokio::test]
async fn test_crypto_integration_with_libp2p_keys() {
    // Create two keypairs using the same method as the network module
    let alice_keypair = identity::Keypair::generate_ed25519();
    let bob_keypair = identity::Keypair::generate_ed25519();

    // Create crypto services
    let mut alice_crypto = CryptoService::new(alice_keypair.clone());
    let mut bob_crypto = CryptoService::new(bob_keypair.clone());

    // Set up public key exchange (simulating network discovery)
    let alice_peer_id = libp2p::PeerId::from(alice_keypair.public());
    let bob_peer_id = libp2p::PeerId::from(bob_keypair.public());

    let alice_public_key = alice_keypair.public().encode_protobuf();
    let bob_public_key = bob_keypair.public().encode_protobuf();

    alice_crypto.add_peer_public_key(bob_peer_id, bob_public_key).unwrap();
    bob_crypto.add_peer_public_key(alice_peer_id, alice_public_key).unwrap();

    // Test message encryption/decryption
    let original_message = b"Hello from Alice to Bob via encrypted channel!";

    // Alice encrypts a message for Bob
    let encrypted = alice_crypto
        .encrypt_message(original_message, &bob_peer_id)
        .expect("Encryption should succeed");

    // Bob decrypts the message
    let decrypted = bob_crypto
        .decrypt_message(&encrypted)
        .expect("Decryption should succeed");

    assert_eq!(original_message, decrypted.as_slice());

    // Test that Bob can also send encrypted messages to Alice
    let bob_message = b"Reply from Bob to Alice!";
    let encrypted_reply = bob_crypto
        .encrypt_message(bob_message, &alice_peer_id)
        .expect("Encryption should succeed");

    let decrypted_reply = alice_crypto
        .decrypt_message(&encrypted_reply)
        .expect("Decryption should succeed");

    assert_eq!(bob_message, decrypted_reply.as_slice());
}

#[tokio::test]
async fn test_crypto_signature_integration() {
    // Test digital signatures work with libp2p keys
    let keypair = identity::Keypair::generate_ed25519();
    let crypto_service = CryptoService::new(keypair);

    let message = b"Important message that needs authentication";

    // Sign the message
    let signature = crypto_service
        .sign_message(message)
        .expect("Signing should succeed");

    // Verify the signature
    let is_valid = crypto_service
        .verify_signature(message, &signature)
        .expect("Verification should succeed");

    assert!(is_valid, "Signature should be valid");

    // Test that tampering with the message invalidates the signature
    let tampered_message = b"Tampered message";
    let is_tampered_valid = crypto_service
        .verify_signature(tampered_message, &signature)
        .expect("Verification should succeed");

    assert!(!is_tampered_valid, "Tampered signature should be invalid");
}

#[tokio::test]
async fn test_crypto_error_handling() {
    let keypair = identity::Keypair::generate_ed25519();
    let crypto_service = CryptoService::new(keypair);

    // Test encryption without peer public key
    let unknown_peer = libp2p::PeerId::random();
    let message = b"test message";

    let result = crypto_service.encrypt_message(message, &unknown_peer);
    assert!(result.is_err());

    match result.unwrap_err() {
        CryptoError::EncryptionFailed(msg) => {
            assert!(msg.contains("Public key not found"));
        }
        other => panic!("Unexpected error type: {:?}", other),
    }
}
