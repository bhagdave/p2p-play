//! Centralized error handling for P2P-Play application
//! 
//! This module defines domain-specific error types that replace the generic
//! `Box<dyn Error>` usage throughout the codebase, providing better error
//! debugging and user experience.

use crate::crypto::CryptoError;
use crate::relay::RelayError;
use thiserror::Error;

/// Main application error type that chains all domain-specific errors
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    
    #[error("UI error: {0}")]
    UI(#[from] UIError),
    
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
    
    #[error("Relay error: {0}")]
    Relay(#[from] RelayError),
    
    #[error("Application error: {0}")]
    Application(String),
}

/// Storage-related errors for database and file operations
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("File I/O error: {0}")]
    FileIO(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("SQLite error: {0}")]
    SQLite(#[from] rusqlite::Error),
    
    #[error("Story not found: {id}")]
    StoryNotFound { id: usize },
    
    #[error("Channel not found: {name}")]
    ChannelNotFound { name: String },
    
    #[error("Invalid story data: {reason}")]
    InvalidStoryData { reason: String },
    
    #[error("Database connection failed: {reason}")]
    DatabaseConnection { reason: String },
    
    #[error("Migration failed: {reason}")]
    Migration { reason: String },
    
    #[error("Batch operation failed: {successful} succeeded, {failed} failed")]
    BatchOperationFailed { successful: usize, failed: usize, failures: Vec<String> },
}

/// Network-related errors for libp2p and P2P operations
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Swarm creation failed: {reason}")]
    SwarmCreation { reason: String },
    
    #[error("Listen failed on address: {address}")]
    ListenFailed { address: String },
    
    #[error("Peer connection failed: {peer_id}")]
    PeerConnectionFailed { peer_id: String },
    
    #[error("Message broadcast failed: {reason}")]
    BroadcastFailed { reason: String },
    
    #[error("DHT operation failed: {reason}")]
    DHTFailed { reason: String },
    
    #[error("Direct message failed: {reason}")]
    DirectMessageFailed { reason: String },
    
    #[error("Bootstrap failed: {reason}")]
    BootstrapFailed { reason: String },
    
    #[error("Protocol error: {protocol} - {reason}")]
    ProtocolError { protocol: String, reason: String },
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Transport error: {reason}")]
    Transport { reason: String },
}

/// UI-related errors for terminal interface operations
#[derive(Error, Debug)]
pub enum UIError {
    #[error("Terminal initialization failed: {0}")]
    TerminalInit(#[from] std::io::Error),
    
    #[error("Terminal rendering failed: {reason}")]
    Rendering { reason: String },
    
    #[error("Input handling failed: {reason}")]
    InputHandling { reason: String },
    
    #[error("State transition failed: from {from} to {to}")]
    StateTransition { from: String, to: String },
    
    #[error("Widget error: {widget} - {reason}")]
    Widget { widget: String, reason: String },
    
    #[error("Layout error: {reason}")]
    Layout { reason: String },
}

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config file not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Invalid config format: {reason}")]
    InvalidFormat { reason: String },
    
    #[error("Config validation failed: {reason}")]
    Validation { reason: String },
    
    #[error("Missing required field: {field}")]
    MissingField { field: String },
    
    #[error("Invalid value for field {field}: {value}")]
    InvalidValue { field: String, value: String },
    
