//! Basic P2P networking example using p2p-core
//! 
//! This example demonstrates how to set up a basic P2P network with topic-based messaging.

use p2p_core::{NetworkService, NetworkConfig};
use libp2p::floodsub::Topic;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🚀 Starting basic P2P network example");
    
    // Create network configuration
    let config = NetworkConfig {
        request_timeout_seconds: 30,
        max_concurrent_streams: 128,
        max_connections_per_peer: 5,
        max_established_total: 100,
        max_pending_outgoing: 8,
    };
    
    // Initialize network service
    println!("📡 Initializing network service...");
    let mut network = NetworkService::new(config).await?;
    
    let local_peer_id = network.local_peer_id();
    println!("🆔 Local Peer ID: {}", local_peer_id);
    
    // Listen on all interfaces, random port
    let addr = libp2p::Multiaddr::from_str("/ip4/0.0.0.0/tcp/0")?;
    println!("🔗 Listening on: {}", addr);
    network.listen_on(addr)?;
    
    // Subscribe to a topic for pub/sub messaging
    let topic = Topic::new("p2p-core-example");
    println!("📢 Subscribing to topic: {:?}", topic);
    network.subscribe(topic.clone());
    
    // Publish a test message
    let message = format!("Hello from peer {}", local_peer_id).into_bytes();
    println!("📤 Publishing message: {:?}", String::from_utf8_lossy(&message));
    network.publish(topic, message)?;
    
    println!("✅ Basic P2P network is running!");
    println!("💡 To connect other peers, use the multiaddr with your peer ID");
    
    // Keep the network running
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    
    println!("🏁 Example complete!");
    Ok(())
}