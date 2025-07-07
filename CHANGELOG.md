# Changelog

All changes to this project will be documented in this file.

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
