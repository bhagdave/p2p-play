# Changelog

All changes to this project will be documented in this file.

## [Unreleased]

### Added
- **Database Connection Pooling**: Optimized database performance with connection pooling
  - Replaced singleton database connection with connection pool using `r2d2` and `r2d2_sqlite`
  - Configured pool with up to 10 concurrent connections and 2 idle connections minimum
  - Added optimized SQLite pragmas: WAL journal mode, NORMAL synchronous, memory temp storage
  - Implemented transaction management utilities for better data consistency
  - Added connection health checks, retry logic, and pool utilization monitoring
  - Maintains backward compatibility with existing API while improving concurrent performance
  - Added comprehensive test suite for connection pooling functionality
  - Fixes issue #173

## [0.9.0] 2025-01-20

### Added
- **Standardized Error Handling**: Replaced generic `Box<dyn Error>` with domain-specific error types
  - Added central `errors.rs` module with `StorageError`, `NetworkError`, `UIError`, `ConfigError`, and `AppError`
  - Enhanced error debugging with structured error chains and context preservation
  - Improved user-friendly error messages in the TUI with appropriate icons
  - Added comprehensive error conversion traits using `thiserror` crate
  - All 101+ generic error usages replaced with specific error types
  - Better error logging in main.rs with complete error chain display
  - Fixes issue #170
- **Auto-Share Configuration**: Added configuration options to control automatic story sharing behavior
  - Added `config auto-share [on|off|status]` command to control global automatic story sharing
  - Added `config sync-days <N>` command to set story sync timeframe (how many days back to sync)
  - New stories now respect the global auto-share setting when determining if they should be public/shared
  - Story synchronization now uses the configured sync timeframe instead of syncing all stories
  - Settings are persisted in `unified_network_config.json` and restored on application restart
  - Default configuration enables auto-share with 30-day sync window for optimal user experience
  - Fixes issue #159
- **Story Synchronization Protocol**: Implemented automatic peer-to-peer story synchronization when connections are established
  - Added new `StorySyncRequest` and `StorySyncResponse` message types for efficient story exchange
  - Bidirectional sync automatically triggers when peers connect without manual intervention
  - Channel-aware filtering ensures peers only receive stories from subscribed channels
  - Timestamp-based incremental sync using `last_sync_timestamp` prevents redundant data transfer
  - Database-level filtering optimization reduces memory usage and improves sync performance
  - Automatic deduplication prevents duplicate stories from being saved
  - UI feedback with 🔄 sync and ✓ checkmark icons to show sync progress
  - Fixes issue #158

## [0.8.5] 2025-08-09

### Fixed
- **Test Coverage Script Failures**: Fixed foreign key constraint violations in database testing
  - Fixed `clear_database_for_testing()` function to delete tables in correct order respecting foreign key constraints
  - Added `story_read_status` table cleanup before deleting referenced `stories` and `channels` tables
  - Fixed FOREIGN KEY constraint errors: channel_subscriptions→channels, story_read_status→stories, story_read_status→channels
  - Updated GitHub workflow to use proper test runner scripts instead of direct cargo test commands
  - All integration tests now pass when run via test runner and coverage scripts
  - Fixes issue #199

## [0.8.4] 2025-08-09

### Fixed
- **Crypto/Relay Error Messaging**: Improved user messaging for crypto/relay errors when messaging offline peers
  - Replaced confusing technical error messages with user-friendly explanations
  - Old: "❌ Failed to create relay message: Crypto error in relay: Encryption failed: Public key not found for peer..."
  - New: "🔐 Cannot send secure message to offline peer 'alice'" + helpful context about message queuing and secure messaging requirements
  - Enhanced error classification in try_relay_delivery() to detect missing public key scenarios specifically
  - Messages are still properly queued for retry when peers come online (no functional changes)
  - Eliminates technical jargon while providing clear guidance and expectations to users
  - Fixes issue #196

## [0.8.3] 2025-08-09

