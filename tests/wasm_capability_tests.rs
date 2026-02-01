use p2p_play::network::{
    WasmCapabilitiesRequest, WasmCapabilitiesResponse, WasmExecutionRequest, WasmExecutionResponse,
};
use p2p_play::types::{WasmOffering, WasmParameter, WasmResourceRequirements};
use std::time::{SystemTime, UNIX_EPOCH};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn create_test_offering() -> WasmOffering {
    WasmOffering {
        id: "test-offering-123".to_string(),
        name: "Test WASM Module".to_string(),
        description: "A test WASM module for unit testing".to_string(),
        ipfs_cid: "QmTest1234567890abcdefghijklmnopqrstuvwxyz12".to_string(),
        parameters: vec![
            WasmParameter {
                name: "input".to_string(),
                param_type: "string".to_string(),
                description: "Input text to process".to_string(),
                required: true,
                default_value: None,
            },
            WasmParameter {
                name: "format".to_string(),
                param_type: "string".to_string(),
                description: "Output format".to_string(),
                required: false,
                default_value: Some("json".to_string()),
            },
        ],
        resource_requirements: WasmResourceRequirements {
            min_fuel: 1000,
            max_fuel: 100000,
            min_memory_mb: 16,
            max_memory_mb: 128,
            estimated_timeout_secs: 30,
        },
        version: "1.0.0".to_string(),
        enabled: true,
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    }
}

// ============================================================================
// WasmOffering Tests
// ============================================================================

#[test]
fn test_wasm_offering_serialization() {
    let offering = create_test_offering();

    let serialized = serde_json::to_string(&offering).expect("Failed to serialize WasmOffering");
    assert!(serialized.contains("Test WASM Module"));
    assert!(serialized.contains("QmTest1234567890"));
    assert!(serialized.contains("1.0.0"));

    let deserialized: WasmOffering =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmOffering");

    assert_eq!(deserialized.id, offering.id);
    assert_eq!(deserialized.name, offering.name);
    assert_eq!(deserialized.ipfs_cid, offering.ipfs_cid);
    assert_eq!(deserialized.version, offering.version);
    assert_eq!(deserialized.enabled, offering.enabled);
    assert_eq!(deserialized.parameters.len(), 2);
}

#[test]
fn test_wasm_parameter_serialization() {
    let param = WasmParameter {
        name: "test_param".to_string(),
        param_type: "bytes".to_string(),
        description: "A test parameter".to_string(),
        required: true,
        default_value: None,
    };

    let serialized = serde_json::to_string(&param).expect("Failed to serialize WasmParameter");
    let deserialized: WasmParameter =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmParameter");

    assert_eq!(deserialized.name, param.name);
    assert_eq!(deserialized.param_type, param.param_type);
    assert_eq!(deserialized.required, param.required);
    assert!(deserialized.default_value.is_none());
}

#[test]
fn test_wasm_parameter_with_default_value() {
    let param = WasmParameter {
        name: "format".to_string(),
        param_type: "string".to_string(),
        description: "Output format".to_string(),
        required: false,
        default_value: Some("json".to_string()),
    };

    let serialized = serde_json::to_string(&param).expect("Failed to serialize");
    let deserialized: WasmParameter = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert_eq!(deserialized.default_value, Some("json".to_string()));
    assert!(!deserialized.required);
}

#[test]
fn test_wasm_resource_requirements_serialization() {
    let requirements = WasmResourceRequirements {
        min_fuel: 500,
        max_fuel: 50000,
        min_memory_mb: 8,
        max_memory_mb: 64,
        estimated_timeout_secs: 15,
    };

    let serialized =
        serde_json::to_string(&requirements).expect("Failed to serialize WasmResourceRequirements");
    let deserialized: WasmResourceRequirements =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmResourceRequirements");

    assert_eq!(deserialized.min_fuel, requirements.min_fuel);
    assert_eq!(deserialized.max_fuel, requirements.max_fuel);
    assert_eq!(deserialized.min_memory_mb, requirements.min_memory_mb);
    assert_eq!(deserialized.max_memory_mb, requirements.max_memory_mb);
    assert_eq!(
        deserialized.estimated_timeout_secs,
        requirements.estimated_timeout_secs
    );
}

#[test]
fn test_wasm_resource_requirements_default() {
    let requirements = WasmResourceRequirements::default_requirements();

    // Should have sensible defaults
    assert!(requirements.min_fuel > 0);
    assert!(requirements.max_fuel >= requirements.min_fuel);
    assert!(requirements.min_memory_mb > 0);
    assert!(requirements.max_memory_mb >= requirements.min_memory_mb);
    assert!(requirements.estimated_timeout_secs > 0);
}

