# P2P Network Crate

This is a reusable peer-to-peer networking library built on libp2p. It provides:

- Peer discovery (mDNS + DHT)
- Message broadcasting (FloodSub)
- Direct messaging between peers
- End-to-end encryption
- Network resilience patterns
- Bootstrap peer management

## Usage

```rust
use p2p_network::{P2PNetwork, NetworkConfig, NetworkEvent};

#[tokio::main]
async fn main() {
    let config = NetworkConfig::default();
    let mut network = P2PNetwork::new(config).await?;
    
    // Handle network events
    while let Some(event) = network.next_event().await {
        match event {
            NetworkEvent::MessageReceived { from, message } => {
                println!("Received from {}: {}", from, message);
            }
            // ... handle other events
        }
    }
}
```