### Added
- **Ctrl+S Auto-Scroll Toggle**: Restored auto-scroll control functionality with Ctrl+S key combination after Escape key was repurposed for navigation
  - Added Ctrl+S key handler that toggles auto-scroll on/off with visual feedback showing current state
  - Primary solution uses Ctrl+S for universal compatibility across all terminals and operating systems
  - Fallback support for ScrollLock and F12 keys on systems where they are available and not captured by desktop environments
  - Enabled enhanced keyboard mode in crossterm to support special key detection where possible
  - Shows "Auto-scroll enabled/disabled (Ctrl+S)" message when toggling for immediate user feedback
  - Updated help text and status bar to document Ctrl+S functionality throughout the interface
  - Maintains existing auto-scroll behavior (End key still enables auto-scroll, arrow keys still disable)
  - Fixes issue #190 where users lost easy auto-scroll toggle capability

### Fixed
- **TUI Story Read Status**: Fixed issue where stories were not marked as read when viewed with Enter key in the Terminal User Interface
  - Added new `AppEvent::StoryViewed` variant to track when stories are viewed in the TUI
  - Enhanced Enter key handler to return story viewed event after displaying content
  - Updated event processor to mark stories as read and refresh unread counts automatically
  - Stories now properly update their read status and unread counts in real-time when viewed
  - Maintains existing event-driven architecture and reuses existing database functions
  - Fixes issue #189

## [0.8.2] 2025-08-08

### Fixed
- **Direct Message Duplicate Delivery**: Fixed prefer_direct flag behavior where direct messages to connected peers were incorrectly sent twice - once via direct request-response protocol and again via relay network as a "backup"
  - When `prefer_direct=true` (default configuration), successful direct messages no longer attempt relay backup delivery
  - Eliminates duplicate message delivery and unnecessary network traffic when direct connections are available
  - Improves privacy by preventing messages intended for direct delivery from being broadcast over the relay network
  - Reduces confusing user experience with multiple success log messages for the same message
  - Users who want relay backup can set `"prefer_direct": false` in `unified_network_config.json` to maintain the previous behavior
  - Added comprehensive tests to validate both `prefer_direct=true` and `prefer_direct=false` configurations
  - Fixes issue #192
- **Windows Release**: Fixed test that was failing on the windows release

## [0.8.0] 2025-08-08

### Added
- **Secure Message Routing via Intermediate Nodes**: Comprehensive relay messaging system enabling secure message delivery through intermediate peers when direct connections are unavailable
  - End-to-end encryption using ChaCha20Poly1305 with only intended recipients able to decrypt message content
  - Transparent fallback from direct messaging to relay network delivery with automatic retry mechanisms
  - Digital signature authentication using Ed25519 to prevent message spoofing and ensure sender verification
  - Hop count limiting (max 3 hops) and rate limiting (10 messages/peer/minute) to prevent network abuse
  - Timestamp-based replay protection with configurable message age windows (5 minutes)
  - Configurable relay behavior through RelayConfig: enable_relay, enable_forwarding, max_hops, prefer_direct, rate_limit_per_peer
  - New `src/relay.rs` module providing complete RelayService implementation for message encryption, forwarding, and validation
  - Enhanced `src/crypto.rs` module with ChaCha20Poly1305 AEAD encryption and Ed25519 digital signatures
  - Extended message types in `src/types.rs`: RelayMessage, RelayConfig, RelayConfirmation for relay infrastructure
  - Integration with existing floodsub network using dedicated relay message handling
  - Resource management with automatic cleanup of expired confirmations and rate limit tracking
  - User feedback showing delivery method: direct connection, relay network, or retry queue
  - Comprehensive integration tests covering end-to-end encryption, multi-hop forwarding, and security controls
  - Fixes issue #153
