# Changelog

All changes to this project will be documented in this file.

## [0.3.3] - 2025-01-04

### Fixed
- Fixed user command output to display without requiring RUST_LOG environment variable
- Replaced logging macros with direct console output for all user-facing commands (help, ls, create, publish, name)
- Commands now work immediately without needing logging configuration

## [0.3.2] - 2025-07-04

### Added
- Added New option 'name' to allow a peer to be aliased
