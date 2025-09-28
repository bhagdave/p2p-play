# P2P-Play Workspace Refactoring Plan

This document outlines the plan to separate P2P-Play into a reusable network crate and frontend application.

## Step 1: Create Workspace Structure

```bash
# Execute these commands in the project root:

# Create network crate directories
mkdir -p p2p-network/src
mkdir -p p2p-network/tests

# Create frontend crate directories
mkdir -p p2p-play-frontend/src
mkdir -p p2p-play-frontend/tests

# Copy current src to frontend as starting point
cp -r src/* p2p-play-frontend/src/

# Copy current tests to both crates as starting point
cp -r tests/* p2p-network/tests/
cp -r tests/* p2p-play-frontend/tests/
```

## Step 2: Network Crate Files

The network crate will include these modules:
- `lib.rs` - Public API
- `network.rs` - Core networking
- `crypto.rs` - Encryption services
- `bootstrap.rs` - DHT bootstrap
- `relay.rs` - Message relay
- `circuit_breaker.rs` - Resilience patterns
- `types.rs` - Core P2P types
- `errors.rs` - Network errors

## Step 3: Frontend Crate Files

The frontend will include:
- `main.rs` - Application entry
- `ui.rs` - Terminal interface
- `handlers.rs` - Command handling
- `storage/` - Database operations
- `event_processor.rs` - Event handling
- Application-specific types and logic

## Step 4: Public API Design

The network crate will expose:
- `P2PNetwork` - Main network client
- `NetworkConfig` - Configuration
- `NetworkEvent` - Event system
- `CryptoService` - Encryption
- Core P2P data structures

## Execute This Plan

1. Run the setup commands above
2. Create the Cargo.toml files provided
3. Implement the public API
4. Refactor the modules
5. Update tests
6. Validate with cargo build and test