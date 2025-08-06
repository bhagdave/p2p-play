use libp2p::{PeerId, identity::Keypair};
use serde::{Serialize, Deserialize};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce, Key
};
use hkdf::Hkdf;
use sha2::Sha256;

/// Encrypted message envelope
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EncryptedPayload {
    pub encrypted_data: Vec<u8>,
    pub nonce: Vec<u8>,           // For authenticated encryption
    pub sender_public_key: Vec<u8>, // For key verification
}

/// Digital signature container
#[derive(Debug, Serialize, Deserialize, Clone)]  
pub struct MessageSignature {
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: u64,
}

/// Crypto service for message encryption/decryption
pub struct CryptoService {
    local_keypair: Keypair,
    local_peer_id: PeerId,
    // Peer public key cache for encryption
    peer_public_keys: std::collections::HashMap<PeerId, Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum CryptoError {
    EncryptionFailed(String),
    DecryptionFailed(String), 
    SignatureFailed(String),
    VerificationFailed(String),
    KeyConversionFailed(String),
    InvalidInput(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::EncryptionFailed(msg) => write!(f, "Encryption failed: {}", msg),
            CryptoError::DecryptionFailed(msg) => write!(f, "Decryption failed: {}", msg),
            CryptoError::SignatureFailed(msg) => write!(f, "Signature failed: {}", msg),
            CryptoError::VerificationFailed(msg) => write!(f, "Verification failed: {}", msg),
            CryptoError::KeyConversionFailed(msg) => write!(f, "Key conversion failed: {}", msg),
            CryptoError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for CryptoError {}

impl CryptoService {
    /// Create a new CryptoService with the given keypair
    pub fn new(keypair: Keypair) -> Self {
        let local_peer_id = PeerId::from(keypair.public());
        Self {
            local_keypair: keypair,
            local_peer_id,
            peer_public_keys: std::collections::HashMap::new(),
        }
    }
    
    /// Add a peer's public key to the cache for encryption
    pub fn add_peer_public_key(&mut self, peer_id: PeerId, public_key: Vec<u8>) {
        self.peer_public_keys.insert(peer_id, public_key);
    }
    
    /// Encrypt message for specific recipient
    pub fn encrypt_message(&self, 
        message: &[u8], 
        recipient_peer_id: &PeerId
    ) -> Result<EncryptedPayload, CryptoError> {
        // Get recipient's public key from cache
        let recipient_public_key = self.peer_public_keys.get(recipient_peer_id)
            .ok_or_else(|| CryptoError::EncryptionFailed(
                format!("Public key not found for peer {}", recipient_peer_id)
            ))?;
        
        // Get our public key
        let our_public_key = self.get_our_public_key()?;
        
        // Create shared secret by combining both public keys
        let shared_secret = self.derive_shared_secret(&our_public_key, recipient_public_key)?;
        
        // Derive encryption key using HKDF
        let hk = Hkdf::<Sha256>::new(None, &shared_secret);
        let mut encryption_key = [0u8; 32];
        hk.expand(b"p2p-play-encryption", &mut encryption_key)
            .map_err(|e| CryptoError::EncryptionFailed(format!("Key derivation failed: {}", e)))?;
        
        // Create cipher instance
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&encryption_key));
        
        // Generate random nonce
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        
        // Encrypt the message
        let encrypted_data = cipher.encrypt(&nonce, message)
            .map_err(|e| CryptoError::EncryptionFailed(format!("Encryption failed: {}", e)))?;
        
        Ok(EncryptedPayload {
            encrypted_data,
            nonce: nonce.to_vec(),
            sender_public_key: our_public_key,
        })
    }
    
    /// Decrypt message intended for local peer
    pub fn decrypt_message(&self, 
        encrypted: &EncryptedPayload
    ) -> Result<Vec<u8>, CryptoError> {
        // Validate nonce size
        if encrypted.nonce.len() != 12 {
            return Err(CryptoError::DecryptionFailed(
                "Invalid nonce size".to_string()
            ));
        }
        
        // Get our public key
        let our_public_key = self.get_our_public_key()?;
        
        // Create shared secret by combining both public keys
        let shared_secret = self.derive_shared_secret(&encrypted.sender_public_key, &our_public_key)?;
        
        // Derive decryption key using HKDF
        let hk = Hkdf::<Sha256>::new(None, &shared_secret);
        let mut decryption_key = [0u8; 32];
        hk.expand(b"p2p-play-encryption", &mut decryption_key)
            .map_err(|e| CryptoError::DecryptionFailed(format!("Key derivation failed: {}", e)))?;
        
        // Create cipher instance
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&decryption_key));
        
        // Create nonce from encrypted payload
        let nonce = Nonce::from_slice(&encrypted.nonce);
        
