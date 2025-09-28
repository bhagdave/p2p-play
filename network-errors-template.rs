//! Error types for the P2P network library

use thiserror::Error;
use libp2p::TransportError;

/// Main error type for network operations
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Network initialization failed: {0}")]
    InitializationError(String),

    #[error("Failed to listen on address: {0}")]
    ListenError(String),

    #[error("Connection failed: {0}")]
    ConnectionError(String),

    #[error("Transport error: {0}")]
    TransportError(#[from] TransportError<std::io::Error>),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] CryptoError),

    #[error("Bootstrap error: {0}")]
    BootstrapError(String),

    #[error("Relay error: {0}")]
    RelayError(String),

    #[error("Circuit breaker error: {0}")]
    CircuitBreakerError(String),

    #[error("Event channel error")]
    EventChannelError,

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Crypto-specific errors
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Key generation failed: {0}")]
    KeyGenerationError(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),

    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    #[error("Key exchange failed: {0}")]
    KeyExchangeFailed(String),

    #[error("Message replay detected")]
    MessageReplay,

    #[error("Invalid message size: expected max {max}, got {actual}")]
    InvalidMessageSize { max: usize, actual: usize },
}

/// Bootstrap-specific errors
#[derive(Error, Debug)]
pub enum BootstrapError {
    #[error("No bootstrap peers configured")]
    NoBootstrapPeers,

    #[error("Bootstrap timeout")]
    BootstrapTimeout,

    #[error("DHT not ready")]
    DhtNotReady,

    #[error("Failed to connect to bootstrap peer: {0}")]
    ConnectionFailed(String),

    #[error("Bootstrap configuration error: {0}")]
    ConfigurationError(String),
}

/// Relay-specific errors
#[derive(Error, Debug)]
pub enum RelayError {
    #[error("Relay not available")]
    RelayNotAvailable,

    #[error("Message too large for relay: {0} bytes")]
    MessageTooLarge(usize),

    #[error("Relay timeout")]
    RelayTimeout,

    #[error("Relay circuit error: {0}")]
    CircuitError(String),
}

/// Circuit breaker errors
#[derive(Error, Debug)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker is open")]
    CircuitOpen,

    #[error("Too many failures: {0}")]
    TooManyFailures(usize),

    #[error("Operation timeout")]
    OperationTimeout,
}

/// Result type aliases for convenience
pub type NetworkResult<T> = Result<T, NetworkError>;
pub type CryptoResult<T> = Result<T, CryptoError>;
pub type BootstrapResult<T> = Result<T, BootstrapError>;
pub type RelayResult<T> = Result<T, RelayError>;
pub type CircuitBreakerResult<T> = Result<T, CircuitBreakerError>;

impl From<BootstrapError> for NetworkError {
    fn from(err: BootstrapError) -> Self {
        NetworkError::BootstrapError(err.to_string())
    }
}

impl From<RelayError> for NetworkError {
    fn from(err: RelayError) -> Self {
        NetworkError::RelayError(err.to_string())
    }
}

impl From<CircuitBreakerError> for NetworkError {
    fn from(err: CircuitBreakerError) -> Self {
        NetworkError::CircuitBreakerError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let bootstrap_err = BootstrapError::NoBootstrapPeers;
        let network_err: NetworkError = bootstrap_err.into();
        
        match network_err {
            NetworkError::BootstrapError(_) => (),
            _ => panic!("Expected BootstrapError conversion"),
        }
    }

    #[test]
    fn test_crypto_error_display() {
        let err = CryptoError::EncryptionFailed("test error".to_string());
        assert!(err.to_string().contains("Encryption failed"));
    }
}