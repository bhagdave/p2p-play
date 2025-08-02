use crate::bootstrap_logger::BootstrapLogger;
use crate::handlers::extract_peer_id_from_multiaddr;
use crate::network::StoryBehaviour;
use crate::types::BootstrapConfig;
use libp2p::swarm::Swarm;
use log::{debug, warn};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

pub struct AutoBootstrap {
    pub status: Arc<Mutex<BootstrapStatus>>,
    config: Option<BootstrapConfig>,
    retry_count: Arc<Mutex<u32>>,
    next_retry_time: Arc<Mutex<Option<Instant>>>,
}

impl AutoBootstrap {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(BootstrapStatus::NotStarted)),
            config: None,
            retry_count: Arc::new(Mutex::new(0)),
            next_retry_time: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the auto bootstrap with config
    pub async fn initialize(
        &mut self,
        bootstrap_config: &BootstrapConfig,
        bootstrap_logger: &BootstrapLogger,
        _error_logger: &crate::error_logger::ErrorLogger,
    ) {
        self.config = Some(bootstrap_config.clone());
        debug!(
            "AutoBootstrap initialized with {} peers",
            bootstrap_config.bootstrap_peers.len()
        );
        bootstrap_logger.log_init(&format!(
            "Bootstrap initialized with {} configured peers",
            bootstrap_config.bootstrap_peers.len()
        ));
    }

    /// Attempt automatic bootstrap with all configured peers
    pub async fn attempt_bootstrap(
        &mut self,
        swarm: &mut Swarm<StoryBehaviour>,
        bootstrap_logger: &BootstrapLogger,
        error_logger: &crate::error_logger::ErrorLogger,
    ) -> bool {
        let config = match &self.config {
            Some(config) => config,
            None => {
                warn!("No bootstrap config available for automatic bootstrap");
                return false;
            }
        };

        if config.bootstrap_peers.is_empty() {
            warn!("No bootstrap peers configured");
            {
                let mut status = self.status.lock().unwrap();
                *status = BootstrapStatus::Failed {
                    attempts: 0,
                    last_error: "No bootstrap peers configured".to_string(),
                };
            }
            return false;
        }

        {
            let mut retry_count = self.retry_count.lock().unwrap();
            *retry_count += 1;
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::InProgress {
                attempts: *retry_count,
                last_attempt: Instant::now(),
            };
        }

        let current_retry_count = *self.retry_count.lock().unwrap();
        bootstrap_logger.log_attempt(&format!(
            "Attempting automatic DHT bootstrap (attempt {}/{})",
            current_retry_count, config.max_retry_attempts
        ));

        let mut peers_added = 0;
        for peer_addr in &config.bootstrap_peers {
            match peer_addr.parse::<libp2p::Multiaddr>() {
                Ok(addr) => {
                    if let Some(peer_id) = extract_peer_id_from_multiaddr(&addr) {
                        swarm
                            .behaviour_mut()
                            .kad
                            .add_address(&peer_id, addr.clone());
                        peers_added += 1;
                        debug!("Added bootstrap peer to DHT: {}", peer_addr);
                    } else {
                        warn!("Failed to extract peer ID from: {}", peer_addr);
                    }
                }
                Err(e) => {
                    warn!("Invalid multiaddr in config '{}': {}", peer_addr, e);
                }
            }
        }

        if peers_added > 0 {
            // Start bootstrap process
            match swarm.behaviour_mut().kad.bootstrap() {
                Ok(_) => {
                    bootstrap_logger
                        .log(&format!("DHT bootstrap started with {} peers", peers_added));
                    true
                }
                Err(e) => {
                    let error_msg = format!("Failed to start DHT bootstrap: {:?}", e);
                    error_logger.log_error(&error_msg);
                    bootstrap_logger.log_error(&error_msg);
                    {
                        let retry_count = *self.retry_count.lock().unwrap();
                        let mut status = self.status.lock().unwrap();
                        *status = BootstrapStatus::Failed {
                            attempts: retry_count,
                            last_error: error_msg,
                        };
                    }
                    false
                }
            }
        } else {
            let error_msg = "No valid bootstrap peers could be added".to_string();
            warn!("{}", error_msg);
            {
                let retry_count = *self.retry_count.lock().unwrap();
                let mut status = self.status.lock().unwrap();
                *status = BootstrapStatus::Failed {
                    attempts: retry_count,
                    last_error: error_msg.clone(),
                };
            }
            bootstrap_logger.log_error(&error_msg);
            false
        }
    }

    /// Check if we should retry bootstrap
    pub fn should_retry(&self) -> bool {
        let config = match &self.config {
            Some(config) => config,
            None => return false,
        };

        let status = self.status.lock().unwrap();
        match &*status {
            BootstrapStatus::Failed { attempts, .. } => *attempts < config.max_retry_attempts,
            BootstrapStatus::NotStarted => true,
            BootstrapStatus::InProgress { .. } => false,
            BootstrapStatus::Connected { .. } => false,
        }
    }

    /// Check if it's time for the next retry
    pub fn is_retry_time(&self) -> bool {
        let next_retry_time = self.next_retry_time.lock().unwrap();
        match *next_retry_time {
            Some(retry_time) => Instant::now() >= retry_time,
            None => true, // First attempt
        }
    }

    /// Schedule the next retry with exponential backoff
    pub fn schedule_next_retry(&mut self) {
        let config = match &self.config {
            Some(config) => config,
            None => return,
        };

        // Exponential backoff: retry_interval * 2^(attempts-1)
        let base_interval_ms = config.retry_interval_ms;
        let retry_count = *self.retry_count.lock().unwrap();
        let backoff_power = (retry_count.saturating_sub(1)).min(5); // Cap at 2^5 = 32x
        let backoff_multiplier = 2_u64.pow(backoff_power);

        // Use saturating_mul to prevent overflow, then convert to Duration safely
        let retry_delay_ms = base_interval_ms.saturating_mul(backoff_multiplier);
        let retry_delay = Duration::from_millis(retry_delay_ms);

        {
            let mut next_retry_time = self.next_retry_time.lock().unwrap();
            *next_retry_time = Some(Instant::now() + retry_delay);
        }

        debug!(
            "Scheduled next bootstrap retry in {} seconds",
            retry_delay.as_secs()
        );
    }

    /// Handle bootstrap success
    pub fn mark_connected(&mut self, peer_count: usize) {
        {
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::Connected {
                peer_count,
                connected_at: Instant::now(),
            };
        }
        {
            let mut retry_count = self.retry_count.lock().unwrap();
            *retry_count = 0;
        }
        {
            let mut next_retry_time = self.next_retry_time.lock().unwrap();
            *next_retry_time = None;
        }
        debug!("Bootstrap marked as connected with {} peers", peer_count);
    }

    /// Handle bootstrap failure
    pub fn mark_failed(&mut self, error: String) {
        {
            let retry_count = *self.retry_count.lock().unwrap();
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: retry_count,
                last_error: error,
            };
        }
        self.schedule_next_retry();
    }

    /// Get the current status for display
    pub fn get_status_string(&self) -> String {
        let status = self.status.lock().unwrap();
        match &*status {
            BootstrapStatus::NotStarted => "DHT: Not started".to_string(),
            BootstrapStatus::InProgress { attempts, .. } => {
                format!("DHT: Bootstrapping (attempt {})", attempts)
            }
            BootstrapStatus::Connected {
                peer_count,
                connected_at,
            } => {
                let elapsed = connected_at.elapsed();
                format!(
                    "DHT: Connected ({} peers, {}s ago)",
                    peer_count,
                    elapsed.as_secs()
                )
            }
            BootstrapStatus::Failed {
                attempts,
                last_error,
            } => {
                format!("DHT: Failed after {} attempts ({})", attempts, last_error)
            }
        }
    }

    /// Reset the bootstrap state (useful for manual retry)
    pub fn reset(&mut self) {
        {
            let mut status = self.status.lock().unwrap();
            *status = BootstrapStatus::NotStarted;
        }
        {
            let mut retry_count = self.retry_count.lock().unwrap();
            *retry_count = 0;
        }
        {
            let mut next_retry_time = self.next_retry_time.lock().unwrap();
            *next_retry_time = None;
        }
    }
}

