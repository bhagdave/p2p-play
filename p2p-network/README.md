# P2P Network Library

A reusable peer-to-peer networking library built on libp2p.

## Features

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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = NetworkConfig::default();
    let mut network = P2PNetwork::new(config).await?;
    
    // Start listening
    network.start_listening().await?;
    
    // Subscribe to topics
    network.subscribe_to_topic("stories")?;
    
    // Handle events
    while let Some(event) = network.next_event().await {
        match event {
            NetworkEvent::MessageReceived { from, topic, data } => {
                println!("Received message on {}: {:?}", topic, data);
            }
            NetworkEvent::PeerConnected { peer } => {
                println!("New peer connected: {}", peer);
            }
            _ => {}
        }
    }
    
    Ok(())
}
```

## Testing

```bash
cargo test
```
