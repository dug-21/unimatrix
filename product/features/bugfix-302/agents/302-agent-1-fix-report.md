# Rust-Dev Report: 302-agent-1-fix

## Files Modified

- `crates/unimatrix-server/src/infra/audit.rs` — added `log_event_async()` async method
- `crates/unimatrix-server/src/services/store_ops.rs` — converted synchronous `audit.log_event()` to `tokio::spawn` fire-and-forget using `log_event_async()`
- `crates/unimatrix-server/src/services/store_correct.rs` — same conversion

## New Tests

- `infra::audit::tests::test_log_event_async_concurrent_does_not_starve` — fires 20 concurrent `log_event_async` calls, asserts all complete within 10s with monotonic IDs
- `infra::audit::tests::test_log_event_async_does_not_block_in_place` — runs a yield-counting background task concurrently with `log_event_async`, asserts background task reaches full yield count (would fail if `block_in_place` stole the thread)

## Tests

- `unimatrix-server`: 1357 passed, 10 failed (pre-existing, GH#303)
- `unimatrix-store`: 98 passed, 0 failed
- New regression tests: 2 passed
- Clippy: 0 new warnings/errors in fix files

## Issues

- `server.rs:456` legacy `insert_with_audit()` / `correct_with_audit()` paths: confirmed dead code (compiler warns), no audit call present — no changes needed.
- Attempted to store lesson "AuditLog::log_event() must not be called from async context — use log_event_async() + fire-and-forget" via `context_store` but received `-32603` (the live server has the unfixed code). Store after deployment.

## Knowledge Stewardship

- Queried: Unimatrix entries #2125 ("Analytics drain unsuitable for writes that callers read back immediately") and #731 ("Batched Fire-and-Forget DB Writes") — confirmed established pattern. Entry #2059 (ADR-002 nxs-011) — confirmed 5 call sites for direct write_pool pattern.
- Declined: attempted `context_store` for "AuditLog::log_event_async() pattern for fire-and-forget audit from async context" — server returned `-32603` (live server has unfixed code). **Store after deployment.**