        // Decrypt the message
        let decrypted_data = cipher.decrypt(nonce, encrypted.encrypted_data.as_ref())
            .map_err(|e| CryptoError::DecryptionFailed(format!("Decryption failed: {}", e)))?;
        
        Ok(decrypted_data)
    }
    
    /// Sign message with local private key
    pub fn sign_message(&self, 
        message: &[u8]
    ) -> Result<MessageSignature, CryptoError> {
        // Get current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Create message to sign (message + timestamp for replay protection)
        let mut message_to_sign = message.to_vec();
        message_to_sign.extend_from_slice(&timestamp.to_be_bytes());
        
        // Sign with our keypair
        let signature = self.local_keypair.sign(&message_to_sign)
            .map_err(|e| CryptoError::SignatureFailed(format!("Signing failed: {}", e)))?;
        
        // Get our public key
        let public_key = self.get_our_public_key()?;
        
        Ok(MessageSignature {
            signature,
            public_key,
            timestamp,
        })
    }
    
    /// Verify message signature
    pub fn verify_signature(&self, 
        message: &[u8], 
        signature: &MessageSignature
    ) -> Result<bool, CryptoError> {
        // Recreate the signed message (message + timestamp)
        let mut message_to_verify = message.to_vec();
        message_to_verify.extend_from_slice(&signature.timestamp.to_be_bytes());
        
        // Convert public key bytes to PublicKey
        let public_key = libp2p::identity::PublicKey::try_decode_protobuf(&signature.public_key)
            .map_err(|e| CryptoError::VerificationFailed(format!("Invalid public key: {}", e)))?;
        
        // Verify signature
        let is_valid = public_key.verify(&message_to_verify, &signature.signature);
        
        Ok(is_valid)
    }
    
    /// Extract public key from PeerId for encryption
    pub fn public_key_from_peer_id(&self, 
        peer_id: &PeerId
    ) -> Result<Vec<u8>, CryptoError> {
        // Check if we have the public key in our cache
        if let Some(public_key) = self.peer_public_keys.get(peer_id) {
            Ok(public_key.clone())
        } else {
            Err(CryptoError::KeyConversionFailed(
                format!("Public key not found for peer {}", peer_id)
            ))
        }
    }
    
    /// Get our own public key in encoded format
    fn get_our_public_key(&self) -> Result<Vec<u8>, CryptoError> {
        Ok(self.local_keypair.public().encode_protobuf())
    }
    
    /// Derive a shared secret from two public keys using a simple hash-based approach
    fn derive_shared_secret(&self, pub_key1: &[u8], pub_key2: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use sha2::{Digest, Sha256};
        
        // Create a deterministic shared secret by hashing the concatenated public keys
        // Sort the keys to ensure the same secret regardless of order
        let mut hasher = Sha256::new();
        
        if pub_key1 < pub_key2 {
            hasher.update(pub_key1);
            hasher.update(pub_key2);
        } else {
            hasher.update(pub_key2);
            hasher.update(pub_key1);
        }
        
        hasher.update(b"p2p-play-shared-secret");
        let result = hasher.finalize();
        Ok(result.to_vec())
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
        assert!(!crypto_service.local_peer_id.to_string().is_empty());
    }

    #[test]
    fn test_message_signing_and_verification() {
        let keypair = create_test_keypair();
        let crypto_service = CryptoService::new(keypair);
        
        let message = b"Hello, World!";
        
        // Sign the message
        let signature = crypto_service.sign_message(message).unwrap();
        
        // Verify the signature
        let is_valid = crypto_service.verify_signature(message, &signature).unwrap();
        assert!(is_valid);
        
        // Verify that tampering with message fails verification
        let tampered_message = b"Hello, World?";
        let is_tampered_valid = crypto_service.verify_signature(tampered_message, &signature).unwrap();
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
        let bob_peer_id = bob_crypto.local_peer_id;
        let bob_public_key = bob_keypair.public().encode_protobuf();
        alice_crypto.add_peer_public_key(bob_peer_id, bob_public_key);
        
        // Add Alice's public key to Bob's cache  
        let alice_peer_id = alice_crypto.local_peer_id;
        let alice_public_key = alice_crypto.local_keypair.public().encode_protobuf();
        bob_crypto.add_peer_public_key(alice_peer_id, alice_public_key);
        
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
        let mut crypto_service = CryptoService::new(keypair);
        
        let peer_id = PeerId::random();
        let public_key = b"test_public_key".to_vec();
        
        // Initially, peer should not be found
        assert!(crypto_service.public_key_from_peer_id(&peer_id).is_err());
        
        // Add peer public key
        crypto_service.add_peer_public_key(peer_id, public_key.clone());
        
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
            other => panic!("Unexpected error: {:?}", other),
        }
    }
}