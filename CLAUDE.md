# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
P2P-Play is a peer-to-peer story sharing application built with Rust using libp2p 0.56.0. The application allows users to create, publish, and share stories across a distributed network of peers using floodsub messaging with mDNS, Kademlia DHT, and ping protocols. Additional features include direct messaging between peers via request-response protocols, channel-based story organization, automatic bootstrap connectivity, SQLite-based local storage, and a terminal-based user interface (TUI) using ratatui.

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
- **main.rs**: Application entry point with main event loop handling UI events, network events, and peer communication. Includes automatic bootstrap retry logic and connection maintenance
- **network.rs**: P2P networking setup using libp2p with floodsub, mDNS, ping, Kademlia DHT, and request-response protocols. Manages peer key persistence and swarm creation
- **storage.rs**: SQLite-based data persistence for stories, channels, subscriptions, and peer information. Supports dynamic database paths for testing
- **handlers.rs**: Command handlers for user interactions including peer management, story operations, direct messaging, and channel subscriptions
- **types.rs**: Data structures and event types including stories, channels, direct messages, and bootstrap configuration
- **ui.rs**: Terminal-based user interface using ratatui with input handling, multiple display panes, and real-time updates
- **bootstrap.rs**: Automatic bootstrap connection management with retry logic and status tracking
- **event_handlers.rs**: Event processing logic for different network and application events
- **error_logger.rs**: File-based error logging system to separate network errors from UI output
- **migrations.rs**: Database schema migration system for SQLite storage

### Key Data Structures
- **Story**: Main content structure with id, name, header, body, public flag, and channel assignment
- **PublishedStory**: Wrapper for stories with publisher information
- **PeerName**: Peer alias mapping for human-readable names
- **DirectMessage**: Structure for peer-to-peer direct messaging with sender/receiver info and timestamp
- **Channel**: Channel definition with name, description, creator, and creation timestamp
- **ChannelSubscription**: Peer subscription to channels with timestamp tracking
- **BootstrapConfig**: Configuration for bootstrap peers with retry settings and validation
- **EventType**: Unified event system for handling user input, network events, UI events, and responses

### Network Architecture
- Uses libp2p with floodsub for message broadcasting and story sharing
- mDNS for automatic peer discovery on local network
- Kademlia DHT for global peer discovery and bootstrap connectivity
- Ping protocol for connection monitoring and keep-alive
- Request-response protocols for direct messaging and node descriptions
- Automatic bootstrap with configurable retry logic and peer management
- Persistent peer key storage in `peer_key` file with Ed25519 keypairs
- Bootstrap configuration in `bootstrap_config.json`

### Data Storage
- SQLite database (`stories.db`) for persistent story, channel, and subscription storage
- Database migrations system for schema evolution
- Dynamic database paths for testing (`TEST_DATABASE_PATH` environment variable)
- JSON backward compatibility for legacy story files
- Error logging to `errors.log` separate from UI output
- Node descriptions stored in `node_description.txt`

### Story and Channel Management
- Stories organized by channels with "general" as default
- Public/private story visibility control
- Automatic saving of received stories from other peers
- Interactive story creation with TUI-based prompts
- Channel subscriptions with automatic "general" channel subscription
- Real-time story synchronization across the network

## User Interface
The application features a terminal-based user interface (TUI) with:
- **Multiple display panes**: Local peers, connected peers, local stories, received stories, direct messages, and log output
- **Interactive input**: Commands processed through input field at bottom
- **Real-time updates**: Live display of peer connections, story updates, and messages
- **Keyboard navigation**: Arrow keys for scrolling, Tab for pane switching
- **Story creation mode**: Multi-step interactive story creation within the TUI

## Application Commands
The application runs interactively with these commands:
- `ls p` - List discovered peers
- `ls c` - List connected peers  
- `ls s all` - Request all public stories from all peers
- `ls s local` - Show local stories
- `create s` - Create a new story interactively in the TUI
- `publish s <story_id>` - Publish a story to the network
- `name <alias>` - Set local peer name
- `msg <peer_alias> <message>` - Send direct message to a peer by their alias name
- `connect <multiaddr>` - Connect to a specific peer
- `bootstrap` - Manually trigger bootstrap connection attempt
- `desc <description>` - Set node description
- `help` - Show available commands
- `quit` or `Ctrl+C` - Exit application

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
- **Unit tests** (`cargo test --lib`): Individual component testing for storage, networking, types, and handlers
- **Integration tests** (`cargo test --test integration_tests`): End-to-end functionality testing with isolated test database
- **Test runner script** (`./scripts/test_runner.sh`): Comprehensive test execution with proper cleanup
- **Separate test database**: Uses `TEST_DATABASE_PATH` environment variable to isolate test data
- **Mock objects**: Testing network behavior and peer interactions
- **Coverage reporting**: Tarpaulin integration for test coverage analysis (`tarpaulin.toml`)

## Dependencies
Key dependencies from Cargo.toml:
- **libp2p 0.56.0**: Core P2P networking with floodsub, mDNS, Kademlia, ping, and request-response
- **tokio 1.43**: Async runtime with comprehensive feature set
- **rusqlite 0.29**: SQLite database integration with bundled sqlite
- **ratatui 0.27**: Terminal UI framework
- **crossterm 0.27**: Cross-platform terminal manipulation
- **serde/serde_json**: Serialization for network messages and configuration
- **chrono**: Timestamp handling with serde support
- **log/env_logger**: Logging infrastructure
- Always run tests using the script ./scripts/test_runner.sh