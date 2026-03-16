# Gate Bugfix Report: bugfix-280

> Gate: Bugfix Validation
> Date: 2026-03-16
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | `compute_report()` replaced with `load_maintenance_snapshot()` in `maintenance_tick()`; all 3 consumed values extracted directly |
| No todo!/unimplemented!/TODO/FIXME | PASS | None in any changed file |
| All tests pass | FAIL | Test binary fails to compile — `make_status_service()` missing `contradiction_cache` argument (#6 of 6) |
| No new clippy warnings in changed crate | PASS | Zero errors in `unimatrix-server` production code; pre-existing errors in `unimatrix-engine`/`unimatrix-observe` are unrelated |
| No unsafe code introduced | PASS | No `unsafe` blocks added |
| Fix is minimal | PASS | Only `status.rs`, `background.rs`, `mcp/response/status.rs` changed; `compute_report()` untouched |
| New tests would catch original bug | WARN | Tests verify `load_maintenance_snapshot()` works but do not assert `compute_report()` is not called during tick; tautological assertion in T-280-01 effectiveness check |
| Integration smoke tests pass | PASS | 19 pass, 1 xfail (pre-existing GH#111) |
| Lifecycle integration suite passes | PASS | 23 pass, 2 xfail (pre-existing GH#238, GH#291) |
| xfail markers have GH issues | PASS | All xfail markers reference existing GH issues |
| Knowledge stewardship: investigator | PASS | `## Knowledge Stewardship` block present with `Queried:` and `Declined:` entries |
| Knowledge stewardship: rust-dev | PASS | `## Knowledge Stewardship` block present with `Queried:` and "nothing novel to store" + reason |
| Knowledge stewardship: verifier | PASS | `## Knowledge Stewardship` block present with `Queried:` and "nothing novel to store" + reason |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS

**Evidence**: `background.rs` line 595 now calls `status_svc.load_maintenance_snapshot().await` instead of `status_svc.compute_report(None, None, false).await`. The new `load_maintenance_snapshot()` runs exactly 2 `spawn_blocking_with_timeout` calls (one for `load_active_entries_with_tags`, one for the effectiveness classify loop) plus one inline computation for `graph_stale_ratio` from in-memory `VectorIndex` atomics. Phases 2 (O(N) ONNX contradiction scan), 3, 4, 6, 7 and Phase 1 distribution queries are eliminated entirely. The `compute_report()` function is untouched and continues serving the `context_status` MCP tool path.

`MaintenanceDataSnapshot` struct is well-defined at `status.rs:198–202` with the three fields the tick consumes: `active_entries`, `graph_stale_ratio`, `effectiveness`.

### No Placeholders
**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` found in any of the three changed files.

### All Tests Pass
**Status**: FAIL

**Issue**: The new unit tests in `maintenance_snapshot_tests` fail to compile. The `make_status_service()` helper function at `status.rs:1733–1750` calls `StatusService::new()` with 5 arguments but the constructor requires 6. The missing argument is `contradiction_cache: ContradictionScanCacheHandle` (which is `Arc<RwLock<Option<ContradictionScanResult>>>`).

```
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
   --> crates/unimatrix-server/src/services/status.rs:1743:9
```

The verifier agent reported "2536 unit tests pass, 0 fail" but this is incorrect — `cargo test --workspace` fails to compile `unimatrix-server` (lib test) due to this error. The production build (`cargo build -p unimatrix-server`) succeeds, confirming this is a test-only compilation failure.

**Fix**: Add `contradiction_cache` to `make_status_service()`:

```rust
use crate::services::contradiction_cache::new_contradiction_cache_handle;

fn make_status_service(store: &Arc<Store>) -> StatusService {
    // ... existing setup ...
    let contradiction_cache = new_contradiction_cache_handle();
    StatusService::new(
        Arc::clone(store),
        vector_index,
        embed_service,
        adapt_service,
        confidence_state,
        contradiction_cache,
    )
}
```

### No New Clippy Warnings
**Status**: PASS

`cargo clippy -p unimatrix-server -- -D warnings` reports zero errors in the three changed files (`services/status.rs`, `background.rs`, `mcp/response/status.rs`). Clippy errors in `unimatrix-engine` and `unimatrix-observe` are pre-existing on `main` and are not caused by this fix.

### No Unsafe Code
**Status**: PASS

No `unsafe` blocks introduced. The `unwrap()` at `background.rs:620` (`effectiveness_opt.as_ref().unwrap()`) is guarded by `if effectiveness_opt.is_some()` immediately above it and carries a `// SAFETY:` comment. This pattern was present in the original code at `background.rs:529` (pre-fix on `main`); it was preserved, not introduced. The gate rule applies to blind unwraps — this is a defended unwrap following the same pattern as the original.

### Fix Is Minimal
**Status**: PASS

Only three files changed. `compute_report()` and all other callers are untouched. The `Default impl for StatusReport` added in `mcp/response/status.rs` is required by the thin-shell construction `StatusReport { graph_stale_ratio, ..Default::default() }` in `background.rs:608–611`. No unrelated changes detected.

### New Tests Would Catch Original Bug
**Status**: WARN

The three new tests exercise `load_maintenance_snapshot()` directly and verify correct output (empty store returns Ok with empty entries, active entries are returned, graph stale ratio is 0.0 on empty index). These tests would fail if `load_maintenance_snapshot()` were removed or broken. However:

1. **Tautological assertion in T-280-01**: The effectiveness assertion at lines 1771–1774 (`snapshot.effectiveness.is_none() || snapshot.effectiveness.is_some()`) is always true regardless of the value. This assertion provides no coverage.

2. **Root cause regression not directly caught**: If a future change reverted `maintenance_tick()` to call `compute_report()` instead of `load_maintenance_snapshot()`, these tests would continue to pass. There is no test that asserts the tick does NOT invoke the 8-phase pipeline.

These are acceptable limitations for a structural-refactor fix. The lifecycle integration suite (`test_lifecycle.py`) provides end-to-end coverage of tick behavior.

### Integration Smoke Tests
**Status**: PASS

19 pass, 1 xfail (`test_store_1000_entries` — pre-existing GH#111). No new failures.

### Lifecycle Integration Suite
**Status**: PASS

23 pass, 2 xfail (both pre-existing). `test_multi_agent_interaction` — GH#238. `test_auto_quarantine_after_consecutive_bad_ticks` — GH#291. No new failures.

### xfail Markers
**Status**: PASS

No new xfail markers added by this fix. All existing xfail markers reference GH issues.

### Knowledge Stewardship
**Status**: PASS (all three agents)

- `280-investigator-report.md`: Present with `Queried:` entries (patterns #1628, #1759, #1560, #1561) and `Declined:` with reason.
- `280-agent-1-fix-report.md`: Present with `Queried:` entry and "nothing novel to store -- the pattern... is straightforward".
- `280-agent-2-verify-report.md`: Present with `Queried:` entry and "nothing novel to store -- testing approach follows established patterns".

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `make_status_service()` missing `contradiction_cache` argument — test binary fails to compile | uni-rust-dev | Add `let contradiction_cache = new_contradiction_cache_handle();` and pass it as the 6th argument to `StatusService::new()` in `make_status_service()` at `status.rs:1733` |
| Tautological effectiveness assertion in T-280-01 | uni-rust-dev | Replace `assert!(snapshot.effectiveness.is_none() \|\| snapshot.effectiveness.is_some(), ...)` with a meaningful assertion (e.g., assert that an empty store produces `None` for effectiveness) |

## Knowledge Stewardship

- Queried: `/uni-store-lesson` — MCP server unavailable in this context; proceeded without results (non-blocking per protocol).
- Stored: nothing novel to store via MCP — lesson to record: "Verifier agent must run `cargo test --workspace` from a clean build, not rely on a warm binary cache, to catch test helper arity mismatches after constructor signature changes." This pattern appeared in bugfix-279 and bugfix-280. Pending storage when MCP is available.
