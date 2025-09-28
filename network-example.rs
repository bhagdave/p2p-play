//! Example usage of the p2p-network library
//! 
//! This example demonstrates how to use the network crate to create
//! a simple P2P application that can discover peers and exchange messages.

use p2p_network::{P2PNetwork, NetworkConfig, NetworkEvent, MessageContent, Message};
use std::io::{self, BufRead};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🚀 Starting P2P Network Example");
    println!("This example will start a P2P node and allow you to send messages");
    
    // Create network configuration
    let config = NetworkConfig {
        peer_name: Some("example-node".to_string()),
        encryption_enabled: true,
        ..NetworkConfig::default()
    };
    
    // Create and start the network
    let mut network = P2PNetwork::new(config).await?;
    
    println!("📡 Local Peer ID: {}", network.local_peer_id());
    
    // Start listening for connections
    network.start_listening().await?;
    println!("👂 Listening for connections...");
    
    // Subscribe to a example topic
    network.subscribe_to_topic("example-chat")?;
    println!("📢 Subscribed to 'example-chat' topic");
    
    // Bootstrap to find other peers
    println!("🔍 Bootstrapping to find peers...");
    network.bootstrap().await?;
    
    // Spawn a task to handle user input
    let stdin = io::stdin();
    tokio::spawn(async move {
        println!("\n💬 Type messages and press Enter to broadcast them:");
        println!("   Commands:");
        println!("   - 'quit' or 'exit' to stop");  
        println!("   - 'peers' to list connected peers");
        println!("   - anything else will be broadcast to all peers\n");
        
        for line in stdin.lock().lines() {
            match line {
                Ok(input) => {
                    match input.trim() {
                        "quit" | "exit" => {
                            std::process::exit(0);
                        }
                        _ => {
                            // For this example, we would need access to network
                            // In a real application, you'd use channels to communicate
                            println!("📤 Would broadcast: {}", input);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading input: {}", e);
                    break;
                }
            }
        }
    });
    
    // Main event loop
    loop {
        match network.next_event().await {
            Some(NetworkEvent::PeerConnected { peer }) => {
                println!("✅ New peer connected: {}", peer);
                
                // Send a greeting message
                let greeting = Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    sender: network.local_peer_id().to_string(),
                    recipient: Some(peer.to_string()),
                    content: MessageContent::Text("Hello from p2p-network example!".to_string()),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                
                if let Err(e) = network.send_direct_message(peer, greeting).await {
                    eprintln!("Failed to send greeting: {}", e);
                }
            }
            
            Some(NetworkEvent::PeerDisconnected { peer }) => {
                println!("❌ Peer disconnected: {}", peer);
            }
            
            Some(NetworkEvent::MessageReceived { from, topic, data }) => {
                println!("📨 Message from {} on '{}': {:?}", from, topic, 
                    String::from_utf8_lossy(&data));
            }
            
            Some(NetworkEvent::DirectMessageReceived { from, message }) => {
                println!("💌 Direct message from {}: {:?}", from, message);
            }
            
            Some(NetworkEvent::TopicSubscribed { topic }) => {
                println!("🔔 Subscribed to topic: {}", topic);
            }
            
            Some(NetworkEvent::BootstrapCompleted) => {
                println!("🎯 Bootstrap completed - connected to DHT network");
            }
            
            Some(NetworkEvent::NetworkError { error }) => {
                eprintln!("🚨 Network error: {}", error);
            }
            
            _ => {
                // Other events or None
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}