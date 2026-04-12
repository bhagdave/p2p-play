use crate::bootstrap_logger::BootstrapLogger;
use crate::constants::BOOTSTRAP_LOG_FILE;
use crate::handlers::{UILogger, extract_peer_id_from_multiaddr};
use crate::network::StoryBehaviour;
use crate::types::BootstrapConfig;
use libp2p::swarm::Swarm;
use log::warn;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const UNIFIED_CONFIG_FILE: &str = "unified_network_config.json";

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

struct BootstrapState {
    status: BootstrapStatus,
    retry_count: u32,
    next_retry_time: Option<Instant>,
}

impl BootstrapState {
    fn new() -> Self {
        Self {
            status: BootstrapStatus::NotStarted,
            retry_count: 0,
            next_retry_time: None,
        }
    }

    fn set_failed(&mut self, error: String) {
        self.status = BootstrapStatus::Failed {
            attempts: self.retry_count,
            last_error: error,
        };
    }
}

pub struct AutoBootstrap {
    state: Arc<Mutex<BootstrapState>>,
    config: Option<BootstrapConfig>,
}

impl AutoBootstrap {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BootstrapState::new())),
            config: None,
        }
    }

    /// Returns `true` if the bootstrap is currently in the `InProgress` state.
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self.state.lock().unwrap().status,
            BootstrapStatus::InProgress { .. }
        )
    }

    /// Non-blocking check: returns `Some(true)` if bootstrap has started (i.e. not
    /// `NotStarted`), `Some(false)` if it hasn't, or `None` if the lock is contended.
    pub fn try_has_started(&self) -> Option<bool> {
        self.state
            .try_lock()
            .ok()
            .map(|s| !matches!(s.status, BootstrapStatus::NotStarted))
    }

    pub  fn initialise(
        &mut self,
        bootstrap_config: &BootstrapConfig,
        bootstrap_logger: &BootstrapLogger,
        _error_logger: &crate::error_logger::ErrorLogger,
    ) {
        self.config = Some(bootstrap_config.clone());
        bootstrap_logger.log_init(&format!(
            "Bootstrap initialised with {} configured peers",
            bootstrap_config.bootstrap_peers.len()
        ));
    }

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
            self.state.lock().unwrap().set_failed("No bootstrap peers configured".to_string());
            return false;
        }

        {
            let mut state = self.state.lock().unwrap();
            state.retry_count += 1;
            state.status = BootstrapStatus::InProgress {
                attempts: state.retry_count,
                last_attempt: Instant::now(),
            };
        }

        let current_retry_count = self.state.lock().unwrap().retry_count;
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
                    } else {
                        warn!("Failed to extract peer ID from: {peer_addr}");
                    }
                }
                Err(e) => {
                    warn!("Invalid multiaddr in config '{peer_addr}': {e}");
                }
            }
        }

        if peers_added > 0 {
            match swarm.behaviour_mut().kad.bootstrap() {
                Ok(_) => {
                    bootstrap_logger
                        .log(&format!("DHT bootstrap started with {peers_added} peers"));
                    true
                }
                Err(e) => {
                    let error_msg = format!("Failed to start DHT bootstrap: {e:?}");
                    error_logger.log_error(&error_msg);
                    bootstrap_logger.log_error(&error_msg);
                    self.state.lock().unwrap().set_failed(error_msg);
                    false
                }
            }
        } else {
            let error_msg = "No valid bootstrap peers could be added".to_string();
            warn!("{error_msg}");
            self.state.lock().unwrap().set_failed(error_msg.clone());
            bootstrap_logger.log_error(&error_msg);
            false
        }
    }

    pub fn should_retry(&self) -> bool {
        let config = match &self.config {
            Some(config) => config,
            None => return false,
        };

        let state = self.state.lock().unwrap();
        match &state.status {
            BootstrapStatus::Failed { attempts, .. } => *attempts < config.max_retry_attempts,
            BootstrapStatus::NotStarted => true,
            BootstrapStatus::InProgress { .. } | BootstrapStatus::Connected { .. } => false,
        }
    }

    pub fn max_retry_attempts(&self) -> u32 {
        self.config
            .as_ref()
            .map(|c| c.max_retry_attempts)
            .unwrap_or(0)
    }

    pub fn is_retry_time(&self) -> bool {
        match self.state.lock().unwrap().next_retry_time {
            Some(retry_time) => Instant::now() >= retry_time,
            None => true, // First attempt
        }
    }

    pub fn schedule_next_retry(&mut self) {
        let config = match &self.config {
            Some(config) => config,
            None => return,
        };

        // Exponential backoff: retry_interval * 2^(attempts-1), capped at 2^5 = 32x
        let base_interval_ms = config.retry_interval_ms;
        let retry_count = self.state.lock().unwrap().retry_count;
        let backoff_power = (retry_count.saturating_sub(1)).min(5);
        let backoff_multiplier = 2_u64.pow(backoff_power);
        let retry_delay = Duration::from_millis(base_interval_ms.saturating_mul(backoff_multiplier));

        self.state.lock().unwrap().next_retry_time = Some(Instant::now() + retry_delay);
    }

    pub fn mark_connected(&mut self, peer_count: usize) {
        let mut state = self.state.lock().unwrap();
        state.status = BootstrapStatus::Connected {
            peer_count,
            connected_at: Instant::now(),
        };
        state.retry_count = 0;
        state.next_retry_time = None;
    }

    pub fn mark_failed(&mut self, error: String) {
        self.state.lock().unwrap().set_failed(error);
        self.schedule_next_retry();
    }

    pub fn get_status_string(&self) -> String {
        let state = self.state.lock().unwrap();
        match &state.status {
            BootstrapStatus::NotStarted => "DHT: Not started".to_string(),
            BootstrapStatus::InProgress { attempts, .. } => {
                format!("DHT: Bootstrapping (attempt {attempts})")
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
                format!("DHT: Failed after {attempts} attempts ({last_error})")
            }
        }
    }

    /// Returns a short, stable label for display in the TUI status bar.
    pub fn get_bootstrap_short_status(&self) -> &'static str {
        let state = self.state.lock().unwrap();
        match &state.status {
            BootstrapStatus::NotStarted => "--",
            BootstrapStatus::InProgress { .. } => "Connecting",
            BootstrapStatus::Connected { .. } => "OK",
            BootstrapStatus::Failed { .. } => "Failed",
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.status = BootstrapStatus::NotStarted;
        state.retry_count = 0;
        state.next_retry_time = None;
    }
}

