use crate::crypto::{CryptoService, CryptoError};
use crate::types::{DirectMessage, RelayConfig, RelayMessage, RelayConfirmation};
use libp2p::PeerId;
use log::{debug, warn};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Service for handling message relay operations
pub struct RelayService {
    config: RelayConfig,
    crypto: CryptoService,
    pending_confirmations: HashMap<String, Instant>, // Message ID -> Send time
    rate_limits: HashMap<PeerId, Vec<Instant>>,      // Rate limiting per peer
}

/// Actions to take after processing relay message
#[derive(Debug)]
pub enum RelayAction {
    DeliverLocally(DirectMessage),     // Message is for us
    ForwardMessage(RelayMessage),      // Forward to other peers  
    DropMessage(String),               // Drop with reason
}

#[derive(Debug)]
pub enum RelayError {
    EncryptionFailed(String),
    DecryptionFailed(String),
    RateLimitExceeded,
    MaxHopsExceeded,
    InvalidMessage(String),
    CryptoError(CryptoError),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::EncryptionFailed(msg) => write!(f, "Relay encryption failed: {}", msg),
            RelayError::DecryptionFailed(msg) => write!(f, "Relay decryption failed: {}", msg),
            RelayError::RateLimitExceeded => write!(f, "Rate limit exceeded for relay"),
            RelayError::MaxHopsExceeded => write!(f, "Maximum hop count exceeded"),
            RelayError::InvalidMessage(msg) => write!(f, "Invalid relay message: {}", msg),
            RelayError::CryptoError(err) => write!(f, "Crypto error in relay: {}", err),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<CryptoError> for RelayError {
    fn from(error: CryptoError) -> Self {
        RelayError::CryptoError(error)
    }
}

impl RelayService {
    /// Create a new RelayService with the given configuration and crypto service
    pub fn new(config: RelayConfig, crypto: CryptoService) -> Self {
        Self {
            config,
            crypto,
            pending_confirmations: HashMap::new(),
            rate_limits: HashMap::new(),
        }
    }

    /// Create relay message from direct message
    pub fn create_relay_message(
        &mut self,
        direct_msg: &DirectMessage,
        target_peer_id: &PeerId,
    ) -> Result<RelayMessage, RelayError> {
        // Check rate limiting first
        if !self.check_rate_limit(&self.crypto.local_peer_id()) {
            return Err(RelayError::RateLimitExceeded);
        }

        // Generate unique message ID
        let message_id = Uuid::new_v4().to_string();

        // Serialize the direct message for encryption
        let message_bytes = serde_json::to_vec(direct_msg)
            .map_err(|e| RelayError::InvalidMessage(format!("Failed to serialize message: {}", e)))?;

        // Encrypt the message for the target recipient
        let encrypted_payload = self
            .crypto
            .encrypt_message(&message_bytes, target_peer_id)?;

        // Sign the message with our private key
        let sender_signature = self
            .crypto
            .sign_message(&message_bytes)?;

        let relay_message = RelayMessage::new(
            message_id.clone(),
            target_peer_id.to_string(),
            direct_msg.to_name.clone(),
            encrypted_payload,
            sender_signature,
            self.config.max_hops,
        );

        // Track the message for confirmation
        self.pending_confirmations.insert(message_id, Instant::now());

        // Update rate limiting
        self.update_rate_limit(&self.crypto.local_peer_id());

        debug!("Created relay message with ID: {}", relay_message.message_id);
        Ok(relay_message)
    }

