use crate::errors::NetworkResult;
use crate::network::P2PBehaviour;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};
use log::{debug, warn, error};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Bootstrap configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    pub peers: Vec<String>,
    pub retry_interval_secs: u64,
    pub max_retries: u32,
    pub initial_delay_secs: u64,
    pub backoff_multiplier: f32,
    pub enabled: bool,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            peers: vec![],
            retry_interval_secs: 30,
            max_retries: 10,
            initial_delay_secs: 5,
            backoff_multiplier: 1.5,
            enabled: true,
        }
    }
}

impl BootstrapConfig {
    pub fn retry_interval(&self) -> Duration {
        Duration::from_secs(self.retry_interval_secs)
    }

    pub fn initial_delay(&self) -> Duration {
        Duration::from_secs(self.initial_delay_secs)
    }
}

/// Bootstrap error types
#[derive(Error, Debug, Clone)]
pub enum BootstrapError {
    #[error("Invalid multiaddr: {0}")]
    InvalidMultiaddr(String),
    #[error("No bootstrap peers configured")]
    NoPeersConfigured,
    #[error("Max retries exceeded: {0}")]
    MaxRetriesExceeded(u32),
    #[error("Bootstrap disabled")]
    Disabled,
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

/// Bootstrap status tracking
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BootstrapStatus {
    #[default]
    NotStarted,
    InProgress {
        attempts: u32,
        last_attempt: Instant,
    },
    Connected {
        peer_count: usize,
        connected_at: Instant,
    },
    Failed {
        attempts: u32,
        last_error: String,
    },
}

/// Bootstrap service for automatic peer discovery
pub struct BootstrapService {
    pub status: Arc<Mutex<BootstrapStatus>>,
    config: BootstrapConfig,
    retry_count: Arc<Mutex<u32>>,
    next_retry_time: Arc<Mutex<Option<Instant>>>,
}

impl BootstrapService {
    /// Create a new bootstrap service
    pub fn new(config: BootstrapConfig) -> Self {
        Self {
            status: Arc::new(Mutex::new(BootstrapStatus::NotStarted)),
            config,
            retry_count: Arc::new(Mutex::new(0)),
            next_retry_time: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if bootstrap is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get current status
    pub fn get_status(&self) -> BootstrapStatus {
        self.status.lock().unwrap().clone()
    }

    /// Attempt to bootstrap connections
    pub async fn attempt_bootstrap(&self, swarm: &mut Swarm<P2PBehaviour>) -> NetworkResult<()> {
        if !self.config.enabled {
            return Err("Bootstrap is disabled".into());
        }

        if self.config.peers.is_empty() {
            return Err("No bootstrap peers configured".into());
        }

        let retry_count = {
            let mut count = self.retry_count.lock().unwrap();
            *count += 1;
            *count
        };

        debug!("Bootstrap attempt #{} starting", retry_count);

        {
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::InProgress {
                attempts: retry_count,
                last_attempt: Instant::now(),
            };
        }

        let mut success_count = 0;
        let mut last_error = String::new();

        for peer_str in &self.config.peers {
            match self.connect_to_bootstrap_peer(swarm, peer_str).await {
                Ok(_) => {
                    success_count += 1;
                    debug!("Successfully connected to bootstrap peer: {}", peer_str);
                }
                Err(e) => {
                    warn!("Failed to connect to bootstrap peer {}: {}", peer_str, e);
                    last_error = e.to_string();
                }
            }
        }

        if success_count > 0 {
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::Connected {
                peer_count: success_count,
                connected_at: Instant::now(),
            };
            debug!("Bootstrap successful: connected to {} peers", success_count);
            Ok(())
        } else {
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: retry_count,
                last_error: last_error.clone(),
            };
            Err(format!("Bootstrap failed after {} attempts: {}", retry_count, last_error).into())
        }
    }

    /// Connect to a specific bootstrap peer
    async fn connect_to_bootstrap_peer(
        &self,
        swarm: &mut Swarm<P2PBehaviour>,
        peer_str: &str,
    ) -> Result<(), BootstrapError> {
        let multiaddr = Multiaddr::from_str(peer_str)
            .map_err(|e| BootstrapError::InvalidMultiaddr(format!("{}: {}", peer_str, e)))?;

        debug!("Attempting to dial bootstrap peer: {}", multiaddr);

        // Extract peer ID from multiaddr if available
        if let Some(peer_id) = extract_peer_id_from_multiaddr(&multiaddr) {
            swarm
                .dial(peer_id)
                .map_err(|e| BootstrapError::ConnectionFailed(e.to_string()))?;
        } else {
            swarm
                .dial(multiaddr.clone())
                .map_err(|e| BootstrapError::ConnectionFailed(e.to_string()))?;
        }

        Ok(())
    }

    /// Check if it's time for a retry
    pub fn should_retry(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        let retry_count = *self.retry_count.lock().unwrap();
        if retry_count >= self.config.max_retries {
            return false;
        }

        let next_retry_time = self.next_retry_time.lock().unwrap();
        match *next_retry_time {
            Some(time) => Instant::now() >= time,
            None => true,
        }
    }

    /// Schedule next retry
    pub fn schedule_next_retry(&self) {
        let retry_count = *self.retry_count.lock().unwrap();
        let delay = self.calculate_retry_delay(retry_count);
        let next_time = Instant::now() + delay;

        *self.next_retry_time.lock().unwrap() = Some(next_time);
        debug!("Next bootstrap retry scheduled in {:?}", delay);
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, retry_count: u32) -> Duration {
        let base_delay = self.config.retry_interval();
        let backoff_factor = self.config.backoff_multiplier.powi(retry_count.saturating_sub(1) as i32);
        let delay_secs = (base_delay.as_secs() as f32 * backoff_factor) as u64;
        Duration::from_secs(delay_secs.min(300)) // Cap at 5 minutes
    }

    /// Reset retry state
    pub fn reset(&self) {
        *self.retry_count.lock().unwrap() = 0;
        *self.next_retry_time.lock().unwrap() = None;
        *self.status.lock().unwrap() = BootstrapStatus::NotStarted;
    }
}

/// Extract peer ID from multiaddr
fn extract_peer_id_from_multiaddr(multiaddr: &Multiaddr) -> Option<PeerId> {
    for protocol in multiaddr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer_id) = protocol {
            return Some(peer_id);
        }
    }
    None
}