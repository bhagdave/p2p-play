use crate::crypto::{CryptoService, CryptoError};
use crate::errors::NetworkResult;
use crate::types::{RelayMessage, RelayConfig, DirectMessage};
use libp2p::PeerId;
use log::{debug, warn, error};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Relay service error types
#[derive(Error, Debug, Clone)]
pub enum RelayError {
    #[error("Crypto error: {0}")]
    CryptoError(#[from] CryptoError),
    #[error("Target peer not found: {0}")]
    TargetPeerNotFound(String),
    #[error("Message too large: {size} > {max}")]
    MessageTooLarge { size: usize, max: usize },
    #[error("TTL exceeded")]
    TtlExceeded,
    #[error("Rate limit exceeded for peer: {0}")]
    RateLimitExceeded(String),
    #[error("Relay disabled")]
    RelayDisabled,
    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),
}

/// Statistics for relay operations
#[derive(Debug, Default, Clone)]
pub struct RelayStats {
    pub messages_relayed: u64,
    pub messages_dropped: u64,
    pub rate_limit_hits: u64,
    pub crypto_errors: u64,
}

/// Rate limiting state per peer
#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// Relay service for forwarding messages between peers
pub struct RelayService {
    crypto: CryptoService,
    config: RelayConfig,
    stats: Arc<Mutex<RelayStats>>,
    rate_limits: Arc<Mutex<HashMap<String, RateLimitEntry>>>,
}

