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
    #[serde(default)]
    pub channels: Vec<crate::types::Channel>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeRequest {
    pub app_name: String,
    pub app_version: String,
    pub peer_id: String,
    #[serde(default)]
    pub wasm_capable: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub app_name: String,
    pub app_version: String,
    #[serde(default)]
    pub wasm_capable: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmCapabilitiesRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub timestamp: u64,
    pub include_parameters: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmCapabilitiesResponse {
    pub peer_id: String,
    pub peer_name: String,
    pub wasm_enabled: bool,
    pub offerings: Vec<crate::types::WasmOffering>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmExecutionRequest {
    pub from_peer_id: String,
    pub from_name: String,
    pub offering_id: String,
    pub ipfs_cid: String,
    pub input: Vec<u8>,
    pub args: Vec<String>,
    pub fuel_limit: Option<u64>,
    pub memory_limit_mb: Option<u32>,
    pub timeout_secs: Option<u64>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WasmExecutionResponse {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub fuel_consumed: u64,
    pub exit_code: i32,
    pub error: Option<String>,
    pub timestamp: u64,
}
