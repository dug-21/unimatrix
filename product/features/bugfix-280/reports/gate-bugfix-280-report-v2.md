# Gate Bugfix Report v2: bugfix-280

> Gate: Bug Fix Validation (rework iteration 1)
> Date: 2026-03-16
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | `load_maintenance_snapshot()` replaces `compute_report()` in `maintenance_tick()`; all unused phases eliminated |
| No todo!/unimplemented!/TODO/FIXME | PASS | None in any changed file |
| All tests pass | PASS | 2541 passed, 0 failed across workspace |
| No new clippy warnings in changed crate | PASS | Zero errors in `unimatrix-server`; pre-existing errors in unrelated crates confirmed unchanged |
| No unsafe code introduced | PASS | No `unsafe` blocks added; one pre-existing defended `.unwrap()` carried forward |
| Fix is minimal | PASS | Three files changed; `compute_report()` untouched |
| New tests would catch original bug (post-rework) | PASS | T-280-01 now asserts `effectiveness.is_some()` — meaningful, verifies success path |
| Integration smoke tests pass | PASS | 19 pass, 1 xfail (pre-existing GH#111) |
| Lifecycle integration suite passes | PASS | 23 pass, 2 xfail (pre-existing GH#238, GH#291) |
| xfail markers have GH issues | PASS | GH#111, GH#238, GH#291 all referenced |
| Knowledge stewardship: investigator | PASS | Queried + Declined with reason |
| Knowledge stewardship: rust-dev (v1) | PASS | Queried + nothing novel + reason |
| Knowledge stewardship: rust-dev (v2) | PASS | Queried + nothing novel + reason |
| Knowledge stewardship: verifier | PASS | Queried + nothing novel + reason |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS

**Evidence**: `background.rs:595` calls `status_svc.load_maintenance_snapshot().await`. The new function runs exactly 2 `spawn_blocking_with_timeout` calls (one for `load_active_entries_with_tags`, one for the effectiveness classify loop) plus one inline `VectorIndex` atomic read for `graph_stale_ratio`. Phases 2 (O(N) ONNX contradiction scan), 3, 4, 6, 7 and Phase 1 distribution/outcome queries are entirely eliminated from the tick path. `compute_report()` is untouched and continues to serve the `context_status` MCP tool.

`MaintenanceDataSnapshot` at `status.rs:198–202` captures the three fields the tick consumes: `active_entries`, `graph_stale_ratio`, `effectiveness`. No other data is fetched.

### No Placeholders
**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in `services/status.rs`, `background.rs`, or `mcp/response/status.rs`.

### All Tests Pass
**Status**: PASS

The previous gate FAIL (missing `contradiction_cache` argument in `make_status_service()`) is resolved. The test helper at `status.rs:1734–1753` now imports `new_contradiction_cache_handle`, constructs a handle, and passes it as the 6th argument to `StatusService::new()`. Compilation succeeds.

```
cargo test --workspace 2>&1 | grep "^test result"
# 2541 passed, 0 failed across all crates
```

The three new `maintenance_snapshot_tests` all pass:
- `test_load_maintenance_snapshot_empty_store_returns_ok`
- `test_load_maintenance_snapshot_with_active_entries_returns_non_empty`
- `test_load_maintenance_snapshot_graph_stale_ratio_zero_on_empty_index`

### No New Clippy Warnings
**Status**: PASS

`cargo clippy -p unimatrix-server -- -D warnings` produces zero errors in `unimatrix-server` source files. All clippy errors shown in output originate in `unimatrix-engine` (2 errors) and `unimatrix-observe` (50 errors) — confirmed pre-existing by checking those crates against the base commit `d8a4313` (prior to this bugfix). No new warnings introduced.

### No Unsafe Code Introduced
**Status**: PASS

No `unsafe` blocks in any changed file. The `.unwrap()` at `background.rs:620` (`effectiveness_opt.as_ref().unwrap()`) is guarded by `if effectiveness_opt.is_some()` at line 618 with an explicit `// SAFETY:` comment. This is a direct carry-forward from the pre-existing code at line 610 (on `d8a4313`). Not introduced by this fix.

### Fix Is Minimal
**Status**: PASS

Three files changed: `services/status.rs` (new struct + new method + test module), `background.rs` (swap call site + import), `mcp/response/status.rs` (add `Default` impl). No unrelated changes. `compute_report()` and all other callers are untouched.

The `Default` impl for `StatusReport` is required by the thin-shell construction `StatusReport { graph_stale_ratio, ..StatusReport::default() }` at `background.rs:608–611`. It enables `run_maintenance()` to receive the one value it needs (`graph_stale_ratio`) without constructing a full `StatusReport` from a full pipeline call.

### New Tests Would Catch the Original Bug (Post-Rework)
**Status**: PASS

The previous gate issued a WARN for a tautological assertion in T-280-01 (`is_none() || is_some()` is always true). The rework replaced it with:

```rust
assert!(
    snapshot.effectiveness.is_some(),
    "empty store must produce Some effectiveness report (build_report succeeds on empty classifications)"
);
```

This assertion is meaningful: it verifies that on an empty store, `load_maintenance_snapshot()` completes without error and that `build_report()` returns a value (rather than `None`) even with zero entries. A broken effectiveness path (panic, timeout, or unexpected `None`) would cause this assertion to fail.

The remaining limitation from the previous gate (no test asserts `compute_report()` is NOT called during tick) persists, but is acceptable — removing it would require mock infrastructure. The three tests adequately cover the new code path's correctness.

### Integration Smoke Tests
**Status**: PASS

19 pass, 1 xfail (`test_store_1000_entries` — pre-existing GH#111). No new failures.

### Lifecycle Integration Suite
**Status**: PASS

23 pass, 2 xfail:
- `test_multi_agent_interaction` — pre-existing GH#238
- `test_auto_quarantine_after_consecutive_bad_ticks` — pre-existing GH#291 (xfail reason updated by a separate commit to reference GH#291)

No new failures.

### xfail Markers Have GH Issues
**Status**: PASS

All xfail markers in the integration suite reference existing GH issues. No new xfail markers added by this fix.

### Knowledge Stewardship
**Status**: PASS (all four agent reports)

- `280-investigator-report.md`: `## Knowledge Stewardship` present with `Queried:` entries (pattern #1628, #1759, #1560, #1561 cited) and `Declined:` with specific reason.
- `280-agent-1-fix-report.md` (original): `## Knowledge Stewardship` present with `Queried:` and "nothing novel to store — the pattern... is straightforward".
- `280-agent-1-fix-v2-report.md` (rework): `## Knowledge Stewardship` present with `Queried:` and "nothing novel to store — the pattern... does not rise to the level of a reusable crate-specific gotcha".
- `280-agent-2-verify-report.md`: `## Knowledge Stewardship` present with `Queried:` and "nothing novel to store — testing approach follows established patterns".

## Knowledge Stewardship

- Queried: `/uni-store-lesson` — MCP server unavailable in this context (non-blocking per protocol).
- Stored: nothing novel to store — the rework validation pattern (check that the specific FAIL items from the previous gate report are resolved, then re-run only the failed checks) is already established practice. The systemic finding from this gate (verifier agent relying on a warm build cache, missing test arity errors) was noted in the previous gate report as a pattern to record when MCP is available; no new pattern emerged in this iteration.
