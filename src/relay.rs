use crate::crypto::{CryptoError, CryptoService};
use crate::types::{DirectMessage, RelayConfig, RelayConfirmation, RelayMessage};
use libp2p::PeerId;
use log::warn;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub struct RelayService {
    config: RelayConfig,
    crypto: CryptoService,
    pending_confirmations: HashMap<String, Instant>, // Message ID -> Send time
    rate_limits: HashMap<PeerId, Vec<Instant>>,      // Rate limiting per peer
}

#[derive(Debug)]
pub enum RelayAction {
    DeliverLocally(DirectMessage),
    ForwardMessage(RelayMessage), 
    DropMessage(String),        
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
            RelayError::EncryptionFailed(msg) => write!(f, "Relay encryption failed: {msg}"),
            RelayError::DecryptionFailed(msg) => write!(f, "Relay decryption failed: {msg}"),
            RelayError::RateLimitExceeded => write!(f, "Rate limit exceeded for relay"),
            RelayError::MaxHopsExceeded => write!(f, "Maximum hop count exceeded"),
            RelayError::InvalidMessage(msg) => write!(f, "Invalid relay message: {msg}"),
            RelayError::CryptoError(err) => write!(f, "Crypto error in relay: {err}"),
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
    pub fn new(config: RelayConfig, crypto: CryptoService) -> Self {
        Self {
            config,
            crypto,
            pending_confirmations: HashMap::new(),
            rate_limits: HashMap::new(),
        }
    }

    pub fn create_relay_message(
        &mut self,
        direct_msg: &DirectMessage,
        target_peer_id: &PeerId,
    ) -> Result<RelayMessage, RelayError> {
        if !self.check_rate_limit(&self.crypto.local_peer_id()) {
            return Err(RelayError::RateLimitExceeded);
        }

        let message_id = Uuid::new_v4().to_string();

        let message_bytes = serde_json::to_vec(direct_msg)
            .map_err(|e| RelayError::InvalidMessage(format!("Failed to serialize message: {e}")))?;

        let encrypted_payload = self
            .crypto
            .encrypt_message(&message_bytes, target_peer_id)?;

        let sender_signature = self.crypto.sign_message(&message_bytes)?;

        let relay_message = RelayMessage::new(
            message_id.clone(),
            target_peer_id.to_string(),
            direct_msg.to_name.clone(),
            encrypted_payload,
            sender_signature,
            self.config.max_hops,
        );

        self.pending_confirmations
            .insert(message_id, Instant::now());

        self.update_rate_limit(&self.crypto.local_peer_id());

        Ok(relay_message)
    }

    pub fn process_relay_message(
        &mut self,
        relay_msg: &RelayMessage,
    ) -> Result<RelayAction, RelayError> {

        if relay_msg.hop_count >= relay_msg.max_hops {
            return Ok(RelayAction::DropMessage(format!(
                "Max hops exceeded: {}/{}",
                relay_msg.hop_count, relay_msg.max_hops
            )));
        }

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if current_time.saturating_sub(relay_msg.timestamp) > 300 {
            return Ok(RelayAction::DropMessage("Message too old".to_string()));
        }

        match self.crypto.decrypt_message(&relay_msg.encrypted_payload) {
            Ok(decrypted_bytes) => {
                match serde_json::from_slice::<DirectMessage>(&decrypted_bytes) {
                    Ok(direct_msg) => {
                        match self
                            .crypto
                            .verify_signature(&decrypted_bytes, &relay_msg.sender_signature)
                        {
                            Ok(true) => {
                                Ok(RelayAction::DeliverLocally(direct_msg))
                            }
                            Ok(false) => {
                                warn!("Signature verification failed for relay message");
                                Ok(RelayAction::DropMessage("Invalid signature".to_string()))
                            }
                            Err(e) => {
                                warn!("Signature verification error: {e}");
                                Ok(RelayAction::DropMessage(format!("Signature error: {e}")))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to deserialize decrypted relay message: {e}");
                        Ok(RelayAction::DropMessage(
                            "Invalid decrypted message format".to_string(),
                        ))
                    }
                }
            }
            Err(_) => {
                if self.should_forward(relay_msg) {
                    let mut forwarded_msg = relay_msg.clone();
                    if let Err(e) = forwarded_msg.increment_hop_count() {
                        return Ok(RelayAction::DropMessage(e));
                    }
                    Ok(RelayAction::ForwardMessage(forwarded_msg))
                } else {
                    Ok(RelayAction::DropMessage("Forwarding disabled".to_string()))
                }
            }
        }
    }

    pub fn should_forward(&self, relay_msg: &RelayMessage) -> bool {
        if !self.config.enable_forwarding {
            return false;
        }

        if !relay_msg.can_forward() {
            return false;
        }

        true
    }

    pub fn check_rate_limit(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(60); // 1 minute window

        self.cleanup_rate_limits(now, window);

        let entries = self.rate_limits.entry(*peer_id).or_default();

        entries.retain(|&timestamp| now.duration_since(timestamp) < window);

        entries.len() < self.config.rate_limit_per_peer as usize
    }

    fn update_rate_limit(&mut self, peer_id: &PeerId) {
        let now = Instant::now();
        let entries = self.rate_limits.entry(*peer_id).or_default();
        entries.push(now);
    }

    fn cleanup_rate_limits(&mut self, now: Instant, window: Duration) {
        self.rate_limits.retain(|_, entries| {
            entries.retain(|&timestamp| now.duration_since(timestamp) < window);
            !entries.is_empty()
        });
    }

    pub fn cleanup_pending_confirmations(&mut self) {
        let timeout = Duration::from_millis(self.config.relay_timeout_ms);
        let now = Instant::now();

        self.pending_confirmations
            .retain(|_, &mut timestamp| now.duration_since(timestamp) < timeout);
    }

    pub fn config(&self) -> &RelayConfig {
        &self.config
    }

    pub fn crypto_service(&mut self) -> &mut CryptoService {
        &mut self.crypto
    }

    #[cfg(test)]
    pub fn crypto_service_for_testing(&mut self) -> &mut CryptoService {
        &mut self.crypto
    }

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