// ============================================================================
// WasmCapabilities Protocol Tests
// ============================================================================

#[test]
fn test_wasm_capabilities_request_serialization() {
    let request = WasmCapabilitiesRequest {
        from_peer_id: "12D3KooWTestPeerId12345".to_string(),
        from_name: "TestNode".to_string(),
        timestamp: current_timestamp(),
        include_parameters: true,
    };

    let serialized =
        serde_json::to_string(&request).expect("Failed to serialize WasmCapabilitiesRequest");
    assert!(serialized.contains("12D3KooWTestPeerId"));
    assert!(serialized.contains("TestNode"));
    assert!(serialized.contains("include_parameters"));

    let deserialized: WasmCapabilitiesRequest =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmCapabilitiesRequest");

    assert_eq!(deserialized.from_peer_id, request.from_peer_id);
    assert_eq!(deserialized.from_name, request.from_name);
    assert!(deserialized.include_parameters);
}

#[test]
fn test_wasm_capabilities_response_serialization() {
    let offerings = vec![create_test_offering()];
    let response = WasmCapabilitiesResponse {
        peer_id: "12D3KooWTestPeerId12345".to_string(),
        peer_name: "TestNode".to_string(),
        wasm_enabled: true,
        offerings,
        timestamp: current_timestamp(),
    };

    let serialized =
        serde_json::to_string(&response).expect("Failed to serialize WasmCapabilitiesResponse");
    assert!(serialized.contains("wasm_enabled"));
    assert!(serialized.contains("TestNode"));
    assert!(serialized.contains("offerings"));

    let deserialized: WasmCapabilitiesResponse =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmCapabilitiesResponse");

    assert_eq!(deserialized.peer_id, response.peer_id);
    assert!(deserialized.wasm_enabled);
    assert_eq!(deserialized.offerings.len(), 1);
    assert_eq!(deserialized.offerings[0].name, "Test WASM Module");
}

#[test]
fn test_wasm_capabilities_response_no_offerings() {
    let response = WasmCapabilitiesResponse {
        peer_id: "12D3KooWTestPeerId12345".to_string(),
        peer_name: "EmptyNode".to_string(),
        wasm_enabled: false,
        offerings: vec![],
        timestamp: current_timestamp(),
    };

    let serialized = serde_json::to_string(&response).expect("Failed to serialize");
    let deserialized: WasmCapabilitiesResponse = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert!(!deserialized.wasm_enabled);
    assert!(deserialized.offerings.is_empty());
}

// ============================================================================
// WasmExecution Protocol Tests
// ============================================================================

#[test]
fn test_wasm_execution_request_serialization() {
    let request = WasmExecutionRequest {
        from_peer_id: "12D3KooWTestPeerId12345".to_string(),
        from_name: "TestNode".to_string(),
        offering_id: "test-offering-123".to_string(),
        ipfs_cid: "QmTest1234567890abcdefghijklmnopqrstuvwxyz12".to_string(),
        input: b"Hello, WASM!".to_vec(),
        args: vec!["--format".to_string(), "json".to_string()],
        fuel_limit: Some(50000),
        memory_limit_mb: Some(64),
        timeout_secs: Some(30),
        timestamp: current_timestamp(),
    };

    let serialized =
        serde_json::to_string(&request).expect("Failed to serialize WasmExecutionRequest");
    assert!(serialized.contains("test-offering-123"));
    assert!(serialized.contains("QmTest1234567890"));

    let deserialized: WasmExecutionRequest =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmExecutionRequest");

    assert_eq!(deserialized.offering_id, request.offering_id);
    assert_eq!(deserialized.ipfs_cid, request.ipfs_cid);
    assert_eq!(deserialized.input, b"Hello, WASM!");
    assert_eq!(deserialized.args.len(), 2);
    assert_eq!(deserialized.fuel_limit, Some(50000));
}

#[test]
fn test_wasm_execution_request_minimal() {
    let request = WasmExecutionRequest {
        from_peer_id: "12D3KooWTestPeerId12345".to_string(),
        from_name: "TestNode".to_string(),
        offering_id: "test-offering-123".to_string(),
        ipfs_cid: "QmMinimal123456789".to_string(),
        input: vec![],
        args: vec![],
        fuel_limit: None,
        memory_limit_mb: None,
        timeout_secs: None,
        timestamp: current_timestamp(),
    };

    let serialized = serde_json::to_string(&request).expect("Failed to serialize");
    let deserialized: WasmExecutionRequest = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert!(deserialized.input.is_empty());
    assert!(deserialized.args.is_empty());
    assert!(deserialized.fuel_limit.is_none());
}

