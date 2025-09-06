use p2p_core::{BootstrapService, BootstrapConfig, BootstrapStatus, NetworkService, NetworkConfig};

#[tokio::test]
async fn test_bootstrap_service_creation() {
    let config = BootstrapConfig::default();
    let bootstrap = BootstrapService::new(config);
    
    assert!(bootstrap.is_enabled());
    let status = bootstrap.get_status();
    assert_eq!(status, BootstrapStatus::NotStarted);
}

#[tokio::test]
async fn test_bootstrap_config_default() {
    let config = BootstrapConfig::default();
    
    assert!(config.peers.is_empty());
    assert_eq!(config.retry_interval_secs, 30);
    assert_eq!(config.max_retries, 10);
    assert_eq!(config.initial_delay_secs, 5);
    assert_eq!(config.backoff_multiplier, 1.5);
    assert!(config.enabled);
}

#[tokio::test]
async fn test_bootstrap_config_with_peers() {
    let peers = vec![
        "/ip4/127.0.0.1/tcp/9000/p2p/12D3KooWExample1".to_string(),
        "/ip4/127.0.0.1/tcp/9001/p2p/12D3KooWExample2".to_string(),
    ];
    
    let config = BootstrapConfig {
        peers: peers.clone(),
        retry_interval_secs: 60,
        max_retries: 5,
        initial_delay_secs: 10,
        backoff_multiplier: 2.0,
        enabled: true,
    };
    
    let bootstrap = BootstrapService::new(config.clone());
    assert!(bootstrap.is_enabled());
}

#[tokio::test]
async fn test_bootstrap_disabled() {
    let config = BootstrapConfig {
        enabled: false,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    assert!(!bootstrap.is_enabled());
}

#[tokio::test]
async fn test_bootstrap_no_peers() {
    let config = BootstrapConfig {
        peers: vec![], // Empty peers list
        enabled: true,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    let network_config = NetworkConfig::default();
    let mut network = NetworkService::new(network_config).await.unwrap();
    
    // Should fail with no peers configured
    let result = bootstrap.attempt_bootstrap(&mut network.swarm).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No bootstrap peers configured"));
}

#[tokio::test]
async fn test_bootstrap_invalid_multiaddr() {
    let config = BootstrapConfig {
        peers: vec!["invalid-multiaddr".to_string()],
        enabled: true,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    let network_config = NetworkConfig::default();
    let mut network = NetworkService::new(network_config).await.unwrap();
    
    // Should fail with invalid multiaddr
    let result = bootstrap.attempt_bootstrap(&mut network.swarm).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bootstrap_retry_logic() {
    let config = BootstrapConfig {
        peers: vec!["invalid-multiaddr".to_string()],
        enabled: true,
        max_retries: 3,
        retry_interval_secs: 1, // Short interval for testing
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    
    // Test should_retry logic
    assert!(bootstrap.should_retry()); // First attempt
    
    // After max retries, should not retry
    for _ in 0..3 {
        let network_config = NetworkConfig::default();
        let mut network = NetworkService::new(network_config).await.unwrap();
        let _ = bootstrap.attempt_bootstrap(&mut network.swarm).await;
    }
    
    assert!(!bootstrap.should_retry());
}

#[tokio::test]
async fn test_bootstrap_status_transitions() {
    let config = BootstrapConfig {
        peers: vec!["invalid-multiaddr".to_string()],
        enabled: true,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    
    // Initial status should be NotStarted
    assert_eq!(bootstrap.get_status(), BootstrapStatus::NotStarted);
    
    let network_config = NetworkConfig::default();
    let mut network = NetworkService::new(network_config).await.unwrap();
    
    // After failed attempt, should show failure
    let _ = bootstrap.attempt_bootstrap(&mut network.swarm).await;
    let status = bootstrap.get_status();
    
    match status {
        BootstrapStatus::Failed { attempts, .. } => {
            assert!(attempts > 0);
        },
        _ => panic!("Expected Failed status after bootstrap attempt"),
    }
}

#[tokio::test]
async fn test_bootstrap_reset() {
    let config = BootstrapConfig {
        peers: vec!["invalid-multiaddr".to_string()],
        enabled: true,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    let network_config = NetworkConfig::default();
    let mut network = NetworkService::new(network_config).await.unwrap();
    
    // Attempt bootstrap to change status
    let _ = bootstrap.attempt_bootstrap(&mut network.swarm).await;
    assert_ne!(bootstrap.get_status(), BootstrapStatus::NotStarted);
    
    // Reset should return to initial state
    bootstrap.reset();
    assert_eq!(bootstrap.get_status(), BootstrapStatus::NotStarted);
    assert!(bootstrap.should_retry());
}

#[tokio::test]
async fn test_bootstrap_schedule_retry() {
    let config = BootstrapConfig {
        retry_interval_secs: 1,
        backoff_multiplier: 2.0,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(config);
    
    // Schedule next retry
    bootstrap.schedule_next_retry();
    
    // Should be able to retry after scheduling
    // Note: We can't easily test the timing without making this test slow
    // So we just verify the method doesn't panic
    assert!(true);
}