//! Encrypted messaging example using p2p-core
//! 
//! This example demonstrates end-to-end encrypted messaging between peers.

use p2p_core::{NetworkService, NetworkConfig, CryptoService, DirectMessage};
use libp2p::{identity, PeerId};

async fn create_peer(name: &str) -> Result<(NetworkService, CryptoService, PeerId), Box<dyn std::error::Error>> {
    // Generate keypair for this peer
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    
    // Create crypto service
    let crypto = CryptoService::new(keypair.clone());
    
    // Create network service
    let config = NetworkConfig::default();
    let network = NetworkService::new(config).await?;
    
    println!("👤 Created peer '{}' with ID: {}", name, peer_id);
    
    Ok((network, crypto, peer_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🔐 Starting encrypted messaging example");
    
    // Create two peers
    let (_alice_network, mut alice_crypto, alice_id) = create_peer("Alice").await?;
    let (_bob_network, mut bob_crypto, bob_id) = create_peer("Bob").await?;
    
    // Exchange public keys (simulating key exchange)
    let alice_keypair = identity::Keypair::generate_ed25519(); // This would be Alice's actual keypair
    let bob_keypair = identity::Keypair::generate_ed25519();   // This would be Bob's actual keypair
    
    let alice_public = alice_keypair.public().encode_protobuf();
    let bob_public = bob_keypair.public().encode_protobuf();
    
    // Add each other's public keys
    alice_crypto.add_peer_public_key(bob_id, bob_public)?;
    bob_crypto.add_peer_public_key(alice_id, alice_public)?;
    
    println!("🤝 Public keys exchanged between Alice and Bob");
    
    // Alice encrypts a message for Bob
    let message_text = "Hello Bob! This is a secret message from Alice.";
    let encrypted = alice_crypto.encrypt_message(message_text.as_bytes(), &bob_id)?;
    
    println!("📤 Alice encrypted message for Bob");
    println!("🔒 Encrypted data length: {} bytes", encrypted.encrypted_data.len());
    
    // Bob decrypts the message
    let decrypted = bob_crypto.decrypt_message(&encrypted)?;
    let decrypted_text = String::from_utf8(decrypted)?;
    
    println!("📥 Bob decrypted message: '{}'", decrypted_text);
    
    // Bob sends a signed reply
    let reply_text = "Hi Alice! I received your secret message safely.";
    let signature = bob_crypto.sign_message(reply_text.as_bytes())?;
    
    println!("✍️  Bob signed reply message");
    
    // Alice verifies Bob's signature
    let is_valid = alice_crypto.verify_signature(reply_text.as_bytes(), &signature)?;
    
    if is_valid {
        println!("✅ Alice verified Bob's signature - message is authentic!");
        println!("📥 Alice received: '{}'", reply_text);
    } else {
        println!("❌ Signature verification failed!");
    }
    
    // Create a direct message structure
    let direct_message = DirectMessage {
        from_peer_id: alice_id.to_string(),
        from_name: "Alice".to_string(),
        to_peer_id: bob_id.to_string(),
        to_name: "Bob".to_string(),
        message: message_text.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        is_outgoing: true,
    };
    
    println!("💌 Created direct message structure");
    println!("   From: {} ({})", direct_message.from_name, direct_message.from_peer_id);
    println!("   To: {} ({})", direct_message.to_name, direct_message.to_peer_id);
    println!("   Message: {}", direct_message.message);
    
    println!("🏁 Encrypted messaging example complete!");
    
    Ok(())
}