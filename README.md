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

The application supports several configuration files to customize network behavior:

### Ping Configuration (`ping_config.json`)

Configure ping keep-alive settings to improve connection stability:

```json
{
  "interval_secs": 30,
  "timeout_secs": 20
}
```

- `interval_secs`: How often to send ping messages (default: 30 seconds)
- `timeout_secs`: How long to wait for ping responses (default: 20 seconds)

If the file doesn't exist, the application uses the default lenient settings shown above. These are more conservative than libp2p's defaults (15s interval, 10s timeout) to reduce connection drops on temporary network hiccups.

### Bootstrap Configuration (`bootstrap_config.json`)

Configure DHT bootstrap peers and retry behavior (existing functionality).

### Direct Message Configuration (`direct_message_config.json`)

Configure direct message retry logic and delivery behavior (existing functionality).
