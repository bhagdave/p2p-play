# Changelog

All changes to this project will be documented in this file.

## [unreleased]

### Changes
- Moved tests to their own files and out of the general code
- Ran `cargo fmt` and `cargo clippy --fix` to clean up the codebase

## [0.7.3] - 2025-07-24

### Fixed
- **Windows Socket Error 10048**: Fixed "Address already in use"  on Windows 10, causing connection failures during transport protocol negotiation
- **Windows Keyboard Error**: Fudge on timeouts to get the key input working on widows 10

## [0.7.2] - 2025-07-23

### Fixed
- **Windows Socket Error 10048**: Fixed "Address already in use" (WSAEADDRINUSE) error that frequently occurred on Windows 10, causing connection failures during transport protocol negotiation
  - **Platform-specific TCP Configuration**: Added conditional compilation to disable port reuse on Windows systems while maintaining optimal performance on Unix systems
  - **Connection Throttling**: Implemented 60-second minimum interval between reconnection attempts to the same peer to reduce rapid reconnection issues
  - **Enhanced Error Handling**: Replaced blocking mutex operations with non-blocking `try_lock()` and graceful fallback mechanisms
  - **Memory Management**: Added periodic cleanup of old connection attempt entries to prevent memory leaks
  - Fixes issue #52

## [0.7.1] - 2025-07-23

### Fixed
- Release workflow fixed

## [0.7.0] - 2025-07-23

### Added
- **Enhanced DHT Bootstrap Functionality**: Comprehensive bootstrap management for better node connectivity and reliability
  - **Persistent Bootstrap Configuration**: Bootstrap peers saved to `bootstrap_config.json` with default peers from libp2p.io for immediate connectivity
  - **Automatic Bootstrap on Startup**: DHT automatically bootstraps using configured peers on application startup
  - **Smart Retry Logic**: Exponential backoff retry mechanism (5s → 10s → 20s → 40s → 80s) with background operation that doesn't block user interaction
  - **Enhanced Command System**: Six new bootstrap management commands with backward compatibility:
    - `dht bootstrap add <multiaddr>` - Add peer to persistent configuration
    - `dht bootstrap remove <multiaddr>` - Remove peer from configuration with validation preventing removal of last peer
    - `dht bootstrap list` - Show all configured peers with status details
    - `dht bootstrap clear` - Clear all configured bootstrap peers
    - `dht bootstrap retry` - Manually trigger bootstrap retry with all configured peers
    - `dht bootstrap <multiaddr>` - Direct bootstrap (original functionality preserved)
  - **Real-time Status Monitoring**: Bootstrap status tracking (`NotStarted` → `InProgress` → `Connected`/`Failed`) with DHT event integration
  - **Thread-safe Implementation**: AutoBootstrap component with Arc&lt;Mutex&gt; protection for concurrent access safety
  - **Comprehensive Validation**: Bootstrap peer validation with graceful error handling for malformed addresses
  - **Multiple Bootstrap Peer Support**: Redundancy through multiple bootstrap peers tried in sequence until successful
  - **Periodic Status Logging**: Bootstrap progress visibility with status updates every 30 seconds in application logs
- **Node Descriptions**: Optional node descriptions that can be shared between peers on the P2P network
  - `create desc <description>` command to create a node description (max 1024 bytes)
  - `show desc` command to display your current node description with byte count
  - `get desc <peer_alias>` command to request description from a connected peer
  - Uses dedicated request-response protocol with structured NodeDescriptionRequest/NodeDescriptionResponse types
  - Comprehensive validation for file size limits, empty descriptions, and peer connectivity
  - Note: All nodes must run the same version to use node descriptions due to protocol compatibility
- **Step-by-step Interactive Story Creation**: Enhanced story creation with guided prompts and improved user experience
  - Interactive prompts for name, header, and body fields with real-time validation
  - Improved cursor positioning and text rendering for multi-line story content
  - Enhanced emoji width calculation for proper text display and cursor management
  - Better error handling and user feedback during story creation process
  - Streamlined UI flow with clearer instructions and visual indicators
- **Kademlia DHT Support**: Complete implementation of Kademlia DHT for internet-wide peer discovery
  - New `dht bootstrap <multiaddr>` command to connect to bootstrap peers and join the DHT network
  - New `dht peers` command to discover closest peers in the DHT network
  - Automatic DHT server mode for accepting queries and providing records to other peers
  - Seamless integration with existing floodsub story sharing and direct messaging
  - Enhanced peer discovery beyond local network (mDNS) to internet-scale connectivity
  - Comprehensive event handling for bootstrap success/failure and peer discovery notifications