- **Refactored Event Processing Architecture**: Extracted the massive main.rs event loop (377 lines) into a dedicated `EventProcessor` struct for better separation of concerns and improved maintainability
  - Created new `src/event_processor.rs` module with structured event handling architecture
  - Eliminated code duplication by consolidating three identical ActionResult handling blocks into a single `handle_action_result` method
  - Extracted complex nested match statements into focused, testable methods for each event type (UI events, network events, swarm events)
  - Added proper error handling separation between different event types and connection error filtering
  - Improved testability with 3 new unit tests specifically for EventProcessor functionality
  - Maintained 100% backward compatibility - all existing functionality preserved with identical behavior
  - Reduced main.rs complexity from 692 lines to 244 lines (65% reduction) while preserving all async/await behavior and tokio::select! logic
  - Enhanced code organization with clear separation between event processing, UI handling, and business logic
  - All 66 existing unit tests continue to pass after refactoring
- **Message Relay Mechanism**: Comprehensive message relay system enabling secure message delivery through intermediate nodes when direct peer connections are unavailable
  - End-to-end encrypted messaging using ChaCha20Poly1305 cryptography with only intended recipients able to decrypt messages
  - Intelligent routing with direct-first strategy that falls back to relay delivery and finally to retry queue for maximum reliability  
  - Security features including rate limiting (10 messages/minute per peer), hop count limits (max 3 hops), and replay protection (5-minute message age window)
  - Resource management with automatic cleanup of expired confirmations and rate limits
  - Enhanced user experience with visual feedback showing delivery method: direct, relay network, or retry queue
  - Seamless integration with existing floodsub infrastructure using dedicated relay topic
  - Configurable relay settings through RelayConfig: enable_relay, enable_forwarding, max_hops, prefer_direct, rate_limit_per_peer
  - New `src/relay.rs` module with complete RelayService implementation including encryption, forwarding, and rate limiting
  - Extended message types in `src/types.rs`: RelayMessage, RelayConfig, RelayConfirmation for relay functionality
  - Enhanced direct message handler with intelligent fallback mechanism in `handle_direct_message_with_relay()` function
  - Comprehensive test coverage with 71 unit tests passing including 5 relay service tests
  - Fixes issue #155
- **Automatic Story Publishing**: Implemented automatic story publishing when stories are created, eliminating the friction of requiring users to manually run `publish s <id>` after story creation
  - Modified `create_new_story_with_channel()` to set stories as public by default instead of private
  - Enhanced story creation handlers to automatically broadcast newly created stories to connected peers
  - Updated event handling to trigger auto-publish when stories are created
  - Stories now automatically appear in other connected peers' story lists immediately upon creation
  - Updated success message to indicate "created and auto-published" 
  - Modified help text to clarify that `create s` now auto-publishes and `publish s` is for manual re-publishing
  - Manual `publish s <id>` command continues to work as before for re-publishing stories
  - Channel subscription filtering is preserved - only subscribed peers receive auto-published stories
  - No breaking changes to existing workflows - maintains full backward compatibility
  - Fixes issue #157
- **Channel Auto-Subscription and Discovery System**: Comprehensive channel auto-subscription and discovery system for enhanced channel discoverability across the P2P network
  - Added `ChannelAutoSubscriptionConfig` with configurable settings for auto-subscription behavior, notifications, and subscription limits
  - Enhanced channel management commands: `ls ch available`, `ls ch unsubscribed`, `sub ch <channel>`, `unsub ch <channel>`, `set auto-sub [on|off|status]`
  - Smart channel discovery with auto-subscription logic, spam prevention, and subscription limits (default: 10 max auto-subscriptions)
  - Storage enhancements for managing available vs subscribed channels with async processing
  - Integrated into unified network configuration for centralized management with persistent configuration and validation
  - Distinguishes between available channels and subscribed channels, providing fine-grained control over subscription behavior
  - Maintains full backward compatibility with existing commands and functionality
