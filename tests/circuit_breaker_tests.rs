use p2p_play::circuit_breaker::*;
use tokio::time::{sleep, Duration};

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
    
    let result = cb.execute(|| async { Ok::<i32, &str>(42) }).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    
    let info = cb.get_state().await;
    assert_eq!(info.total_successes, 1);
}

#[tokio::test]
async fn test_circuit_breaker_execute_failure() {
    let config = CircuitBreakerConfig::default();
    let cb = CircuitBreaker::new(config);
    
    let result = cb.execute(|| async { Err::<i32, &str>("test error") }).await;
    assert!(result.is_err());
    
    let info = cb.get_state().await;
    assert_eq!(info.total_failures, 1);
}

#[tokio::test]
async fn test_circuit_breaker_execute_timeout() {
    let config = CircuitBreakerConfig {
        operation_timeout: Duration::from_millis(10),
        ..Default::default()
    };
    let cb = CircuitBreaker::new(config);
    
    let result = cb.execute(|| async {
        sleep(Duration::from_millis(50)).await;
        Ok::<i32, &str>(42)
    }).await;
    
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CircuitBreakerError::OperationTimeout { .. }));
    
    let info = cb.get_state().await;
    assert_eq!(info.total_failures, 1);
}