- **Story Deletion**: New `delete s <id>` command to permanently remove stories from local storage
- Local story deletion functionality with proper error handling and user feedback
- Comprehensive test coverage for story deletion including edge cases and database operations
- test_runner script now uses tarpaulin to produce coverage report
- **TUI Auto-scroll**: Added intelligent auto-scroll functionality for terminal output
  - Automatically scrolls to show new messages when they arrive
  - Preserves manual scroll position when user scrolls up to read history
  - End key re-enables auto-scroll and jumps to latest messages
  - Status bar shows current auto-scroll state (AUTO: ON/OFF)
  - Maintains user control while ensuring new content is visible

### Fixed
- **TUI Responsiveness**: Fixed TUI interface keystrokes ('c', 'i', 'q') taking a long time to register, especially during network operations
  - Removed blocking 1-second sleep during story publishing and 2-second sleep during connection establishment
  - Moved heavy I/O operations (story saving) to background tasks using `tokio::spawn()` to prevent blocking the main event loop
  - Enhanced event loop to prioritize UI events over network events for immediate response
  - Made `UILogger` cloneable to support error reporting from background tasks
  - UI commands now process immediately regardless of network activity while preserving all existing functionality
- **Terminal UI Text Rendering**: Fixed text overlap and readability issues in the Output panel where text was rendering without proper spacing
- Improved text wrapping by changing from `Wrap { trim: true }` to `Wrap { trim: false }` to preserve spacing
- Enhanced text rendering using explicit ratatui `Text`/`Line`/`Span` structures for better text handling
- Updated layout constraints from percentage-based to minimum width constraints for improved display stability
- Removed handshake failures and network error messages from console output to prevent TUI interface disruption
- Replaced println! statements in network.rs with proper logging calls
- Network connection errors (incoming/outgoing) now log to error file instead of console
- Replaced unsafe environment variable manipulation in tests with safe alternatives using temporary databases

### Changed
- Updated logging system from pretty_env_logger to env_logger for better control
- Added custom logger configuration to filter libp2p internal errors from console
- Enhanced ErrorLogger with log_network_error() method for network-specific error handling
- Configured log level filtering to suppress noisy libp2p module messages (libp2p_swarm, libp2p_tcp, etc.)
- Removed noisy connection and disconnection messages from TUI output log to improve user experience
- Connection establishment messages ("Connected to new peer: {peer_id}") no longer appear in the output log
- Disconnection messages ("Disconnected from {name}: {peer_id}") no longer appear in the output log  
- Failed connection messages ("Failed to connect to {peer_id}: {error}") no longer appear in the output log
- Connection status remains visible in the dedicated "Connected Peers" section
- Connection events are still logged to file for debugging purposes
- Test suite now uses safe Rust code exclusively, eliminating all unsafe blocks from storage tests
- Additional test coverage with target of over 40%

## [0.6.0] - 2025-07-16

### Added
- **Channel System**: Complete channel-based story organization with subscription management
- `ls ch` - List all available channels with descriptions
- `ls sub` - List your channel subscriptions
- `create ch name|description` - Create new channels for organizing stories
- `sub <channel>` - Subscribe to specific channels to receive their stories
- `unsub <channel>` - Unsubscribe from channels you no longer want to follow
- Enhanced `create s` command with optional channel parameter: `create s name|header|body[|channel]`
- Automatic subscription to "general" channel for all new users
- Channel-based story filtering across the P2P network - peers only receive stories from subscribed channels
- SQLite database tables for channels (`channels`) and subscriptions (`channel_subscriptions`)
- Show current alias when 'name' command is used without arguments
- Added test coverage for 'name' command functionality without arguments
- Test verifies that typing just 'name' shows current alias or helpful message if no alias is set
- File-based error logging system that writes errors to `errors.log` instead of displaying them in the UI
- New ErrorLogger module with timestamped error logging and comprehensive test coverage
- Added chrono dependency for UTC timestamps in error logs
- Added the clear output functionality for the TUI: Press 'c' in Normal mode to clear all output from the scrolling log area
- Added comprehensive test coverage for clear output functionality including edge cases and key binding integration
- Updated UI instructions and help text to include information about the clear output feature
- **Proper point-to-point direct messaging using libp2p request-response protocol**
- Added `request-response` and `cbor` features to libp2p dependency for true peer-to-peer communication
- Created `DirectMessageRequest` and `DirectMessageResponse` types for proper message serialization
- Added delivery confirmations for direct messages with success/failure feedback
- Enhanced security with sender identity validation to prevent message spoofing
- Added timeout and retry policies for request-response protocol reliability

