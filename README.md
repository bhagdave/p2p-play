# p2p-play

P2P-Play is a peer-to-peer story sharing application built with Rust using the [libp2p library (0.56.0)](https://github.com/libp2p/rust-libp2p/releases/tag/v0.56.0). The application allows users to create, publish, and share stories across a distributed network of peers using a modern terminal user interface.

## Features

### Core Functionality
- **Story Creation & Sharing**: Create and publish stories that can be shared across the peer-to-peer network
- **Peer Discovery**: Automatic peer discovery using mDNS for local networks and DHT (Kademlia) for broader connectivity
- **Direct Messaging**: Send private messages directly to other peers by their alias names
- **Channel System**: Organize stories into channels and subscribe to channels of interest
- **Local Storage**: SQLite database for persistent local story storage
- **Terminal UI**: Modern terminal user interface built with ratatui for interactive story management

### Network Features
- **Floodsub Messaging**: Reliable message broadcasting using libp2p's floodsub protocol
- **Connection Management**: Automatic connection maintenance and peer monitoring
- **Bootstrap Discovery**: Configurable bootstrap peers for network connectivity
- **Ping Keep-Alive**: Configurable ping settings to maintain stable connections
- **Direct Message Retry**: Automatic retry logic for failed direct messages

### Message Notifications
- **Visual Indicators**: Color-coded conversations show unread messages in magenta
- **Flash Notifications**: Brief visual flash in status bar when new messages arrive
- **Sound Alerts**: Optional audio notifications (configurable, disabled by default)
- **Enhanced Timestamps**: Relative time display (now, 5m ago, Mon 14:30) in conversation views
- **Delivery Status**: Visual checkmarks (✓) for delivered outgoing messages
- **Unread Counters**: Real-time unread message counts in status bar and conversation panel

### User Interface
- **Interactive Commands**: Full command-line interface for all operations
- **Real-time Updates**: Live display of connected peers, local stories, and received messages
- **Story Creation Wizard**: Step-by-step interactive story creation process
- **Multi-panel View**: Separate sections for peers, stories, logs, and input

## Available Commands

The application provides an interactive command-line interface with the following commands:

| Command | Description |
|---------|-------------|
| `ls p` | List all discovered peers |
| `ls c` | List currently connected peers |
| `ls s all` | Request all public stories from all peers |
| `ls s local` | Show your local stories |
| `create s` | Start interactive story creation wizard |
| `publish s <story_id>` | Publish a story to the network by ID |
| `name <alias>` | Set your peer's display name/alias |
| `msg <peer_alias> <message>` | Send a direct message to a peer |
| `connect <multiaddr>` | Connect to a specific peer using multiaddr |
| `help` | Show all available commands |
| `quit` | Exit the application |

## Getting Started

### Prerequisites
- Rust 1.88+ 
- Cargo package manager

### Building and Running
```bash
# Clone the repository
git clone https://github.com/bhagdave/p2p-play.git
cd p2p-play

# Build the project
cargo build

# Run the application
cargo run

# Run with debug logging
RUST_LOG=debug cargo run
```

### Testing
```bash
# Run all tests
cargo test

# Run with the test runner script
./scripts/test_runner.sh
```

## Project Origins

This project was originally inspired by a [LogRocket blog post](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/) and the linked [GitHub repository](https://github.com/zupzup/rust-peer-to-peer-example). However, the codebase has been considerably evolved and modernized since then, and may not resemble the original implementation.

I should also add that I am using this project to test different code assistants and see how they work and how to integrate them into a normal workflow. This represents an ongoing experiment in AI-assisted development and learning.

## Configuration

The application uses a unified network configuration file (`unified_network_config.json`) that consolidates all network-related settings. This file is automatically created with default values when you first run the application.

### Unified Network Configuration (`unified_network_config.json`)

The configuration file contains four main sections:

```json
{
  "bootstrap": {
    "bootstrap_peers": [
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa"
    ],
    "retry_interval_ms": 5000,
    "max_retry_attempts": 5,
    "bootstrap_timeout_ms": 30000
  },
  "network": {
    "connection_maintenance_interval_seconds": 300,
    "request_timeout_seconds": 60,
    "max_concurrent_streams": 100,
    "max_connections_per_peer": 1,
    "max_pending_incoming": 10,
    "max_pending_outgoing": 10,
    "max_established_total": 100,
    "connection_establishment_timeout_seconds": 30
  },
  "ping": {
    "interval_secs": 30,
    "timeout_secs": 20
  },
  "direct_message": {
    "max_retry_attempts": 3,
    "retry_interval_seconds": 30,
    "enable_connection_retries": true,
    "enable_timed_retries": true
  }
}
```

### Configuration Sections

#### Bootstrap Configuration
- `bootstrap_peers`: List of DHT bootstrap peers for network discovery
- `retry_interval_ms`: Time between bootstrap retry attempts (milliseconds)
- `max_retry_attempts`: Maximum number of bootstrap retry attempts
- `bootstrap_timeout_ms`: Timeout for bootstrap operations (milliseconds)

#### Network Configuration
- `connection_maintenance_interval_seconds`: Interval for connection maintenance tasks
- `request_timeout_seconds`: Timeout for network requests
- `max_concurrent_streams`: Maximum concurrent streams per connection
- `max_connections_per_peer`: Maximum connections per peer (default: 1)
- `max_pending_incoming`: Maximum pending incoming connections
- `max_pending_outgoing`: Maximum pending outgoing connections
- `max_established_total`: Maximum total established connections
- `connection_establishment_timeout_seconds`: Timeout for connection establishment

#### Ping Configuration
- `interval_secs`: How often to send ping messages (default: 30 seconds)
- `timeout_secs`: How long to wait for ping responses (default: 20 seconds)

These ping settings are more conservative than libp2p's defaults (15s interval, 10s timeout) to reduce connection drops on temporary network hiccups.

#### Direct Message Configuration
- `max_retry_attempts`: Maximum retry attempts for failed direct messages
- `retry_interval_seconds`: Time between retry attempts
- `enable_connection_retries`: Enable retries when connections fail
- `enable_timed_retries`: Enable automatic timed retries

### Hot Reloading Configuration

You can reload the network configuration at runtime using the `reload config` command in the application. This allows you to update settings without restarting the application.

**Note:** Some configuration changes (such as bootstrap peers and ping settings) may require restarting the application to take full effect.

### Legacy Configuration Files

For backward compatibility, the application still recognizes individual configuration files:
- `bootstrap_config.json`
- `network_config.json` 
- `ping_config.json`
- `direct_message_config.json`

However, the unified configuration file (`unified_network_config.json`) takes precedence and is the recommended approach.