- **Refactored Storage Operations**: Significantly reduced code duplication in storage.rs by creating reusable database operation patterns
  - Created new `src/storage/` module structure with specialized submodules:
    - `utils.rs` - Common utilities (timestamp generation, ID generation, boolean conversion)
    - `mappers.rs` - Result mapping functions for converting database rows to structs
    - `query_builder.rs` - Fluent query builder for complex database operations  
    - `traits.rs` - Generic CRUD operation traits and configuration management patterns
  - Eliminated duplicate Story row-to-struct mapping logic across 6+ functions using `map_row_to_story`
  - Replaced duplicate timestamp generation patterns with centralized `get_current_timestamp()` utility
  - Replaced duplicate ID generation patterns with reusable `get_next_id()` function
  - Standardized boolean handling with `rust_bool_to_db()` and `db_bool_to_rust()` converters
  - Added 8 new unit tests covering the refactored storage components
  - Maintained 100% backward compatibility - all existing functionality preserved
  - All 57 tests continue to pass after refactoring
- **Crypto Module for End-to-End Encryption**: Comprehensive cryptographic security module providing message encryption, decryption, and digital signatures for secure P2P communications
  - **ChaCha20-Poly1305 AEAD**: Industry-standard authenticated encryption with associated data for message confidentiality and integrity
  - **Ed25519 Digital Signatures**: Fast, secure digital signatures using existing libp2p keypairs with timestamp-based replay protection
  - **Secure Key Derivation**: HKDF-SHA256 for deriving encryption keys from shared secrets with proper cryptographic safety
  - **Memory Security**: Automatic secure memory clearing using zeroize for sensitive cryptographic key material
  - **Input Validation**: Comprehensive validation including message size limits (1MB), public key format verification, and empty input checks
  - **Replay Protection**: Enforced timestamp validation with configurable time windows (5 minutes) to prevent replay attacks
  - **Security Constants**: Replaced hardcoded magic strings with named constants for better maintainability and security
  - **Enhanced Error Handling**: Detailed error messages with proper input validation and timestamp verification
  - **Public Key Management**: Efficient caching and retrieval of peer public keys with seamless libp2p integration and format validation
  - **Comprehensive API**: Complete encrypt/decrypt/sign/verify functionality following exact specification requirements
  - **Integration Ready**: Designed for DirectMessage encryption and secure message routing through intermediate nodes
  - **Memory Safety**: Proper handling of sensitive cryptographic data with secure random nonce generation and automatic memory clearing
  - **Extensive Testing**: 9 unit tests and 3 integration tests with 100% roundtrip verification, input validation, and replay protection coverage
  - **Usage Documentation**: Complete examples in `docs/crypto_usage.md` with integration patterns and security guarantees
  - **Dependencies Added**: chacha20poly1305 (0.10), hkdf (0.12), rand (0.8), sha2 (0.10), zeroize (1.8)
  - **Module Export**: Available as `p2p_play::crypto` for easy external integration
  - Fixes issue #154
- **Batch Story Deletion**: Enhanced story deletion functionality to support deleting multiple stories at once using comma-separated IDs
  - Modified `handle_delete_story` function to parse comma-separated story IDs while maintaining full backward compatibility
  - Added robust error handling that reports invalid IDs but continues processing remaining valid ones  
  - Gracefully handles spaces and empty entries from trailing commas (e.g., `delete s 1, 2, 3,` works correctly)
  - Updated help text from `delete s <id>` to `delete s <id1>[,<id2>,<id3>...]` to reflect new functionality
  - Support for mixed valid/invalid scenarios: `delete s 1,999,2` reports errors for invalid IDs but continues processing valid ones
  - Returns `RefreshStories` if any stories were successfully deleted, ensuring UI updates appropriately
  - Comprehensive test suite with 7 new test cases covering single/multiple deletion, error handling, and edge cases
  - Usage examples: `delete s 1` (single), `delete s 1,2,3` (multiple), `delete s 1, 2, 3` (with spaces)
  - Fixes issue #150
