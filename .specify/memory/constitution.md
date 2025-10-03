<!--
Sync Impact Report - Constitution v1.0.0
===========================================
Version Change: Initial constitution (no previous version) → v1.0.0
Rationale: MINOR bump - Initial constitution establishment with comprehensive principles

Added Sections:
- I. Test-Driven Development (NON-NEGOTIABLE)
- II. Code Quality & Maintainability
- III. User Experience Consistency
- IV. Performance Standards
- V. Security Requirements
- Development Workflow section
- Quality Gates section

Modified Principles: N/A (initial version)
Removed Sections: N/A (initial version)

Templates Status:
✅ plan-template.md - Constitution Check section aligns with new principles
✅ spec-template.md - Requirements sections support principle validation
✅ tasks-template.md - Task categories reflect TDD, quality, and security principles

Follow-up TODOs:
- RATIFICATION_DATE set to TODO - awaiting project team decision on official adoption date
-->

# P2P-Play Constitution

## Core Principles

### I. Test-Driven Development (NON-NEGOTIABLE)

**Tests MUST be written before implementation.** The TDD cycle is strictly enforced:

1. Write failing tests that specify desired behavior
2. Obtain user/stakeholder approval of test scenarios
3. Verify tests fail (red state)
4. Implement minimal code to pass tests (green state)
5. Refactor while keeping tests passing

**Rationale**: TDD ensures code correctness, prevents regressions, and creates living
documentation. The non-negotiable nature reflects the project's commitment to
reliability in distributed systems where debugging is complex.

**Requirements**:
- All features MUST have integration tests covering user scenarios
- All public APIs MUST have contract tests validating interfaces
- Tests MUST use the test runner script (`./scripts/test_runner.sh`)
- Unit test coverage MUST be maintained above 70% for core modules
- Tests MUST be isolated (use `TEST_DATABASE_PATH` for database tests)

### II. Code Quality & Maintainability

**Code MUST be clean, idiomatic, and maintainable.** Quality is enforced through
tooling and review standards to ensure the codebase remains approachable and
extensible as the project grows.

**Requirements**:
- All code MUST pass `cargo fmt --check` before commit
- All code MUST pass `cargo clippy` with zero warnings
- Function complexity MUST be justified if exceeding 15 cyclomatic complexity
- Public APIs MUST have comprehensive documentation comments
- Error handling MUST use Result types with descriptive error messages
- Dependencies MUST be reviewed quarterly using `cargo machete`
- Module cohesion MUST be maintained (single responsibility principle)

**Rationale**: P2P-Play is an experimental learning project testing AI-assisted
development. Maintaining high code quality standards ensures the codebase serves as
an effective teaching tool and enables productive AI collaboration.

### III. User Experience Consistency

**Terminal UI interactions MUST be consistent, intuitive, and reliable.** Users
should experience predictable behavior across all features and receive clear
feedback for all actions.

**Requirements**:
- All user commands MUST provide immediate visual feedback in the TUI
- Error messages MUST be actionable and displayed in the TUI log pane
- Command syntax MUST follow established patterns (`ls <type>`, `create <type>`)
- Loading states MUST be indicated for operations exceeding 500ms
- Success/failure states MUST be clearly distinguished (colors, symbols)
- Multi-step interactions MUST guide users through each step
- Keyboard navigation MUST be consistent across all UI panes

**Rationale**: As a terminal-based application, the TUI is the primary interface.
Consistency reduces cognitive load and makes the application accessible to users
at all skill levels.

### IV. Performance Standards

**The application MUST maintain responsive network operations and efficient resource
usage.** Performance targets reflect the distributed nature of P2P networking and
the constraints of running multiple peers on developer machines.

**Requirements**:
- UI responsiveness MUST be maintained (<100ms for input processing)
- Database operations MUST complete within 200ms for single-record queries
- Network connection establishment MUST timeout at 30 seconds
- Bootstrap DHT discovery MUST retry with exponential backoff (5s intervals)
- Direct message delivery MUST retry up to 3 times with 30s intervals
- Memory usage MUST remain under 100MB for typical operation (5-10 peers)
- SQLite connection pooling MUST be used for concurrent access
- Async operations MUST use tokio efficiently (avoid blocking the runtime)

