pub const BOOTSTRAP_LOG_FILE: &str = "bootstrap.log";
pub const ERRORS_LOG_FILE: &str = "errors.log";
pub const WASM_MAGIC: &[u8] = b"\0asm";
pub const WASM_VERSION: &[u8] = &[0x01, 0x00, 0x00, 0x00];
pub const WASM_HEADER_LEN: usize = 8;
pub const PIPE_BUFFER_SIZE: usize = 64 * 1024;
pub const BYTES_PER_MB: usize = 1024 * 1024;
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
pub const HTTP_READ_BUF_SIZE: usize = 4096;
pub const HTTP_HEADER_END: &[u8] = b"\r\n\r\n";
/// Default async fiber stack size for Wasmtime (8 MiB).
pub const DEFAULT_ASYNC_STACK_SIZE: usize = 8 * 1024 * 1024;
pub const WASM_PARAM_TYPES: &[&str] = &["string", "bytes", "json", "int", "float", "bool", "file"];

pub const UNIFIED_CONFIG_FILE: &str = "unified_network_config.json";

pub struct ContentLimits;

impl ContentLimits {
    pub const STORY_NAME_MAX: usize = 100;
    pub const STORY_HEADER_MAX: usize = 200;
    pub const STORY_BODY_MAX: usize = 10_000;
    pub const CHANNEL_NAME_MAX: usize = 50;
    pub const CHANNEL_DESCRIPTION_MAX: usize = 200;
    pub const PEER_NAME_MAX: usize = 30;
    pub const DIRECT_MESSAGE_MAX: usize = 1_000;
    pub const NODE_DESCRIPTION_MAX: usize = 2_000;

    // WASM offering limits
    pub const WASM_OFFERING_NAME_MAX: usize = 100;
    pub const WASM_OFFERING_DESCRIPTION_MAX: usize = 500;
    pub const WASM_IPFS_CID_MAX: usize = 100;
    pub const WASM_VERSION_MAX: usize = 20;
    pub const WASM_PARAM_NAME_MAX: usize = 50;
    pub const WASM_PARAM_TYPE_MAX: usize = 20;
}

pub const APP_PROTOCOL: &str = "/p2p-play/handshake/1.0.0";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

pub const TCP_LISTEN_BACKLOG: u32 = 1024;
pub const TCP_TTL: u32 = 64;
pub const YAMUX_MAX_STREAMS: usize = 512;
pub const SWARM_IDLE_CONNECTION_TIMEOUT_SECS: u64 = 60;
pub const SWARM_DIAL_CONCURRENCY_FALLBACK: u8 = 8;
// Security constants
pub const ENCRYPTION_CONTEXT: &[u8] = b"p2p-play-encryption";
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB limit
pub const REPLAY_PROTECTION_WINDOW_SECS: u64 = 300; // 5 minutes
pub const MIN_PUBLIC_KEY_SIZE: usize = 32; // Minimum expected public key size

pub const BOOTSTRAP_RETRY_INTERVAL_SECS: u64 = 5;
pub const BOOTSTRAP_STATUS_LOG_INTERVAL_SECS: u64 = 60;
pub const DM_RETRY_INTERVAL_SECS: u64 = 10;
pub const HANDSHAKE_TIMEOUT_SECS: u64 = 60;
