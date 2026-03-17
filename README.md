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

## Terminal Requirements

P2P-Play uses [ratatui](https://github.com/ratatui-org/ratatui) for its terminal user interface. The following requirements must be met for correct rendering:

- **Minimum terminal size**: 80×24 columns×rows recommended — ratatui layouts may panic or render incorrectly in smaller terminals.
- **UTF-8 encoding**: The terminal must support UTF-8. On Windows CMD, run `chcp 65001` before launching, or switch to [Windows Terminal](https://aka.ms/terminal) which enables UTF-8 by default.
- **Colour support**: 256-colour mode is recommended for the best visual experience. Terminals limited to 16 colours will fall back gracefully but some visual indicators (e.g. unread message highlights) may appear less distinct.
- **Known working terminals**: iTerm2, GNOME Terminal, Windows Terminal, Alacritty, kitty.

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

### Quick Start

Once the application is running, follow these five steps to get up and running quickly:

1. **Set your peer name** — Give yourself a human-readable alias so other peers can identify you:
   ```
   name <alias>
   ```
   *Example:* `name alice`

2. **Discover peers** — On a local network, mDNS handles peer discovery automatically. You should see nearby peers appear in the peers panel within a few seconds. To connect to a peer on a different network, use a bootstrap peer address:
   ```
   connect <multiaddr>
   ```
   *Example:* `connect /ip4/1.2.3.4/tcp/4001/p2p/QmPeerID...`

3. **Check connected peers** — Verify you have peers available to share stories with:
   ```
   ls c
   ```

4. **Create a story** — Launch the interactive story creation wizard:
   ```
   create s
   ```
   Follow the prompts to enter a name, header, body, and channel for your story.

5. **Publish your story** — Share your story with all connected peers using the story ID shown after creation:
   ```
   publish s <story_id>
   ```

For a full list of commands, type `help` inside the application or see the [Available Commands](#available-commands) section.

### Testing
```bash
# Run all tests
cargo test

# Run with the test runner script
./scripts/test_runner.sh
```

## Platform Notes

### Windows

- **TCP listener binding**: On Windows, the application binds the TCP listener to `127.0.0.1` (localhost only) instead of `0.0.0.0` (all interfaces). This is intentional to avoid common port-conflict errors on Windows, but it means the node is **only reachable from the same machine**. Peers on other machines on your local network will not be able to connect to a Windows node that is listening only on `127.0.0.1`. mDNS may still allow those peers to *discover* the Windows node on the local subnet, but it will typically advertise only a loopback address, so connection attempts from other machines will fail.
- **mDNS limitations**: mDNS may be blocked by the Windows Firewall by default. If peer discovery is not working, check that your firewall allows mDNS traffic (UDP port 5353).
- **Terminal (UTF-8)**: The TUI uses UTF-8 characters (borders, icons, status indicators). Ensure your terminal emulator is set to UTF-8 encoding. Windows Terminal and recent versions of PowerShell support this out of the box; the legacy Command Prompt (`cmd.exe`) may display garbled characters.

### Linux / macOS

- The TCP listener binds to `0.0.0.0` (all interfaces), so the node is reachable from other machines on the local network as expected.
- mDNS peer discovery works out of the box on most distributions and macOS versions.

## Project Origins

This project was originally inspired by a [LogRocket blog post](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/) and the linked [GitHub repository](https://github.com/zupzup/rust-peer-to-peer-example). However, the codebase has been considerably evolved and modernized since then, and may not resemble the original implementation.

I should also add that I am using this project to test different code assistants and see how they work and how to integrate them into a normal workflow. This represents an ongoing experiment in AI-assisted development and learning.

## Configuration

The application uses a unified network configuration file (`unified_network_config.json`) that consolidates all network-related settings. This file is automatically created with default values when you first run the application.

### Unified Network Configuration (`unified_network_config.json`)

The configuration file contains multiple sections covering all network and resource management settings:

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
  },
  "wasm": {
    "default_fuel_limit": 10000000,
    "default_memory_limit_mb": 64,
    "max_memory_limit_mb": 1024,
    "default_timeout_secs": 30
  }
}
```

**Note:** The configuration file also includes `channel_auto_subscription`, `message_notifications`, `relay`, `auto_share`, and `circuit_breaker` sections. See the `unified_network_config.json.example` file for a complete configuration example.

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

#### WASM Configuration
- `default_fuel_limit`: Default computational limit for WASM execution (default: 10,000,000 instructions)
- `default_memory_limit_mb`: Default memory limit for WASM modules (default: 64 MB)
- `max_memory_limit_mb`: Maximum allowed memory limit for WASM modules (default: 1024 MB / 1 GB)
- `default_timeout_secs`: Default execution timeout for WASM modules (default: 30 seconds)

The WASM configuration controls resource limits for executing WebAssembly modules, preventing excessive resource consumption and ensuring system stability.

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