#[test]
fn test_wasm_execution_response_success() {
    let response = WasmExecutionResponse {
        success: true,
        stdout: b"Hello from WASM!".to_vec(),
        stderr: vec![],
        fuel_consumed: 12345,
        exit_code: 0,
        error: None,
        timestamp: current_timestamp(),
    };

    let serialized =
        serde_json::to_string(&response).expect("Failed to serialize WasmExecutionResponse");
    let deserialized: WasmExecutionResponse =
        serde_json::from_str(&serialized).expect("Failed to deserialize WasmExecutionResponse");

    assert!(deserialized.success);
    assert_eq!(deserialized.stdout, b"Hello from WASM!");
    assert!(deserialized.stderr.is_empty());
    assert_eq!(deserialized.fuel_consumed, 12345);
    assert_eq!(deserialized.exit_code, 0);
    assert!(deserialized.error.is_none());
}

#[test]
fn test_wasm_execution_response_failure() {
    let response = WasmExecutionResponse {
        success: false,
        stdout: vec![],
        stderr: b"Error: Out of fuel".to_vec(),
        fuel_consumed: 100000,
        exit_code: 1,
        error: Some("WASM execution exceeded fuel limit".to_string()),
        timestamp: current_timestamp(),
    };

    let serialized = serde_json::to_string(&response).expect("Failed to serialize");
    let deserialized: WasmExecutionResponse = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert!(!deserialized.success);
    assert_eq!(deserialized.exit_code, 1);
    assert!(deserialized.error.is_some());
    assert!(deserialized.error.unwrap().contains("fuel limit"));
}

// ============================================================================
// Validation Tests
// ============================================================================

#[test]
fn test_validate_wasm_offering_name() {
    use p2p_play::validation::ContentValidator;

    // Valid names (alphanumeric, hyphens, underscores, dots - no spaces)
    assert!(ContentValidator::validate_wasm_offering_name("test-module").is_ok());
    assert!(ContentValidator::validate_wasm_offering_name("Module_v1.0").is_ok());
    assert!(ContentValidator::validate_wasm_offering_name("MyWASMModule").is_ok());

    // Invalid - contains spaces (not allowed)
    assert!(ContentValidator::validate_wasm_offering_name("My WASM Module").is_err());

    // Empty name
    assert!(ContentValidator::validate_wasm_offering_name("").is_err());

    // Too long (over 100 chars)
    let long_name = "a".repeat(101);
    assert!(ContentValidator::validate_wasm_offering_name(&long_name).is_err());
}

#[test]
fn test_validate_wasm_offering_description() {
    use p2p_play::validation::ContentValidator;

    // Valid descriptions
    assert!(ContentValidator::validate_wasm_offering_description("A simple WASM module").is_ok());
    assert!(ContentValidator::validate_wasm_offering_description("Processes data").is_ok());

    // Empty description is NOT valid
    assert!(ContentValidator::validate_wasm_offering_description("").is_err());

    // Too long (over 500 chars)
    let long_desc = "a".repeat(501);
    assert!(ContentValidator::validate_wasm_offering_description(&long_desc).is_err());
}

#[test]
fn test_validate_ipfs_cid() {
    use p2p_play::validation::ContentValidator;

    // Valid CIDv0 (starts with Qm)
    assert!(ContentValidator::validate_ipfs_cid("QmTest1234567890abcdefghijklmnopqrstuvwxyz12").is_ok());
    assert!(ContentValidator::validate_ipfs_cid("QmShort").is_ok()); // Short CIDv0 is accepted

    // Valid CIDv1 (starts with bafy)
    assert!(ContentValidator::validate_ipfs_cid("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").is_ok());

    // Invalid - empty
    assert!(ContentValidator::validate_ipfs_cid("").is_err());

    // Invalid - wrong prefix
    assert!(ContentValidator::validate_ipfs_cid("invalid123").is_err());
    assert!(ContentValidator::validate_ipfs_cid("Zb2rhtest").is_err()); // CIDv1b (not bafy)
}

#[test]
fn test_validate_wasm_version() {
    use p2p_play::validation::ContentValidator;

    // Valid semver versions
    assert!(ContentValidator::validate_wasm_version("1.0.0").is_ok());
    assert!(ContentValidator::validate_wasm_version("0.1.0").is_ok());
    assert!(ContentValidator::validate_wasm_version("10.20.30").is_ok());
    assert!(ContentValidator::validate_wasm_version("1.0.0-alpha").is_ok());
    assert!(ContentValidator::validate_wasm_version("1.0.0-beta.1").is_ok());
    assert!(ContentValidator::validate_wasm_version("1.0").is_ok()); // Two-part versions are valid

    // Invalid versions
    assert!(ContentValidator::validate_wasm_version("").is_err());
    assert!(ContentValidator::validate_wasm_version("v1.0.0").is_err()); // No 'v' prefix
    assert!(ContentValidator::validate_wasm_version("invalid").is_err()); // Non-numeric
}

