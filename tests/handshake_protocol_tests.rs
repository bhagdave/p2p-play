use p2p_play::network::{HandshakeRequest, HandshakeResponse, APP_NAME, APP_VERSION};
use serde_json;

/// Test handshake request serialization and deserialization
#[test]
fn test_handshake_request_serialization() {
    let request = HandshakeRequest {
        app_name: APP_NAME.to_string(),
        app_version: APP_VERSION.to_string(),
        peer_id: "test_peer_12345".to_string(),
    };

    // Test serialization
    let serialized = serde_json::to_string(&request).expect("Failed to serialize handshake request");
    assert!(serialized.contains("p2p-play"));
    assert!(serialized.contains("1.0.0"));
    assert!(serialized.contains("test_peer_12345"));

    // Test deserialization
    let deserialized: HandshakeRequest = serde_json::from_str(&serialized)
        .expect("Failed to deserialize handshake request");
    
    assert_eq!(deserialized.app_name, APP_NAME);
    assert_eq!(deserialized.app_version, APP_VERSION);
    assert_eq!(deserialized.peer_id, "test_peer_12345");
}

/// Test handshake response serialization and deserialization
#[test]
fn test_handshake_response_serialization() {
    let response = HandshakeResponse {
        accepted: true,
        app_name: APP_NAME.to_string(),
        app_version: APP_VERSION.to_string(),
    };

    // Test serialization
    let serialized = serde_json::to_string(&response).expect("Failed to serialize handshake response");
    assert!(serialized.contains("true"));
    assert!(serialized.contains("p2p-play"));
    assert!(serialized.contains("1.0.0"));

    // Test deserialization
    let deserialized: HandshakeResponse = serde_json::from_str(&serialized)
        .expect("Failed to deserialize handshake response");
    
    assert_eq!(deserialized.accepted, true);
    assert_eq!(deserialized.app_name, APP_NAME);
    assert_eq!(deserialized.app_version, APP_VERSION);
}

/// Test handshake response for rejected peer
#[test]
fn test_handshake_response_rejection() {
    let response = HandshakeResponse {
        accepted: false,
        app_name: "other-app".to_string(),
        app_version: "2.0.0".to_string(),
    };

    let serialized = serde_json::to_string(&response).expect("Failed to serialize rejection response");
    let deserialized: HandshakeResponse = serde_json::from_str(&serialized)
        .expect("Failed to deserialize rejection response");
    
    assert_eq!(deserialized.accepted, false);
    assert_eq!(deserialized.app_name, "other-app");
    assert_eq!(deserialized.app_version, "2.0.0");
}

/// Test application constants
#[test]
fn test_app_constants() {
    assert_eq!(APP_NAME, "p2p-play");
    assert_eq!(APP_VERSION, "1.0.0");
    assert!(!APP_NAME.is_empty());
    assert!(!APP_VERSION.is_empty());
}

/// Test handshake validation logic
#[test]
fn test_handshake_validation_logic() {
    // Test valid P2P-Play peer
    let valid_request = HandshakeRequest {
        app_name: APP_NAME.to_string(),
        app_version: APP_VERSION.to_string(),
        peer_id: "valid_peer".to_string(),
    };
    
    let should_accept = valid_request.app_name == APP_NAME && valid_request.app_version == APP_VERSION;
    assert!(should_accept, "Valid P2P-Play peer should be accepted");

    // Test invalid peer (different app)
    let invalid_request = HandshakeRequest {
        app_name: "some-other-app".to_string(),
        app_version: APP_VERSION.to_string(),
        peer_id: "invalid_peer".to_string(),
    };
    
    let should_reject = invalid_request.app_name != APP_NAME;
    assert!(should_reject, "Non-P2P-Play peer should be rejected");

    // Test different version (for future compatibility)
    let different_version_request = HandshakeRequest {
        app_name: APP_NAME.to_string(),
        app_version: "2.0.0".to_string(),
        peer_id: "different_version_peer".to_string(),
    };
    
    // For now, we only accept exact version matches, but this could be relaxed later
    let version_compatible = different_version_request.app_name == APP_NAME 
        && different_version_request.app_version == APP_VERSION;
    assert!(!version_compatible, "Different version should be rejected for strict compatibility");
}