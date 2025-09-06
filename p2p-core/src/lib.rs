//! # P2P Core - Reusable P2P Networking Library
//! 
//! A comprehensive peer-to-peer networking library built on top of libp2p,
//! providing high-level abstractions for building distributed applications.
//! 
//! ## Features
//! 
//! - **Multi-protocol Support**: Floodsub, mDNS, Kademlia DHT, Ping, Request-Response
//! - **End-to-End Encryption**: ChaCha20Poly1305 encryption with ECDH key exchange
//! - **Bootstrap Management**: Automatic peer discovery and connection maintenance
//! - **Circuit Breakers**: Network resilience and failure recovery
//! - **Message Validation**: Built-in message validation and sanitization
//! - **Relay Services**: Message forwarding for offline peers
//! 
//! ## Quick Start
//! 
//! ```rust,no_run
//! use p2p_core::{NetworkService, NetworkConfig};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = NetworkConfig::default();
//!     let mut network = NetworkService::new(config).await?;
//!     
//!     // Start the network service
//!     network.start().await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod crypto;
pub mod network;
pub mod relay;
pub mod bootstrap;
pub mod circuit_breaker;
pub mod validation;
pub mod errors;
pub mod types;

// Re-export commonly used types
pub use crypto::{CryptoService, CryptoError, EncryptedPayload, MessageSignature};
pub use network::{NetworkService, P2PBehaviour, NetworkBehaviourEvent};
pub use relay::{RelayService, RelayError, RelayStats};
pub use bootstrap::{BootstrapService, BootstrapConfig, BootstrapError, BootstrapStatus};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerError};
pub use errors::{NetworkError, NetworkResult, ConfigError, ConfigResult};
pub use types::{
    NetworkConfig, PingConfig, DirectMessageRequest, DirectMessageResponse,
    NodeDescriptionRequest, NodeDescriptionResponse, HandshakeRequest, HandshakeResponse,
    EventType, PeerEvent, NetworkEvent, RelayConfig, RelayMessage, DirectMessage
};

/// Default network configuration
pub fn default_config() -> NetworkConfig {
    NetworkConfig::default()
}

/// Initialize logging for the P2P core library
pub fn init_logging() {
    env_logger::init();
}