- **Channel-Based TUI Navigation**: Implemented hierarchical channel → stories navigation system in the Terminal User Interface
  - Added `ViewMode` enum with `Channels` and `Stories(String)` variants to track navigation state between channel list and story views
  - Enhanced keyboard navigation with Enter key to drill down from channels to stories and Escape key to return to channels view
  - Updated main display area to conditionally show either channel list with story counts or filtered stories within selected channel
  - Added dynamic title bars and context-aware help text that update based on current navigation level
  - Channel view displays: `📂 channel_name (X stories) - description` with story counts for each channel
  - Stories view displays: `📖/📕 id: Story Name` filtered by selected channel with visual indicators for public/private stories
  - Comprehensive test coverage for ViewMode variants, navigation state management, and UI display formatting
- **Distributed Channel Broadcasting**: Implemented consistent channel broadcasting following the same pattern as story publishing
  - Added `PublishedChannel` struct to wrap channels with publisher information, similar to `PublishedStory`
  - Updated channel creation to broadcast `PublishedChannel` messages instead of raw `Channel` messages
  - Enhanced floodsub message handling to process `PublishedChannel` messages with publisher information displayed to users
  - Maintained backward compatibility by supporting both `PublishedChannel` and legacy `Channel` message formats
  - Added comprehensive unit tests for `PublishedChannel` functionality including serialization and equality tests
  - Channel broadcasting now follows the exact same pattern as story broadcasting for consistency across the application

### Fixed
- **Windows Unicode Compatibility**: Fixed display of emoji icons in Windows terminals that showed as empty squares
  - Created cross-platform `Icons` utility with ASCII alternatives for Windows (e.g., `[ID]`, `[DIR]`, `[BOOK]` instead of 🏷️, 📂, 📖)
  - Replaced all hardcoded Unicode emojis throughout UI with conditional compilation using `#[cfg(windows)]`
  - Updated cursor positioning logic to account for different display widths between ASCII and Unicode icons
  - Non-Windows platforms continue to display colorful Unicode emojis for better user experience
  - Added comprehensive tests for icon utility functionality
- **Channel Subscription Database Integrity**: Fixed critical issue where channel subscriptions could fail silently due to missing foreign key enforcement
  - Enabled SQLite foreign key constraints (`PRAGMA foreign_keys = ON`) in database connections to ensure referential integrity
  - Enhanced subscription error handling to check if channels exist before attempting subscription
  - Improved user feedback when subscribing to non-existent channels with clear error messages
  - Added better debugging messages for channel creation and subscription processes
  - Fixed issue where users could subscribe to channels but wouldn't see them listed in the UI due to constraint violations
- **Bootstrap DNS errors**: Fixed multiple bootstrap DNS and handshake issues to try and get DHT bootstrap working

## [0.7.5] 2025-08-03

### Fixed
- **Duplicate Peer Names in UI**: Fixed critical issue where multiple peers appeared with identical names in the UI Connected Peers section
  - Enhanced UI display logic to use 20 characters instead of 8 for peer ID truncation, ensuring uniqueness between peers with similar ID prefixes
  - Improved default name detection to properly distinguish between system-generated default names and custom user names
  - Updated peer connection tests to validate the new display formatting logic
- **Peer Connection Visibility**: Fixed issue where connected peers were not visible in the UI's "Connected Peers" section
  - Peers are now automatically added to peer_names when connections are established, ensuring immediate visibility
  - Default peer names use full peer IDs to prevent naming collisions between peers with similar ID prefixes
  - Enhanced UI display formatting to safely handle peer ID truncation without panic risks
  - Direct messaging now works immediately after peer connection without waiting for name broadcasts
- **TUI Story Display**: Fixed received stories not appearing in TUI interface immediately after being received
  - Resolved race condition where story saving was asynchronous but UI refresh happened immediately
  - Stories received from other peers now appear in TUI Stories panel immediately after the automatic refresh
  - Ensures consistency between command-line `ls s` output and TUI Stories panel
  - Modified handle_input_event in src/event_handlers.rs to trim input before processing

