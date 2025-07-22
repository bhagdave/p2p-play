use crate::handlers::{extract_peer_id_from_multiaddr, UILogger};
use crate::network::StoryBehaviour;
use crate::storage::load_bootstrap_config;
use crate::types::BootstrapConfig;
use libp2p::swarm::Swarm;
use log::{debug, error, warn};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapStatus {
    NotStarted,
    InProgress { attempts: u32, last_attempt: Instant },
    Connected { peer_count: usize, connected_at: Instant },
    Failed { attempts: u32, last_error: String },
}

impl Default for BootstrapStatus {
    fn default() -> Self {
        BootstrapStatus::NotStarted
    }
}

pub struct AutoBootstrap {
    pub status: BootstrapStatus,
    config: Option<BootstrapConfig>,
    retry_count: u32,
    next_retry_time: Option<Instant>,
}

impl AutoBootstrap {
    pub fn new() -> Self {
        Self {
            status: BootstrapStatus::NotStarted,
            config: None,
            retry_count: 0,
            next_retry_time: None,
        }
    }

    /// Initialize the auto bootstrap with config
    pub async fn initialize(&mut self, ui_logger: &UILogger) {
        match load_bootstrap_config().await {
            Ok(config) => {
                self.config = Some(config.clone());
                debug!("AutoBootstrap initialized with {} peers", config.bootstrap_peers.len());
                ui_logger.log(format!("Bootstrap initialized with {} configured peers", config.bootstrap_peers.len()));
            }
            Err(e) => {
                error!("Failed to load bootstrap config for AutoBootstrap: {}", e);
                ui_logger.log(format!("Bootstrap initialization failed: {}", e));
            }
        }
    }