**Rationale**: P2P applications are inherently latency-sensitive and resource-
constrained. Setting explicit targets ensures the application remains practical
for development and testing with multiple local peers.

### V. Security Requirements

**Security MUST be built-in, not bolted-on.** While P2P-Play is an experimental
project, it handles user-generated content and peer communications, requiring
defensive security practices.

**Requirements**:
- All peer communications MUST use libp2p's Noise protocol encryption
- Peer identities MUST use Ed25519 keypairs stored securely in `peer_key`
- User input MUST be validated and sanitized before database storage
- SQL queries MUST use parameterized statements (no string concatenation)
- File operations MUST validate paths to prevent directory traversal
- Error messages MUST NOT leak sensitive information (paths, keys, internal state)
- Dependencies MUST be updated monthly for security patches
- Network timeouts MUST be enforced to prevent resource exhaustion
- Configuration files MUST NOT contain credentials or private keys

**Rationale**: Even experimental projects should model good security practices.
P2P networks are particularly vulnerable to malicious peers, making defensive
coding essential.

## Development Workflow

**All development MUST follow the established workflow** to maintain project
consistency and enable effective AI collaboration:

1. **Planning Phase**: Features begin with `/specify` workflow
   - Feature spec defines WHAT and WHY (not HOW)
   - Implementation plan details technical approach
   - Design documents precede code implementation

2. **Implementation Phase**: Code follows TDD principles
   - Tests written first and verified to fail
   - Implementation makes tests pass
   - Refactoring improves design without breaking tests

3. **Review Phase**: Quality gates must pass before merge
   - All tests pass via `./scripts/test_runner.sh`
   - Code formatting passes `cargo fmt --check`
   - Linting passes `cargo clippy` with zero warnings
   - Coverage maintains or improves baseline

4. **Documentation Phase**: Changes update relevant docs
   - `CLAUDE.md` updated for significant architectural changes
   - `README.md` updated for user-facing feature changes
   - API documentation updated for public interface changes

## Quality Gates

**The following gates MUST pass before considering a feature complete**:

### Automated Gates
- ✅ All tests pass (`./scripts/test_runner.sh`)
- ✅ Code formatting valid (`cargo fmt --check`)
- ✅ No clippy warnings (`cargo clippy`)
- ✅ Build succeeds (`cargo build`)
- ✅ No unused dependencies (`cargo machete`)
- ✅ A completed entry in the changelog for each feature

### Manual Gates
- ✅ User scenarios from spec.md manually verified in running application
- ✅ Performance targets from plan.md validated under load
- ✅ Security requirements reviewed (input validation, error handling)
- ✅ UI consistency verified across affected features
- ✅ Error paths tested (network failures, invalid input, timeouts)

### Documentation Gates
- ✅ Public APIs documented with examples
- ✅ Complex algorithms explained with comments
- ✅ Configuration changes documented in README.md
- ✅ Breaking changes noted in CLAUDE.md

## Governance

**This constitution supersedes all other development practices.** Amendments require
explicit documentation, approval, and a migration plan for affected code.

**Enforcement**:
- All code reviews MUST verify constitutional compliance
- Violations MUST be documented in plan.md Complexity Tracking with justification
- Complexity MUST be justified against simpler alternatives
- Deviations without justification MUST be rejected

**Amendment Process**:
1. Propose amendment with rationale in issue or discussion
2. Document impact on existing code and practices
3. Update constitution with version increment (semantic versioning)
4. Update all dependent templates and documentation
5. Communicate changes to all contributors

**Guidance**: Use `CLAUDE.md` for detailed technical guidance during development.
The constitution defines non-negotiable principles; CLAUDE.md provides practical
implementation details.

**Version**: 1.0.0 | **Ratified**: TODO(RATIFICATION_DATE): Awaiting official adoption | **Last Amended**: 2025-10-03