    #[error("Config serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Config I/O error: {0}")]
    IO(#[from] std::io::Error),
}

/// Result type aliases for common error combinations
pub type AppResult<T> = Result<T, AppError>;
pub type StorageResult<T> = Result<T, StorageError>;
pub type NetworkResult<T> = Result<T, NetworkError>;
pub type UIResult<T> = Result<T, UIError>;
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Detects the network protocol from an error message
/// 
/// Analyzes error text to identify which libp2p protocol is likely involved
/// based on common protocol-specific terms and patterns.
fn detect_protocol_from_error(error_message: &str) -> String {
    let lower_msg = error_message.to_lowercase();
    
    // Check for specific protocol indicators in order of specificity
    if lower_msg.contains("floodsub") || lower_msg.contains("pubsub") || lower_msg.contains("topic") {
        "floodsub".to_string()
    } else if lower_msg.contains("mdns") || lower_msg.contains("multicast") {
        "mdns".to_string()
    } else if lower_msg.contains("ping") || lower_msg.contains("pong") {
        "ping".to_string()
    } else if lower_msg.contains("kad") || lower_msg.contains("kademlia") || lower_msg.contains("dht") {
        "kad".to_string()
    } else if lower_msg.contains("request_response") || lower_msg.contains("request-response") {
        "request_response".to_string()
    } else if lower_msg.contains("transport") || lower_msg.contains("tcp") || lower_msg.contains("quic") {
        "transport".to_string()
    } else if lower_msg.contains("identify") {
        "identify".to_string()
    } else if lower_msg.contains("noise") || lower_msg.contains("encryption") {
        "noise".to_string()
    } else if lower_msg.contains("yamux") || lower_msg.contains("multiplex") {
        "yamux".to_string()
    } else {
        "unknown".to_string()
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for AppError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        AppError::Application(err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for AppError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        AppError::Application(err.to_string())
    }
}

impl From<String> for StorageError {
    fn from(err: String) -> Self {
        StorageError::Database(err)
    }
}

impl From<&str> for StorageError {
    fn from(err: &str) -> Self {
        StorageError::Database(err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for StorageError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        StorageError::Database(err.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for StorageError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        StorageError::Database(err.to_string())
    }
}

impl From<String> for NetworkError {
    fn from(err: String) -> Self {
        let protocol = detect_protocol_from_error(&err);
        NetworkError::ProtocolError {
            protocol,
            reason: err,
        }
    }
}

impl From<&str> for NetworkError {
    fn from(err: &str) -> Self {
        let protocol = detect_protocol_from_error(err);
        NetworkError::ProtocolError {
            protocol,
            reason: err.to_string(),
        }
    }
}

impl From<String> for UIError {
    fn from(err: String) -> Self {
        UIError::Rendering { reason: err }
    }
}

impl From<&str> for UIError {
    fn from(err: &str) -> Self {
        UIError::Rendering { reason: err.to_string() }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for UIError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        UIError::InputHandling {
            reason: format!("Failed to send UI event: {err}"),
        }
    }
}

impl From<String> for ConfigError {
    fn from(err: String) -> Self {
        ConfigError::InvalidFormat { reason: err }
    }
}

impl From<&str> for ConfigError {
    fn from(err: &str) -> Self {
        ConfigError::InvalidFormat { reason: err.to_string() }
    }
}

impl StorageError {
    /// Create a StorageError from any error type with context
    pub fn from_error<E: std::error::Error>(error: E, context: &str) -> Self {
        StorageError::Database(format!("{context}: {error}"))
    }
    
    /// Create a database connection error with context
    pub fn connection_error(reason: impl Into<String>) -> Self {
        StorageError::DatabaseConnection {
            reason: reason.into(),
        }
    }
    
    /// Create an invalid data error with context
    pub fn invalid_data(reason: impl Into<String>) -> Self {
        StorageError::InvalidStoryData {
            reason: reason.into(),
        }
    }
    
    /// Create a batch operation error with summary
    pub fn batch_operation_failed(
        successful: usize, 
        failed: usize, 
        failures: Vec<String>
    ) -> Self {
        StorageError::BatchOperationFailed {
            successful,
            failed,
            failures,
        }
    }
}

impl NetworkError {
    /// Create a NetworkError from any error type with context
    pub fn from_error<E: std::error::Error>(error: E, context: &str) -> Self {
        let error_msg = format!("{context}: {error}");
        let protocol = detect_protocol_from_error(&error_msg);
        NetworkError::ProtocolError {
            protocol,
            reason: error_msg,
        }
    }
    
    /// Create a protocol error with context
    pub fn protocol_error(protocol: impl Into<String>, reason: impl Into<String>) -> Self {
        NetworkError::ProtocolError {
            protocol: protocol.into(),
            reason: reason.into(),
        }
    }
}

impl UIError {
    /// Create a UIError from any error type with context
    pub fn from_error<E: std::error::Error>(error: E, context: &str) -> Self {
        UIError::Rendering {
            reason: format!("{context}: {error}"),
        }
    }
    
    /// Create a widget error with context
    pub fn widget_error(widget: impl Into<String>, reason: impl Into<String>) -> Self {
        UIError::Widget {
            widget: widget.into(),
            reason: reason.into(),
        }
    }
}

impl ConfigError {
    /// Create a ConfigError from any error type with context
    pub fn from_error<E: std::error::Error>(error: E, context: &str) -> Self {
        ConfigError::InvalidFormat {
            reason: format!("{context}: {error}"),
        }
    }
    
    /// Create a validation error with context
    pub fn validation_error(reason: impl Into<String>) -> Self {
        ConfigError::Validation {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_chain_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let storage_error = StorageError::FileIO(io_error);
        let app_error = AppError::Storage(storage_error);
        
        assert!(app_error.to_string().contains("Storage error"));
        assert!(app_error.to_string().contains("file not found"));
    }
    
    #[test]
    fn test_storage_error_helpers() {
        let error = StorageError::connection_error("timeout");
        assert!(matches!(error, StorageError::DatabaseConnection { .. }));
        
        let error = StorageError::invalid_data("malformed JSON");
        assert!(matches!(error, StorageError::InvalidStoryData { .. }));
    }
    
    #[test]
    fn test_network_error_helpers() {
        let error = NetworkError::protocol_error("floodsub", "timeout");
        assert!(matches!(error, NetworkError::ProtocolError { .. }));
    }
    
    #[test]
    fn test_ui_error_helpers() {
        let error = UIError::widget_error("story_list", "render failed");
        assert!(matches!(error, UIError::Widget { .. }));
    }
    
    #[test]
    fn test_config_error_helpers() {
        let error = ConfigError::validation_error("invalid port");
        assert!(matches!(error, ConfigError::Validation { .. }));
    }
    
    #[test]
    fn test_protocol_detection() {
        // Test floodsub detection
        assert_eq!(detect_protocol_from_error("floodsub connection failed"), "floodsub");
        assert_eq!(detect_protocol_from_error("topic subscription error"), "floodsub");
        assert_eq!(detect_protocol_from_error("pubsub timeout"), "floodsub");
        
        // Test mdns detection
        assert_eq!(detect_protocol_from_error("mdns discovery failed"), "mdns");
        assert_eq!(detect_protocol_from_error("multicast error"), "mdns");
        
        // Test ping detection
        assert_eq!(detect_protocol_from_error("ping timeout"), "ping");
        assert_eq!(detect_protocol_from_error("pong not received"), "ping");
        
        // Test kad detection
        assert_eq!(detect_protocol_from_error("kad bootstrap failed"), "kad");
        assert_eq!(detect_protocol_from_error("kademlia lookup timeout"), "kad");
        assert_eq!(detect_protocol_from_error("dht operation failed"), "kad");
        
        // Test transport detection
        assert_eq!(detect_protocol_from_error("tcp connection refused"), "transport");
        assert_eq!(detect_protocol_from_error("quic handshake failed"), "transport");
        assert_eq!(detect_protocol_from_error("transport error"), "transport");
        
        // Test unknown
        assert_eq!(detect_protocol_from_error("generic error message"), "unknown");
    }
    
    #[test]
    fn test_batch_operation_error() {
        let failures = vec!["Story 1 not found".to_string(), "Story 3 invalid".to_string()];
        let error = StorageError::batch_operation_failed(2, 2, failures.clone());
        
        // Test display message includes counts first
        let display = format!("{}", error);
        assert!(display.contains("2 succeeded"));
        assert!(display.contains("2 failed"));
        
        match error {
            StorageError::BatchOperationFailed { successful, failed, failures: f } => {
                assert_eq!(successful, 2);
                assert_eq!(failed, 2);
                assert_eq!(f, failures);
            }
            _ => panic!("Expected BatchOperationFailed variant"),
        }
    }
    
    #[test]
    fn test_network_error_protocol_detection() {
        let error: NetworkError = "floodsub timeout occurred".into();
        match error {
            NetworkError::ProtocolError { protocol, reason } => {
                assert_eq!(protocol, "floodsub");
                assert_eq!(reason, "floodsub timeout occurred");
            }
            _ => panic!("Expected ProtocolError variant"),
        }
        
        let error: NetworkError = "unknown connection issue".into();
        match error {
            NetworkError::ProtocolError { protocol, .. } => {
                assert_eq!(protocol, "unknown");
            }
            _ => panic!("Expected ProtocolError variant"),
        }
    }
}