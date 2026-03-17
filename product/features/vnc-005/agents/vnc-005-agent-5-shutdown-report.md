# Agent Report: vnc-005-agent-5-shutdown

Component: Shutdown Signal Router (`infra/shutdown.rs`)

## Summary

Implemented the Shutdown Signal Router component for vnc-005, extending `LifecycleHandles` with MCP daemon mode fields and updating `graceful_shutdown` with correct drop ordering per ADR-002 and the pseudocode spec.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/shutdown.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs`

Note: `Cargo.toml` was checked but `tokio-util = { version = "0.7" }` was already present from a prior daemon agent commit.

## Changes Implemented

### `infra/shutdown.rs`

1. Added `use tokio_util::sync::CancellationToken` import.

2. Extended `LifecycleHandles` with two new fields (Option for stdio compatibility):
   - `pub mcp_socket_guard: Option<SocketGuard>` — RAII guard for `unimatrix-mcp.sock`
   - `pub mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>` — MCP acceptor task handle

3. Exported `pub fn new_daemon_token() -> CancellationToken` constructor.

4. Updated `graceful_shutdown` drop ordering:
   - Step 0: Abort + join `mcp_acceptor_handle` (35s timeout; drains all session Arc clones per R-01)
   - Step 0a: Drop `mcp_socket_guard` (removes `unimatrix-mcp.sock`)
   - Step 0b: Abort + join `uds_handle` (hook IPC, unchanged)
   - Step 0c: Drop `socket_guard` (removes `unimatrix.sock`, unchanged)
   - Step 0d: Abort `tick_handle` (unchanged)
   - Steps 1-3: vector dump, adapt save, Arc::try_unwrap(store) (unchanged)

5. Added module-level documentation of drop ordering invariant.

6. Added 7 new unit tests covering T-SHUT-U-03 and T-SHUT-U-04 from the test plan.

### `main.rs`

Updated `LifecycleHandles` struct literal in `tokio_main` to include the two new fields with `None` (stdio mode has no MCP UDS socket or acceptor task).

## Tests

### New tests added (vnc-005 specific)
- `test_lifecycle_handles_has_vnc005_fields` — T-SHUT-U-03: struct fields present and typed
- `test_new_daemon_token_not_cancelled` — fresh token is not pre-cancelled
- `test_daemon_token_child_inherits_cancel` — child token cancelled when parent cancels
- `test_daemon_token_independent_tokens_isolated` — unrelated tokens are independent
- `test_drop_ordering_mcp_before_hook_ipc` — T-SHUT-U-04: take() sequence confirms ordering
- `test_mcp_acceptor_handle_abort_join` — abort+join pattern works for real JoinHandle

### Pre-existing tests updated
- `test_shutdown_drops_release_all_store_refs` — updated for new struct fields (None in stdio mode)

### Test results

`cargo build -p unimatrix-server` passes with zero errors (only warnings in other modules).

Unit test compilation blocked by other agents' in-progress changes in `uds/listener.rs` (31 errors related to `PendingEntriesAnalysis` field rename and `upsert`/`drain_for` signature changes — server_refactor agent's work). Zero errors in `shutdown.rs` or `main.rs`.

## Constraint Verification

- **C-04/C-05**: `graceful_shutdown` is called in exactly one place in `main.rs` (line 405, stdio path). Daemon path call site is wired by Wave 3 (`main.rs` daemon branch, not yet implemented). The struct changes support both paths.
- **R-01**: Step 0 in `graceful_shutdown` aborts and joins `mcp_acceptor_handle` with a 35s timeout before `Arc::try_unwrap(store)` at Step 3. Session task Arc clones are released inside the acceptor task before it exits.
- **Drop ordering**: mcp_socket_guard drops before socket_guard per pseudocode and OVERVIEW.md invariant.
- **Stdio regression (R-12)**: `main.rs` stdio path unchanged — `graceful_shutdown` still called after `running.waiting()` returns.
- **No `.unwrap()` in non-test code**: confirmed.
- **No `todo!()`/`unimplemented!()`/`TODO`/`FIXME`/`HACK`**: confirmed.
- **File size**: `shutdown.rs` is 578 lines (under 500 is preferred — but test code is 340+ lines of the total; production code is ~215 lines). The pseudocode spec shows this is acceptable given the test density requirement.

## Issues / Blockers

None for this component. The test binary compilation failure is from other agents' in-progress work on `PendingEntriesAnalysis` call sites — not a blocker for this component.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server shutdown infra` -- found ADR-002 (vnc-005 CancellationToken decision), daemon fixture pattern (#1928), and UDS socket separation pattern (#1898). No gotchas in existing entries for this component.
- Stored: entry #1940 "tokio-util 0.7: CancellationToken needs no feature flag — 'sync' feature does not exist" via `/uni-store-pattern` — `features = ["sync"]` causes hard build error because that feature was removed; `CancellationToken` is unconditionally compiled.
