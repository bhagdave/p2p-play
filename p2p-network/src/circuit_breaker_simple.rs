//! Circuit breaker pattern implementation for network resilience

use crate::errors::{CircuitBreakerError, CircuitBreakerResult};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing, rejecting requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub recovery_timeout: Duration,
    pub success_threshold: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 3,
        }
    }
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<CircuitBreakerState>>,
    failure_count: Arc<Mutex<usize>>,
    success_count: Arc<Mutex<usize>>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(Mutex::new(0)),
            success_count: Arc::new(Mutex::new(0)),
            last_failure_time: Arc::new(Mutex::new(None)),
        }
    }

    pub fn can_execute(&self) -> CircuitBreakerResult<bool> {
        let state = self.state.lock().unwrap();
        match *state {
            CircuitBreakerState::Closed => Ok(true),
            CircuitBreakerState::Open => {
                if let Some(last_failure) = *self.last_failure_time.lock().unwrap() {
                    if last_failure.elapsed() > self.config.recovery_timeout {
                        drop(state);
                        *self.state.lock().unwrap() = CircuitBreakerState::HalfOpen;
                        *self.success_count.lock().unwrap() = 0;
                        Ok(true)
                    } else {
                        Err(CircuitBreakerError::CircuitOpen)
                    }
                } else {
                    Ok(true)
                }
            }
            CircuitBreakerState::HalfOpen => Ok(true),
        }
    }

    pub fn on_success(&self) {
        let mut state = self.state.lock().unwrap();
        match *state {
            CircuitBreakerState::HalfOpen => {
                let mut success_count = self.success_count.lock().unwrap();
                *success_count += 1;
                if *success_count >= self.config.success_threshold {
                    *state = CircuitBreakerState::Closed;
                    *self.failure_count.lock().unwrap() = 0;
                }
            }
            CircuitBreakerState::Closed => {
                *self.failure_count.lock().unwrap() = 0;
            }
            _ => {}
        }
    }

    pub fn on_failure(&self) {
        let mut failure_count = self.failure_count.lock().unwrap();
        *failure_count += 1;
        
        if *failure_count >= self.config.failure_threshold {
            *self.state.lock().unwrap() = CircuitBreakerState::Open;
            *self.last_failure_time.lock().unwrap() = Some(Instant::now());
        }
    }

    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.lock().unwrap().clone()
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}