### Added
- **Unified Network Configuration**: Consolidated 4 separate network configuration files into a single `unified_network_config.json` with hot reload capability
  - Merged `bootstrap_config.json`, `network_config.json`, `ping_config.json`, and `direct_message_config.json` into unified structure
  - Added new `reload config` command for runtime configuration updates without application restart
  - Provides detailed feedback showing loaded configuration values and restart requirements
  - Maintained backward compatibility with existing individual config files for smooth migration
  - Enhanced configuration validation with meaningful error messages for all network settings
  - Improved configuration organization with logical grouping of related network parameters
  - Updated comprehensive documentation in README.md with complete configuration examples and usage instructions
- **Swarm Configuration Improvements**: Implemented comprehensive swarm configuration with connection limits and enhanced event filtering
  - Added granular connection control through new NetworkConfig fields: `max_connections_per_peer` (default: 1), `max_pending_incoming` (default: 10), `max_pending_outgoing` (default: 10), `max_established_total` (default: 100), and `connection_establishment_timeout_seconds` (default: 30)
  - Enhanced Connection Manager with configurable dial concurrency based on `max_pending_outgoing` setting
  - Improved swarm configuration with better timeout management and connection establishment parameters
  - Added connection event filtering to reduce UI/console noise while maintaining comprehensive file logging
  - Filters common connection errors (timeouts, refused connections, broken pipes) and categorizes errors to only show unexpected issues in UI
  - Uses serde defaults to ensure existing config files continue working with backward compatibility
  - Comprehensive validation with meaningful error messages and graceful handling of legacy configuration files
- **Configurable Network Timeouts**: Implemented configurable request-response timeouts with comprehensive validation to improve network reliability
  - **Extended existing NetworkConfig** from main branch with `request_timeout_seconds` and `max_concurrent_streams` fields
  - **Increased default timeout** from 30 to 60 seconds for better reliability on slower networks
  - **User-configurable settings** via `network_config.json` for custom timeout, concurrency, and connection maintenance values
  - **Comprehensive validation** prevents extreme configurations that could break the system
  - **Graceful fallback** to safe defaults when configuration file is missing or invalid
  - **Enhanced debugging** with configuration value logging for troubleshooting
  - Addresses network connectivity issues and premature timeouts on slower connections
- **Configurable Network Connection Maintenance Interval**: Implemented configurable network connection maintenance to reduce connection churn and improve stability
  - Replaced hardcoded 30-second connection maintenance interval with configurable value via `network_config.json`
  - Changed default maintenance interval from 30 seconds to 300 seconds (5 minutes) to significantly reduce connection churn
  - Added `NetworkConfig` structure with validation to prevent extreme values (minimum 60s, maximum 3600s)
  - Configuration automatically created with sensible defaults if file is missing or invalid
  - Provides clear error messages for invalid configurations with graceful fallback to defaults
  - Users can customize maintenance frequency based on their specific network requirements
  - Validation ensures users cannot configure values that would cause excessive churn or make network unresponsive
  - Example configuration: `{"connection_maintenance_interval_seconds": 300, "request_timeout_seconds": 60, "max_concurrent_streams": 100}`
- Enhanced network tests to verify TCP configuration improvements
- Test coverage for connection limit functionality and swarm configuration
- Comprehensive test suite for enhanced TCP features
- **Configurable Ping Settings**: Enhanced network connectivity reliability with configurable ping keep-alive settings
  - Implemented more lenient default ping settings (30s interval, 20s timeout vs. previous 15s interval, 10s timeout)
  - Added file-based configuration support via `ping_config.json` for customizing ping behavior
  - Created `PingConfig` structure with validation and error handling for loading configuration
  - Configuration falls back to sensible defaults if file is missing or invalid
  - Improved connection stability for peers with temporary network hiccups
  - Note: libp2p 0.56.0 doesn't support configurable max_failures, so only interval and timeout are configurable

