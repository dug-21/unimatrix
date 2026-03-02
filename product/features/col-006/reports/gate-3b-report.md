# Gate 3b Report: Code Review Validation

## Feature: col-006 Hook Transport Layer ("Cortical Implant")
## Gate: 3b (Code Review)
## Result: PASS

## Summary

All 7 components implemented across 4 waves. Code matches pseudocode and architecture. Zero test regressions. Zero compiler warnings on project code.

## Wave Results

### Wave 1: Foundation (engine-extraction + wire-protocol)
- Created `crates/unimatrix-engine/` with 7 modules
- Extracted `project.rs`, `confidence.rs`, `coaccess.rs` from server to engine
- Added `socket_path` field to `ProjectPaths`
- Server re-exports via `pub use unimatrix_engine::{confidence, coaccess, project}`
- Wire protocol types: HookRequest (8 variants), HookResponse (5 variants), HookInput, ImplantEvent, EntryPayload, TransportError
- Framing: write_frame/read_frame with 4-byte BE u32 length prefix + JSON payload
- Serde limitation: `RecordEvent`/`RecordEvents` changed from newtype to struct variants (serde `#[serde(tag = "type")]` cannot serialize newtype variants containing sequences)

### Wave 2: Transport Layer (transport + authentication + event-queue)
- `Transport` trait with 5 methods: connect, request, fire_and_forget, disconnect, is_connected
- `LocalTransport` over `std::os::unix::net::UnixStream` (no tokio)
- Socket existence fast-fail, auto-connect, fire_and_forget disconnects after write
- Peer credentials via `nix` crate (SO_PEERCRED on Linux) -- `std::os::unix::net::UCred` / `peer_cred()` remains unstable
- 3-layer auth: filesystem permissions + UID verification + process lineage (advisory)
- EventQueue: JSONL files with `pending-{timestamp}.jsonl` naming, 1000 events/file, 10 file max, 7-day pruning, best-effort replay

### Wave 3: Server Integration (uds-listener + hook-subcommand)
- `SocketGuard` RAII for socket cleanup (analogous to `PidGuard`)
- `handle_stale_socket()`: unconditional unlink after PidGuard (ADR-004)
- `start_uds_listener()`: tokio::net::UnixListener, 0o600 permissions, per-connection spawn (R-19)
- Connection handler: std -> tokio conversion for auth, async read/write for framing
- `dispatch_request()`: Ping -> Pong, SessionRegister/SessionClose/RecordEvent/RecordEvents -> Ack (log-only, ADR-007)
- `hook` subcommand: `fn main()` with early branch (no `#[tokio::main]`), sync path per ADR-002
- Hook builds `HookRequest` from event name + stdin JSON, with ppid fallback for session_id
- Graceful degradation: server unavailable -> queue fire-and-forget events, skip sync queries
- `cortical-implant` agent bootstrapped in `registry.rs` (TrustLevel::Internal, Read + Search)
- Shutdown: UDS abort -> socket guard drop -> vector dump -> compaction

## ADR Compliance

| ADR | Status | Notes |
|-----|--------|-------|
| ADR-001 (Engine extraction boundary) | Compliant | Re-exports preserve backward compatibility |
| ADR-002 (Hook sync runtime) | Compliant | `fn main()` branches before tokio init |
| ADR-003 (Layered auth) | Compliant | UID via nix SO_PEERCRED, lineage advisory |
| ADR-004 (Socket lifecycle) | Compliant | Unconditional unlink after PidGuard |
| ADR-005 (Wire protocol) | Compliant | Length-prefixed JSON, serde-tagged enums |
| ADR-006 (Defensive parsing) | Compliant | serde(default), serde(flatten), Option everywhere |
| ADR-007 (No schema v4) | Compliant | All handlers log-and-ack only |

## Test Results

| Crate | Tests | Status |
|-------|-------|--------|
| unimatrix-adapt | 64 | PASS |
| unimatrix-core | 21 | PASS |
| unimatrix-embed | 76 | PASS (18 ignored) |
| unimatrix-engine | 154 | PASS (NEW CRATE) |
| unimatrix-observe | 236 | PASS |
| unimatrix-server | 524 | PASS (+22 new) |
| unimatrix-store | 187 | PASS |
| unimatrix-vector | 104 | PASS |
| **Total** | **1366** | **0 failures** |

Baseline was 1199 tests. Net new: 167 tests (154 in engine + 22 in server - 9 moved from server to engine).

## New Dependencies

| Crate | Version | Purpose | Used By |
|-------|---------|---------|---------|
| nix | 0.31.2 | Safe Unix API (SO_PEERCRED, getuid) | unimatrix-engine, unimatrix-server |

## Files Created/Modified

### New Crate: crates/unimatrix-engine/
- `Cargo.toml`
- `src/lib.rs`
- `src/project.rs` (moved from server, +socket_path)
- `src/confidence.rs` (moved from server verbatim)
- `src/coaccess.rs` (moved from server verbatim)
- `src/wire.rs` (new)
- `src/transport.rs` (new)
- `src/auth.rs` (new)
- `src/event_queue.rs` (new)

### Modified in crates/unimatrix-server/
- `Cargo.toml` (+unimatrix-engine, +nix deps)
- `src/lib.rs` (re-exports + new module declarations)
- `src/main.rs` (subcommand + early branch + UDS startup)
- `src/shutdown.rs` (+socket_guard, +uds_handle, +shutdown steps)
- `src/registry.rs` (+cortical-implant bootstrap)

### New in crates/unimatrix-server/
- `src/uds_listener.rs`
- `src/hook.rs`

### Deleted from crates/unimatrix-server/
- `src/project.rs`
- `src/confidence.rs`
- `src/coaccess.rs`

## Risk Coverage

| Risk | Status |
|------|--------|
| R-01 (Engine extraction breaks MCP tools) | Mitigated: zero regression, re-exports preserve API |
| R-03 (Socket lifecycle ordering) | Mitigated: PidGuard before bind, SocketGuard RAII |
| R-07 (Wire protocol framing) | Mitigated: length validation, EOF detection, per-connection isolation |
| R-14 (Connection leak) | Mitigated: connection-per-request, handler task spawn |
| R-18 (Tokio in hook process) | Mitigated: early branch, no tokio imports in hook.rs |
| R-19 (UDS listener crashes server) | Mitigated: tokio::spawn per connection, accept loop continues on error |
| R-20 (Bootstrap idempotency) | Mitigated: if-not-exists check for cortical-implant |

## Deviations from Pseudocode

1. **RecordEvent/RecordEvents**: Changed from newtype variants to struct variants due to serde `#[serde(tag = "type")]` limitation with sequences. JSON format uses `{event: {...}}` and `{events: [...]}` instead of directly flattening.

2. **Auth implementation**: Used `nix` crate instead of `std::os::unix::net::UCred` (unstable in Rust 1.93). This adds an external dependency but maintains `#![forbid(unsafe_code)]`.

3. **macOS auth**: The `getpeereid` path uses `nix::unistd::getpeereid` with raw fd, not a direct nix socket option. Functionally equivalent.

## Gate Verdict: PASS
