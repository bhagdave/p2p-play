use crate::constants::{
    ENCRYPTION_CONTEXT, MAX_MESSAGE_SIZE, MIN_PUBLIC_KEY_SIZE, REPLAY_PROTECTION_WINDOW_SECS,
};
use crate::errors::CryptoError;
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use curve25519_dalek::edwards::CompressedEdwardsY;
use hkdf::Hkdf;
use libp2p::{PeerId, identity::Keypair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(ZeroizeOnDrop)]
struct SecureKey([u8; 32]);

impl SecureKey {
    fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[derive(ZeroizeOnDrop)]
/// Zeroizing wrapper for X25519 shared secrets to avoid heap allocation of key material.
struct SharedSecret([u8; 32]);

impl SharedSecret {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

fn get_current_timestamp_with_error(
    error_constructor: fn(String) -> CryptoError,
) -> Result<u64, CryptoError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| error_constructor("System time is before UNIX epoch".to_string()))
}

fn get_current_timestamp() -> Result<u64, CryptoError> {
    get_current_timestamp_with_error(CryptoError::VerificationFailed)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EncryptedPayload {
    pub encrypted_data: Vec<u8>,
    pub nonce: Vec<u8>,             // For authenticated encryption
    pub sender_public_key: Vec<u8>, // For key verification
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MessageSignature {
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Clone)]
pub struct CryptoService {
    local_keypair: Keypair,
    // Peer public key cache for encryption
    peer_public_keys: std::collections::HashMap<PeerId, Vec<u8>>,
}

impl CryptoService {
    pub fn new(keypair: Keypair) -> Self {
        Self {
            local_keypair: keypair,
            peer_public_keys: std::collections::HashMap::new(),
        }
    }

    /// Add a peer's public key to the cache for encryption
    #[allow(dead_code)]
    pub fn add_peer_public_key(
        &mut self,
        peer_id: PeerId,
        public_key: Vec<u8>,
    ) -> Result<(), CryptoError> {
        // Validate public key format and size
        if public_key.is_empty() {
            return Err(CryptoError::InvalidInput(
                "Public key cannot be empty".to_string(),
            ));
        }

        if public_key.len() < MIN_PUBLIC_KEY_SIZE {
            return Err(CryptoError::InvalidInput(format!(
                "Public key too small: {} bytes (minimum: {})",
                public_key.len(),
                MIN_PUBLIC_KEY_SIZE
            )));
        }

        // Try to decode the public key to verify it's valid
        libp2p::identity::PublicKey::try_decode_protobuf(&public_key)
            .map_err(|e| CryptoError::InvalidInput(format!("Invalid public key format: {e}")))?;

        self.peer_public_keys.insert(peer_id, public_key);
        Ok(())
    }

    pub fn local_peer_id(&self) -> PeerId {
        PeerId::from(self.local_keypair.public())
    }

    pub fn encrypt_message(
        &self,
        message: &[u8],
        recipient_peer_id: &PeerId,
    ) -> Result<EncryptedPayload, CryptoError> {
        if message.is_empty() {
            return Err(CryptoError::InvalidInput(
                "Message cannot be empty".to_string(),
            ));
        }

        if message.len() > MAX_MESSAGE_SIZE {
            return Err(CryptoError::InvalidInput(format!(
                "Message too large: {} bytes (maximum: {})",
                message.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        let recipient_public_key =
            self.peer_public_keys
                .get(recipient_peer_id)
                .ok_or_else(|| {
                    CryptoError::EncryptionFailed(format!(
                        "Public key not found for peer {recipient_peer_id}"
                    ))
                })?;

        let our_public_key = self.get_our_public_key()?;

        let shared_secret = self.derive_shared_secret(recipient_public_key)?;

        let secure_key =
            self.derive_key(shared_secret.as_slice(), CryptoError::EncryptionFailed)?;

        let cipher = self.create_cipher(&secure_key);

        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

        let encrypted_data = cipher
            .encrypt(&nonce, message)
            .map_err(|e| CryptoError::EncryptionFailed(format!("Encryption failed: {e}")))?;

        Ok(EncryptedPayload {
            encrypted_data,
            nonce: nonce.to_vec(),
            sender_public_key: our_public_key,
        })
    }

    pub fn decrypt_message(&self, encrypted: &EncryptedPayload) -> Result<Vec<u8>, CryptoError> {
        if encrypted.encrypted_data.is_empty() {
            return Err(CryptoError::DecryptionFailed(
                "Encrypted data cannot be empty".to_string(),
            ));
        }

        if encrypted.nonce.len() != 12 {
            return Err(CryptoError::DecryptionFailed(format!(
                "Invalid nonce size: {} (expected: 12)",
                encrypted.nonce.len()
            )));
        }

        if encrypted.sender_public_key.len() < MIN_PUBLIC_KEY_SIZE {
            return Err(CryptoError::DecryptionFailed(format!(
                "Sender public key too small: {} bytes",
                encrypted.sender_public_key.len()
            )));
        }

        let shared_secret = self.derive_shared_secret(&encrypted.sender_public_key)?;

        let secure_key =
            self.derive_key(shared_secret.as_slice(), CryptoError::DecryptionFailed)?;

        let cipher = self.create_cipher(&secure_key);

        let nonce = Nonce::from_slice(&encrypted.nonce);

        let decrypted_data = cipher
            .decrypt(nonce, encrypted.encrypted_data.as_ref())
            .map_err(|e| CryptoError::DecryptionFailed(format!("Decryption failed: {e}")))?;

        if decrypted_data.len() > MAX_MESSAGE_SIZE {
            return Err(CryptoError::DecryptionFailed(
                "Decrypted message exceeds size limit".to_string(),
            ));
        }

        Ok(decrypted_data)
    }

    pub fn sign_message(&self, message: &[u8]) -> Result<MessageSignature, CryptoError> {
        if message.is_empty() {
            return Err(CryptoError::SignatureFailed(
                "Message cannot be empty".to_string(),
            ));
        }

        if message.len() > MAX_MESSAGE_SIZE {
            return Err(CryptoError::SignatureFailed(format!(
                "Message too large: {} bytes (maximum: {})",
                message.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        let timestamp = get_current_timestamp_with_error(CryptoError::SignatureFailed)?;
        let mut message_to_sign = message.to_vec();
        message_to_sign.extend_from_slice(&timestamp.to_be_bytes());

        let signature = self
            .local_keypair
            .sign(&message_to_sign)
            .map_err(|e| CryptoError::SignatureFailed(format!("Signing failed: {e}")))?;

        let public_key = self.get_our_public_key()?;

        Ok(MessageSignature {
            signature,
            public_key,
            timestamp,
        })
    }

    pub fn verify_signature(
        &self,
        message: &[u8],
        signature: &MessageSignature,
    ) -> Result<bool, CryptoError> {
        if message.is_empty() {
            return Err(CryptoError::VerificationFailed(
                "Message cannot be empty".to_string(),
            ));
        }

        if message.len() > MAX_MESSAGE_SIZE {
            return Err(CryptoError::VerificationFailed(format!(
                "Message too large: {} bytes (maximum: {})",
                message.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        if signature.public_key.len() < MIN_PUBLIC_KEY_SIZE {
            return Err(CryptoError::VerificationFailed(format!(
                "Public key too small: {} bytes",
                signature.public_key.len()
            )));
        }

        // Check for replay attacks - reject messages older than the time window
        let current_time = get_current_timestamp()?;
        if current_time.saturating_sub(signature.timestamp) > REPLAY_PROTECTION_WINDOW_SECS {
            return Err(CryptoError::VerificationFailed(format!(
                "Message too old (timestamp: {}, current: {}, max age: {}s)",
                signature.timestamp, current_time, REPLAY_PROTECTION_WINDOW_SECS
            )));
        }

        // Reject messages from the future (with small tolerance for clock skew)
        if signature.timestamp > current_time + 60 {
            // 1 minute tolerance
            return Err(CryptoError::VerificationFailed(format!(
                "Message from future (timestamp: {}, current: {})",
                signature.timestamp, current_time
            )));
        }

        // Recreate the signed message (message + timestamp)
        let mut message_to_verify = message.to_vec();
        message_to_verify.extend_from_slice(&signature.timestamp.to_be_bytes());

        // Convert public key bytes to PublicKey
        let public_key = libp2p::identity::PublicKey::try_decode_protobuf(&signature.public_key)
            .map_err(|e| CryptoError::VerificationFailed(format!("Invalid public key: {e}")))?;

        // Verify signature
        let is_valid = public_key.verify(&message_to_verify, &signature.signature);

        Ok(is_valid)
    }

    #[allow(dead_code)]
    pub fn public_key_from_peer_id(&self, peer_id: &PeerId) -> Result<Vec<u8>, CryptoError> {
        if let Some(public_key) = self.peer_public_keys.get(peer_id) {
            Ok(public_key.clone())
        } else {
            Err(CryptoError::InvalidInput(format!(
                "Public key not found for peer {peer_id}"
            )))
        }
    }

    fn get_our_public_key(&self) -> Result<Vec<u8>, CryptoError> {
        Ok(self.local_keypair.public().encode_protobuf())
    }

    /// Perform X25519 ECDH using our Ed25519 keypair and a peer's protobuf-encoded Ed25519 public key.
    /// Both sides independently derive the same shared secret (DH commutativity).
    fn derive_shared_secret(&self, their_pubkey_bytes: &[u8]) -> Result<SharedSecret, CryptoError> {
        let our_secret = self.ed25519_to_x25519_secret()?;
        let their_pubkey = Self::ed25519_pubkey_to_x25519(their_pubkey_bytes)?;
        let shared = our_secret.diffie_hellman(&their_pubkey);
        Self::validate_shared_secret(shared.to_bytes())
    }

    fn validate_shared_secret(shared_bytes: [u8; 32]) -> Result<SharedSecret, CryptoError> {
        if shared_bytes.iter().all(|&byte| byte == 0) {
            Err(CryptoError::InvalidInput(
                "Rejected low-order peer public key".to_string(),
            ))
        } else {
            Ok(SharedSecret(shared_bytes))
        }
    }

    /// Convert our Ed25519 signing key to an X25519 static secret via RFC 8037:
    /// SHA-512 the 32-byte seed, take the first 32 bytes, apply Curve25519 clamping.
    fn ed25519_to_x25519_secret(&self) -> Result<X25519Secret, CryptoError> {
        let ed25519_kp = self
            .local_keypair
            .clone()
            .try_into_ed25519()
            .map_err(|_| CryptoError::InvalidInput("Keypair must be Ed25519".into()))?;

        let mut hash: [u8; 64] = Sha512::digest(ed25519_kp.secret().as_ref()).into();

        let mut x25519_bytes = [0u8; 32];
        x25519_bytes.copy_from_slice(&hash[..32]);
        // Curve25519 clamping (RFC 7748 §5)
        x25519_bytes[0] &= 248;
        x25519_bytes[31] &= 127;
        x25519_bytes[31] |= 64;

        let secret = X25519Secret::from(x25519_bytes);
        x25519_bytes.zeroize();
        hash.zeroize();
        Ok(secret)
    }

    /// Convert a protobuf-encoded Ed25519 public key to an X25519 public key via the
    /// birational map from twisted Edwards to Montgomery form.
    fn ed25519_pubkey_to_x25519(pubkey_bytes: &[u8]) -> Result<X25519PublicKey, CryptoError> {
        let libp2p_pk = libp2p::identity::PublicKey::try_decode_protobuf(pubkey_bytes)
            .map_err(|e| CryptoError::InvalidInput(format!("Invalid public key: {e}")))?;

        let ed25519_pk = libp2p_pk
            .try_into_ed25519()
            .map_err(|_| CryptoError::InvalidInput("Public key must be Ed25519".into()))?;

        let compressed = CompressedEdwardsY(ed25519_pk.to_bytes());
        let edwards_point = compressed
            .decompress()
            .ok_or_else(|| CryptoError::InvalidInput("Invalid Ed25519 public key point".into()))?;

        Ok(X25519PublicKey::from(
            edwards_point.to_montgomery().to_bytes(),
        ))
    }

    fn derive_key(
        &self,
        shared_secret: &[u8],
        err_variant: fn(String) -> CryptoError,
    ) -> Result<SecureKey, CryptoError> {
        let hk = Hkdf::<Sha256>::new(None, shared_secret);
        let mut key_data = [0u8; 32];
        hk.expand(ENCRYPTION_CONTEXT, &mut key_data)
            .map_err(|e| err_variant(format!("Key derivation failed: {e}")))?;
        let secure_key = SecureKey::new(key_data);
        key_data.zeroize();
        Ok(secure_key)
    }

    fn create_cipher(&self, key: &SecureKey) -> ChaCha20Poly1305 {
        ChaCha20Poly1305::new(Key::from_slice(key.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity;

    fn create_test_keypair() -> Keypair {
        identity::Keypair::generate_ed25519()
    }

    #[test]
    fn test_crypto_service_creation() {
        let keypair = create_test_keypair();
        let crypto_service = CryptoService::new(keypair);

        // Should be able to create without panicking
        assert!(!crypto_service.local_peer_id().to_string().is_empty());
    }

    #[test]
    fn test_message_signing_and_verification() {
        let keypair = create_test_keypair();
        let crypto_service = CryptoService::new(keypair);

        let message = b"Hello, World!";

        // Sign the message
        let signature = crypto_service.sign_message(message).unwrap();

        // Verify the signature
        let is_valid = crypto_service
            .verify_signature(message, &signature)
            .unwrap();
        assert!(is_valid);

        // Verify that tampering with message fails verification
        let tampered_message = b"Hello, World?";
        let is_tampered_valid = crypto_service
            .verify_signature(tampered_message, &signature)
            .unwrap();
        assert!(!is_tampered_valid);
    }

    #[test]
    fn test_encryption_decryption_roundtrip() {
        let alice_keypair = create_test_keypair();
        let bob_keypair = create_test_keypair();

        let mut alice_crypto = CryptoService::new(alice_keypair);
        let mut bob_crypto = CryptoService::new(bob_keypair.clone());

        let message = b"Secret message from Alice to Bob";

        // Add Bob's public key to Alice's cache
        let bob_peer_id = bob_crypto.local_peer_id();
        let bob_public_key = bob_keypair.public().encode_protobuf();
        alice_crypto
            .add_peer_public_key(bob_peer_id, bob_public_key)
            .unwrap();

        // Add Alice's public key to Bob's cache
        let alice_peer_id = alice_crypto.local_peer_id();
        let alice_public_key = alice_crypto.local_keypair.public().encode_protobuf();
        bob_crypto
            .add_peer_public_key(alice_peer_id, alice_public_key)
            .unwrap();

        // Encrypt message from Alice to Bob
        let encrypted = alice_crypto.encrypt_message(message, &bob_peer_id).unwrap();

        // Decrypt message at Bob's side
        let decrypted = bob_crypto.decrypt_message(&encrypted).unwrap();

        // Verify the roundtrip worked
        assert_eq!(message, decrypted.as_slice());

        // Verify encryption produces different ciphertext with different nonces
        let encrypted2 = alice_crypto.encrypt_message(message, &bob_peer_id).unwrap();
        assert_ne!(encrypted.encrypted_data, encrypted2.encrypted_data);
        assert_ne!(encrypted.nonce, encrypted2.nonce);

        // But both decrypt to the same message
        let decrypted2 = bob_crypto.decrypt_message(&encrypted2).unwrap();
        assert_eq!(message, decrypted2.as_slice());
    }

    #[test]
    fn test_error_display() {
        let error = CryptoError::EncryptionFailed("test error".to_string());
        assert_eq!(error.to_string(), "Encryption failed: test error");

        let error = CryptoError::InvalidInput("bad input".to_string());
        assert_eq!(error.to_string(), "Invalid input: bad input");
    }

    #[test]
    fn test_public_key_management() {
        let keypair = create_test_keypair();
        let mut crypto_service = CryptoService::new(keypair.clone());

        let peer_id = PeerId::random();
        // Use a real public key format instead of test bytes
        let public_key = keypair.public().encode_protobuf();

        // Initially, peer should not be found
        assert!(crypto_service.public_key_from_peer_id(&peer_id).is_err());

        // Add peer public key
        crypto_service
            .add_peer_public_key(peer_id, public_key.clone())
            .unwrap();

        // Now it should be found
        let retrieved_key = crypto_service.public_key_from_peer_id(&peer_id).unwrap();
        assert_eq!(public_key, retrieved_key);
    }

    #[test]
    fn test_encryption_without_public_key() {
        let keypair = create_test_keypair();
        let crypto_service = CryptoService::new(keypair);

        let unknown_peer_id = PeerId::random();
        let message = b"test message";

        // Should fail when trying to encrypt to unknown peer
        let result = crypto_service.encrypt_message(message, &unknown_peer_id);
        assert!(result.is_err());

        match result.unwrap_err() {
            CryptoError::EncryptionFailed(msg) => {
                assert!(msg.contains("Public key not found"));
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_input_validation() {
        let keypair = create_test_keypair();
        let mut crypto_service = CryptoService::new(keypair.clone());

        // Test empty message encryption
        let peer_id = PeerId::random();
        let result = crypto_service.encrypt_message(b"", &peer_id);
        assert!(matches!(result.unwrap_err(), CryptoError::InvalidInput(_)));

        // Test empty message signing
        let result = crypto_service.sign_message(b"");
        assert!(matches!(
            result.unwrap_err(),
            CryptoError::SignatureFailed(_)
        ));

        // Test invalid public key addition
        let result = crypto_service.add_peer_public_key(peer_id, vec![]);
        assert!(matches!(result.unwrap_err(), CryptoError::InvalidInput(_)));

        // Test too small public key
        let result = crypto_service.add_peer_public_key(peer_id, vec![1, 2, 3]);
        assert!(matches!(result.unwrap_err(), CryptoError::InvalidInput(_)));

        // Test invalid public key format
        let result = crypto_service.add_peer_public_key(peer_id, vec![0; 64]);
        assert!(matches!(result.unwrap_err(), CryptoError::InvalidInput(_)));
    }

    #[test]
    fn test_message_size_limits() {
        let alice_keypair = create_test_keypair();
        let bob_keypair = create_test_keypair();

        let mut alice_crypto = CryptoService::new(alice_keypair);
        let bob_crypto = CryptoService::new(bob_keypair.clone());

        let bob_peer_id = bob_crypto.local_peer_id();
        let bob_public_key = bob_keypair.public().encode_protobuf();
        alice_crypto
            .add_peer_public_key(bob_peer_id, bob_public_key)
            .unwrap();

        // Test message at limit (should succeed)
        let large_message = vec![42u8; 1024 * 1024]; // 1MB
        let result = alice_crypto.encrypt_message(&large_message, &bob_peer_id);
        assert!(result.is_ok());

        // Test message over limit (should fail)
        let oversized_message = vec![42u8; 1024 * 1024 + 1]; // 1MB + 1 byte
        let result = alice_crypto.encrypt_message(&oversized_message, &bob_peer_id);
        assert!(matches!(result.unwrap_err(), CryptoError::InvalidInput(_)));
    }

    #[test]
    fn test_replay_protection() {
        let keypair = create_test_keypair();
        let crypto_service = CryptoService::new(keypair);

        let message = b"test message";

        // Create a signature
        let mut signature = crypto_service.sign_message(message).unwrap();

        // Valid signature should pass
        let result = crypto_service.verify_signature(message, &signature);
        assert!(result.is_ok() && result.unwrap());

        // Modify timestamp to be too old (should fail)
        signature.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 400; // 400 seconds ago (beyond 300 second window)

        let result = crypto_service.verify_signature(message, &signature);
        assert!(matches!(
            result.unwrap_err(),
            CryptoError::VerificationFailed(_)
        ));

        // Modify timestamp to be from future (should fail)
        signature.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 120; // 2 minutes in future

        let result = crypto_service.verify_signature(message, &signature);
        assert!(matches!(
            result.unwrap_err(),
            CryptoError::VerificationFailed(_)
        ));
    }

    #[test]
    fn test_validate_shared_secret_rejects_all_zero_secret() {
        let result = CryptoService::validate_shared_secret([0u8; 32]);
        assert!(matches!(result, Err(CryptoError::InvalidInput(_))));
    }
}
