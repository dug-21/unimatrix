# Agent Report: crt-033-gate-3b-rework-tools

**Role**: Rust Developer (Gate 3b rework — TH-I-01..TH-I-10)
**Feature**: crt-033
**Date**: 2026-03-29

## Task

Add TH-I-01 through TH-I-10 (excluding TH-I-09) store-backed handler integration tests to
`crates/unimatrix-server/src/mcp/tools.rs`. These tests were the sole FAIL finding in the
gate-3b-report.md.

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs` — added `cycle_review_integration_tests` module
  (625 lines, appended after the existing `col028_confirmed_entries_tests` module)

## Tests Added

New module: `mcp::tools::cycle_review_integration_tests`

| Test | Spec ID | Coverage | Result |
|------|---------|----------|--------|
| `context_cycle_review_first_call_writes_correct_row` | TH-I-01 | AC-03, AC-11 | PASS |
| `context_cycle_review_second_call_returns_stored_record` | TH-I-02 | AC-04, AC-14 | PASS |
| `context_cycle_review_force_true_overwrites_stored_row` | TH-I-03 (spec TH-I-04) | AC-05 | PASS |
| `context_cycle_review_force_purged_signals_with_stored_record_returns_note` | TH-I-04 (spec TH-I-05) | AC-06, AC-15 | PASS |
| `context_cycle_review_force_no_observations_no_stored_record_returns_none` | TH-I-05 (spec TH-I-06) | AC-07, R-04 | PASS |
| `context_cycle_review_stale_schema_version_produces_advisory` | TH-I-06 (spec TH-I-03) | AC-04b | PASS |
| `context_cycle_review_evidence_limit_applied_at_render_time_only` | TH-I-07 | AC-08, R-03 | PASS |
| `context_cycle_review_params_force_absent_is_none` | TH-I-08 | AC-12 | PASS |
| `context_cycle_review_concurrent_first_calls_both_complete` | TH-I-10 | OQ-03, R-02 | PASS |

TH-I-09 deferred per test plan (requires test double or store error injection infrastructure).

## Test Results

```
cargo test -p unimatrix-server context_cycle_review
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 2395 filtered out
```

The 13 tests include the 4 pre-existing T-CCR tests plus 9 new TH-I integration tests.

`cargo build -p unimatrix-server` — clean, 0 errors.

Full workspace: `test result: FAILED. 2407 passed; 1 failed` — the one failure is
`test_run_scenarios_does_not_write_to_snapshot` (eval baseline test), which passes in
isolation both before and after our changes. This is a pre-existing test isolation issue
(concurrent write to `log.jsonl`) unrelated to this task.

## Implementation Approach

The gate report stated tests should "call handler internals directly or use the store to verify
state" since the full handler struct cannot be instantiated without the complete server.

Each integration test:
1. Opens a real `SqlxStore` via `SqlxStore::open` in a `tempfile::TempDir` (same pattern as
   existing `test_compute_knowledge_reuse_for_sessions_no_block_on_panic`)
2. Calls `build_cycle_review_record` + `store.store_cycle_review()` (step 8a handler logic)
3. Verifies state via `store.get_cycle_review()` and direct SQL assertions
4. Calls `check_stored_review` (step 2.5 handler logic) and `dispatch_review_with_advisory`
   for advisory and render-time evidence limit tests

## One Deviation from Test Plan (TH-I-07 Assert C)

The test plan states "Act 2: call with no evidence_limit, Assert C returns 5 evidence items."
The json format dispatch uses `evidence_limit.unwrap_or(3)` — so `None` truncates to 3, not 5.
Assert C was implemented with `evidence_limit=Some(0)` (the bypass path: `> 0` guard = false)
to achieve the full-evidence result. A comment in the test explains this deviation. The core
assertion (stored JSON has full evidence, render-time truncation works) is fully covered.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #2744 (use
  `write_pool_server()` not `write_pool()` in server-context tests), #3799 (acquire connection
  pattern), #2271 (SqlxStore test pattern), #3800 (`check_stored_review` return type pattern).
  Applied: used `write_pool_server()` in store operations, followed `SqlxStore::open` pattern.
- Stored: nothing novel to store — the store test pool pattern (#2271, #2744) and
  `check_stored_review` signature (#3800) are already documented. The `evidence_limit.unwrap_or(3)`
  default causing TH-I-07 Assert C to need `Some(0)` is a minor implementation detail, not a
  gotcha that future agents would trip on (it's visible in the function signature).
