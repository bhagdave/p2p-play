# Repository Guidelines

## Project Structure & Modules
- Core source lives in `src/`; entrypoint at `src/main.rs`, shared logic in `src/lib.rs`.
- Networking (`src/network.rs`, `src/relay.rs`, `src/network_circuit_breakers.rs`) and protocol handlers (`src/event_handlers.rs`, `src/handlers.rs`) sit alongside storage (`src/storage/`) and crypto (`src/crypto.rs`).
- Terminal UI flows are in `src/ui.rs`; data types in `src/types.rs`; validation helpers in `src/validation.rs`.
- Integration and scenario tests are under `tests/`; shared test helpers in `tests/common/`.
- Dev scripts live in `scripts/` (test orchestration and coverage). Configuration defaults land in `unified_network_config.json` (auto-created on first run).

## Build, Test, and Development Commands
- `cargo build` — compile the workspace.
- `cargo run` — launch the TUI node (use `RUST_LOG=debug cargo run` for verbose logs).
- `./scripts/test_runner.sh` — runs the curated test matrix with `TEST_DATABASE_PATH=./test_stories.db` and serializes suites that need isolation.
- `cargo test` — quick local check; target a suite with `cargo test --test conversation_tests -- --nocapture`.
- `./scripts/test_coverage.sh` — generate tarpaulin HTML coverage (output `tarpaulin-report.html`).

## Coding Style & Naming Conventions
- Rustfmt defaults (4-space indent, max width 100) are canonical; avoid manual alignment that fights `cargo fmt`.
- Modules and functions follow `snake_case`; types and traits `PascalCase`; constants `SCREAMING_SNAKE_CASE`.
- Prefer small, focused async functions; reuse helpers in `event_handlers`/`handlers` instead of duplicating logic.
- Use `log` macros (`info!`, `warn!`, `error!`) with contextual fields; wire new errors through the existing error types in `src/errors.rs`.

## Testing Guidelines
- Favor integration tests under `tests/` to exercise network flows, and keep data setup in `tests/common`.
- Many suites rely on SQLite fixtures; set `TEST_DATABASE_PATH` or use the test runner to isolate state.
- Name tests after behavior (e.g., `handles_stale_peers_gracefully`) and keep them deterministic (test runner pins thread counts where needed).
- For coverage or long suites, run `./scripts/test_runner.sh` before pushing to catch ordering issues.

## Commit & Pull Request Guidelines
- Commit messages are short and descriptive (imperative or concise statements); include issue/PR numbers when relevant (e.g., `Add circuit breaker metrics (#123)`).
- Update `CHANGELOG.md` for user-facing changes and keep commits scoped and atomic.
- PRs should: describe intent and behavior changes, link related issues, note config impacts, and include screenshots or logs for UI/network traces when helpful.
- Ensure checks match the template: build, tests, `cargo fmt --check`, `cargo clippy`, and docs/config updates as needed.

## Configuration & Safety
- `unified_network_config.json` controls bootstrap peers, limits, and ping settings; reload at runtime via the `reload config` command, but restart if core network parameters change.
- Do not commit real peer keys or private datasets; sample DBs in the repo are for tests only.
- When adding new config, document defaults and safe ranges in `README.md` or inline comments, and prefer JSON keys that align with existing config naming.
