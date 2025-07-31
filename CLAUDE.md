# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
P2P-Play is a peer-to-peer story sharing application built with Rust using libp2p 0.56.0. The application allows users to create, publish, and share stories across a distributed network of peers using floodsub messaging with mDNS and DHT peer discovery.  Additional features include direct messaging between peers, local story management, and a command-line interface for user interactions.

## Development Commands

### Build and Run
```bash
# Build the project
cargo build

# Run the application
cargo run

# Run with logging
RUST_LOG=debug cargo run
```

### Testing
```bash
# Run all tests using the test runner script
./scripts/test_runner.sh

# Run unit tests only
cargo test --lib

# Run integration tests only
cargo test --test integration_tests

# Run all tests with output
cargo test
```

### Code Quality
```bash
# Check code formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy for linting
cargo clippy

# Check for unused dependencies
cargo machete
```

## Architecture Overview

### Core Components
- **main.rs**: Application entry point with main event loop handling user input, network events, and peer communication
- **network.rs**: P2P networking setup using libp2p with floodsub, mDNS, and ping protocols. Manages peer keys and swarm creation
- **storage.rs**: Local file system operations for stories and peer data persistence
- **handlers.rs**: Command handlers for user interactions (list peers, create stories, publish stories, etc.)
- **types.rs**: Data structures and event types for the application

### Key Data Structures
- **Story**: Main content structure with id, name, header, body, and public flag
- **PublishedStory**: Wrapper for stories with publisher information
- **PeerName**: Peer alias mapping for human-readable names
- **DirectMessage**: Structure for peer-to-peer direct messaging with sender/receiver info and timestamp
- **EventType**: Unified event system for handling user input, network events, and responses

### Network Architecture
- Uses libp2p with floodsub for message broadcasting
- mDNS for automatic peer discovery on local network
- Ping protocol for connection monitoring
- Direct messaging between peers using floodsub with broadcast + filtering
- Persistent peer key storage in `peer_key` file
- Peer name persistence in `peer_name.json`

### Story Management
- Stories stored locally in JSON format
- Public/private story visibility control
- Automatic saving of received stories from other peers
- Interactive story creation with user prompts

## Application Commands
The application runs interactively with these commands:
- `ls p` - List discovered peers
- `ls c` - List connected peers
- `ls s all` - Request all public stories from all peers
- `ls s local` - Show local stories
- `create s` - Create a new story interactively
- `publish s <story_id>` - Publish a story to the network
- `name <alias>` - Set local peer name
- `msg <peer_alias> <message>` - Send direct message to a peer by their alias name
- `connect <multiaddr>` - Connect to a specific peer
- `help` - Show available commands
- `quit` - Exit application

## Logging
The application uses multiple logging mechanisms:

### TUI Logging
- User interactions and responses are shown in the terminal UI
- Connection status and peer information
- Story management feedback

### File Logging
- **Bootstrap Log** (`bootstrap.log`): Automatic bootstrap connection attempts, status updates, and DHT connectivity information
- **Error Log** (`errors.log`): Network errors and connection issues that would otherwise clutter the UI
- **Standard Log**: Debug and info messages via `env_logger` (can be directed to files using `RUST_LOG` environment variable)

### Log Categories
- `BOOTSTRAP_INIT`: Bootstrap configuration and initialization
- `BOOTSTRAP_ATTEMPT`: Automatic bootstrap retry attempts  
- `BOOTSTRAP_STATUS`: DHT connection status and peer counts
- `BOOTSTRAP_ERROR`: Bootstrap failures and errors
- `NETWORK_ERROR`: Network connection and protocol errors

## Testing Strategy
- Unit tests for individual components and functions
- Integration tests for end-to-end functionality
- Test runner script for convenient test execution
- Mock objects for testing network behavior
