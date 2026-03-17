# WASM Sharing and Execution Test Plan

## Purpose

This plan defines how to validate end-to-end sharing, discovery, and remote execution of WASM modules in P2P-Play.

## Goals

- Verify nodes can advertise WASM capabilities and discover peer offerings.
- Verify remote execution requests are accepted, executed safely, and return deterministic results.
- Verify system behavior under invalid input, resource pressure, and failure conditions.
- Verify configuration flags for WASM sharing/execution are respected.

## Scope

### In Scope

- WASM offering data model and serialization.
- WASM capabilities request/response protocol.
- WASM execution request/response protocol.
- Runtime execution behavior (success + failure paths).
- Resource and safety controls (fuel, timeout, memory limits).
- Integration paths between peers for capability discovery and execution.

### Out of Scope (for this plan)

- Front-end UX polish for WASM controls.
- External IPFS reliability/performance benchmarking at internet scale.
- Multi-region network latency benchmarking.

## Test Environments

- **Local CI-like environment**: Linux container, single machine, Rust toolchain.
- **Multi-process local network**: 2-3 peers on localhost.
- **Stress environment (optional)**: constrained CPU/memory cgroup to validate limits.

## Entry Criteria

- Project builds successfully.
- Default migrations run successfully.
- Test WASM payloads are available (minimal valid module, invalid module, fuel-heavy module).

## Exit Criteria

- All critical and high-priority scenarios pass.
- No open critical defects in sharing or execution flows.
- Non-critical failures are documented with mitigation or follow-up issues.

---

## Test Tracks

## 1) Baseline Automated Coverage (Current Suite)

Run existing tests first to establish a healthy baseline:

```bash
cargo test --test wasm_capability_tests
cargo test --test wasm_executor_tests
```

Expected outcomes:
- Protocol payloads serialize/deserialize correctly.
- Basic execution pipeline handles valid and invalid WASM content.
- Resource controls (fuel/timeouts where implemented) are enforced.

## 2) Configuration and Validation Tests

### Scenarios

1. WASM advertising disabled on node A (`advertise_capabilities=false`): node B should not discover offerings from A.
2. Remote execution disabled on node A (`config.wasm.capability.allow_remote_execution=false`, or `wasm config execute off` via CLI): execution requests should be rejected with explicit error response.
3. Invalid config values (fuel/memory/timeout limits) fail validation at startup or config-check boundaries.
4. Max concurrent execution limit is enforced when multiple requests are sent simultaneously.

### Suggested Implementation

- Add targeted integration tests near existing network integration tests.
- Validate both wire response and server-side log/error state.

## 3) Capability Sharing / Discovery Integration

### Scenario Matrix

- **Single offering**: A advertises one module, B discovers exact metadata.
- **Multiple offerings**: A advertises several modules; B receives complete list.
- **Parameter inclusion toggle (TODO / future behavior)**: when implemented, requests with and without parameters should yield responses that respect the `include_parameters` flag. Currently, the capabilities responder ignores this flag and always returns full offerings (including parameters), so tests SHOULD NOT expect different response shapes based on the toggle yet.
- **Stale offering refresh**: updated offering metadata (version/description) propagates after refresh.
- **Disabled offering visibility**: disabled offerings are not returned to remote peers.

### Checks

- In capabilities responses, validate `WasmOffering.id` and associated metadata (`ipfs_cid`, version, resource constraints); in execution requests, validate `execution_request.offering_id` and ensure it matches the previously discovered `WasmOffering.id` and metadata across hops.
- Timestamps are present and monotonic where expected.

## 4) Remote Execution Integration

### Happy Path

1. Node B discovers offering from A.
2. Node B submits execution request with valid input.
3. Node A fetches WASM bytes by CID.
4. Node A executes module successfully.
5. Node B receives success response with expected output and execution metadata.

### Negative Paths

- Invalid WASM bytes / wrong magic header.
- Missing CID (fetch failure / content not found).
- Input schema mismatch for required parameters.
- Fuel exhaustion.
- Timeout exceeded.
- Memory limit exceeded (or graceful fail if hard limit not yet enforced).
- Unauthorized or malformed request payload.

### Assertions

- Failures return actionable and non-sensitive errors.
- Runtime does not panic; node remains operational for subsequent requests.
- Execution IDs / correlation data (if present) are unique and traceable.

## 5) Resilience and Recovery

### Scenarios

- Request timeout between peers (network delay/drop).
- Executor process experiences one failure, then successfully handles subsequent requests.
- Peer disconnect during in-flight execution request.
- Reconnection and retry behavior for idempotent execution requests.

### Success Criteria

- Service degrades gracefully (clear error, no crash).
- Recovery path succeeds without manual data repair.

## 6) Security and Abuse Resistance

### Scenarios

- Oversized input payloads.
- Repeated rapid execution requests (rate/concurrency pressure).
- Malicious WASM sample (invalid sections/opcodes, trap behavior).
- Attempted resource over-allocation beyond configured maximums.

### Success Criteria

- Requests are rejected or bounded per policy.
- No uncontrolled memory/CPU growth.
- No host file/network escape through execution environment.

## 7) Observability and Diagnostics

### Verify

- Logs contain: peer ID, offering ID, CID, status, duration, and error class.
- Error messages are user-actionable.
- Metrics (if available) increment for success/failure/timeout/resource-limit events.

---

## Manual End-to-End Test Script (2 Peers)

1. Start **Peer A** with WASM advertising + execution enabled.
2. Register one known test offering on Peer A.
3. Start **Peer B** with discovery enabled.
4. From Peer B, request A's capabilities.
5. Verify offering metadata matches source.
6. Submit valid execution request and verify output.
7. Submit invalid execution request (bad CID) and verify controlled failure.
8. Submit constrained request (very low fuel/timeout) and verify limit handling.
9. Confirm both peers remain healthy and continue exchanging non-WASM traffic.

## Regression Checklist (Per Release)

- [ ] WASM capability tests pass.
- [ ] WASM executor tests pass.
- [ ] No Serde schema drift in request/response types (JSON/CBOR).
- [ ] At least one successful remote execution in integration environment.
- [ ] At least one failure scenario each for fetch, validation, timeout, and fuel.
- [ ] Logs/telemetry verified for one success and one failure path.

## Recommended Future Additions

- Dedicated end-to-end integration test file for multi-peer WASM request/response lifecycle.
- Property-based tests for execution request payload boundaries.
- Chaos-style tests for intermittent fetch/network failures.
- Performance profile tests for concurrent WASM execution throughput.