### Changed
- **BREAKING CHANGE**: Updated `Story` structure to include `channel` field - not backward compatible with v0.5.x
- **Network Protocol**: Stories now include channel information in serialization format
- **Story Filtering**: Peer-to-peer story sharing now filtered by channel subscriptions
- Stories default to "general" channel if no channel specified
- Database migration automatically adds channel support to existing stories
- Enhanced help text to include all new channel-related commands
- Errors from story operations (list, create, publish) are now logged to file instead of being displayed in the UI
- Cleaner user interface experience with errors no longer cluttering the display
- Error logging includes fallback to stderr if file writing fails
- **Replaced broadcast-based direct messaging with true point-to-point protocol**
- Direct messages now use `request-response` protocol instead of `floodsub` broadcasting
- Messages are sent directly to intended recipients only, eliminating network overhead
- Enhanced privacy as messages are no longer broadcast to all peers
- Added proper error handling and delivery confirmations for direct messages

### Technical Details
- Channel subscriptions are stored per peer in SQLite database
- Automatic database migration adds channel column to existing story tables
- Stories from unsubscribed channels are filtered out during network communication
- Default "general" channel created automatically on first run
- Backward compatibility for story storage while maintaining network protocol breaking change
- Tests wil now only work with the test runner or you will get db failures

## [0.5.0] - 2025-07-14

### Added
- Added Terminal User Interface (TUI) built with ratatui for improved user experience
- Multi-panel layout with status bar, output log, connected peers panel, and stories panel
- Interactive controls with keyboard navigation and visual feedback
- Real-time connection status updates and story sharing notifications
- Added "show story <id>" command to display full story details including header, body, and public status

### Performance
- Optimized peer name sorting performance with caching in direct message parsing
- Implemented SortedPeerNamesCache to maintain pre-sorted peer names, reducing complexity from O(n log n) per direct message command to O(1)
- Cache only updates when peer list changes (new peers, disconnections, name changes)
- Maintains identical functional behavior while significantly improving performance for direct messaging in environments with many peers


## [0.4.1] - 2025-07-13

### Changed
- Addressed deprecation warnings that were appearing during compilation by replacing deprecated floodsub type aliases with their non-deprecated equivalents.
- Fixed direct messaging failure when peer names contained spaces. The command msg Alice Smith Hello world was incorrectly parsed as sending "Smith Hello world" to peer "Alice" instead of sending "Hello world" to peer "Alice Smith".
- Modified peer name handling in handle_floodsub_event to check if name has changed
- Added proper logging for first-time names vs. changed names vs. unchanged names
- Ensures peer names are fixed when first received and only updated if they change


## [0.4.0] - 2025-07-07

### Added
- Direct messaging functionality between peers using alias names
- New `msg <peer_alias> <message>` command for sending private messages
- DirectMessage data structure with sender/receiver info and timestamp
- Message filtering to ensure only intended recipients see direct messages
- Visual indicators (📨 emoji) for received direct messages
- Command validation to ensure sender has set their name and recipient exists
- Comprehensive unit tests for direct messaging functionality

### Changed
- Updated JSON storage of stories from json to sqlite
- Refactored story management to use SQLite for persistent storage
- Updated peer alias storage to use SQLite instead of JSON
- Updated help text to include new messaging command
- Enhanced floodsub event handler to process DirectMessage types
- Extended event system to support direct message events
- Updated libp2p dependencies to latest version
- Removed unused dependencies and cleaned up Cargo.toml

### Technical Details
- Uses broadcast + filtering approach for message delivery
- Messages are sent via existing floodsub infrastructure but only displayed to intended recipients
- Future enhancement planned for true point-to-point messaging using libp2p request-response protocol
- SQLite database schema includes tables for stories and peer aliases

## [0.3.6] - 2025-07-06

### Added
- Creates stories.json file on startup on stories

### Changed
- Refactored the large event handling logic in main.rs by extracting it into a dedicated event_handlers.rs module

## [0.3.5] - 2025-07-05

### Added
- Interactive story creation mode: Users can now create stories by typing `create s` and being prompted for each element (name, header, body)
- Input validation for story creation with clear error messages for empty inputs
- Updated help text to show both interactive and legacy creation modes

### Changed
- Enhanced `create s` command to support both interactive mode (no arguments) and legacy pipe-separated format (with arguments)
- Improved user experience with guided prompts for story elements

## [0.3.4] - 2025-07-05

### Added
- Added persistent storage for peer aliases across application restarts
- Peer names are now automatically saved when set via the `name` command
- Saved peer names are loaded automatically on application startup
- Added comprehensive unit tests for peer name persistence functionality

### Changed
- Users no longer need to re-enter their alias after restarting the application
- Peer names are stored in JSON format in `./peer_name.json`

## [0.3.3] - 2025-07-04

### Fixed
- Fixed user command output to display without requiring RUST_LOG environment variable
- Replaced logging macros with direct console output for all user-facing commands (help, ls, create, publish, name)
- Commands now work immediately without needing logging configuration

## [0.3.2] - 2025-07-04

### Added
- Added New option 'name' to allow a peer to be aliased
