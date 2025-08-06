# Crypto Module Usage Examples

The crypto module provides end-to-end encryption and digital signatures for the P2P-Play application.

## Basic Usage

```rust
use p2p_play::crypto::{CryptoService, CryptoError};
use p2p_play::network::KEYS;

// Create crypto service with existing libp2p keys
let mut crypto_service = CryptoService::new(KEYS.clone());

// Add peer public keys (would normally come from network discovery)
let peer_id = libp2p::PeerId::random();
let peer_public_key = some_keypair.public().encode_protobuf();
crypto_service.add_peer_public_key(peer_id, peer_public_key);

// Encrypt a message
let message = b"Hello, secure world!";
let encrypted = crypto_service.encrypt_message(message, &peer_id)?;

// Sign a message
let signature = crypto_service.sign_message(message)?;

// Verify a signature
let is_valid = crypto_service.verify_signature(message, &signature)?;
```

## Integration with DirectMessage

```rust
use p2p_play::types::DirectMessage;
use serde_json;

// Encrypt a DirectMessage
let dm = DirectMessage::new(
    "sender_peer_id".to_string(),
    "Alice".to_string(),
    "Bob".to_string(),
    "Secret message".to_string()
);

let dm_bytes = serde_json::to_vec(&dm)?;
let encrypted = crypto_service.encrypt_message(&dm_bytes, &recipient_peer_id)?;

// Decrypt at recipient
let decrypted_bytes = crypto_service.decrypt_message(&encrypted)?;
let dm: DirectMessage = serde_json::from_slice(&decrypted_bytes)?;
```

## Error Handling

The crypto module provides comprehensive error handling:

```rust
match crypto_service.encrypt_message(message, &unknown_peer) {
    Ok(encrypted) => {
        // Handle successful encryption
    }
    Err(CryptoError::EncryptionFailed(msg)) => {
        // Handle encryption failure
    }
    Err(CryptoError::KeyConversionFailed(msg)) => {
        // Handle key conversion issues
    }
    Err(other) => {
        // Handle other crypto errors
    }
}
```

## Security Features

- **ChaCha20-Poly1305**: Authenticated encryption with associated data (AEAD)
- **Ed25519 Signatures**: Fast, secure digital signatures
- **HKDF Key Derivation**: Secure key derivation from shared secrets
- **Nonce Generation**: Cryptographically secure random nonces
- **Replay Protection**: Timestamp-based signature verification
- **Memory Safety**: Secure handling of cryptographic material