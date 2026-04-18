//! Wire-protocol data transfer objects for all request/response protocols.
//!
//! Each pair of `Request`/`Response` structs corresponds to one libp2p
//! request-response behaviour defined in the parent module.

/// Application-level protocol identifier used during peer handshakes.
pub const APP_PROTOCOL: &str = "/p2p-play/handshake/1.0.0";
/// Application version string derived from `Cargo.toml`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Application name string derived from `Cargo.toml`.
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

// ── Direct messaging ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub to_name: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirectMessageResponse {
    pub received: bool,
    pub timestamp: u64,
}

// ── Node description ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeDescriptionResponse {
    pub description: Option<String>,
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
}

// ── Story sync ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StorySyncRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub last_sync_timestamp: u64,
    pub subscribed_channels: Vec<String>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StorySyncResponse {
    pub stories: Vec<crate::types::Story>,
    pub from_peer_id: String,
    pub from_name: String,
    pub sync_timestamp: u64,
    /// Channels associated with the stories being shared (for retroactive discovery)
    #[serde(default)]
    pub channels: Vec<crate::types::Channel>,
}

// ── Handshake ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeRequest {
    pub app_name: String,
    pub app_version: String,
    pub peer_id: String,
    /// Whether this node supports WASM capability advertisement
    #[serde(default)]
    pub wasm_capable: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub app_name: String,
    pub app_version: String,
    /// Whether this node supports WASM capability advertisement
    #[serde(default)]
    pub wasm_capable: bool,
}

// ── WASM capabilities ────────────────────────────────────────────────────────

/// Request to query a peer's WASM capabilities
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmCapabilitiesRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
    /// Whether to include full parameter definitions in the response
    pub include_parameters: bool,
}

/// Response containing a peer's WASM offerings
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmCapabilitiesResponse {
    pub peer_id: String,
    pub peer_name: String,
    /// Whether this node has WASM execution enabled
    pub wasm_enabled: bool,
    /// List of WASM offerings this node provides
    pub offerings: Vec<crate::types::WasmOffering>,
    pub timestamp: u64,
}

// ── WASM remote execution ────────────────────────────────────────────────────

/// Request to execute a WASM module on a remote peer
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmExecutionRequest {
    pub from_peer_id: String,
    pub from_name: String,
    /// The offering ID to execute
    pub offering_id: String,
    /// IPFS CID for verification
    pub ipfs_cid: String,
    /// Stdin input data
    pub input: Vec<u8>,
    /// Command-line arguments
    pub args: Vec<String>,
    /// Optional fuel limit override
    pub fuel_limit: Option<u64>,
    /// Optional memory limit override (MB)
    pub memory_limit_mb: Option<u32>,
    /// Optional timeout override (seconds)
    pub timeout_secs: Option<u64>,
    pub timestamp: u64,
}

/// Response from a WASM execution request
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmExecutionResponse {
    /// Whether execution succeeded
    pub success: bool,
    /// Stdout output
    pub stdout: Vec<u8>,
    /// Stderr output
    pub stderr: Vec<u8>,
    /// Fuel consumed during execution
    pub fuel_consumed: u64,
    /// Exit code (0 for success)
    pub exit_code: i32,
    /// Error message if execution failed
    pub error: Option<String>,
    pub timestamp: u64,
}