impl RelayService {
    /// Create a new relay service
    pub fn new(config: RelayConfig, crypto: CryptoService) -> Self {
        Self {
            crypto,
            config,
            stats: Arc::new(Mutex::new(RelayStats::default())),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create an encrypted relay message
    pub fn create_relay_message(
        &self,
        direct_message: &DirectMessage,
        target_peer_id: &PeerId,
    ) -> Result<RelayMessage, RelayError> {
        // Serialize the direct message
        let message_bytes = serde_json::to_vec(direct_message)
            .map_err(|e| RelayError::InvalidMessageFormat(e.to_string()))?;

        // Check message size
        if message_bytes.len() > self.config.max_message_size {
            return Err(RelayError::MessageTooLarge {
                size: message_bytes.len(),
                max: self.config.max_message_size,
            });
        }

        // Encrypt the message
        let encrypted_payload = self.crypto.encrypt_message(&message_bytes, target_peer_id)?;

        // Create signature
        let signature = self.crypto.sign_message(&message_bytes)?;

        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(RelayMessage {
            target_peer_id: target_peer_id.to_string(),
            encrypted_payload,
            signature,
            timestamp,
            message_type: "direct_message".to_string(),
            relay_ttl: self.config.max_ttl,
        })
    }

    /// Process a received relay message
    pub fn process_relay_message(&self, relay_msg: &RelayMessage) -> NetworkResult<Option<DirectMessage>> {
        // Check TTL
        if relay_msg.relay_ttl == 0 {
            debug!("Dropping relay message with expired TTL");
            self.increment_stat(|stats| stats.messages_dropped += 1);
            return Ok(None);
        }

        // Try to decrypt the message (only succeeds if we're the target)
        match self.decrypt_relay_message(relay_msg) {
            Ok(decrypted_message) => {
                debug!("Successfully decrypted relay message for local peer");
                self.increment_stat(|stats| stats.messages_relayed += 1);
                Ok(Some(decrypted_message))
            }
            Err(CryptoError::DecryptionFailed(_)) => {
                // Not for us, continue relaying if TTL allows
                debug!("Message not for us, continuing relay");
                Ok(None)
            }
            Err(e) => {
                warn!("Crypto error processing relay message: {}", e);
                self.increment_stat(|stats| stats.crypto_errors += 1);
                Err(format!("Crypto error: {}", e).into())
            }
        }
    }

    /// Decrypt a relay message (only works if we're the target)
    fn decrypt_relay_message(&self, relay_msg: &RelayMessage) -> Result<DirectMessage, CryptoError> {
        // Extract the sender's peer ID from the encrypted payload
        let _sender_peer_id = PeerId::try_from(relay_msg.encrypted_payload.sender_public_key.clone())
            .map_err(|e| CryptoError::DecryptionFailed(format!("Invalid sender peer ID: {:?}", e)))?;

        // Decrypt the message
        let decrypted_bytes = self.crypto.decrypt_message(&relay_msg.encrypted_payload)?;

        // Verify signature
        match self.crypto.verify_signature(&decrypted_bytes, &relay_msg.signature) {
            Ok(true) => {}, // Signature valid, continue
            Ok(false) => {
                return Err(CryptoError::VerificationFailed(
                    "Relay message signature verification failed".to_string(),
                ));
            },
            Err(e) => return Err(e),
        }

        // Deserialize the direct message
        let direct_message: DirectMessage = serde_json::from_slice(&decrypted_bytes)
            .map_err(|e| CryptoError::DecryptionFailed(format!("Failed to deserialize message: {}", e)))?;

        Ok(direct_message)
    }

    /// Create a forwarded relay message with decremented TTL
    pub fn create_forwarded_message(&self, original: &RelayMessage) -> Option<RelayMessage> {
        if original.relay_ttl <= 1 {
            debug!("Cannot forward message with TTL <= 1");
            return None;
        }

        let mut forwarded = original.clone();
        forwarded.relay_ttl = original.relay_ttl - 1;
        forwarded.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        debug!("Created forwarded message with TTL {}", forwarded.relay_ttl);
        Some(forwarded)
    }

    /// Check if we can forward messages (rate limiting)
    pub fn can_forward(&self, from_peer: &str) -> bool {
        self.check_rate_limit(from_peer)
    }

    /// Check rate limiting for a peer
    fn check_rate_limit(&self, peer_id: &str) -> bool {
        let mut rate_limits = self.rate_limits.lock().unwrap();
        let now = Instant::now();
        let window_duration = Duration::from_secs(60); // 1 minute window
        let max_per_window = 10; // Max 10 relays per minute per peer

        match rate_limits.get_mut(peer_id) {
            Some(entry) => {
                // Check if we're in a new window
                if now.duration_since(entry.window_start) > window_duration {
                    entry.count = 1;
                    entry.window_start = now;
                    true
                } else if entry.count < max_per_window {
                    entry.count += 1;
                    true
                } else {
                    self.increment_stat(|stats| stats.rate_limit_hits += 1);
                    false
                }
            }
            None => {
                rate_limits.insert(peer_id.to_string(), RateLimitEntry {
                    count: 1,
                    window_start: now,
                });
                true
            }
        }
    }

    /// Update relay configuration
    pub fn update_config(&mut self, new_config: RelayConfig) {
        debug!("Updating relay configuration: {:?}", new_config);
        self.config = new_config;
    }

    /// Get relay statistics
    pub fn get_stats(&self) -> RelayStats {
        self.stats.lock().unwrap().clone()
    }

    /// Reset relay statistics
    pub fn reset_stats(&self) {
        *self.stats.lock().unwrap() = RelayStats::default();
    }

    /// Helper to increment statistics
    fn increment_stat<F>(&self, f: F)
    where
        F: FnOnce(&mut RelayStats),
    {
        let mut stats = self.stats.lock().unwrap();
        f(&mut stats);
    }

    /// Clean up old rate limit entries
    pub fn cleanup_rate_limits(&self) {
        let mut rate_limits = self.rate_limits.lock().unwrap();
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(3600); // 1 hour

        rate_limits.retain(|_, entry| {
            now.duration_since(entry.window_start) < cleanup_threshold
        });
    }
}

impl RelayConfig {
    /// Validate relay configuration
    pub fn validate(&self) -> NetworkResult<()> {
        if self.max_ttl == 0 {
            return Err("Max TTL must be greater than 0".into());
        }
        if self.max_message_size == 0 {
            return Err("Max message size must be greater than 0".into());
        }
        if self.relay_timeout_secs == 0 {
            return Err("Relay timeout must be greater than 0".into());
        }
        Ok(())
    }

    /// Get relay timeout as Duration
    pub fn relay_timeout(&self) -> Duration {
        Duration::from_secs(self.relay_timeout_secs)
    }
}