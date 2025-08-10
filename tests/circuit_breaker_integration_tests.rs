use p2p_play::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};
use p2p_play::network_circuit_breakers::{NetworkCircuitBreakers, NetworkHealthSummary};
use p2p_play::types::NetworkCircuitBreakerConfig;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_circuit_breaker_integration_with_network_operations() {
    // Create circuit breaker configuration similar to production
    let config = NetworkCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 2, // Low threshold for testing
        success_threshold: 2,
        timeout_secs: 1, // Short timeout for testing
        operation_timeout_secs: 1,
    };

    let network_breakers = NetworkCircuitBreakers::new(&config);

    // Test that healthy operations work normally
    let result = network_breakers
        .execute("story_publish", || async {
            Ok::<String, String>("Story published successfully".to_string())
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Story published successfully");

    // Verify network health is good initially
    let health = network_breakers.health_summary().await;
    assert!(health.overall_healthy);
    assert_eq!(health.failed_operations, 0);

    // Simulate repeated failures to trigger circuit breaker
    for _ in 0..3 {
        let result = network_breakers
            .execute("story_publish", || async {
                Err::<String, String>("Network connection failed".to_string())
            })
            .await;

        assert!(result.is_err());
    }

    // Verify network health shows problems
    let health = network_breakers.health_summary().await;
    assert!(!health.overall_healthy);
    assert!(health.failed_operations > 0);

    // Verify circuit breaker blocks subsequent requests
    let result = network_breakers
        .execute("story_publish", || async {
            Ok::<String, String>("This should be blocked".to_string())
        })
        .await;

    assert!(result.is_err());
    if let Err(CircuitBreakerError::CircuitOpen { circuit_name }) = result {
        assert_eq!(circuit_name, "story_publish");
    } else {
        panic!("Expected CircuitOpen error, got {:?}", result);
    }
}

#[tokio::test]
async fn test_circuit_breaker_recovery_after_timeout() {
    let config = NetworkCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 1,
        success_threshold: 1,
        timeout_secs: 1,
        operation_timeout_secs: 1,
    };

    let network_breakers = NetworkCircuitBreakers::new(&config);

    // Trigger circuit breaker to open
    let result = network_breakers
        .execute("peer_connection", || async {
            Err::<(), String>("Connection failed".to_string())
        })
        .await;
    assert!(result.is_err());

    // Verify circuit is open
    assert!(!network_breakers.can_execute("peer_connection").await);

    // Wait for timeout period
    sleep(Duration::from_millis(1100)).await;

    // Circuit should now allow requests (half-open state)
    assert!(network_breakers.can_execute("peer_connection").await);

    // Successful operation should close the circuit
    let result = network_breakers
        .execute("peer_connection", || async {
            Ok::<String, String>("Connection successful".to_string())
        })
        .await;

    assert!(result.is_ok());

    // Verify network health is restored
    let health = network_breakers.health_summary().await;
    assert!(health.overall_healthy);
}

#[tokio::test]
async fn test_disabled_circuit_breakers() {
    let config = NetworkCircuitBreakerConfig {
        enabled: false, // Disabled circuit breakers
        ..Default::default()
    };

    let network_breakers = NetworkCircuitBreakers::new(&config);

    // Should always allow execution when disabled
    assert!(network_breakers.can_execute("any_operation").await);

    // Multiple failures should not block operations
    for _ in 0..10 {
        let result = network_breakers
            .execute("test_operation", || async {
                Err::<(), String>("This should not be blocked".to_string())
            })
            .await;

        // Operation should still execute even if it fails
        assert!(result.is_err());
        if let Err(CircuitBreakerError::OperationFailed(_)) = result {
            // This is expected - the operation failed but was not blocked
        } else {
            panic!("Expected OperationFailed, got {:?}", result);
        }
    }

    // Network health should show as healthy since circuit breakers are disabled
    let health = network_breakers.health_summary().await;
    assert!(health.overall_healthy);
    assert_eq!(health.total_operations, 0); // No circuit breakers when disabled
}

#[tokio::test]
async fn test_network_health_summary_functionality() {
    let config = NetworkCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 1,
        success_threshold: 1,
        timeout_secs: 60,
        operation_timeout_secs: 30,
    };

    let network_breakers = NetworkCircuitBreakers::new(&config);

    // Initial health should be good
    let health = network_breakers.health_summary().await;
    assert!(health.overall_healthy);
    assert!(health.total_operations > 0); // Should have multiple circuit breakers
    assert_eq!(health.healthy_operations, health.total_operations);
    assert_eq!(health.failed_operations, 0);

    // Test status string formatting
    let status = health.status_string();
    assert!(status.contains("Network Healthy"));
    assert!(status.contains(&format!(
        "{}/{}",
        health.total_operations, health.total_operations
    )));

    // Cause some failures
    network_breakers
        .on_failure("message_broadcast", "Test failure")
        .await;

    let health = network_breakers.health_summary().await;
    assert!(!health.overall_healthy);
    assert!(health.failed_operations > 0);

    let status = health.status_string();
    assert!(status.contains("Network Issues"));
}

#[tokio::test]
async fn test_circuit_breaker_timeout_handling() {
    let config = NetworkCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 5,
        success_threshold: 2,
        timeout_secs: 60,
        operation_timeout_secs: 1, // Very short timeout
    };

    let network_breakers = NetworkCircuitBreakers::new(&config);

    // Test operation that times out
    let result = network_breakers
        .execute("dht_bootstrap", || async {
            // Simulate a long-running operation that will timeout
            sleep(Duration::from_secs(2)).await;
            Ok::<String, String>("Should timeout before this".to_string())
        })
        .await;

    assert!(result.is_err());
    if let Err(CircuitBreakerError::OperationTimeout {
        circuit_name,
        timeout,
    }) = result
    {
        assert_eq!(circuit_name, "dht_bootstrap");
        assert_eq!(timeout, Duration::from_secs(1));
    } else {
        panic!("Expected OperationTimeout error, got {:?}", result);
    }

    // Timeout should count as a failure
    let health = network_breakers.health_summary().await;
    let status = network_breakers.get_all_status().await;
    let dht_status = status.get("dht_bootstrap").unwrap();
    assert_eq!(dht_status.total_failures, 1);
}
