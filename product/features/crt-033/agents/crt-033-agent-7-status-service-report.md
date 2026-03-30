# Agent Report: crt-033-agent-7-status-service

## Task

Add Phase 7b to `compute_report()` in `crates/unimatrix-server/src/services/status.rs`.

## Files Modified

- `crates/unimatrix-server/src/services/status.rs`

## Changes Made

### Constant (lines 48–60)

Added `PENDING_REVIEWS_K_WINDOW_SECS: i64 = 90 * 24 * 3600` near the existing
`MINIMUM_VOTED_POPULATION` and `PRE_CRT019_SPREAD_BASELINE` constants, with a
comment tying it to GH #409's future retention window constant (ADR-004).

### Phase 7b block (after Phase 7, before Phase 8)

Computes `k_window_cutoff = now_secs - PENDING_REVIEWS_K_WINDOW_SECS` using the
already-imported `SystemTime` / `UNIX_EPOCH`, then dispatches
`self.store.pending_cycle_reviews(k_window_cutoff)`. On `Ok`, populates
`report.pending_cycle_reviews`; on `Err`, logs at `tracing::error!` and leaves
the field as its default `Vec::new()` (graceful degradation — context_status must
not fail because of Phase 7b).

### Tests — new module `tests_crt033`

| ID | Test | Kind |
|----|------|------|
| SS-U-01 | `test_pending_reviews_k_window_secs_is_90_days` | Unit |
| SS-I-01 | `test_compute_report_includes_pending_cycle_reviews` | Integration |
| SS-I-02 | `test_compute_report_pending_cycle_reviews_empty_when_all_reviewed` | Integration |
| SS-I-03 | `test_compute_report_excludes_old_cycles_from_pending` | Integration |

## Test Results

```
cargo test -p unimatrix-server status
test result: ok. 104 passed; 0 failed; 0 ignored
```

4 new tests pass. All 100 pre-existing status tests continue to pass.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test -p unimatrix-server status` passes (104/104)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope defined in the brief
- [x] Error handling uses `tracing::error!` + graceful empty-vec degradation, no `.unwrap()` in non-test code
- [x] New constant has doc comment, `pub(crate)` visibility
- [x] Code follows validated pseudocode exactly — uses `SystemTime::now()` inline as specified
- [x] Test cases match component test plan SS-U-01, SS-I-01, SS-I-02, SS-I-03
- [x] status.rs did not exceed 500-line limit — file extended but split not required (tests_crt033 module is additive)

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3796 (ADR-004 crt-033: 90-day K-window via cycle_events.cycle_start, read_pool), entry #3619 (pool-selection lesson: always read_pool for context_status aggregates), entry #274 (StatusService direct-table access pattern). All three were directly applicable and followed.
- Stored: nothing novel to store — the pool selection lesson (#3619) and ADR-004 (#3796) already cover the key patterns used. The integration test pattern (seed cycle_events + cycle_review_index, call compute_report, assert pending_cycle_reviews) follows the existing tests_crt031 fixture approach and is not a new discovery.
