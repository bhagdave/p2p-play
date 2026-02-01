use p2p_play::network::{APP_NAME, APP_VERSION, HandshakeRequest, HandshakeResponse};

/// Test handshake request serialization and deserialization
#[test]
fn test_handshake_request_serialization() {
    let request = HandshakeRequest {
        app_name: APP_NAME.to_string(),
        app_version: APP_VERSION.to_string(),
        peer_id: "test_peer_12345".to_string(),
        wasm_capable: true,
    };

    // Test serialization
    let serialized =
        serde_json::to_string(&request).expect("Failed to serialize handshake request");
    assert!(serialized.contains("p2p-play"));
    assert!(serialized.contains("0.10.6"));
    assert!(serialized.contains("test_peer_12345"));

    // Test deserialization
    let deserialized: HandshakeRequest =
        serde_json::from_str(&serialized).expect("Failed to deserialize handshake request");

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
        wasm_capable: true,
    };

    // Test serialization
    let serialized =
        serde_json::to_string(&response).expect("Failed to serialize handshake response");
    assert!(serialized.contains("true"));
    assert!(serialized.contains("p2p-play"));
    assert!(serialized.contains("0.10.6"));

    // Test deserialization
    let deserialized: HandshakeResponse =
        serde_json::from_str(&serialized).expect("Failed to deserialize handshake response");

    assert!(deserialized.accepted);
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
        wasm_capable: false,
    };

    let serialized =
        serde_json::to_string(&response).expect("Failed to serialize rejection response");
    let deserialized: HandshakeResponse =
        serde_json::from_str(&serialized).expect("Failed to deserialize rejection response");

    assert!(!deserialized.accepted);
    assert_eq!(deserialized.app_name, "other-app");
    assert_eq!(deserialized.app_version, "2.0.0");
}

/// Test application constants
#[test]
fn test_app_constants() {
    assert_eq!(APP_NAME, "p2p-play");
    assert_eq!(APP_VERSION, "0.10.6");
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
        wasm_capable: true,
    };

    // Now we only check app name, not version
    let should_accept = valid_request.app_name == APP_NAME;
    assert!(should_accept, "Valid P2P-Play peer should be accepted");

    // Test invalid peer (different app)
    let invalid_request = HandshakeRequest {
        app_name: "some-other-app".to_string(),
        app_version: APP_VERSION.to_string(),
        peer_id: "invalid_peer".to_string(),
        wasm_capable: false,
    };

    let should_reject = invalid_request.app_name != APP_NAME;
    assert!(should_reject, "Non-P2P-Play peer should be rejected");

    // Test different version (for future compatibility)
    let different_version_request = HandshakeRequest {
        app_name: APP_NAME.to_string(),
        app_version: "2.0.0".to_string(),
        peer_id: "different_version_peer".to_string(),
        wasm_capable: true,
    };

    // With relaxed version checking, only app name matters
    let version_compatible = different_version_request.app_name == APP_NAME;
    assert!(
        version_compatible,
        "Different version should be accepted with app name matching"
    );
}
