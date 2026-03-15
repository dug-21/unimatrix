# Agent Report: 266-agent-1-fix

**Bug**: GH #266 — MCP server fails after ~15 minutes idle (tick instability)
**Branch**: `worktree-bugfix/266-tick-instability`
**Commit**: `24465cf`

## Files Modified

- `crates/unimatrix-store/src/read.rs`
- `crates/unimatrix-server/src/services/supersession.rs`
- `crates/unimatrix-server/src/background.rs`

## New Functions

- `Store::query_all_entries() -> Result<Vec<EntryRecord>, StoreError>` in `read.rs`

## Changes Made

### Fix 1 — `Store::query_all_entries` (read.rs)

Added after `query_by_status`. Single `lock_conn()`, single `SELECT {ENTRY_COLUMNS} FROM entries` (no WHERE clause), single `load_tags_for_entries()` batch. Follows `load_active_entries_with_tags` structural model exactly.

### Fix 2 — `SupersessionState::rebuild` (supersession.rs)

Replaced 4x `query_by_status` loop (4 mutex acquisitions, 8 SQL statements) with one `store.query_all_entries()` call. Removed now-unused `Status` import.

### Fix 3 — Background tick timeout (background.rs)

Wrapped the supersession `spawn_blocking` in `tokio::time::timeout(TICK_TIMEOUT, ...)` matching all other tick operations. The three-level match handles: `Ok(Ok(Ok(new_state)))`, `Ok(Ok(Err(e)))`, `Ok(Err(e))` (panic), `Err(_)` (timeout — retains existing cache, does not update guard).

## Build / Test / Clippy

- `cargo build --workspace`: pass
- `cargo test --workspace`: 2335 passed, 0 failed
- `cargo clippy -p unimatrix-store -- -D warnings`: pass (clean)
- `cargo clippy -p unimatrix-server -- -D warnings`: zero errors in `unimatrix-server` source; pre-existing errors in `unimatrix-observe`/`unimatrix-engine` dependency crates are out of scope

## Issues / Blockers

None. Fix is minimal and contained to the three specified files.

## Knowledge Stewardship

- Queried: /uni-query-patterns skipped — server intermittently unstable per brief instructions
- Stored: nothing novel to store — fix follows established patterns (`load_active_entries_with_tags` structural model, `tokio::time::timeout(TICK_TIMEOUT, ...)` pattern already documented in background.rs comments). The three-level nested Result match for `timeout + spawn_blocking` is a known Tokio pattern, not a new discovery.
