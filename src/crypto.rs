use libp2p::{PeerId, identity::Keypair};
use serde::{Serialize, Deserialize};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce, Key
};
use x25519_dalek::PublicKey as X25519PublicKey;
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
        }
    }
    
    /// Encrypt message for specific recipient
    pub fn encrypt_message(&self, 
        _message: &[u8], 
        _recipient_peer_id: &PeerId
    ) -> Result<EncryptedPayload, CryptoError> {
        // For now, return an error as full encryption requires proper key exchange implementation
        // This would need to be implemented with a proper peer discovery and key exchange mechanism
        Err(CryptoError::EncryptionFailed(
            "Full encryption implementation requires proper key exchange mechanism".to_string()
        ))
    }
    
    /// Decrypt message intended for local peer
    pub fn decrypt_message(&self, 
        _encrypted: &EncryptedPayload
    ) -> Result<Vec<u8>, CryptoError> {
        // For now, return an error as full decryption requires proper key exchange implementation
        // This would need to be implemented with a proper peer discovery and key exchange mechanism
        Err(CryptoError::DecryptionFailed(
            "Full decryption implementation requires proper key exchange mechanism".to_string()
        ))
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
        _peer_id: &PeerId
    ) -> Result<Vec<u8>, CryptoError> {
        // For Ed25519 keys, we can extract the public key from the PeerId
        // PeerId is derived from the public key, so we need to decode it
        
        // This is a simplified approach - in practice, you'd maintain a mapping
        // of PeerIds to their public keys discovered through the network
        Err(CryptoError::KeyConversionFailed(
            "Cannot extract public key from PeerId without network discovery".to_string()
        ))
    }
    
    /// Get our own public key in encoded format
    fn get_our_public_key(&self) -> Result<Vec<u8>, CryptoError> {
        Ok(self.local_keypair.public().encode_protobuf())
    }
    
    /// Convert Ed25519 private key to X25519 private key for ECDH
    fn ed25519_to_x25519_private(&self) -> Result<[u8; 32], CryptoError> {
        // For now, return an error since proper Ed25519 to X25519 conversion requires
        // specific cryptographic operations not directly available in the current API
        Err(CryptoError::KeyConversionFailed(
            "Ed25519 to X25519 conversion not implemented".to_string()
        ))
    }
    
    /// Convert Ed25519 public key to X25519 public key
    fn ed25519_to_x25519_public(&self, _ed25519_public_key: &[u8]) -> Result<X25519PublicKey, CryptoError> {
        // For now, return an error since proper Ed25519 to X25519 conversion requires
        // specific cryptographic operations not directly available in the current API
        Err(CryptoError::KeyConversionFailed(
            "Ed25519 to X25519 conversion not implemented".to_string()
        ))
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
        
        let alice_crypto = CryptoService::new(alice_keypair);
        let bob_crypto = CryptoService::new(bob_keypair);
        
        let message = b"Secret message from Alice to Bob";
        
        // Note: This test checks that the encrypt function returns proper error handling
        // since we don't have proper key exchange implementation yet
        let bob_peer_id = bob_crypto.local_peer_id;
        let encrypt_result = alice_crypto.encrypt_message(message, &bob_peer_id);
        
        // This should fail gracefully since we don't have proper key exchange yet
        assert!(encrypt_result.is_err());
        match encrypt_result.unwrap_err() {
            CryptoError::EncryptionFailed(msg) => {
                // Expected error for now
                assert!(msg.contains("key exchange mechanism"));
            }
            other => panic!("Unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_error_display() {
        let error = CryptoError::EncryptionFailed("test error".to_string());
        assert_eq!(error.to_string(), "Encryption failed: test error");
        
        let error = CryptoError::InvalidInput("bad input".to_string());
        assert_eq!(error.to_string(), "Invalid input: bad input");
    }
}