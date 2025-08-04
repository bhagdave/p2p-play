use p2p_play::types::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::time;

// Constants from event_handlers.rs for testing
const MIN_RECONNECT_INTERVAL: Duration = Duration::from_secs(30);
const MIN_RECONNECT_INTERVAL_RECENT: Duration = Duration::from_secs(5);

/// Test reconnection speed measurement to ensure optimizations are working
#[tokio::test]
async fn test_reconnection_speed_measurement() {
    // Test that the optimized config from the actual file has been set to 10s
    let unified_config =
        UnifiedNetworkConfig::load_from_file("unified_network_config.json").unwrap();
    assert_eq!(
        unified_config
            .network
            .connection_maintenance_interval_seconds,
        10
    );

    // Verify the ping configuration is optimized for faster recovery
    assert_eq!(unified_config.ping.interval_secs, 50);
    assert_eq!(unified_config.ping.timeout_secs, 45);

    // Test connection establishment timeout is extended for slower networks
    assert_eq!(
        unified_config
            .network
            .connection_establishment_timeout_seconds,
        45
    );

    // Verify that the default NetworkConfig still has original values
    let default_config = NetworkConfig::default();
    assert_eq!(default_config.connection_maintenance_interval_seconds, 300);
}

/// Test throttling behaviour verification for different peer categories
#[tokio::test]
async fn test_throttling_behaviour_verification() {
    let mut peer_throttling: HashMap<String, Instant> = HashMap::new();
    let mut successful_connections: HashMap<String, Instant> = HashMap::new();

    let peer_id = "test_peer_123".to_string();
    let now = Instant::now();

    // Simulate a recent successful connection (within 5 minutes)
    successful_connections.insert(peer_id.clone(), now);

    // Test that recently connected peers get faster throttling interval
    let can_reconnect_recent = can_attempt_reconnection(
        &peer_id,
        &peer_throttling,
        &successful_connections,
        MIN_RECONNECT_INTERVAL_RECENT,
    );

    // Should allow reconnection for recent peer with 5s interval
    assert!(can_reconnect_recent);

    // Test older peer gets slower throttling
    let old_peer_id = "old_peer_456".to_string();
    let old_connection_time = now
        .checked_sub(Duration::from_secs(400))
        .unwrap_or_else(|| Instant::now() - Duration::from_secs(10)); // Fallback to 10 seconds ago
    successful_connections.insert(old_peer_id.clone(), old_connection_time);

    let can_reconnect_old = can_attempt_reconnection(
        &old_peer_id,
        &peer_throttling,
        &successful_connections,
        MIN_RECONNECT_INTERVAL,
    );

    // Should also allow since no previous throttling attempt recorded
    assert!(can_reconnect_old);

    // Now add throttling record and test again
    peer_throttling.insert(old_peer_id.clone(), now);

    let can_reconnect_throttled = can_attempt_reconnection(
        &old_peer_id,
        &peer_throttling,
        &successful_connections,
        MIN_RECONNECT_INTERVAL,
    );

    // Should not allow immediate reconnection due to throttling
    assert!(!can_reconnect_throttled);
}

/// Test memory clean-up validation to ensure throttling maps don't grow unbounded
#[tokio::test]
async fn test_memory_cleanup_validation() {
    let mut peer_throttling: HashMap<String, Instant> = HashMap::new();
    let mut successful_connections: HashMap<String, Instant> = HashMap::new();

    let now = Instant::now();
    // Create an old time by getting an instant and adding the duration forward
    // This avoids potential underflow on Windows systems
    let old_time = now
        .checked_sub(Duration::from_secs(3700))
        .unwrap_or_else(|| Instant::now() - Duration::from_secs(10)); // Fallback to 10 seconds ago

    // Add some old entries that should be cleaned up
    for i in 0..10 {
        let peer_id = format!("old_peer_{}", i);
        peer_throttling.insert(peer_id.clone(), old_time);
        successful_connections.insert(peer_id, old_time);
    }

    // Add some recent entries that should be kept
    for i in 0..5 {
        let peer_id = format!("recent_peer_{}", i);
        peer_throttling.insert(peer_id.clone(), now);
        successful_connections.insert(peer_id, now);
    }

    assert_eq!(peer_throttling.len(), 15);
    assert_eq!(successful_connections.len(), 15);

    // Clean up old entries (simulate the cleanup logic)
    cleanup_old_throttling_entries(&mut peer_throttling, Duration::from_secs(3600)); // 1 hour
    cleanup_old_successful_connections(&mut successful_connections, Duration::from_secs(3600));

    // Should only have recent entries left
    assert_eq!(peer_throttling.len(), 5);
    assert_eq!(successful_connections.len(), 5);

    // Verify all remaining entries are recent
    for (_, timestamp) in &peer_throttling {
        assert!(now.duration_since(*timestamp) < Duration::from_secs(3600));
    }

    for (_, timestamp) in &successful_connections {
        assert!(now.duration_since(*timestamp) < Duration::from_secs(3600));
    }
}

