use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::Mutex;
use log::{debug, warn};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    /// Normal operation - requests are allowed through
    Closed,
    /// Failure threshold exceeded - requests are blocked
    Open { opened_at: Instant },
    /// Testing if service has recovered - limited requests allowed
    HalfOpen,
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of successes needed to close from half-open state
    pub success_threshold: u32,
    /// Time to wait before transitioning from open to half-open
    pub timeout: Duration,
    /// Maximum time to wait for an operation
    pub operation_timeout: Duration,
    /// Name for logging and identification
    pub name: String,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(60),
            operation_timeout: Duration::from_secs(30),
            name: "default".to_string(),
        }
    }
}

/// Circuit breaker implementation for network operations
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<CircuitBreakerState>>,
}

#[derive(Debug)]
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    total_requests: u64,
    total_failures: u64,
    total_successes: u64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(config: CircuitBreakerConfig) -> Self {
        debug!("Creating circuit breaker: {}", config.name);
        Self {
            config,
            state: Arc::new(Mutex::new(CircuitBreakerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                total_requests: 0,
                total_failures: 0,
                total_successes: 0,
            })),
        }
    }

    /// Check if the circuit breaker allows the operation to proceed
    pub async fn can_execute(&self) -> bool {
        let mut state = self.state.lock().await;
        state.total_requests += 1;

        match &state.state {
            CircuitState::Closed => true,
            CircuitState::Open { opened_at } => {
                if opened_at.elapsed() >= self.config.timeout {
                    debug!("Circuit breaker {} transitioning from Open to HalfOpen", self.config.name);
                    state.state = CircuitState::HalfOpen;
                    state.success_count = 0;
                    true
                } else {
                    debug!("Circuit breaker {} is open, blocking request", self.config.name);
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful operation
    pub async fn on_success(&self) {
        let mut state = self.state.lock().await;
        state.total_successes += 1;

        match state.state {
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    debug!("Circuit breaker {} transitioning from HalfOpen to Closed", self.config.name);
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                }
            }
            CircuitState::Open { .. } => {
                // Should not happen, but handle gracefully
                warn!("Unexpected success in Open state for circuit breaker {}", self.config.name);
            }
        }
    }

    /// Record a failed operation
    pub async fn on_failure(&self, error: &str) {
        let mut state = self.state.lock().await;
        state.total_failures += 1;
        state.last_failure_time = Some(Instant::now());

        match state.state {
            CircuitState::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.config.failure_threshold {
                    warn!("Circuit breaker {} opening due to failures ({})", self.config.name, state.failure_count);
                    state.state = CircuitState::Open { opened_at: Instant::now() };
                }
            }
            CircuitState::HalfOpen => {
                warn!("Circuit breaker {} failed in HalfOpen state, returning to Open", self.config.name);
                state.state = CircuitState::Open { opened_at: Instant::now() };
                state.failure_count += 1;
                state.success_count = 0;
            }
            CircuitState::Open { .. } => {
                // Already open, just increment counter
                state.failure_count += 1;
            }
        }

        debug!("Circuit breaker {} recorded failure: {}", self.config.name, error);
    }

    /// Get current circuit breaker state information
    pub async fn get_state(&self) -> CircuitBreakerInfo {
        let state = self.state.lock().await;
        CircuitBreakerInfo {
            name: self.config.name.clone(),
            state: state.state.clone(),
            failure_count: state.failure_count,
            success_count: state.success_count,
            total_requests: state.total_requests,
            total_failures: state.total_failures,
            total_successes: state.total_successes,
            failure_rate: if state.total_requests > 0 {
                state.total_failures as f64 / state.total_requests as f64
            } else {
                0.0
            },
            last_failure_time: state.last_failure_time,
        }
    }

    /// Execute an operation with circuit breaker protection
    pub async fn execute<T, E, F, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        if !self.can_execute().await {
            return Err(CircuitBreakerError::CircuitOpen {
                circuit_name: self.config.name.clone(),
            });
        }

        // Execute the operation with timeout
        let result = tokio::time::timeout(self.config.operation_timeout, operation()).await;

        match result {
            Ok(Ok(success)) => {
                self.on_success().await;
                Ok(success)
            }
            Ok(Err(error)) => {
                self.on_failure(&format!("{}", error)).await;
                Err(CircuitBreakerError::OperationFailed(error))
            }
            Err(_timeout) => {
                let timeout_msg = format!("Operation timed out after {:?}", self.config.operation_timeout);
                self.on_failure(&timeout_msg).await;
                Err(CircuitBreakerError::OperationTimeout {
                    circuit_name: self.config.name.clone(),
                    timeout: self.config.operation_timeout,
                })
            }
        }
    }
}

/// Information about the current state of a circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerInfo {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub total_requests: u64,
    pub total_failures: u64,
    pub total_successes: u64,
    pub failure_rate: f64,
    pub last_failure_time: Option<Instant>,
}

impl CircuitBreakerInfo {
    /// Check if the circuit breaker is healthy (closed state with low failure rate)
    pub fn is_healthy(&self) -> bool {
        matches!(self.state, CircuitState::Closed) && self.failure_rate < 0.5
    }

    /// Get a human-readable status string
    pub fn status_string(&self) -> String {
        match &self.state {
            CircuitState::Closed => {
                if self.total_requests > 0 {
                    format!("Healthy ({:.1}% success rate)", (1.0 - self.failure_rate) * 100.0)
                } else {
                    "Healthy (no requests yet)".to_string()
                }
            }
            CircuitState::Open { opened_at } => {
                format!("Failed ({} failures, open for {:?})", 
                    self.failure_count, 
                    opened_at.elapsed())
            }
            CircuitState::HalfOpen => {
                format!("Testing recovery ({} successes needed)", 
                    3_u32.saturating_sub(self.success_count))
            }
        }
    }
}

/// Errors that can occur when using the circuit breaker
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker '{circuit_name}' is open")]
    CircuitOpen { circuit_name: String },
    
    #[error("Operation failed: {0}")]
    OperationFailed(E),
    
    #[error("Operation timed out after {timeout:?} in circuit '{circuit_name}'")]
    OperationTimeout { circuit_name: String, timeout: Duration },
}

