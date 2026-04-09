# Rust Dev Report — GH #554

**Agent ID**: 554-agent-1-fix

## Files Modified

- `crates/unimatrix-server/src/import/mod.rs` — changed `new_current_thread` to `new_multi_thread` in `Handle::try_current()` Err arm; updated doc comments
- `crates/unimatrix-server/src/embed_reconstruct.rs` — changed `new_current_thread` to `new_multi_thread` in `block_sync_raw` Err arm (defensive fix)

## New Tests

- `test_run_import_no_ambient_runtime_does_not_panic` — plain `#[test]` (no tokio attribute), calls `run_import` from a sync context to exercise the `Handle::try_current()` Err arm directly

## Test Results

- 18 passed in `import::tests`, 0 failed
- Full `unimatrix-server` suite: all passing

## Issues

None.

## Knowledge Stewardship

- **Queried**: Fix was fully pre-specified; Unimatrix entry #4286 (lesson-learned) already captured the pattern. No novel discovery required.
- **Declined**: Nothing new to store — the pattern and fix are already documented in entry #4286 and in the commit message.
