# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**IMPORTANT**: All development MUST comply with the Project Constitution (`.specify/memory/constitution.md`), which defines non-negotiable principles for TDD, code quality, UX consistency, performance, and security.

## Project Overview
P2P-Play is a peer-to-peer story sharing application built with Rust using libp2p 0.56.0. The application allows users to create, publish, and share stories across a distributed network of peers using floodsub messaging with mDNS, Kademlia DHT, and ping protocols. Features include direct messaging between peers (with relay support and retry queues), channel-based story organization, automatic bootstrap connectivity, WASM capability exchange (advertise and remotely execute WebAssembly modules), SQLite-based local storage, full-text story search, story export, and a terminal-based user interface (TUI) using ratatui.

## Development Commands

### Build and Run
```bash
# Build the project
cargo build

# Run the application
cargo run

# Run with logging
RUST_LOG=debug cargo run

# Use a custom data directory (isolates stories.db, logs, peer_key)
cargo run -- --data-dir /tmp/p2p-play-node1
```

### Testing
```bash
# Preferred: full test matrix (builds wasm fixture, sets TEST_DATABASE_PATH)
./scripts/test_runner.sh

# Unit/library tests
cargo test --lib --features test-utils

# A specific integration test file
cargo test --test conversation_tests --features test-utils -- --nocapture

# A single named test
cargo test --test conversation_tests --features test-utils test_message_ordering -- --nocapture

# All tests (no isolation guarantees)
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

# Generate HTML coverage report (output: tarpaulin-report.html)
./scripts/test_coverage.sh
```

## Architecture Overview

