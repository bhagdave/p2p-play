//! # P2P Network Library
//!
//! A reusable peer-to-peer networking library built on libp2p that provides:
//! - Peer discovery (mDNS + DHT) 
//! - Message broadcasting (FloodSub)
//! - Direct messaging between peers
//! - End-to-end encryption
//! - Network resilience patterns
//! - Bootstrap peer management
//!
//! ## Quick Start
//!
//! ```rust
//! use p2p_network::{P2PNetwork, NetworkConfig, NetworkEvent};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = NetworkConfig::default();
//!     let mut network = P2PNetwork::new(config).await?;
//!     
//!     network.start_listening().await?;
//!     network.subscribe_to_topic("my-topic")?;
//!     
//!     while let Some(event) = network.next_event().await {
//!         match event {
//!             NetworkEvent::MessageReceived { from, topic, data } => {
//!                 println!("Message from {} on {}: {:?}", from, topic, data);
//!             }
//!             NetworkEvent::PeerConnected { peer } => {
//!                 println!("New peer: {}", peer);
//!             }
//!             _ => {}
//!         }
//!     }
//!     Ok(())
//! }
//! ```

pub mod network;
pub mod crypto;
pub mod bootstrap;
pub mod relay;
pub mod circuit_breaker;
pub mod errors;
pub mod types;

// Re-export main types for convenience
pub use network::{P2PNetwork, NetworkBehaviour};
pub use crypto::{CryptoService, EncryptedPayload, MessageSignature};
pub use bootstrap::AutoBootstrap;
pub use relay::RelayService;
pub use circuit_breaker::CircuitBreaker;
pub use errors::{NetworkError, NetworkResult};
pub use types::{
    NetworkConfig, NetworkEvent, PeerInfo, Message, DirectMessage,
    Story, Channel, PeerMessage, BootstrapConfig, PingConfig
};

/// Create a new P2P network with default configuration
pub async fn create_default_network() -> NetworkResult<P2PNetwork> {
    let config = NetworkConfig::default();
    P2PNetwork::new(config).await
}

/// Create a new P2P network with custom configuration  
pub async fn create_network_with_config(config: NetworkConfig) -> NetworkResult<P2PNetwork> {
    P2PNetwork::new(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_default_network() {
        let result = create_default_network().await;
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert!(config.encryption_enabled);
        assert!(!config.listen_addresses.is_empty());
    }
}