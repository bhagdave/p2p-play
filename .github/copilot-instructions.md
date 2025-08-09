
# P2P-Play Development Instructions

**ALWAYS follow these instructions first and only fall back to additional search and context gathering if the information here is incomplete or found to be in error.**

P2P-Play is a peer-to-peer story sharing application built with Rust using libp2p 0.56.0. The application allows users to create, publish, and share stories across a distributed network of peers using floodsub messaging with mDNS and DHT peer discovery. Additional features include direct messaging between peers, local story management, and a command-line interface for user interactions.

## Working Effectively

### Build and Test Commands
Build the project:
```bash
cargo build
# NEVER CANCEL: Development build takes ~4 minutes. Set timeout to 10+ minutes.

cargo build --release
# NEVER CANCEL: Release build takes ~2.5 minutes. Set timeout to 5+ minutes.
```

Run tests:
```bash
# Use the comprehensive test runner script (RECOMMENDED)
./scripts/test_runner.sh
# NEVER CANCEL: Test suite takes ~1 minute. Set timeout to 3+ minutes.

# Alternative - run tests directly 
cargo test
# NEVER CANCEL: All tests take ~1 minute. Set timeout to 3+ minutes.
```

### Code Quality Commands
Format code (REQUIRED before committing):
```bash
cargo fmt --check  # Check if formatting is needed
cargo fmt          # Apply automatic formatting
```

Lint code (REQUIRED after completing work):
```bash
cargo clippy
# NEVER CANCEL: Linting takes ~20 seconds. Set timeout to 2+ minutes.
```

### Running the Application
```bash
cargo run
# Starts the Terminal User Interface (TUI) application
# Application listens on multiple network interfaces (127.0.0.1, local IP)
# Creates configuration files and SQLite database on first run
```

## Manual Validation Scenarios

**CRITICAL**: After making any changes, ALWAYS run through these validation scenarios to ensure functionality works correctly.

### Basic Application Validation
1. **Start Application**: Run `cargo run` and verify TUI loads without errors
2. **Interactive Story Creation**: 
   - Press 'i' to enter input mode
   - Type `create s` and press Enter
   - Follow prompts to create a story (name, header, body, channel)
   - Verify story is created successfully and appears in channel count
3. **Help System**: Type `help` to verify all commands are displayed
4. **Exit Application**: Press 'q' to quit cleanly

### Network Functionality Validation  
1. **Configuration Loading**: Verify "Loaded unified network config from file" appears
2. **Network Listening**: Verify local node listening addresses are displayed
3. **DHT Bootstrap**: Verify bootstrap peers are added to DHT (connection failures to public peers are normal in sandboxed environments)

### File System Validation
After running the application, verify these files are created:
- `stories.db` - SQLite database for story storage
- `unified_network_config.json` - Network configuration
- `peer_key` - Persistent peer identity
- `bootstrap.log` - Bootstrap connection logs  
- `errors.log` - Error logs

## Architecture Overview

