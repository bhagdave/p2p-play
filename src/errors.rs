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
}

/// Network-related errors for libp2p and P2P operations
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Failed to create swarm: {reason}")]
    SwarmCreation { reason: String },
    
    #[error("Failed to listen on address: {address}")]
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
}

impl NetworkError {
    /// Create a NetworkError from any error type with context
    pub fn from_error<E: std::error::Error>(error: E, context: &str) -> Self {
        NetworkError::ProtocolError {
            protocol: "unknown".to_string(),
            reason: format!("{context}: {error}"),
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
}