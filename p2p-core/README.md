# P2P Core - Reusable P2P Networking Library

A peer-to-peer networking library built on top of libp2p, providing high-level abstractions for building distributed applications.

## Features

- **Multi-protocol Support**: Floodsub, mDNS, Kademlia DHT, Ping, Request-Response
- **End-to-End Encryption**: ChaCha20Poly1305 encryption with ECDH key exchange
- **Bootstrap Management**: Automatic peer discovery and connection maintenance
- **Circuit Breakers**: Network resilience and failure recovery
- **Message Validation**: Built-in message validation and sanitization
- **Relay Services**: Message forwarding for offline peers
- **Comprehensive Configuration**: Flexible configuration for all networking aspects

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
p2p-core = { path = "../p2p-core" }  # or from crates.io when published
```

### Basic Usage

```rust
use p2p_core::{NetworkService, NetworkConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create network configuration
    let config = NetworkConfig::default();
    
    // Initialize network service
    let mut network = NetworkService::new(config).await?;
    
    // Listen on localhost
    let addr = "/ip4/127.0.0.1/tcp/0".parse()?;
    network.listen_on(addr)?;
    
    // Subscribe to topics for pub/sub messaging
    let topic = libp2p::floodsub::Topic::new("my-topic");
    network.subscribe(topic.clone());
    
    // Publish messages
    let message = b"Hello, P2P world!".to_vec();
    network.publish(topic, message)?;
    
    Ok(())
}
```

### Usage with All Features

```rust
use p2p_core::{
    NetworkService, NetworkConfig, PingConfig, BootstrapService, BootstrapConfig,
    CryptoService, RelayService, RelayConfig, CircuitBreaker, CircuitBreakerConfig
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure networking
    let network_config = NetworkConfig {
        request_timeout_seconds: 30,
        max_concurrent_streams: 128,
        max_connections_per_peer: 5,
        max_established_total: 100,
        max_pending_outgoing: 8,
    };
    
    let ping_config = PingConfig {
        interval_secs: 30,
        timeout_secs: 10,
    };
    
    // Create network service
    let mut network = NetworkService::new_with_ping(network_config, ping_config).await?;
    
    // Set up bootstrap for peer discovery
    let bootstrap_config = BootstrapConfig {
        peers: vec![
            "/ip4/127.0.0.1/tcp/9000/p2p/12D3KooWExample".to_string()
        ],
        retry_interval_secs: 30,
        max_retries: 10,
        enabled: true,
        ..Default::default()
    };
    
    let bootstrap = BootstrapService::new(bootstrap_config);
    
    // Configure encryption
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let crypto = CryptoService::new(keypair);
    
    // Set up message relay
    let relay_config = RelayConfig::default();
    let relay_service = RelayService::new(relay_config, crypto);
    
    // Add circuit breaker for resilience
    let cb_config = CircuitBreakerConfig::default();
    let circuit_breaker = CircuitBreaker::new(cb_config);
    
    // Start listening
    let addr = "/ip4/0.0.0.0/tcp/0".parse()?;
    network.listen_on(addr)?;
    
    // Attempt bootstrap
    bootstrap.attempt_bootstrap(&mut network.swarm).await?;
    
    Ok(())
}
```

## Core Components

### NetworkService
High-level networking service that manages libp2p swarm and protocols.

```rust
let mut network = NetworkService::new(NetworkConfig::default()).await?;
network.subscribe(topic);
network.publish(topic, message)?;
```

### CryptoService
End-to-end encryption for secure peer-to-peer communication.

```rust
let keypair = libp2p::identity::Keypair::generate_ed25519();
let crypto = CryptoService::new(keypair);
let encrypted = crypto.encrypt_message(data, &target_peer_id)?;
```

### BootstrapService
Automatic peer discovery and bootstrap connection management.

```rust
let config = BootstrapConfig::default();
let bootstrap = BootstrapService::new(config);
bootstrap.attempt_bootstrap(&mut swarm).await?;
```

### RelayService
Message forwarding for offline peers with encryption and rate limiting.

```rust
let relay = RelayService::new(RelayConfig::default(), crypto_service);
let relay_msg = relay.create_relay_message(&direct_message, &target_peer)?;
```

### CircuitBreaker
Network resilience and failure recovery with configurable thresholds.

```rust
let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
let result = cb.execute(|| async { networking_operation().await }).await?;
```

## Configuration

### NetworkConfig
```rust
NetworkConfig {
    request_timeout_seconds: 30,
    max_concurrent_streams: 128,
    max_connections_per_peer: 5,
    max_established_total: 100,
    max_pending_outgoing: 8,
}
```

### PingConfig
```rust
PingConfig {
    interval_secs: 30,  // Ping interval
    timeout_secs: 10,   // Ping timeout
}
```

### BootstrapConfig
```rust
BootstrapConfig {
    peers: vec!["multiaddr1", "multiaddr2"],
    retry_interval_secs: 30,
    max_retries: 10,
    enabled: true,
}
```

## Testing

Run the comprehensive test suite:

```bash
# Run all tests
./scripts/test_runner.sh

