use p2p_play::network_circuit_breakers::*;
use p2p_play::types::NetworkCircuitBreakerConfig;

#[tokio::test]
async fn test_network_circuit_breakers_disabled() {
    let config = NetworkCircuitBreakerConfig {
        enabled: false,
        ..Default::default()
    };
    
    let breakers = NetworkCircuitBreakers::new(&config);
    assert!(breakers.can_execute("peer_connection").await);
    
    // Should not have any circuit breakers when disabled
    let status = breakers.get_all_status().await;
    assert!(status.is_empty());
    
    // Health summary should show disabled state
    let health = breakers.health_summary().await;
    assert!(health.overall_healthy);
    assert_eq!(health.total_operations, 0);
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