    /// Process incoming relay message (decrypt if for us, forward if not)
    pub fn process_relay_message(
        &mut self,
        relay_msg: &RelayMessage,
    ) -> Result<RelayAction, RelayError> {
        debug!("Processing relay message ID: {}", relay_msg.message_id);

        // Check if message has exceeded maximum hops
        if relay_msg.hop_count >= relay_msg.max_hops {
            return Ok(RelayAction::DropMessage(format!(
                "Max hops exceeded: {}/{}",
                relay_msg.hop_count, relay_msg.max_hops
            )));
        }

        // Check if message is too old (basic replay protection)
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        if current_time.saturating_sub(relay_msg.timestamp) > 300 { // 5 minutes
            return Ok(RelayAction::DropMessage("Message too old".to_string()));
        }

        // Try to decrypt the message to see if it's intended for us
        match self.crypto.decrypt_message(&relay_msg.encrypted_payload) {
            Ok(decrypted_bytes) => {
                // Successfully decrypted - this message is for us
                match serde_json::from_slice::<DirectMessage>(&decrypted_bytes) {
                    Ok(direct_msg) => {
                        // Verify the signature to ensure authenticity
                        match self.crypto.verify_signature(&decrypted_bytes, &relay_msg.sender_signature) {
                            Ok(true) => {
                                debug!("Successfully decrypted relay message for local delivery");
                                Ok(RelayAction::DeliverLocally(direct_msg))
                            }
                            Ok(false) => {
                                warn!("Signature verification failed for relay message");
                                Ok(RelayAction::DropMessage("Invalid signature".to_string()))
                            }
                            Err(e) => {
                                warn!("Signature verification error: {}", e);
                                Ok(RelayAction::DropMessage(format!("Signature error: {}", e)))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to deserialize decrypted relay message: {}", e);
                        Ok(RelayAction::DropMessage("Invalid decrypted message format".to_string()))
                    }
                }
            }
            Err(_) => {
                // Failed to decrypt - this message is not for us
                // Check if we should forward it
                if self.should_forward(relay_msg) {
                    // Create forwarded message with incremented hop count
                    let mut forwarded_msg = relay_msg.clone();
                    if let Err(e) = forwarded_msg.increment_hop_count() {
                        return Ok(RelayAction::DropMessage(e));
                    }
                    
                    debug!("Forwarding relay message (hop: {})", forwarded_msg.hop_count);
                    Ok(RelayAction::ForwardMessage(forwarded_msg))
                } else {
                    Ok(RelayAction::DropMessage("Forwarding disabled".to_string()))
                }
            }
        }
    }

    /// Check if we should forward a message
    pub fn should_forward(&self, relay_msg: &RelayMessage) -> bool {
        // Don't forward if forwarding is disabled
        if !self.config.enable_forwarding {
            return false;
        }

        // Don't forward if message has reached max hops
        if !relay_msg.can_forward() {
            return false;
        }

        // Basic rate limiting for forwarding (could be more sophisticated)
        // For now, we allow forwarding if the service is configured to do so
        true
    }

    /// Apply rate limiting checks
    pub fn check_rate_limit(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(60); // 1 minute window
        
        // Clean up old entries
        self.cleanup_rate_limits(now, window);
        
        // Get or create entry for this peer
        let entries = self.rate_limits.entry(*peer_id).or_insert_with(Vec::new);
        
        // Remove entries outside the time window
        entries.retain(|&timestamp| now.duration_since(timestamp) < window);
        
        // Check if under the limit
        entries.len() < self.config.rate_limit_per_peer as usize
    }

    /// Update rate limiting for a peer
    fn update_rate_limit(&mut self, peer_id: &PeerId) {
        let now = Instant::now();
        let entries = self.rate_limits.entry(*peer_id).or_insert_with(Vec::new);
        entries.push(now);
    }

    /// Clean up old rate limiting entries
    fn cleanup_rate_limits(&mut self, now: Instant, window: Duration) {
        self.rate_limits.retain(|_, entries| {
            entries.retain(|&timestamp| now.duration_since(timestamp) < window);
            !entries.is_empty()
        });
    }

    /// Generate delivery confirmation
    pub fn create_confirmation(
        &self,
        message_id: &str,
        hop_count: u8,
    ) -> RelayConfirmation {
        RelayConfirmation::new(
            message_id.to_string(),
            self.crypto.local_peer_id().to_string(),
            hop_count,
        )
    }

    /// Clean up pending confirmations that have timed out
    pub fn cleanup_pending_confirmations(&mut self) {
        let timeout = Duration::from_millis(self.config.relay_timeout_ms);
        let now = Instant::now();
        
        self.pending_confirmations.retain(|_, &mut timestamp| {
            now.duration_since(timestamp) < timeout
        });
    }

    /// Get current configuration
    pub fn config(&self) -> &RelayConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config(&mut self, new_config: RelayConfig) -> Result<(), String> {
        // Validate the new configuration
        new_config.validate()?;
        
        self.config = new_config;
        debug!("Updated relay configuration");
        Ok(())
    }

    /// Get access to the crypto service
    pub fn crypto_service(&mut self) -> &mut CryptoService {
        &mut self.crypto
    }

    /// Get access to the crypto service for testing
    #[cfg(test)]
    pub fn crypto_service_for_testing(&mut self) -> &mut CryptoService {
        &mut self.crypto
    }

    /// Update rate limiting for testing
    #[cfg(test)]
    pub fn test_update_rate_limit(&mut self, peer_id: &PeerId) {
        self.update_rate_limit(peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoService;
    use libp2p::identity::Keypair;

    fn create_test_crypto_service() -> CryptoService {
        let keypair = Keypair::generate_ed25519();
        CryptoService::new(keypair)
    }

    #[test]
    fn test_relay_service_creation() {
        let config = RelayConfig::new();
        let crypto = create_test_crypto_service();
        let relay_service = RelayService::new(config, crypto);

        assert!(relay_service.config.enable_relay);
        assert!(relay_service.config.enable_forwarding);
        assert_eq!(relay_service.config.max_hops, 3);
    }

    #[test]
    fn test_rate_limiting() {
        let config = RelayConfig {
            rate_limit_per_peer: 2,
            ..RelayConfig::new()
        };
        let crypto = create_test_crypto_service();
        let mut relay_service = RelayService::new(config, crypto);

        let peer_id = PeerId::random();

        // Should allow first two attempts
        assert!(relay_service.check_rate_limit(&peer_id));
        relay_service.update_rate_limit(&peer_id);

        assert!(relay_service.check_rate_limit(&peer_id));
        relay_service.update_rate_limit(&peer_id);

        // Third attempt should be rejected
        assert!(!relay_service.check_rate_limit(&peer_id));
    }

    #[test]
    fn test_relay_config_validation() {
        let mut config = RelayConfig::new();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid max_hops should fail
        config.max_hops = 0;
        assert!(config.validate().is_err());

        config.max_hops = 15; // Too high
        assert!(config.validate().is_err());

        // Reset and test other fields
        config = RelayConfig::new();
        config.relay_timeout_ms = 0;
        assert!(config.validate().is_err());

        config = RelayConfig::new();
        config.rate_limit_per_peer = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_forward_logic() {
        let config = RelayConfig::new();
        let crypto = create_test_crypto_service();
        let relay_service = RelayService::new(config, crypto);

        // Create a relay message that can be forwarded
        let relay_msg = RelayMessage {
            message_id: "test-id".to_string(),
            target_peer_id: "target".to_string(),
            target_name: "target_user".to_string(),
            encrypted_payload: crate::crypto::EncryptedPayload {
                encrypted_data: vec![1, 2, 3],
                nonce: vec![4, 5, 6],
                sender_public_key: vec![7, 8, 9],
            },
            sender_signature: crate::crypto::MessageSignature {
                signature: vec![10, 11, 12],
                public_key: vec![13, 14, 15],
                timestamp: 1000,
            },
            hop_count: 1,
            max_hops: 3,
            timestamp: 1000,
            relay_attempt: true,
        };

        // Should allow forwarding
        assert!(relay_service.should_forward(&relay_msg));

        // Test with forwarding disabled
        let config_no_forward = RelayConfig {
            enable_forwarding: false,
            ..RelayConfig::new()
        };
        let crypto2 = create_test_crypto_service();
        let relay_service_no_forward = RelayService::new(config_no_forward, crypto2);
        assert!(!relay_service_no_forward.should_forward(&relay_msg));

        // Test with max hops reached
        let relay_msg_max_hops = RelayMessage {
            hop_count: 3,
            max_hops: 3,
            ..relay_msg
        };
        assert!(!relay_service.should_forward(&relay_msg_max_hops));
    }

    #[test]
    fn test_cleanup_pending_confirmations() {
        let config = RelayConfig {
            relay_timeout_ms: 100, // Very short timeout for testing
            ..RelayConfig::new()
        };
        let crypto = create_test_crypto_service();
        let mut relay_service = RelayService::new(config, crypto);

        // Add a pending confirmation
        relay_service.pending_confirmations.insert(
            "test-id".to_string(),
            Instant::now() - Duration::from_millis(200), // Already expired
        );

        assert_eq!(relay_service.pending_confirmations.len(), 1);

        // Cleanup should remove expired confirmations
        relay_service.cleanup_pending_confirmations();
        assert_eq!(relay_service.pending_confirmations.len(), 0);
    }
}