    /// Attempt automatic bootstrap with all configured peers
    pub async fn attempt_bootstrap(&mut self, swarm: &mut Swarm<StoryBehaviour>, ui_logger: &UILogger) -> bool {
        let config = match &self.config {
            Some(config) => config,
            None => {
                warn!("No bootstrap config available for automatic bootstrap");
                return false;
            }
        };

        if config.bootstrap_peers.is_empty() {
            warn!("No bootstrap peers configured");
            self.status = BootstrapStatus::Failed { 
                attempts: 0, 
                last_error: "No bootstrap peers configured".to_string() 
            };
            return false;
        }

        self.retry_count += 1;
        self.status = BootstrapStatus::InProgress { 
            attempts: self.retry_count, 
            last_attempt: Instant::now() 
        };

        ui_logger.log(format!("Attempting automatic DHT bootstrap (attempt {}/{})", 
            self.retry_count, config.max_retry_attempts));

        let mut peers_added = 0;
        for peer_addr in &config.bootstrap_peers {
            match peer_addr.parse::<libp2p::Multiaddr>() {
                Ok(addr) => {
                    if let Some(peer_id) = extract_peer_id_from_multiaddr(&addr) {
                        swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
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
                    ui_logger.log(format!("DHT bootstrap started with {} peers", peers_added));
                    true
                }
                Err(e) => {
                    let error_msg = format!("Failed to start DHT bootstrap: {:?}", e);
                    error!("{}", error_msg);
                    ui_logger.log(error_msg.clone());
                    self.status = BootstrapStatus::Failed { 
                        attempts: self.retry_count, 
                        last_error: error_msg 
                    };
                    false
                }
            }
        } else {
            let error_msg = "No valid bootstrap peers could be added".to_string();
            warn!("{}", error_msg);
            self.status = BootstrapStatus::Failed { 
                attempts: self.retry_count, 
                last_error: error_msg.clone() 
            };
            ui_logger.log(error_msg);
            false
        }
    }

    /// Check if we should retry bootstrap
    pub fn should_retry(&self) -> bool {
        let config = match &self.config {
            Some(config) => config,
            None => return false,
        };

        match &self.status {
            BootstrapStatus::Failed { attempts, .. } => {
                *attempts < config.max_retry_attempts
            }
            BootstrapStatus::NotStarted => true,
            BootstrapStatus::InProgress { .. } => false,
            BootstrapStatus::Connected { .. } => false,
        }
    }

    /// Check if it's time for the next retry
    pub fn is_retry_time(&self) -> bool {
        match self.next_retry_time {
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
        let base_interval = Duration::from_millis(config.retry_interval_ms);
        let backoff_multiplier = 2_u64.pow((self.retry_count.saturating_sub(1)).min(5) as u32); // Cap at 2^5 = 32x
        let retry_delay = base_interval * backoff_multiplier as u32;

        self.next_retry_time = Some(Instant::now() + retry_delay);
        
        debug!("Scheduled next bootstrap retry in {} seconds", retry_delay.as_secs());
    }

    /// Handle bootstrap success
    pub fn mark_connected(&mut self, peer_count: usize) {
        self.status = BootstrapStatus::Connected { 
            peer_count, 
            connected_at: Instant::now() 
        };
        self.retry_count = 0;
        self.next_retry_time = None;
        debug!("Bootstrap marked as connected with {} peers", peer_count);
    }

    /// Handle bootstrap failure
    pub fn mark_failed(&mut self, error: String) {
        self.status = BootstrapStatus::Failed { 
            attempts: self.retry_count, 
            last_error: error 
        };
        self.schedule_next_retry();
    }

    /// Get the current status for display
    pub fn get_status_string(&self) -> String {
        match &self.status {
            BootstrapStatus::NotStarted => "DHT: Not started".to_string(),
            BootstrapStatus::InProgress { attempts, .. } => {
                format!("DHT: Bootstrapping (attempt {})", attempts)
            }
            BootstrapStatus::Connected { peer_count, connected_at } => {
                let elapsed = connected_at.elapsed();
                format!("DHT: Connected ({} peers, {}s ago)", peer_count, elapsed.as_secs())
            }
            BootstrapStatus::Failed { attempts, last_error } => {
                format!("DHT: Failed after {} attempts ({})", attempts, last_error)
            }
        }
    }

    /// Reset the bootstrap state (useful for manual retry)
    pub fn reset(&mut self) {
        self.status = BootstrapStatus::NotStarted;
        self.retry_count = 0;
        self.next_retry_time = None;
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
    ui_logger: &UILogger
) {
    if !auto_bootstrap.should_retry() {
        return;
    }

    if !auto_bootstrap.is_retry_time() {
        return;
    }

    // Attempt bootstrap
    if auto_bootstrap.attempt_bootstrap(swarm, ui_logger).await {
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
    use tokio::sync::mpsc;

    fn create_test_ui_logger() -> UILogger {
        let (sender, _receiver) = mpsc::unbounded_channel();
        UILogger::new(sender)
    }

    #[test]
    fn test_auto_bootstrap_new() {
        let bootstrap = AutoBootstrap::new();
        assert!(matches!(bootstrap.status, BootstrapStatus::NotStarted));
        assert_eq!(bootstrap.retry_count, 0);
        assert!(bootstrap.next_retry_time.is_none());
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
        bootstrap.status = BootstrapStatus::InProgress { 
            attempts: 1, 
            last_attempt: Instant::now() 
        };
        assert!(!bootstrap.should_retry());
        
        // Should not retry when connected
        bootstrap.status = BootstrapStatus::Connected { 
            peer_count: 5, 
            connected_at: Instant::now() 
        };
        assert!(!bootstrap.should_retry());
        
        // Should retry when failed but under max attempts
        bootstrap.status = BootstrapStatus::Failed { 
            attempts: 2, 
            last_error: "error".to_string() 
        };
        assert!(bootstrap.should_retry());
        
        // Should not retry when failed and at max attempts
        bootstrap.status = BootstrapStatus::Failed { 
            attempts: 5, 
            last_error: "error".to_string() 
        };
        assert!(!bootstrap.should_retry());
    }

    #[test]
    fn test_mark_connected() {
        let mut bootstrap = AutoBootstrap::new();
        bootstrap.retry_count = 3;
        bootstrap.next_retry_time = Some(Instant::now());
        
        bootstrap.mark_connected(10);
        
        match bootstrap.status {
            BootstrapStatus::Connected { peer_count, .. } => {
                assert_eq!(peer_count, 10);
            }
            _ => panic!("Expected Connected status"),
        }
        
        assert_eq!(bootstrap.retry_count, 0);
        assert!(bootstrap.next_retry_time.is_none());
    }

    #[test]
    fn test_mark_failed() {
        let mut bootstrap = AutoBootstrap::new();
        bootstrap.retry_count = 2;
        
        // Add a test config
        bootstrap.config = Some(BootstrapConfig::new());
        
        bootstrap.mark_failed("Test error".to_string());
        
        match bootstrap.status {
            BootstrapStatus::Failed { attempts, last_error } => {
                assert_eq!(attempts, 2);
                assert_eq!(last_error, "Test error");
            }
            _ => panic!("Expected Failed status"),
        }
        
        assert!(bootstrap.next_retry_time.is_some());
    }

    #[test]
    fn test_reset() {
        let mut bootstrap = AutoBootstrap::new();
        bootstrap.retry_count = 5;
        bootstrap.next_retry_time = Some(Instant::now());
        bootstrap.status = BootstrapStatus::Failed { 
            attempts: 5, 
            last_error: "error".to_string() 
        };
        
        bootstrap.reset();
        
        assert!(matches!(bootstrap.status, BootstrapStatus::NotStarted));
        assert_eq!(bootstrap.retry_count, 0);
        assert!(bootstrap.next_retry_time.is_none());
    }

    #[test]
    fn test_status_string() {
        let mut bootstrap = AutoBootstrap::new();
        
        assert_eq!(bootstrap.get_status_string(), "DHT: Not started");
        
        bootstrap.status = BootstrapStatus::InProgress { 
            attempts: 2, 
            last_attempt: Instant::now() 
        };
        assert_eq!(bootstrap.get_status_string(), "DHT: Bootstrapping (attempt 2)");
        
        bootstrap.status = BootstrapStatus::Connected { 
            peer_count: 5, 
            connected_at: Instant::now() 
        };
        let status_str = bootstrap.get_status_string();
        assert!(status_str.starts_with("DHT: Connected (5 peers"));
        
        bootstrap.status = BootstrapStatus::Failed { 
            attempts: 3, 
            last_error: "timeout".to_string() 
        };
        assert_eq!(bootstrap.get_status_string(), "DHT: Failed after 3 attempts (timeout)");
    }

    #[tokio::test]
    async fn test_initialize_without_config_file() {
        let mut bootstrap = AutoBootstrap::new();
        let ui_logger = create_test_ui_logger();
        
        // This will try to load config and may succeed if bootstrap_config.json exists
        bootstrap.initialize(&ui_logger).await;
        
        // The test just ensures initialization doesn't panic
    }
}