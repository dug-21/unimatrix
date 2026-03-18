# Agent Report: 308-agent-1-fix

## Task

Fix GH #308: MCP connection drops mid-session since nxs-011 (sqlx write pool contention).

Apply fire-and-forget async pattern at 5 call sites that still used the blocking `log_event()` bridge (`block_in_place`), missed by GH #302.

## Files Modified

- `/workspaces/unimatrix-bugfix-mcp/crates/unimatrix-server/src/server.rs`
- `/workspaces/unimatrix-bugfix-mcp/crates/unimatrix-server/src/background.rs`

## Changes Applied

### server.rs — 3 sites fixed

1. **Line ~457** (`insert_with_audit`): replaced `self.audit.log_event(audit_event_with_target).map_err(...)` with fire-and-forget spawn.
2. **Line ~510** (`correct_with_audit`): same pattern.
3. **Line ~809** (quarantine/restore/deprecate shared fn): same pattern, removed the `?` propagation since fire-and-forget does not return a Result.

### background.rs — 2 sites fixed

4. **`emit_tick_skipped_audit`**: was a sync fn calling `audit_log.log_event()`, converted body to `Arc::clone(audit_log)` + `tokio::spawn(async move { ... log_event_async().await ... })`. Warning preserved via tracing inside the spawn.
5. **`emit_auto_quarantine_audit`**: same conversion.

### Pattern used (matches GH #302 idiom from store_ops.rs / store_correct.rs)

```rust
{
    let audit = Arc::clone(&self.audit);
    tokio::spawn(async move {
        let _ = audit.log_event_async(event).await;
    });
}
```

For background.rs free functions (taking `&Arc<AuditLog>`):
```rust
let audit = Arc::clone(audit_log);
tokio::spawn(async move {
    if let Err(e) = audit.log_event_async(event).await {
        tracing::warn!(error = %e, "...");
    }
});
```

### Test fixes

4 existing tests that read the audit log immediately after calling the emit function broke because the write is now deferred. Fixed by adding `tokio::time::sleep(Duration::from_millis(50)).await` after the call to allow the spawned task time to acquire the DB connection and commit.

## New Tests Added

In `server.rs` tests module:

- **`test_insert_with_audit_does_not_block_under_concurrent_writes`**: fires 10 concurrent `insert_with_audit` calls under a 10s timeout, asserts all complete and all entries are stored.
- **`test_quarantine_restore_audit_does_not_block`**: calls `quarantine_with_audit` and `restore_with_audit` each under a 10s timeout, asserting no starvation.

## Test Results

Before changes: 1357 passed, 10 failed (pre-existing: import + mcp identity pool timeouts)
After changes: 1359 passed, 10 failed (same pre-existing failures, no new failures)

Net: +2 passing (new tests), 0 regressions.

## Issues / Blockers

None. The 10 pre-existing failures (GH #303) are unrelated to this fix:
- `import::tests::*` — 7 tests (pre-existing)
- `mcp::identity::tests::*` — 3 tests (pre-existing pool timeout under concurrency, GH #303)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found entry #2126 "Use block_in_place (not Handle::current().block_on) when bridging async into sync", which confirmed the root cause analysis. No fire-and-forget pattern entry found (the GH #302 pattern existed in code but wasn't in Unimatrix).
- Stored: nothing novel to store — the fire-and-forget pattern is already documented in code comments (GH #302 references) and the `log_event_async` docstring. The gotcha worth noting is that tests which verify audit events written by fire-and-forget spawns must sleep or poll, not rely on synchronous ordering. This is an extension of the GH #302 pattern rather than a new discovery.
