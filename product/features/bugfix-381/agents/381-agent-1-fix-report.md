# Agent Report: 381-agent-1-fix

## Summary

Implemented the approved fix for GH #381: missing UDS observation log points and silently-ignored RUST_LOG.

## Files Modified

- `crates/unimatrix-server/src/main.rs` — 3 sites
- `crates/unimatrix-server/src/uds/listener.rs` — 4 log points + signature change

## Changes Made

### Step 1: EnvFilter — 3 sites in main.rs

Replaced the static `with_env_filter("info")` / `with_env_filter("debug")` pattern at all three `tokio_main_*` entry points (`tokio_main_daemon`, `tokio_main_stdio`, `tokio_main_bridge`) with:

```rust
let default_level = if cli.verbose { "debug" } else { "info" };
let filter = tracing_subscriber::EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
```

Added the RUST_LOG comment at each site per the approved spec.

### Step 2: Four log points in listener.rs

**LP-3** (top of SubagentStart block, before `maybe_goal` lookup):
```rust
tracing::debug!(target: "unimatrix_server::obs", session_id, query_preview, "UDS: SubagentStart received");
```

**LP-4** (existing `debug!` at goal-present branch): added `target: "unimatrix_server::obs"` — same level, same message, now participates in the obs filter group.

**`handle_context_search` signature**: added `source: Option<String>` parameter. One call site updated.

**LP-1** (after `filtered` is built, before injection tracking):
```rust
tracing::debug!(target: "unimatrix_server::obs", session_id, source, result_count, query_preview, "UDS: ContextSearch executed");
```

**LP-2** (inside `!filtered.is_empty()` guard, after `record_injection`):
```rust
tracing::debug!(target: "unimatrix_server::obs", session_id, entry_count, entries, "UDS: injecting entries");
```

## New Tests

None — per the approved fix spec. Existing tests cover the modified code paths and all pass.

## Test Results

`cargo test --workspace`: all suites passed, zero failures.

Counts per suite (representative):
- unimatrix-server lib: 2047 passed, 0 failed
- unimatrix-server integration: 16+16 passed, 0 failed
- All other crates: 0 failures

## Build / Lint

- `cargo build -p unimatrix-server`: clean (pre-existing warnings only, none from modified files)
- `cargo clippy -p unimatrix-server -- -D warnings`: no errors in modified files (`listener.rs`, `main.rs`)

## Commit

Branch: `bugfix/381-uds-obs-logging`
Commit: `d4ef9cb fix(obs): add UDS observation debug logs + respect RUST_LOG (#381)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `tracing RUST_LOG EnvFilter uds logging observability` — found entry #3453 (investigator-stored lesson confirming the EnvFilter fix approach); no other relevant patterns
- Stored: entry #3457 "UDS obs log group: use target: 'unimatrix_server::obs' on debug! for operator-togglable visibility" via `/uni-store-pattern` — covers the `target:` grouping pattern for per-subsystem filter control, which is distinct from the EnvFilter lesson and useful for future log point additions