### Core Components
- **main.rs**: Application entry point with main event loop
- **network.rs**: P2P networking setup using libp2p with floodsub, mDNS, and ping protocols
- **storage/**: Local file system operations for stories and peer data persistence  
- **handlers.rs**: Command handlers for user interactions
- **ui.rs**: Terminal User Interface built with ratatui
- **types.rs**: Data structures and event types

### Key Data Structures
- **Story**: Main content structure with id, name, header, body, and public flag
- **PublishedStory**: Wrapper for stories with publisher information
- **DirectMessage**: Structure for peer-to-peer direct messaging
- **Channel**: Story organization system with subscription management

### Network Architecture
- Uses libp2p with floodsub for message broadcasting
- mDNS for automatic peer discovery on local network  
- Ping protocol for connection monitoring
- Direct messaging between peers using request-response protocol
- Persistent peer key storage and peer name persistence

## Project Structure

### Source Files (`src/`)
- `main.rs` - Application entry point and main event loop
- `network.rs` - P2P networking with libp2p configuration
- `storage/` - SQLite database operations and file management
- `handlers.rs` - Command processing and business logic
- `ui.rs` - Terminal User Interface with ratatui
- `types.rs` - Core data structures and configuration types
- `event_processor.rs` - Event handling architecture
- `crypto.rs` - End-to-end encryption for secure messaging
- `relay.rs` - Message relay system for offline peer communication
- `bootstrap.rs` - DHT bootstrap management
- `error_logger.rs` - Centralized error logging system

### Test Files (`tests/`)
The project has comprehensive test coverage with 72 unit tests and multiple integration test files:
- `integration_tests.rs` - End-to-end functionality tests
- `storage_tests.rs` - Database and file system tests  
- `network_tests.rs` - P2P networking tests
- `ui_tests.rs` - Terminal interface tests
- Additional specialized test files for specific features

### Configuration Files
- `Cargo.toml` - Rust project configuration with libp2p dependencies
- `tarpaulin.toml` - Code coverage configuration (excludes UI files, targets 40%+ coverage)
- `.github/workflows/` - CI/CD pipelines for testing and releases

## Common Development Tasks

### Adding New Features
1. **Design**: Plan the feature and identify which modules need changes
2. **Types**: Add new data structures to `src/types.rs` if needed
3. **Storage**: Add database schema changes to `src/storage/` if needed
4. **Handlers**: Add command handlers to `src/handlers.rs`
5. **UI**: Add UI components to `src/ui.rs` for TUI integration
6. **Tests**: Add comprehensive tests in appropriate `tests/` files
7. **Validation**: Run build, test, lint, and manual validation scenarios

### Debugging Network Issues  
- Check `errors.log` for network-related errors
- Check `bootstrap.log` for DHT connection status
- Connection failures to public bootstrap peers are normal in sandboxed environments
- Local peer discovery via mDNS should work in most environments

### Database Operations
- Stories are stored in SQLite database (`stories.db`)
- Use existing storage functions in `src/storage/` modules
- Database migrations are handled automatically
- Test database isolation is managed by `TEST_DATABASE_PATH` environment variable

## Application Commands

The application provides an interactive command-line interface:

| Command | Description |
|---------|-------------|
| `help` | Show all available commands |
| `create s` | Interactive story creation wizard |
| `ls s local` | Show local stories |  
| `ls s all` | Request stories from all peers |
| `show story <id>` | Display full story details |
| `delete s <id1>[,<id2>,...]` | Delete one or more stories |
| `publish s <id>` | Manually republish a story |
| `ls ch [available\|unsubscribed]` | List channels |
| `create ch <name>\|<description>` | Create new channel |
| `sub <channel>` / `unsub <channel>` | Manage channel subscriptions |
| `name <alias>` | Set peer display name |
| `msg <peer_alias> <message>` | Send direct message |
| `dht bootstrap <multiaddr>` | Connect to DHT bootstrap peer |
| `reload config` | Reload network configuration |
| `quit` | Exit application |

## CI/CD Information

### GitHub Workflows
- **Release Pipeline** (`.github/workflows/release.yml`): Builds cross-platform binaries
- **Test Pipeline**: Runs `./scripts/test_runner.sh` for validation
- Builds for Linux, Windows, and macOS (both x86_64 and aarch64)

### Release Process
- Tests must pass before any release
- Release builds are optimized and cross-compiled  
- Assets are uploaded automatically for GitHub releases

## Troubleshooting

### Build Issues
- **Long build times**: First build downloads many dependencies (~4 minutes normal)
- **Clippy warnings**: Many warnings are normal for development, focus on errors
- **Format check failures**: Run `cargo fmt` to fix automatically

### Runtime Issues  
- **TUI corruption**: Application uses proper logging to files to avoid terminal corruption
- **Network timeouts**: Bootstrap connection failures are normal in sandboxed environments
- **Database locks**: Tests use single-threaded execution to avoid database conflicts

### Test Issues
- **Database conflicts**: Tests use `TEST_DATABASE_PATH` for isolation
- **Network tests**: May timeout in restricted network environments
- **Coverage**: Target is 40%+ code coverage, excluding UI files

## Development Workflow

1. **Start Development**: Run tests first to ensure clean state
2. **Make Changes**: Follow existing code patterns and architecture
3. **Validate Early**: Build and test frequently during development
4. **Format Code**: Run `cargo fmt` before committing
5. **Final Validation**: Run `cargo clippy` and complete manual validation scenarios
6. **Update Changelog**: Add entry to `CHANGELOG.md` for any changes made