### Fixed
- **TUI Corruption Prevention**: Replaced inappropriate `error!` macro usage throughout the codebase with proper `ErrorLogger` infrastructure
  - Replaced 24 network-related `error!` calls in event_handlers.rs with `log_network_error!` macro calls
  - Fixed peer discovery dial failures, story broadcast errors, subscription check errors, Kademlia bootstrap and DHT errors
  - Addressed connection monitoring failures, direct message validation errors, and node description request/response errors
  - Replaced 5 runtime `error!` calls in main.rs with `ErrorLogger.log_error()` calls for UI event handling and drawing errors
  - Updated bootstrap.rs, network.rs, and storage.rs to use proper error logging instead of console output
  - Preserved 12 critical initialization and cleanup `error!` calls that should remain visible to users
  - Network and runtime errors now properly log to files instead of corrupting the TUI display
  - Maintains same error logging behavior while preventing inappropriate console output during normal operation

### Enhanced
- **Network Reconnection Speed**: Implemented faster network reconnections with intelligent throttling for improved connection recovery
  - Reduced connection maintenance interval from **300 seconds to 30 seconds** (10x faster)
  - Added intelligent dual throttling intervals for reconnection attempts:
    - Recently connected peers (within 5 minutes): **15-second interval** for faster reconnection
    - Other peers: **60-second interval** (unchanged) for stable reconnection patterns
  - Implemented tracking of successful connections to optimize future reconnection attempts
  - Added immediate connection maintenance triggers when connections are lost
  - Reduced minimum configuration validation threshold from 60s to 30s to support faster maintenance intervals
  - Provides **10x faster connection recovery** while maintaining network stability through intelligent reconnection timing
  - Addresses issue where network reconnections were taking too long after peer connections dropped
- **DHT Bootstrap Message Management**: Improved TUI user experience by moving repetitive DHT bootstrap success messages to dedicated log files
  - DHT bootstrap success messages now write to `bootstrap.log` file instead of cluttering the TUI interface
  - Bootstrap error messages still appear in the TUI for user visibility as they should be
  - Enhanced `handle_kad_event()` function to accept `BootstrapLogger` parameter for proper message routing
  - Updated function signatures throughout event handling system to support bootstrap logging
  - Eliminates repetitive "DHT bootstrap successful with peer:" messages from TUI output log
  - Provides cleaner user interface while maintaining full bootstrap activity tracking in log files
- **Network TCP Configuration**: Significantly improved TCP transport configuration for better connectivity and resource management
  - Added connection limits to prevent resource exhaustion with optimal pending connection thresholds
  - Enhanced TCP socket configuration with explicit TTL settings and optimized listen backlog (1024)
  - Improved connection pooling with increased yamux stream limits (512 concurrent streams)
  - Configured swarm with dial concurrency factor (8) for better connection attempts
  - Added idle connection timeout (60 seconds) for automatic resource cleanup
  - Platform-specific optimizations for Windows and non-Windows systems
  - Added memory-connection-limits feature to libp2p dependencies

### Technical Details
- Updated `src/network.rs` with enhanced TCP transport configuration (lines 149-180)
- Improved yamux multiplexing configuration for better connection reuse
- Enhanced swarm configuration with connection management features

### Removed
- **Obsolete Peer Listing Commands**: Removed `ls p` (list discovered peers) and `ls c` (list connected peers) commands
  - Commands were redundant due to TUI's dedicated "Connected Peers" section showing real-time peer information
  - Removed command handlers and associated functions from event handling system
  - Updated help text and error messages to reflect command removal
  - Simplified codebase by removing unused imports and functionality

## [0.7.4] 2025-08-02

### Added
- **Direct Message Retry Logic**: Implemented robust retry mechanism for direct messages with automatic delivery attempts
  - Messages automatically retry up to 3 times before reporting failure
  - Connection-based retries: pending messages retry when target peer reconnects  
  - Time-based retries: background task processes pending messages every 30 seconds
  - Silent operation: users only see final success/failure, not individual retry attempts
  - Configurable retry count, intervals, and enable/disable features via `direct_message_config.json`
  - Thread-safe message queue with Arc<Mutex<Vec<PendingDirectMessage>>> for concurrent access
  - Improved reliability of peer-to-peer communication with backward compatibility maintained
  - Enhanced user experience with automatic retry handling instead of manual retry requirements