### Core Components
- **main.rs**: Composition root. Resolves `--data-dir`, initialises the TUI, ensures SQLite DB and `unified_network_config.json` exist, creates the libp2p swarm, sets up loggers/channels, and hands control to `EventProcessor`.
- **event_processor.rs**: Main application loop. Multiplexes UI events, swarm events, timers (connection maintenance, bootstrap retry, DM retry, handshake timeout, network-health), and internal channels. Routes work into `event_handlers.rs` and the `handlers/` submodules.
- **lib.rs**: Shared library logic re-exported for tests and modules.
- **network/mod.rs** + **network/protocol.rs**: libp2p swarm construction and behaviour wiring. Defines `StoryBehaviour` combining floodsub, mDNS, Kademlia, ping, and six request-response protocols (direct messages, node descriptions, story sync, handshake verification, WASM capability discovery, WASM execution). Manages peer key persistence.
- **relay.rs**: Floodsub-based relay service for delivering direct messages to offline/unreachable peers via intermediate nodes.
- **circuit_breaker.rs**: Generic circuit-breaker state machine (`Closed → Open → HalfOpen`) with configurable thresholds and timeouts.
- **network_circuit_breakers.rs**: Wires circuit breakers to network operations for connection resilience.
- **handlers/**: Domain-split command handler submodules (see below).
- **event_handlers.rs**: Low-level swarm event processing: floodsub messages, mDNS discovery, ping events, all six request-response event types, Kademlia events, and relay forwarding.
- **storage/core.rs**: SQLite operations for stories, channels, subscriptions, conversations, read state, peer aliases, WASM offerings, bootstrap config, and unified config persistence, plus local node-description file helpers.
- **storage/mappers.rs**: Row-to-type mapping helpers.
- **storage/utils.rs**: Storage utility helpers.
- **migrations.rs**: SQLite schema migration system; guarantees default `general` channel exists.
- **types.rs**: All domain data structures and the `EventType` enum.
- **validation.rs**: `ContentValidator` and `ContentSanitizer` for user-controlled text (stories, channels, peer names, messages, WASM offerings). Always use these for new input.
- **errors.rs**: Typed error boundaries: `StorageError`, `NetworkError`, `ConfigError`, `WasmExecutionError`, `CryptoError`, `FetchError`, `RelayError`, and `ValidationError`. Prefer these over `Box<dyn Error>` in application code.
- **crypto.rs**: ChaCha20-Poly1305 encryption, HKDF key derivation, Ed25519 / X25519 key exchange, and replay-protection logic for direct messages.
- **ui.rs**: Ratatui TUI: stateful, mode-driven (`Normal`, `StoryCreation`, `QuickReply`, `ConversationView`, `MessageCompose`). Multiple display panes; keyboard navigation; scroll-lock toggle.
- **bootstrap.rs**: Auto-bootstrap manager with configurable retry logic and status tracking; writes to `bootstrap.log` via `BootstrapLogger`.
- **bootstrap_logger.rs**: Structured file logger for bootstrap events.
- **error_logger.rs**: File logger for network errors (writes to `errors.log`), keeping the TUI clean.
- **file_logger.rs**: Generic file-based logger shared by bootstrap and error loggers.
- **wasm_executor.rs**: Wasmtime-based WASM execution engine. Supports fuel limiting, memory caps, per-execution timeouts, LRU module caching, WASI preview1 stdio, and typed parameter passing.
- **content_fetcher.rs**: `ContentFetcher` trait + `GatewayFetcher` implementation for fetching WASM binaries from IPFS gateways (default: `https://dweb.link`).
- **constants.rs**: All app-wide constants: `APP_NAME`, `APP_VERSION` (from `Cargo.toml`), `APP_PROTOCOL`, `ContentLimits`, WASM limits, TCP/Yamux tuning, security constants.
- **data_dir.rs**: `get_data_path(filename)` helper; routes all runtime files through `DATA_DIR` env var (set from `--data-dir` CLI flag).
- **time.rs**: `current_unix_timestamp()` helper.

### Handler Submodules (`src/handlers/`)
| Submodule      | Responsibility                                                        |
|----------------|-----------------------------------------------------------------------|
| `stories`      | Create, list, publish, show, delete, search, filter, export stories   |
| `channels`     | Create, list, subscribe, unsubscribe, auto-subscription management    |
| `messaging`    | Direct messages, peer naming, relay dispatch, DM retry queue          |
| `bootstrap`    | DHT bootstrap peer management (`dht bootstrap add/remove/list/clear`) |
| `config`       | Help text, config reload, auto-share toggle, sync-days, descriptions  |
| `wasm`         | WASM offering CRUD, toggle, parameter management, query & run remote  |

Shared infrastructure (`UILogger`, `PeerState`, `SortedPeerNamesCache`, peer resolution helpers) lives in `handlers/mod.rs` and is `use super::*`-imported by all submodules.

### Key Data Structures
- **Story**: id, name, header, body, public flag, channel, created_at, auto_share override
- **PublishedStory**: Story + publisher peer ID
- **SearchQuery** / **SearchResult**: Full-text search with channel/date/visibility filters and FTS5 relevance scoring
- **DirectMessage**: from/to peer IDs and names, message text, timestamp, is_outgoing flag
- **Conversation**: Aggregated thread of DirectMessages with unread count and last_activity
- **RelayMessage** / **RelayConfirmation**: Encrypted, signed, hop-limited relay payloads
- **Channel** / **ChannelSubscription**: Channel metadata and per-peer subscription records
- **WasmOffering**: id, name, description, IPFS CID, parameters (`Vec<WasmParameter>`), resource requirements, version, enabled flag, timestamps
- **WasmParameter**: name, param_type (string/bytes/json/int/float/bool/file), description, required, default_value
- **WasmResourceRequirements**: fuel range, memory range, estimated timeout
- **WasmConfig** / **WasmCapabilityConfig**: Per-node WASM execution and advertising settings
- **UnifiedNetworkConfig**: Single config file combining BootstrapConfig, NetworkConfig, PingConfig, DirectMessageConfig, ChannelAutoSubscriptionConfig, MessageNotificationConfig, RelayConfig, AutoShareConfig, NetworkCircuitBreakerConfig, WasmConfig
- **EventType**: Unified enum covering all UI, floodsub, mDNS, ping, Kademlia, and six request-response event types

### Network Architecture
- libp2p floodsub for broadcast story/channel/peer-name messages
- mDNS for automatic local-network peer discovery
- Kademlia DHT for global peer discovery and bootstrap connectivity
- Ping protocol for connection monitoring and keep-alive
- **Six request-response protocols** (CBOR-encoded):
  1. `DirectMessage` — encrypted peer-to-peer messages
  2. `NodeDescription` — pull a peer's description text
  3. `StorySync` — targeted story synchronisation
  4. `Handshake` — version-gated peer verification (new peers are quarantined until the handshake succeeds)
  5. `WasmCapabilities` — advertise/discover WASM offerings
  6. `WasmExecution` — request remote execution of a peer's WASM module
- Floodsub relay topic for offline DM delivery through intermediate peers
- Persistent peer key (`peer_key`) using Ed25519 keypairs
- `unified_network_config.json` — primary runtime config; auto-created on first run; reload at runtime with `reload config` (note: `dht bootstrap add/remove/list/clear` currently persists peers in `bootstrap_config.json`)
- CLI `--data-dir <path>` sets `DATA_DIR` and routes all data files through `get_data_path()`

### Data Storage
- SQLite database (`stories.db`) via r2d2 connection pool
- FTS5 virtual table for full-text story search
- Migrations system (`migrations.rs`) for schema evolution; default `general` channel guaranteed
- `TEST_DATABASE_PATH` env var for test isolation; `clear_database_for_testing()` for clean-slate tests
- Local node descriptions are persisted in `node_description.txt`
- Peer aliases persisted in `peers` table; used for WASM remote-peer display fallback

### Story and Channel Management
- Stories organised by channels; `general` is the default and always auto-subscribed
- Public/private visibility; per-story `auto_share` override and global `config auto-share` setting
- `config sync-days <N>` limits story sync to the last N days
- Interactive TUI story creation (`create s`) and direct-command creation (`create s name|header|body[|channel]`)
- Search with `search <query> [channel:<c>] [author:<p>] [recent:<d>] [public|private]`
- Filter with `filter channel <name>` or `filter recent <days>`
- Export stories to `./exports/` as Markdown or JSON (`export s <id|all> <md|json>`)
- Delete one or more stories: `delete s <id1>[,<id2>,...]`
- Auto-subscription to new channels (configurable via `set auto-sub [on|off|status]`)

### WASM Capability Exchange
- Nodes advertise WASM modules as "offerings" stored locally in SQLite
- Offerings reference WASM binaries by IPFS CID; fetched from an IPFS gateway at execution time
- Remote peers can query offerings (`wasm query <peer>`) and execute them (`wasm run <peer> <id> [args...]`)
- Execution is sandboxed via Wasmtime with fuel limits, memory caps, timeouts, and WASI stdio
- LRU cache of compiled modules avoids re-fetching/re-compiling on repeated calls
- `wasm config` shows current WasmConfig (fuel limit, memory limit, timeout)

## User Interface
The application features a terminal-based user interface (TUI) with:
- **Multiple display panes**: Local peers, connected peers, local stories, received stories, direct messages/conversations, and log output
- **Input modes**: Normal command input, story creation wizard, quick reply (`r`), conversation view, and multi-line message composition (`compose <peer>`)
- **Real-time updates**: Live peer connections, story updates, unread message counts, and network status
- **Keyboard navigation**: Arrow keys for scrolling, Tab for pane switching, scroll-lock toggle
- **Tab auto-complete**: Peer name completion in message commands

## Application Commands

### Stories
- `ls s` / `ls s all` — list local stories / request all public stories from peers
- `create s name|header|body[|channel]` — create and auto-publish a story
- `publish s <id>` — manually publish/re-publish a story
- `show story <id>` — show full story details
- `delete s <id1>[,<id2>,...]` — delete one or more stories
- `search <query> [channel:<c>] [author:<p>] [recent:<d>] [public|private]` — full-text search
- `filter channel <name>` / `filter recent <days>` — filter displayed stories
- `export s <id|all> <md|json>` — export to `./exports/`

### Channels
- `ls ch [available|unsubscribed]` — list channels
- `ls sub` — list your subscriptions
- `create ch name|description` — create a channel
- `sub <channel>` / `unsub <channel>` — subscribe / unsubscribe
- `set auto-sub [on|off|status]` — manage automatic channel subscription

### Messaging
- `msg <peer_alias> <message>` — send direct message
- `compose <peer_alias>` — enter multi-line message composition mode
- `r` — quick reply in conversation view
- `name <alias>` — set your peer name
- `peer id` — show your full peer ID

### Node Descriptions
- `create desc <description>` — set your node description
- `show desc` — show your node description
- `get desc <peer_alias>` — request description from a peer

### Network & Bootstrap
- `connect <multiaddr>` — connect to a specific peer
- `dht bootstrap add/remove/list/clear/retry` — manage bootstrap peer list in `bootstrap_config.json`
- `dht bootstrap <multiaddr>` — bootstrap directly with a peer
- `dht peers` — find closest peers in DHT
- `reload config` — reload `unified_network_config.json` at runtime

### Configuration
- `config auto-share [on|off|status]` — toggle automatic story sharing on connect
- `config sync-days <N>` — set story sync window in days

### WASM
- `wasm create <name>|<desc>|<ipfs_cid>|<version>` — create WASM offering
- `wasm ls [local|remote|all]` — list offerings
- `wasm show <id>` — show offering details
- `wasm toggle <id>` — enable/disable offering
- `wasm delete <id>` — delete offering
- `wasm param <offering_id> add <name>|<type>|<desc>|<required>[|<default>]` — add parameter
- `wasm query <peer_alias>` — query peer's WASM capabilities
- `wasm run <peer_alias> <offering_id> [args...]` — execute remote WASM
- `wasm config` — show WASM execution configuration

### General
- `help` — show all available commands
- `quit` / `Ctrl+C` — exit application

## Logging
The application uses multiple logging mechanisms:

### TUI Logging
- All user-facing feedback is sent through `UILogger` to the TUI log pane
- Never use raw `println!`/`eprintln!` — it corrupts the terminal UI

### File Logging
- **Bootstrap Log** (`bootstrap.log`): Bootstrap attempts, DHT status, connectivity info
- **Error Log** (`errors.log`): Network errors and connection issues
- **Standard Log**: Debug/info via `env_logger` (controlled by `RUST_LOG`)

### Log Categories
- `BOOTSTRAP_INIT`: Bootstrap configuration and initialization
- `BOOTSTRAP_ATTEMPT`: Automatic bootstrap retry attempts
- `BOOTSTRAP_STATUS`: DHT connection status and peer counts
- `BOOTSTRAP_ERROR`: Bootstrap failures and errors
- `NETWORK_ERROR`: Network connection and protocol errors

## Testing Strategy
- **Test runner** (`./scripts/test_runner.sh`): Preferred; builds `test-wasm-add` for `wasm32-wasip1`, enables `test-utils` feature, runs suites with `TEST_DATABASE_PATH=./test_stories.db`, and serialises suites requiring isolation.
- **Unit tests** (`cargo test --lib --features test-utils`): Per-module tests in `src/`.
- **Integration tests** (`tests/`): ~45 test files covering storage, network, crypto, UI, WASM, conversations, circuit breakers, performance, and more.
- **Shared test helpers** (`tests/common/`): Fixtures, database setup, swarm helpers that mirror the production protocol stack including handshake and story-sync protocols.
- **Isolated databases**: Use `TEST_DATABASE_PATH` or `clear_database_for_testing()` for clean-state tests.
- **WASM fixture** (`test-wasm-add/`): A `wasm32-wasip1` crate built by the test runner for WASM executor tests.
- **Coverage**: `./scripts/test_coverage.sh` generates `tarpaulin-report.html` via tarpaulin.

## Dependencies
Key dependencies from Cargo.toml:
- **libp2p 0.56.0**: Core P2P networking with floodsub, mDNS, Kademlia, ping, relay, request-response, noise, yamux, TCP, DNS
- **tokio 1.43**: Async runtime
- **rusqlite 0.29** + **r2d2** + **r2d2_sqlite**: SQLite with connection pooling
- **ratatui 0.30**: Terminal UI framework
- **crossterm 0.27**: Cross-platform terminal manipulation
- **wasmtime 36.0** + **wasmtime-wasi 36.0**: WASM execution engine with WASI support
- **reqwest 0.12** (rustls-tls): HTTP client for IPFS gateway fetches
- **lru 0.7**: LRU cache for compiled WASM modules
- **serde / serde_json**: Serialization
- **chrono**: Timestamps with serde support
- **chacha20poly1305 / hkdf / sha2 / x25519-dalek / curve25519-dalek / zeroize**: Cryptographic primitives for DM encryption
- **uuid**: UUID v4 for WASM offering IDs
- **clap 4**: CLI argument parsing (`--data-dir`)
- **thiserror / anyhow**: Error handling
- **log / env_logger**: Logging infrastructure
- **once_cell**: Lazy statics for KEYS, PEER_ID, TOPIC