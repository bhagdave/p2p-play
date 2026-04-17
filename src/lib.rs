pub mod bootstrap;
pub mod bootstrap_logger;
pub mod circuit_breaker;
pub mod constants;
pub mod content_fetcher;
pub mod crypto;
pub mod data_dir;
pub mod error_logger;
pub mod errors;
pub mod event_handlers;
pub mod event_processor;
pub mod file_logger;
pub mod handlers;
pub mod migrations;
pub mod network;
pub mod network_circuit_breakers;
pub mod relay;
pub mod storage;
pub mod types;
pub mod ui;
pub mod validation;
pub mod wasm_executor;

pub use crypto::*;
pub use errors::*;
pub use storage::*;
pub use types::*;

/// Returns the current Unix timestamp in seconds.
///
/// Centralised helper used throughout the codebase so the
/// `SystemTime::now().duration_since(UNIX_EPOCH)...` pattern
/// is not repeated in every module.
pub fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
