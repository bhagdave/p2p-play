use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerInfo};
use crate::types::NetworkCircuitBreakerConfig;
use std::collections::HashMap;
use log::{debug, warn};

/// Manages circuit breakers for different network operations
#[derive(Debug)]
pub struct NetworkCircuitBreakers {
    breakers: HashMap<String, CircuitBreaker>,
    enabled: bool,
}

impl NetworkCircuitBreakers {
    /// Create a new network circuit breakers manager
    pub fn new(config: &NetworkCircuitBreakerConfig) -> Self {
        debug!("Creating network circuit breakers (enabled: {})", config.enabled);
        
        let mut breakers = HashMap::new();
        
        if config.enabled {
            // Create circuit breakers for different network operations
            let operations = vec![
                "peer_connection",
                "dht_bootstrap", 
                "message_broadcast",
                "direct_message",
                "story_publish",
                "story_sync",
            ];
            
            for operation in operations {
                let cb_config = config.to_circuit_breaker_config(operation.to_string());
                let circuit_breaker = CircuitBreaker::new(cb_config);
                breakers.insert(operation.to_string(), circuit_breaker);
            }
        }
        
        Self {
            breakers,
            enabled: config.enabled,
        }
    }

    /// Get a circuit breaker for a specific operation
    pub fn get(&self, operation: &str) -> Option<&CircuitBreaker> {
        if self.enabled {
            self.breakers.get(operation)
        } else {
            None
        }
    }

    /// Execute an operation with circuit breaker protection if enabled
    pub async fn execute<T, E, F, Fut>(
        &self,
        operation: &str,
        func: F,
    ) -> Result<T, crate::circuit_breaker::CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.execute(func).await
        } else {
            // Circuit breakers disabled, execute directly
            match func().await {
                Ok(result) => Ok(result),
                Err(error) => Err(crate::circuit_breaker::CircuitBreakerError::OperationFailed(error)),
            }
        }
    }

    /// Check if an operation can be executed (not blocked by circuit breaker)
    pub async fn can_execute(&self, operation: &str) -> bool {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.can_execute().await
        } else {
            true // Always allow if disabled
        }
    }

    /// Record a successful operation
    pub async fn on_success(&self, operation: &str) {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.on_success().await;
        }
    }

    /// Record a failed operation
    pub async fn on_failure(&self, operation: &str, error: &str) {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.on_failure(error).await;
        }
    }

    /// Get status information for all circuit breakers
    pub async fn get_all_status(&self) -> HashMap<String, CircuitBreakerInfo> {
        let mut status = HashMap::new();
        
        for (operation, circuit_breaker) in &self.breakers {
            let info = circuit_breaker.get_state().await;
            status.insert(operation.clone(), info);
        }
        
        status
    }

    /// Check if any circuit breaker is in a failed state
    pub async fn has_failures(&self) -> bool {
        if !self.enabled {
            return false;
        }
        
        for circuit_breaker in self.breakers.values() {
            let info = circuit_breaker.get_state().await;
            if !info.is_healthy() {
                return true;
            }
        }
        false
    }

    /// Get a summary of network health
    pub async fn health_summary(&self) -> NetworkHealthSummary {
        if !self.enabled {
            return NetworkHealthSummary {
                overall_healthy: true,
                total_operations: 0,
                healthy_operations: 0,
                failed_operations: 0,
                details: HashMap::new(),
            };
        }

        let mut total_operations = 0;
        let mut healthy_operations = 0;
        let mut failed_operations = 0;
        let mut details = HashMap::new();

        for (operation, circuit_breaker) in &self.breakers {
            let info = circuit_breaker.get_state().await;
            total_operations += 1;
            
            if info.is_healthy() {
                healthy_operations += 1;
            } else {
                failed_operations += 1;
            }
            
            details.insert(operation.clone(), info.status_string());
        }

        NetworkHealthSummary {
            overall_healthy: failed_operations == 0,
            total_operations,
            healthy_operations,
            failed_operations,
            details,
        }
    }
}

/// Summary of network health across all circuit breakers
#[derive(Debug, Clone)]
pub struct NetworkHealthSummary {
    pub overall_healthy: bool,
    pub total_operations: usize,
    pub healthy_operations: usize,
    pub failed_operations: usize,
    pub details: HashMap<String, String>,
}

impl NetworkHealthSummary {
    /// Get a human-readable health status
    pub fn status_string(&self) -> String {
        if self.overall_healthy {
            format!("Network Healthy ({}/{} operations)", self.healthy_operations, self.total_operations)
        } else {
            format!("Network Issues ({}/{} operations failing)", self.failed_operations, self.total_operations)
        }
    }

    /// Get detailed status for UI display
    pub fn detailed_status(&self) -> Vec<String> {
        let mut status = vec![self.status_string()];
        
        for (operation, detail) in &self.details {
            status.push(format!("  {}: {}", operation, detail));
        }
        
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_network_circuit_breakers_disabled() {
        let config = NetworkCircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        };
        
        let breakers = NetworkCircuitBreakers::new(&config);
        assert!(!breakers.enabled);
        assert!(breakers.can_execute("peer_connection").await);
        
        // Should not have any circuit breakers when disabled
        let status = breakers.get_all_status().await;
        assert!(status.is_empty());
    }

    #[tokio::test]
    async fn test_network_circuit_breakers_enabled() {
        let config = NetworkCircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            timeout_secs: 1,
            ..Default::default()
        };
        
        let breakers = NetworkCircuitBreakers::new(&config);
        assert!(breakers.enabled);
        assert!(breakers.can_execute("peer_connection").await);
        
        // Should have circuit breakers when enabled
        let status = breakers.get_all_status().await;
        assert!(!status.is_empty());
        assert!(status.contains_key("peer_connection"));
        assert!(status.contains_key("message_broadcast"));
    }

    #[tokio::test]
    async fn test_network_circuit_breakers_failure_handling() {
        let config = NetworkCircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1, // Low threshold for testing
            timeout_secs: 1,
            ..Default::default()
        };
        
        let breakers = NetworkCircuitBreakers::new(&config);
        
        // Record a failure
        breakers.on_failure("peer_connection", "test error").await;
        
        // Circuit should now be open
        assert!(!breakers.can_execute("peer_connection").await);
        assert!(breakers.has_failures().await);
        
        let health = breakers.health_summary().await;
        assert!(!health.overall_healthy);
        assert_eq!(health.failed_operations, 1);
    }

    #[tokio::test]
    async fn test_network_circuit_breakers_execute() {
        let config = NetworkCircuitBreakerConfig {
            enabled: true,
            operation_timeout_secs: 1,
            ..Default::default()
        };
        
        let breakers = NetworkCircuitBreakers::new(&config);
        
        // Test successful execution
        let result = breakers.execute("peer_connection", || async {
            Ok::<i32, &str>(42)
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        
        // Test failed execution
        let result = breakers.execute("peer_connection", || async {
            Err::<i32, &str>("test error")
        }).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_summary() {
        let config = NetworkCircuitBreakerConfig {
            enabled: true,
            ..Default::default()
        };
        
        let breakers = NetworkCircuitBreakers::new(&config);
        
        let health = breakers.health_summary().await;
        assert!(health.overall_healthy);
        assert!(health.total_operations > 0);
        assert_eq!(health.healthy_operations, health.total_operations);
        assert_eq!(health.failed_operations, 0);
        
        let status = health.status_string();
        assert!(status.contains("Network Healthy"));
    }
}