- **Chronological Story Ordering**: Stories are now displayed in chronological order with newest stories appearing first
  - Added `created_at` timestamp field to stories table in SQLite database
  - Database queries changed from `ORDER BY id` to `ORDER BY created_at DESC` for proper chronological sorting
  - Automatic timestamp assignment for all new stories using system time
  - Backwards compatibility through database migration that adds `created_at` column to existing stories
  - Existing stories without timestamps initially show with `created_at = 0` and appear last in chronological order
- **Enhanced Story Display Format**: Improved story list formatting with channel information and visual indicators
  - Updated story list format to show: `📖 [channel] id: Story Name` 
  - Enhanced command-line display: `📖 Public | Channel: general | id: Story Name`
  - Added channel information to story details view for better organization
  - Different emoji indicators for public (📖) vs private (📕) stories
- **Channel Broadcasting**: Implemented channel sharing functionality between P2P nodes to enable automatic propagation of channels across the network
  - Enhanced `create ch` command to broadcast newly created channels via floodsub to all connected peers
  - Added automatic reception and local storage of channels shared by other peers
  - Channels are serialized to JSON and transmitted using the existing floodsub infrastructure
  - Remote channels are automatically saved to local SQLite database upon reception
  - Provides user feedback for both channel creation ("Channel 'name' created successfully") and reception ("📺 Received channel 'name' from network")
  - Leverages existing P2P networking protocols for seamless channel distribution without requiring additional network configuration
- **Bootstrap Logging**: Added dedicated bootstrap.log file for bootstrap connection attempts and status updates to reduce TUI clutter
  - Created BootstrapLogger module with categorized logging levels (BOOTSTRAP_INIT, BOOTSTRAP_ATTEMPT, BOOTSTRAP_STATUS, BOOTSTRAP_ERROR)
  - All bootstrap activity now logged to bootstrap.log with UTC timestamps
  - Moved periodic bootstrap status logging from TUI to file for cleaner user interface
  - Bootstrap configuration loading and DHT connection status updates no longer appear in terminal interface
  - Fallback to standard logging if file operations fail

### Fixed
- **TUI Auto-scroll Re-enable Functionality**: Fixed issue where new messages were not immediately visible after re-enabling auto-scroll mode. When users pressed 'End' to re-enable auto-scroll after manual scrolling, the scroll offset was not properly reset, causing new messages to not be automatically displayed at the bottom of the output log. Enhanced the auto-scroll re-enable logic to reset the scroll offset when transitioning back to auto-scroll mode, ensuring new messages are immediately visible. Addresses feedback on PR for issue #102.
- **TUI Auto-scroll to Manual Scroll Transition**: Fixed jarring jump behavior when transitioning from auto-scroll to manual scrolling in the TUI. When auto-scroll was active and users pressed arrow keys to manually scroll, the view would incorrectly jump to the stored scroll offset (often position 0) instead of smoothly continuing from the current display position. Enhanced `scroll_up()` and `scroll_down()` methods to capture the current auto-scroll position before transitioning to manual scroll mode, providing an intuitive and seamless scrolling experience. Fixes issue #102.
- **TUI Connection Error Display**: Removed "Failed to connect" messages from TUI output while preserving error logging to `errors.log` file for debugging purposes. This eliminates unnecessary noise in the user interface while maintaining full error tracking capabilities. Fixes issue #100.
- Fixed test coverage reporting configuration with tarpaulin.

### Changes
- Moved tests to their own files and out of the general code
- Ran `cargo fmt` and `cargo clippy --fix` to clean up the codebase
- Created docs folder and moved any docs into it
- Created new `scripts/` directory
- Moved `test_runner.sh` → `scripts/test_runner.sh`
- Moved `test_coverge.sh` → `scripts/test_coverge.sh`
- **GitHub Workflow**: Updated `.github/workflows/release.yml` to reference `./scripts/test_runner.sh` instead of `./test_runner.sh`
- **Documentation**: Updated `CLAUDE.md` test runner command to use new path

### Added
- More tests to increase test coverage
- Added test_coverage.sh file to check current coverage

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
