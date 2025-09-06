use thiserror::Error;

/// Network-specific error types
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Network configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Generic network error: {0}")]
    Other(String),
}

/// Result type for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Configuration-related error types
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),
}

/// Result type for configuration operations
pub type ConfigResult<T> = Result<T, ConfigError>;

impl From<String> for NetworkError {
    fn from(msg: String) -> Self {
        NetworkError::Other(msg)
    }
}

impl From<&str> for NetworkError {
    fn from(msg: &str) -> Self {
        NetworkError::Other(msg.to_string())
    }
}

impl From<String> for ConfigError {
    fn from(msg: String) -> Self {
        ConfigError::Invalid(msg)
    }
}

impl From<&str> for ConfigError {
    fn from(msg: &str) -> Self {
        ConfigError::Invalid(msg.to_string())
    }
}