# Run specific test categories
cargo test --test crypto_integration_tests
cargo test --test network_integration_tests
cargo test --test bootstrap_tests

# Generate coverage report
./scripts/coverage.sh

# Simulate CI environment
./scripts/test_ci_simulation.sh
```

## Test Coverage

The library includes comprehensive tests covering:

- **Crypto Integration Tests**: Encryption, decryption, key management
- **Network Configuration Tests**: Validation and configuration management
- **Network Integration Tests**: Service creation and protocol handling
- **Bootstrap Tests**: Peer discovery and connection management
- **Circuit Breaker Tests**: Resilience patterns and failure recovery
- **Relay Tests**: Message forwarding and rate limiting

## CI/CD Integration

GitHub Actions workflow included for:
- Automated testing on multiple Rust versions
- Code formatting and linting checks
- Security audits
- Test coverage reporting
- Documentation generation

## Error Handling

The library provides error types:

```rust
use p2p_core::{NetworkError, CryptoError, BootstrapError, RelayError, CircuitBreakerError};

match network_operation().await {
    Ok(result) => { /* handle success */ },
    Err(NetworkError::ConnectionFailed(msg)) => { /* handle connection error */ },
    Err(NetworkError::PeerNotFound(peer_id)) => { /* handle missing peer */ },
    Err(other) => { /* handle other errors */ },
}
```

## Development

### Building
```bash
cargo build
```

### Running Tests
```bash
cargo test
```

### Generating Documentation
```bash
cargo doc --no-deps --all-features
```

### Code Quality
```bash
cargo fmt --all --check  # Format check
cargo clippy -- -D warnings  # Linting
cargo audit  # Security audit
```

## Minimum Supported Rust Version (MSRV)

This crate requires Rust 1.70 or later.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Architecture

```
┌─────────────────┐
│  NetworkService │  ← High-level API
└─────────────────┘
         │
    ┌────┴────┐
    │  libp2p │  ← Core P2P protocols
    └─────────┘
         │
┌────────┼────────┐
│ ┌──────▼──────┐ │
│ │ CryptoSvc   │ │  ← Security layer
│ └─────────────┘ │
│ ┌─────────────┐ │
│ │ RelaySvc    │ │  ← Message forwarding
│ └─────────────┘ │
│ ┌─────────────┐ │
│ │ Bootstrap   │ │  ← Peer discovery
│ └─────────────┘ │
│ ┌─────────────┐ │
│ │Circuit Break│ │  ← Resilience
│ └─────────────┘ │
└─────────────────┘
```

## Supported Protocols

- **Floodsub**: Topic-based publish/subscribe messaging
- **mDNS**: Local network peer discovery
- **Kademlia DHT**: Distributed hash table for global peer discovery
- **Ping**: Connection health monitoring
- **Request-Response**: Direct peer-to-peer messaging
- **Noise**: Transport-layer encryption
- **Yamux**: Stream multiplexing

## Examples

See the `examples/` directory for working examples of various use cases.