impl Default for AutoBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function for running automatic bootstrap with retry logic
pub async fn run_auto_bootstrap_with_retry(
    auto_bootstrap: &mut AutoBootstrap,
    swarm: &mut Swarm<StoryBehaviour>,
    bootstrap_logger: &BootstrapLogger,
    error_logger: &crate::error_logger::ErrorLogger,
) {
    if !auto_bootstrap.should_retry() {
        return;
    }

    if !auto_bootstrap.is_retry_time() {
        return;
    }

    // Attempt bootstrap
    if auto_bootstrap
        .attempt_bootstrap(swarm, bootstrap_logger, error_logger)
        .await
    {
        // Bootstrap started successfully, but we need to wait for results
        // The actual success/failure will be handled by DHT events
    } else {
        // Bootstrap failed immediately, schedule retry
        auto_bootstrap.schedule_next_retry();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_bootstrap_logger() -> BootstrapLogger {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        BootstrapLogger::new(path)
    }

    #[test]
    fn test_auto_bootstrap_new() {
        let bootstrap = AutoBootstrap::new();
        let status = bootstrap.status.lock().unwrap();
        assert!(matches!(*status, BootstrapStatus::NotStarted));
        let retry_count = bootstrap.retry_count.lock().unwrap();
        assert_eq!(*retry_count, 0);
        let next_retry_time = bootstrap.next_retry_time.lock().unwrap();
        assert!(next_retry_time.is_none());
    }

    #[test]
    fn test_bootstrap_status_default() {
        let status = BootstrapStatus::default();
        assert!(matches!(status, BootstrapStatus::NotStarted));
    }

    #[test]
    fn test_should_retry_logic() {
        let mut bootstrap = AutoBootstrap::new();

        // Add a test config
        bootstrap.config = Some(BootstrapConfig::new());

        // Should retry when not started
        assert!(bootstrap.should_retry());

        // Should not retry when in progress
        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::InProgress {
                attempts: 1,
                last_attempt: Instant::now(),
            };
        }
        assert!(!bootstrap.should_retry());

        // Should not retry when connected
        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Connected {
                peer_count: 5,
                connected_at: Instant::now(),
            };
        }
        assert!(!bootstrap.should_retry());

        // Should retry when failed but under max attempts
        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: 2,
                last_error: "error".to_string(),
            };
        }
        assert!(bootstrap.should_retry());

        // Should not retry when failed and at max attempts
        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: 5,
                last_error: "error".to_string(),
            };
        }
        assert!(!bootstrap.should_retry());
    }

    #[test]
    fn test_mark_connected() {
        let mut bootstrap = AutoBootstrap::new();
        {
            let mut retry_count = bootstrap.retry_count.lock().unwrap();
            *retry_count = 3;
        }
        {
            let mut next_retry_time = bootstrap.next_retry_time.lock().unwrap();
            *next_retry_time = Some(Instant::now());
        }

        bootstrap.mark_connected(10);

        let status = bootstrap.status.lock().unwrap();
        match &*status {
            BootstrapStatus::Connected { peer_count, .. } => {
                assert_eq!(*peer_count, 10);
            }
            _ => panic!("Expected Connected status"),
        }

        let retry_count = bootstrap.retry_count.lock().unwrap();
        assert_eq!(*retry_count, 0);
        let next_retry_time = bootstrap.next_retry_time.lock().unwrap();
        assert!(next_retry_time.is_none());
    }

    #[test]
    fn test_mark_failed() {
        let mut bootstrap = AutoBootstrap::new();
        {
            let mut retry_count = bootstrap.retry_count.lock().unwrap();
            *retry_count = 2;
        }

        // Add a test config
        bootstrap.config = Some(BootstrapConfig::new());

        bootstrap.mark_failed("Test error".to_string());

        let status = bootstrap.status.lock().unwrap();
        match &*status {
            BootstrapStatus::Failed {
                attempts,
                last_error,
            } => {
                assert_eq!(*attempts, 2);
                assert_eq!(last_error, "Test error");
            }
            _ => panic!("Expected Failed status"),
        }

        let next_retry_time = bootstrap.next_retry_time.lock().unwrap();
        assert!(next_retry_time.is_some());
    }

    #[test]
    fn test_reset() {
        let mut bootstrap = AutoBootstrap::new();
        {
            let mut retry_count = bootstrap.retry_count.lock().unwrap();
            *retry_count = 5;
        }
        {
            let mut next_retry_time = bootstrap.next_retry_time.lock().unwrap();
            *next_retry_time = Some(Instant::now());
        }
        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: 5,
                last_error: "error".to_string(),
            };
        }

        bootstrap.reset();

        let status = bootstrap.status.lock().unwrap();
        assert!(matches!(*status, BootstrapStatus::NotStarted));
        let retry_count = bootstrap.retry_count.lock().unwrap();
        assert_eq!(*retry_count, 0);
        let next_retry_time = bootstrap.next_retry_time.lock().unwrap();
        assert!(next_retry_time.is_none());
    }

    #[test]
    fn test_status_string() {
        let bootstrap = AutoBootstrap::new();

        assert_eq!(bootstrap.get_status_string(), "DHT: Not started");

        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::InProgress {
                attempts: 2,
                last_attempt: Instant::now(),
            };
        }
        assert_eq!(
            bootstrap.get_status_string(),
            "DHT: Bootstrapping (attempt 2)"
        );

        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Connected {
                peer_count: 5,
                connected_at: Instant::now(),
            };
        }
        let status_str = bootstrap.get_status_string();
        assert!(status_str.starts_with("DHT: Connected (5 peers"));

        {
            let mut status = bootstrap.status.lock().unwrap();
            *status = BootstrapStatus::Failed {
                attempts: 3,
                last_error: "timeout".to_string(),
            };
        }
        assert_eq!(
            bootstrap.get_status_string(),
            "DHT: Failed after 3 attempts (timeout)"
        );
    }

    #[tokio::test]
    async fn test_initialize_without_config_file() {
        let mut bootstrap = AutoBootstrap::new();
        let bootstrap_logger = create_test_bootstrap_logger();
        let error_logger = crate::error_logger::ErrorLogger::new("test_errors.log");
        let test_config = BootstrapConfig::new();
        
        bootstrap.initialize(&test_config, &bootstrap_logger, &error_logger).await;

        // The test just ensures initialization doesn't panic
    }
}
