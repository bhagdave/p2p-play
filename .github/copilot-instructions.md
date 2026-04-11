# P2P-Play Copilot Instructions

P2P-Play is a Rust/libp2p terminal application for peer-to-peer story sharing, direct messaging, channel subscriptions, and optional WASM capability exchange.

## Build, test, and lint commands

Use these commands as the default workflow:

```bash
cargo build
cargo run
cargo fmt --check
cargo fmt
cargo clippy
```

For the full test matrix, prefer the repo script:

```bash
./scripts/test_runner.sh
```

That script builds the `test-wasm-add` fixture for `wasm32-wasip1`, enables the `test-utils` feature, and runs the suites with `TEST_DATABASE_PATH=./test_stories.db`.

Useful targeted test commands:

```bash
# library/unit-style coverage
cargo test --lib --features test-utils

# run one integration test file
cargo test --test conversation_tests --features test-utils -- --nocapture

# run one named test inside a file
cargo test --test conversation_tests --features test-utils test_message_ordering -- --nocapture
```

When testing runtime file placement, use the CLI data-dir override instead of writing into the repo root:

```bash
cargo run -- --data-dir /tmp/p2p-play-node1
```

## High-level architecture

`src/main.rs` is the composition root. It resolves `--data-dir` before the Tokio runtime starts, initializes the UI, ensures the SQLite database and unified config exist, creates the libp2p swarm, sets up loggers/channels, and then hands control to `EventProcessor`.

`src/event_processor.rs` is the real application loop. It multiplexes UI events, swarm events, timers, and internal channels, then routes work into `event_handlers.rs` and `handlers.rs`. If a change affects interactive behavior, networking, retries, unread counts, or periodic maintenance, it usually touches this loop plus one of those handler modules.

`src/network.rs` defines the composite `StoryBehaviour`. The app is not just floodsub: it combines floodsub, mDNS, Kademlia, ping, and several request-response protocols for direct messages, node descriptions, story sync, handshake verification, WASM capability discovery, and remote WASM execution.

Peer compatibility is handshake-gated. New peers are not treated as full application peers until the handshake succeeds. `event_handlers.rs` adds only verified peers to the floodsub partial view and UI, and ignores floodsub messages from unverified peers.

Storage is centered in `src/storage/core.rs` with migrations in `src/migrations.rs`. SQLite is the source of truth for stories, channels, subscriptions, conversations, and read state. Migrations also guarantee the default `general` channel exists.

The TUI in `src/ui.rs` is stateful and mode-driven. Story creation, quick reply, conversation view, and message composition are separate input modes, so user-facing command changes often need coordinated updates across `ui.rs`, `handlers.rs`, and `event_processor.rs`.

Logging is intentionally split: UI-facing feedback goes through `UILogger`, while network/bootstrap problems are written to `errors.log` and `bootstrap.log` via the file loggers. Avoid adding raw stdout/stderr logging that would corrupt the terminal UI.

## Key conventions

Always route repo data files through the data-dir helpers. `--data-dir` sets `DATA_DIR` early, and `get_data_path()` is the convention for locating `stories.db`, logs, config, and `peer_key`. New persisted files should follow the same pattern.

Treat `unified_network_config.json` as the primary runtime config. Startup ensures it exists, and `reload config` reloads the file, but settings baked into swarm construction still require a restart. When changing config behavior, check both startup loading in `main.rs` and runtime reload handling in the command/event flow.

`general` is a special default channel across the codebase. Migrations insert it, startup auto-subscribes the local peer if needed, story creation defaults to it, and incoming stories are accepted for `general` even when subscription lookup fails. Preserve that behavior unless the feature explicitly changes channel semantics.

Validate and sanitize user-controlled text with `ContentValidator` and `ContentSanitizer` from `src/validation.rs`. Handler code already follows this pattern for stories, channels, names, and messages; new commands should reuse those helpers instead of open-coding checks.

Reuse the domain error types in `src/errors.rs` and the existing file loggers. This codebase prefers explicit storage/network/UI/config error boundaries over generic `Box<dyn Error>` plumbing in application code.

Tests that touch SQLite should isolate their database path with `TEST_DATABASE_PATH`. If a test needs a clean database, prefer `p2p_play::storage::clear_database_for_testing()`; it resets state in foreign-key-safe order and reinserts the default `general` channel.

For integration tests that need swarms, reuse the helpers in `tests/common/mod.rs`. They mirror the production protocol stack, including handshake and story-sync request-response protocols, so tests stay aligned with the real network behavior.
