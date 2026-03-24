use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerInfo};
use crate::types::NetworkCircuitBreakerConfig;
use std::collections::HashMap;

#[derive(Debug)]
pub struct NetworkCircuitBreakers {
    breakers: HashMap<String, CircuitBreaker>,
    enabled: bool,
}

impl NetworkCircuitBreakers {
    pub fn new(config: &NetworkCircuitBreakerConfig) -> Self {
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

    pub fn get(&self, operation: &str) -> Option<&CircuitBreaker> {
        if self.enabled {
            self.breakers.get(operation)
        } else {
            None
        }
    }

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
            match func().await {
                Ok(result) => Ok(result),
                Err(error) => {
                    Err(crate::circuit_breaker::CircuitBreakerError::OperationFailed(error))
                }
            }
        }
    }

    #[allow(dead_code)]
    pub async fn can_execute(&self, operation: &str) -> bool {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.can_execute().await
        } else {
            true // Always allow if disabled
        }
    }

    #[allow(dead_code)]
    pub async fn on_success(&self, operation: &str) {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.on_success().await;
        }
    }

    #[allow(dead_code)]
    pub async fn on_failure(&self, operation: &str, error: &str) {
        if let Some(circuit_breaker) = self.get(operation) {
            circuit_breaker.on_failure(error).await;
        }
    }

    #[allow(dead_code)]
    pub async fn get_all_status(&self) -> HashMap<String, CircuitBreakerInfo> {
        let mut status = HashMap::new();

        for (operation, circuit_breaker) in &self.breakers {
            let info = circuit_breaker.get_state().await;
            status.insert(operation.clone(), info);
        }

        status
    }

    #[allow(dead_code)]
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

    pub async fn health_summary(&self) -> NetworkHealthSummary {
        if !self.enabled {
            return NetworkHealthSummary {
                overall_healthy: true,
                total_operations: 0,
                healthy_operations: 0,
                failed_operations: 0,
            };
        }

        let mut total_operations = 0;
        let mut healthy_operations = 0;
        let mut failed_operations = 0;

        for circuit_breaker in self.breakers.values() {
            let info = circuit_breaker.get_state().await;
            total_operations += 1;

            if info.is_healthy() {
                healthy_operations += 1;
            } else {
                failed_operations += 1;
            }
        }

        NetworkHealthSummary {
            overall_healthy: failed_operations == 0,
            total_operations,
            healthy_operations,
            failed_operations,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkHealthSummary {
    pub overall_healthy: bool,
    #[allow(dead_code)]
    pub total_operations: usize,
    #[allow(dead_code)]
    pub healthy_operations: usize,
    #[allow(dead_code)]
    pub failed_operations: usize,
}

impl NetworkHealthSummary {
    /// Get a human-readable health status
    #[allow(dead_code)]
    pub fn status_string(&self) -> String {
        if self.overall_healthy {
            format!(
                "Network Healthy ({}/{} operations)",
                self.healthy_operations, self.total_operations
            )
        } else {
            format!(
                "Network Issues ({}/{} operations failing)",
                self.failed_operations, self.total_operations
            )
        }
    }
}