#[test]
fn test_validate_wasm_param_name() {
    use p2p_play::validation::ContentValidator;

    // Valid parameter names
    assert!(ContentValidator::validate_wasm_param_name("input").is_ok());
    assert!(ContentValidator::validate_wasm_param_name("output_format").is_ok());
    assert!(ContentValidator::validate_wasm_param_name("param1").is_ok());

    // Invalid - empty
    assert!(ContentValidator::validate_wasm_param_name("").is_err());

    // Invalid - too long (over 50 chars)
    let long_name = "a".repeat(51);
    assert!(ContentValidator::validate_wasm_param_name(&long_name).is_err());
}

#[test]
fn test_validate_wasm_param_type() {
    use p2p_play::validation::ContentValidator;

    // Valid types: string, bytes, json, int, float, bool, file
    assert!(ContentValidator::validate_wasm_param_type("string").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("bytes").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("json").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("int").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("float").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("bool").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("file").is_ok());
    assert!(ContentValidator::validate_wasm_param_type("STRING").is_ok()); // Case-insensitive

    // Invalid types
    assert!(ContentValidator::validate_wasm_param_type("").is_err());
    assert!(ContentValidator::validate_wasm_param_type("unknown_type").is_err());
    assert!(ContentValidator::validate_wasm_param_type("integer").is_err()); // Use "int" instead
    assert!(ContentValidator::validate_wasm_param_type("boolean").is_err()); // Use "bool" instead
}

// ============================================================================
// Edge Cases and Boundary Tests
// ============================================================================

#[test]
fn test_wasm_offering_with_empty_parameters() {
    let offering = WasmOffering {
        id: "empty-params".to_string(),
        name: "No Params Module".to_string(),
        description: "A module with no parameters".to_string(),
        ipfs_cid: "QmEmpty1234567890abcdefghijklmnopqrstuvwxyz12".to_string(),
        parameters: vec![],
        resource_requirements: WasmResourceRequirements::default_requirements(),
        version: "1.0.0".to_string(),
        enabled: true,
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };

    let serialized = serde_json::to_string(&offering).expect("Failed to serialize");
    let deserialized: WasmOffering = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert!(deserialized.parameters.is_empty());
}

#[test]
fn test_wasm_offering_disabled() {
    let offering = WasmOffering {
        id: "disabled-offering".to_string(),
        name: "Disabled Module".to_string(),
        description: "A disabled WASM module".to_string(),
        ipfs_cid: "QmDisabled34567890abcdefghijklmnopqrstuvwxyz12".to_string(),
        parameters: vec![],
        resource_requirements: WasmResourceRequirements::default_requirements(),
        version: "1.0.0".to_string(),
        enabled: false,
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
    };

    let serialized = serde_json::to_string(&offering).expect("Failed to serialize");
    let deserialized: WasmOffering = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert!(!deserialized.enabled);
}

#[test]
fn test_wasm_execution_with_binary_data() {
    let binary_input: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD];
    let request = WasmExecutionRequest {
        from_peer_id: "12D3KooWTestPeerId12345".to_string(),
        from_name: "TestNode".to_string(),
        offering_id: "binary-test".to_string(),
        ipfs_cid: "QmBinary1234567890abcdefghijklmnopqrstuvwxyz12".to_string(),
        input: binary_input.clone(),
        args: vec![],
        fuel_limit: None,
        memory_limit_mb: None,
        timeout_secs: None,
        timestamp: current_timestamp(),
    };

    let serialized = serde_json::to_string(&request).expect("Failed to serialize");
    let deserialized: WasmExecutionRequest = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert_eq!(deserialized.input, binary_input);
}

#[test]
fn test_large_fuel_values() {
    let requirements = WasmResourceRequirements {
        min_fuel: 1_000_000_000,
        max_fuel: u64::MAX,
        min_memory_mb: 1,
        max_memory_mb: 4096,
        estimated_timeout_secs: 3600,
    };

    let serialized = serde_json::to_string(&requirements).expect("Failed to serialize");
    let deserialized: WasmResourceRequirements = serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert_eq!(deserialized.max_fuel, u64::MAX);
}
