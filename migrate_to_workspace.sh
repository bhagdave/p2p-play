#!/bin/bash

# P2P-Play Workspace Migration Script
# This script demonstrates the complete migration from monolithic to workspace structure

set -e

echo "=== P2P-Play Workspace Migration ==="
echo "This will separate the application into a network crate and frontend."
echo ""

# Step 1: Create workspace structure
echo "Step 1: Creating workspace structure..."
mkdir -p p2p-network/src
mkdir -p p2p-network/tests
mkdir -p p2p-play-frontend/src
mkdir -p p2p-play-frontend/tests

# Step 2: Copy template files for network crate
echo "Step 2: Setting up network crate..."
cp network-crate-Cargo.toml p2p-network/Cargo.toml
cp network-lib-template.rs p2p-network/src/lib.rs
cp network-interface-template.rs p2p-network/src/network.rs
cp network-types-template.rs p2p-network/src/types.rs  
cp network-errors-template.rs p2p-network/src/errors.rs

# Copy network-specific source files
echo "  - Copying core network modules..."
cp src/crypto.rs p2p-network/src/crypto.rs
cp src/bootstrap.rs p2p-network/src/bootstrap.rs
cp src/relay.rs p2p-network/src/relay.rs
cp src/circuit_breaker.rs p2p-network/src/circuit_breaker.rs

# Step 3: Copy template files for frontend crate
echo "Step 3: Setting up frontend crate..."
cp frontend-crate-Cargo.toml p2p-play-frontend/Cargo.toml

# Copy all remaining source files to frontend
echo "  - Copying application-specific modules..."
cp -r src/* p2p-play-frontend/src/

# Step 4: Copy tests appropriately
echo "Step 4: Distributing tests..."

# Network crate tests
echo "  - Setting up network crate tests..."
cp tests/network_tests.rs p2p-network/tests/ 2>/dev/null || echo "    network_tests.rs not found, skipping"
cp tests/crypto_*.rs p2p-network/tests/ 2>/dev/null || echo "    crypto tests not found, skipping"
cp tests/bootstrap_*.rs p2p-network/tests/ 2>/dev/null || echo "    bootstrap tests not found, skipping"
cp tests/circuit_breaker_*.rs p2p-network/tests/ 2>/dev/null || echo "    circuit breaker tests not found, skipping"  
cp tests/relay_*.rs p2p-network/tests/ 2>/dev/null || echo "    relay tests not found, skipping"

# Frontend tests (all tests initially, can be cleaned up later)
echo "  - Setting up frontend crate tests..."
cp -r tests/* p2p-play-frontend/tests/

# Step 5: Update workspace Cargo.toml
echo "Step 5: Updating workspace configuration..."
cat > Cargo.toml << 'EOF'
[workspace]
members = ["p2p-network", "p2p-play-frontend"]
resolver = "2"

[workspace.dependencies]
# Shared dependencies can be defined here
tokio = { version = "1.43", features = ["io-util", "io-std", "macros", "rt", "rt-multi-thread", "sync", "fs"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
log = "0.4"
thiserror = "1.0"
EOF

# Step 6: Create README files
echo "Step 6: Creating documentation..."

cat > p2p-network/README.md << 'EOF'
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
EOF

cat > p2p-play-frontend/README.md << 'EOF'
# P2P-Play Frontend

A peer-to-peer story sharing application with Terminal User Interface.

This is the frontend application that uses the `p2p-network` library to provide
a complete story sharing experience with a rich terminal interface.

## Features

- Terminal User Interface (TUI)
- Story creation and sharing
- Channel-based organization
- Direct messaging
- Local storage with SQLite
- Story search and management

## Running

```bash
cargo run
```

## Testing

```bash
cargo test
```
EOF

# Step 7: Create migration checklist
echo "Step 7: Creating migration checklist..."

cat > MIGRATION_CHECKLIST.md << 'EOF'
# Migration Checklist

This checklist tracks the progress of separating P2P-Play into a network crate and frontend.

## ✅ Completed
- [x] Created workspace structure
- [x] Set up basic Cargo.toml files
- [x] Created network crate template files
- [x] Distributed source files between crates
- [x] Set up test structure

## 🔄 In Progress
- [ ] Update imports in network crate
- [ ] Update imports in frontend crate  
- [ ] Implement network crate public API
- [ ] Remove network dependencies from frontend
- [ ] Update error handling interfaces
- [ ] Test network crate independently

## ⏳ Todo
- [ ] Clean up duplicate code between crates
- [ ] Update documentation
- [ ] Add network crate examples
- [ ] Validate full application functionality
- [ ] Update CI/CD pipelines for workspace
- [ ] Publish network crate to crates.io

## Manual Steps Required

1. **Update Network Crate Imports**: Remove UI-specific imports from network modules
2. **Update Frontend Imports**: Change to use `p2p_network::*` instead of local modules
3. **Interface Adaptation**: Ensure the frontend uses only the public API from network crate
4. **Error Handling**: Update error types to work across crate boundaries
5. **Testing**: Ensure all tests pass in the new structure
6. **Documentation**: Update README files and examples

## Commands to Run After Migration

```bash
# Test network crate
cd p2p-network && cargo test

# Test frontend crate  
cd p2p-play-frontend && cargo test

# Test full workspace
cargo test

# Build full workspace
cargo build
```
EOF

echo ""
echo "=== Migration Complete! ==="
echo ""
echo "📁 Created workspace structure:"
echo "   - p2p-network/     (reusable networking library)"
echo "   - p2p-play-frontend/ (TUI application)"
echo ""
echo "📋 Next steps:"
echo "   1. Review MIGRATION_CHECKLIST.md for remaining tasks"
echo "   2. Update imports in both crates"
echo "   3. Test the separated crates: cargo test"
echo "   4. Fix any compilation issues"
echo ""
echo "🚀 Once complete, you can publish p2p-network to crates.io!"