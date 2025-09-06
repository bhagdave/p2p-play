use p2p_core::circuit_breaker::*;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn test_circuit_breaker_closed_state() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: Duration::from_millis(100),
        operation_timeout: Duration::from_millis(50),
        name: "test".to_string(),
    };

    let cb = CircuitBreaker::new(config);
    assert!(cb.can_execute().await);

    let info = cb.get_state().await;
    assert_eq!(info.state, CircuitState::Closed);
    assert_eq!(info.failure_count, 0);
}

#[tokio::test]
async fn test_circuit_breaker_opens_on_failures() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(100),
        operation_timeout: Duration::from_millis(50),
        name: "test".to_string(),
    };

    let cb = CircuitBreaker::new(config);

    // Record failures to trigger circuit opening
    cb.on_failure("test error 1").await;
    cb.on_failure("test error 2").await;

    let info = cb.get_state().await;
    assert!(matches!(info.state, CircuitState::Open { .. }));
    assert!(!cb.can_execute().await);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_recovery() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(50),
        operation_timeout: Duration::from_millis(50),
        name: "test".to_string(),
    };

    let cb = CircuitBreaker::new(config);

    // Open the circuit
    cb.on_failure("test error 1").await;
    cb.on_failure("test error 2").await;

    // Wait for timeout
    sleep(Duration::from_millis(60)).await;

    // Should now allow execution (half-open)
    assert!(cb.can_execute().await);

    let info = cb.get_state().await;
    assert_eq!(info.state, CircuitState::HalfOpen);

    // Record successes to close the circuit
    cb.on_success().await;
    cb.on_success().await;

    let info = cb.get_state().await;
    assert_eq!(info.state, CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_breaker_execute_success() {
    let config = CircuitBreakerConfig::default();
    let cb = CircuitBreaker::new(config);

    // Execute successful operation
    let result = cb
        .execute(|| async { Ok::<String, String>("success".to_string()) })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");

    let info = cb.get_state().await;
    assert_eq!(info.success_count, 1);
}

#[tokio::test]
async fn test_circuit_breaker_execute_failure() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        ..Default::default()
    };
    let cb = CircuitBreaker::new(config);

    // Execute failing operation
    let result = cb
        .execute(|| async { Err::<String, String>("operation failed".to_string()) })
        .await;

    assert!(result.is_err());
    // The error is wrapped in CircuitBreakerError::OperationFailed
    match result.unwrap_err() {
        p2p_core::CircuitBreakerError::OperationFailed(e) => {
            assert_eq!(e, "operation failed");
        }
        other => panic!("Expected OperationFailed error, got: {:?}", other),
    }

    let info = cb.get_state().await;
    assert_eq!(info.failure_count, 1);
    assert!(matches!(info.state, CircuitState::Open { .. }));
}

#[tokio::test]
async fn test_circuit_breaker_timeout() {
    let config = CircuitBreakerConfig {
        operation_timeout: Duration::from_millis(10),
        failure_threshold: 1,
        ..Default::default()
    };
    let cb = CircuitBreaker::new(config);

    // Execute operation that takes longer than timeout
    let result = cb
        .execute(|| async {
            sleep(Duration::from_millis(20)).await;
            Ok::<String, String>("success".to_string())
        })
        .await;

    assert!(result.is_err());
    // The error should be wrapped in CircuitBreakerError::OperationTimeout
    match result.unwrap_err() {
        p2p_core::CircuitBreakerError::OperationTimeout { .. } => {
            // Expected timeout error
        }
        other => panic!("Expected OperationTimeout error, got: {:?}", other),
    }

    let info = cb.get_state().await;
    assert_eq!(info.failure_count, 1);
}

#[tokio::test]
async fn test_circuit_breaker_config_validation() {
    // Test valid config
    let valid_config = CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 2,
        timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_millis(500),
        name: "test".to_string(),
    };
    
    // Should not panic
    let _cb = CircuitBreaker::new(valid_config);

    // Test zero thresholds (should still work, just different behavior)
    let zero_threshold_config = CircuitBreakerConfig {
        failure_threshold: 0,
        success_threshold: 0,
        timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_millis(500),
        name: "test_zero".to_string(),
    };
    
    let _cb = CircuitBreaker::new(zero_threshold_config);
}

#[tokio::test]
async fn test_circuit_breaker_statistics() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        ..Default::default()
    };
    let cb = CircuitBreaker::new(config);

    // Initially all counters should be zero
    let info = cb.get_state().await;
    assert_eq!(info.failure_count, 0);
    assert_eq!(info.success_count, 0);

    // Record some operations
    cb.on_success().await;
    cb.on_success().await;
    cb.on_failure("error 1").await;

    let info = cb.get_state().await;
    assert_eq!(info.failure_count, 1);
    assert_eq!(info.success_count, 2);
}

#[tokio::test]
async fn test_circuit_breaker_name() {
    let config = CircuitBreakerConfig {
        name: "custom-circuit".to_string(),
        ..Default::default()
    };
    let cb = CircuitBreaker::new(config);

    let info = cb.get_state().await;
    assert_eq!(info.name, "custom-circuit");
}