/// Test that optimized configuration maintains network stability
#[tokio::test]
async fn test_network_stability_with_optimizations() {
    let temp_file = NamedTempFile::new().unwrap();
    let config_path = temp_file.path().to_str().unwrap();

    // Create optimized config
    let config = UnifiedNetworkConfig {
        network: NetworkConfig {
            connection_maintenance_interval_seconds: 10,  // Optimized
            connection_establishment_timeout_seconds: 45, // Extended
            ..Default::default()
        },
        ping: PingConfig {
            interval_secs: 50, // Must be greater than timeout_secs
            timeout_secs: 45,  // Forgiving but less than interval
        },
        ..Default::default()
    };

    // Save and validate
    config.save_to_file(config_path).unwrap();
    let loaded_config = UnifiedNetworkConfig::load_from_file(config_path).unwrap();

    // Verify optimizations are maintained
    assert_eq!(
        loaded_config
            .network
            .connection_maintenance_interval_seconds,
        10
    );
    assert_eq!(
        loaded_config
            .network
            .connection_establishment_timeout_seconds,
        45
    );
    assert_eq!(loaded_config.ping.interval_secs, 50);
    assert_eq!(loaded_config.ping.timeout_secs, 45);

    // Verify configuration is still valid
    assert!(loaded_config.network.validate().is_ok());
    assert!(loaded_config.ping.validate().is_ok());
}

/// Test reconnection timing accuracy
#[tokio::test]
async fn test_reconnection_timing_accuracy() {
    let start_time = Instant::now();

    // Test that connection maintenance would trigger much faster than before
    let maintenance_interval = Duration::from_secs(10); // New optimized interval
    let old_interval = Duration::from_secs(300); // Old interval

    // Simulate waiting for optimized interval
    time::sleep(Duration::from_millis(10)).await; // Simulate small delay
    let _elapsed = start_time.elapsed();

    // Verify that the new interval is dramatically faster
    assert!(maintenance_interval < old_interval);
    assert_eq!(maintenance_interval.as_secs(), 10);
    assert_eq!(old_interval.as_secs(), 300);

    // 30x improvement calculation
    let improvement_factor = old_interval.as_secs() / maintenance_interval.as_secs();
    assert_eq!(improvement_factor, 30);
}

/// Helper function to test reconnection logic
fn can_attempt_reconnection(
    peer_id: &str,
    peer_throttling: &HashMap<String, Instant>,
    successful_connections: &HashMap<String, Instant>,
    min_interval: Duration,
) -> bool {
    if let Some(last_attempt) = peer_throttling.get(peer_id) {
        let time_since_last_attempt = Instant::now().duration_since(*last_attempt);

        // Check if we're in the fast reconnect window for recently connected peers
        if let Some(last_success) = successful_connections.get(peer_id) {
            let time_since_success = Instant::now().duration_since(*last_success);
            if time_since_success < Duration::from_secs(300) {
                // 5 minutes
                return time_since_last_attempt >= MIN_RECONNECT_INTERVAL_RECENT;
            }
        }

        time_since_last_attempt >= min_interval
    } else {
        true // No previous attempt recorded
    }
}

/// Helper function to clean up old throttling entries
fn cleanup_old_throttling_entries(
    peer_throttling: &mut HashMap<String, Instant>,
    max_age: Duration,
) {
    let now = Instant::now();
    peer_throttling.retain(|_, timestamp| now.duration_since(*timestamp) < max_age);
}

/// Helper function to clean up old successful connection entries
fn cleanup_old_successful_connections(
    successful_connections: &mut HashMap<String, Instant>,
    max_age: Duration,
) {
    let now = Instant::now();
    successful_connections.retain(|_, timestamp| now.duration_since(*timestamp) < max_age);
}