impl Default for AutoBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run_auto_bootstrap_with_retry(
    auto_bootstrap: &mut AutoBootstrap,
    swarm: &mut Swarm<StoryBehaviour>,
    bootstrap_logger: &BootstrapLogger,
    error_logger: &crate::error_logger::ErrorLogger,
    ui_logger: &UILogger,
) {
    if !auto_bootstrap.should_retry() {
        return;
    }

    if !auto_bootstrap.is_retry_time() {
        return;
    }

    if auto_bootstrap
        .attempt_bootstrap(swarm, bootstrap_logger, error_logger)
        .await
    {
        // Bootstrap started successfully; actual success/failure arrives via DHT events
    } else {
        auto_bootstrap.schedule_next_retry();

        let max_retries = auto_bootstrap.max_retry_attempts();
        if auto_bootstrap.should_retry() {
            ui_logger.log(format!(
                "Bootstrap attempt failed — will retry (up to {max_retries} attempts). Check {BOOTSTRAP_LOG_FILE} for details."
            ));
        } else {
            ui_logger.log(format!(
                "Bootstrap failed after reaching the maximum of {max_retries} attempts — check {BOOTSTRAP_LOG_FILE} or add peers to {UNIFIED_CONFIG_FILE}"
            ));
        }
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
        let state = bootstrap.state.lock().unwrap();
        assert!(matches!(state.status, BootstrapStatus::NotStarted));
        assert_eq!(state.retry_count, 0);
        assert!(state.next_retry_time.is_none());
    }

    #[test]
    fn test_bootstrap_status_default() {
        let status = BootstrapStatus::default();
        assert!(matches!(status, BootstrapStatus::NotStarted));
    }

    #[test]
    fn test_should_retry_logic() {
        let mut bootstrap = AutoBootstrap::new();

        bootstrap.config = Some(BootstrapConfig::new());

        // Should retry when not started
        assert!(bootstrap.should_retry());

        // Should not retry when in progress
        bootstrap.state.lock().unwrap().status = BootstrapStatus::InProgress {
            attempts: 1,
            last_attempt: Instant::now(),
        };
        assert!(!bootstrap.should_retry());

        // Should not retry when connected
        bootstrap.state.lock().unwrap().status = BootstrapStatus::Connected {
            peer_count: 5,
            connected_at: Instant::now(),
        };
        assert!(!bootstrap.should_retry());

        // Should retry when failed but under max attempts
        bootstrap.state.lock().unwrap().status = BootstrapStatus::Failed {
            attempts: 2,
            last_error: "error".to_string(),
        };
        assert!(bootstrap.should_retry());

        // Should not retry when failed and at max attempts
        bootstrap.state.lock().unwrap().status = BootstrapStatus::Failed {
            attempts: 10,
            last_error: "error".to_string(),
        };
        assert!(!bootstrap.should_retry());
    }

    #[test]
    fn test_mark_connected() {
        let mut bootstrap = AutoBootstrap::new();
        {
            let mut state = bootstrap.state.lock().unwrap();
            state.retry_count = 3;
            state.next_retry_time = Some(Instant::now());
        }

        bootstrap.mark_connected(10);

        let state = bootstrap.state.lock().unwrap();
        match &state.status {
            BootstrapStatus::Connected { peer_count, .. } => {
                assert_eq!(*peer_count, 10);
            }
            _ => panic!("Expected Connected status"),
        }
        assert_eq!(state.retry_count, 0);
        assert!(state.next_retry_time.is_none());
    }

    #[test]
    fn test_mark_failed() {
        let mut bootstrap = AutoBootstrap::new();
        bootstrap.state.lock().unwrap().retry_count = 2;
        bootstrap.config = Some(BootstrapConfig::new());

        bootstrap.mark_failed("Test error".to_string());

        let state = bootstrap.state.lock().unwrap();
        match &state.status {
            BootstrapStatus::Failed {
                attempts,
                last_error,
            } => {
                assert_eq!(*attempts, 2);
                assert_eq!(last_error, "Test error");
            }
            _ => panic!("Expected Failed status"),
        }
        drop(state);

        let next_retry_time = bootstrap.state.lock().unwrap().next_retry_time;
        assert!(next_retry_time.is_some());
    }

    #[test]
    fn test_reset() {
        let mut bootstrap = AutoBootstrap::new();
        {
            let mut state = bootstrap.state.lock().unwrap();
            state.retry_count = 5;
            state.next_retry_time = Some(Instant::now());
            state.status = BootstrapStatus::Failed {
                attempts: 5,
                last_error: "error".to_string(),
            };
        }

        bootstrap.reset();

        let state = bootstrap.state.lock().unwrap();
        assert!(matches!(state.status, BootstrapStatus::NotStarted));
        assert_eq!(state.retry_count, 0);
        assert!(state.next_retry_time.is_none());
    }

    #[test]
    fn test_status_string() {
        let bootstrap = AutoBootstrap::new();

        assert_eq!(bootstrap.get_status_string(), "DHT: Not started");

        bootstrap.state.lock().unwrap().status = BootstrapStatus::InProgress {
            attempts: 2,
            last_attempt: Instant::now(),
        };
        assert_eq!(
            bootstrap.get_status_string(),
            "DHT: Bootstrapping (attempt 2)"
        );

        bootstrap.state.lock().unwrap().status = BootstrapStatus::Connected {
            peer_count: 5,
            connected_at: Instant::now(),
        };
        let status_str = bootstrap.get_status_string();
        assert!(status_str.starts_with("DHT: Connected (5 peers"));

        bootstrap.state.lock().unwrap().status = BootstrapStatus::Failed {
            attempts: 3,
            last_error: "timeout".to_string(),
        };
        assert_eq!(
            bootstrap.get_status_string(),
            "DHT: Failed after 3 attempts (timeout)"
        );
    }

    #[test]
    fn test_bootstrap_short_status() {
        let bootstrap = AutoBootstrap::new();

        assert_eq!(bootstrap.get_bootstrap_short_status(), "--");

        bootstrap.state.lock().unwrap().status = BootstrapStatus::InProgress {
            attempts: 1,
            last_attempt: Instant::now(),
        };
        assert_eq!(bootstrap.get_bootstrap_short_status(), "Connecting");

        bootstrap.state.lock().unwrap().status = BootstrapStatus::Connected {
            peer_count: 3,
            connected_at: Instant::now(),
        };
        assert_eq!(bootstrap.get_bootstrap_short_status(), "OK");

        bootstrap.state.lock().unwrap().status = BootstrapStatus::Failed {
            attempts: 2,
            last_error: "timeout".to_string(),
        };
        assert_eq!(bootstrap.get_bootstrap_short_status(), "Failed");
    }

    #[tokio::test]
    async fn test_initialise_without_config_file() {
        let mut bootstrap = AutoBootstrap::new();
        let bootstrap_logger = create_test_bootstrap_logger();
        let error_logger = crate::error_logger::ErrorLogger::new("test_errors.log");
        let test_config = BootstrapConfig::new();

        bootstrap
            .initialise(&test_config, &bootstrap_logger, &error_logger);
    }
}
