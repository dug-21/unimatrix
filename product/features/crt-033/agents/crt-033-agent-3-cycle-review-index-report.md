# Agent Report: crt-033-agent-3-cycle-review-index

## Task

Implement `crates/unimatrix-store/src/cycle_review_index.rs` — new store module for crt-033 memoization.

## Files Created / Modified

- **Created**: `crates/unimatrix-store/src/cycle_review_index.rs` (new module, 477 lines)
- **Modified**: `crates/unimatrix-store/src/lib.rs` (added `pub mod cycle_review_index;` and re-exports)

## What Was Implemented

### Structs and Constants

- `CycleReviewRecord` — `#[derive(Debug, Clone)]` struct with `feature_cycle: String`, `schema_version: u32`, `computed_at: i64`, `raw_signals_available: i32`, `summary_json: String`
- `SUMMARY_SCHEMA_VERSION: u32 = 1` — single definition, no duplicate anywhere
- `SUMMARY_JSON_MAX_BYTES: usize = 4 * 1024 * 1024` — private ceiling constant

### Methods on `SqlxStore`

- `get_cycle_review(&self, feature_cycle: &str) -> Result<Option<CycleReviewRecord>>` — uses `read_pool()`, `fetch_optional`
- `store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>` — uses `write_pool_server().acquire().await` directly (not spawn_blocking, not pool-direct execute); enforces 4MB ceiling pre-write; `INSERT OR REPLACE`
- `pending_cycle_reviews(&self, k_window_cutoff: i64) -> Result<Vec<String>>` — uses `read_pool()`, `SELECT DISTINCT` K-window set-difference against `cycle_events`

### lib.rs Wiring

- `pub mod cycle_review_index;` added to module declarations
- `pub use cycle_review_index::{CycleReviewRecord, SUMMARY_SCHEMA_VERSION};` added to re-exports

## Tests

14 tests in `#[cfg(test)]` block inside the module + 2 `sqlite_parity.rs` tests that picked up automatically from the migration agent's work = **16 total, 16 passed, 0 failed**.

### Test Coverage

| Test | Spec Ref |
|------|----------|
| `test_summary_schema_version_is_one` | CRS-U-02 |
| `test_store_cycle_review_4mb_ceiling_exceeded` | CRS-U-03 |
| `test_store_cycle_review_4mb_ceiling_boundary` | CRS-U-04 |
| `test_get_cycle_review_missing_returns_none` | CRS-I-01 |
| `test_store_and_get_cycle_review_round_trip` | CRS-I-02 |
| `test_raw_signals_available_zero_round_trip` | edge case (i32 binding AC-16) |
| `test_store_cycle_review_overwrites_prior` | CRS-I-03 |
| `test_pending_cycle_reviews_returns_unreviewed_cycles` | CRS-I-04 |
| `test_pending_cycle_reviews_empty_when_all_reviewed` | CRS-I-05 |
| `test_pending_cycle_reviews_excludes_outside_k_window` | CRS-I-06 |
| `test_pending_cycle_reviews_excludes_cycle_end_only` | CRS-I-07 |
| `test_pending_cycle_reviews_boundary_is_inclusive` | CRS-I-08 |
| `test_pending_cycle_reviews_distinct_on_cycle_id` | CRS-I-09 |
| `test_concurrent_store_same_cycle_last_writer_wins` | CRS-I-10 |

### Tests Not Covered in This Module

- CRS-U-01 (CycleReviewRecord serde round-trip) and CRS-U-05/06 (RetrospectiveReport serde): these require `serde` derives on `CycleReviewRecord`, which the pseudocode did not specify. The struct is `Debug + Clone` only; the handler layer serializes `RetrospectiveReport` directly via `serde_json` and stores the JSON string. The round-trip through the store is covered by CRS-I-02.
- CRS-G-01/G-02 (static grep checks): CI-layer checks; no Rust test required.

## Constraint Compliance

| Constraint | Status |
|-----------|--------|
| `store_cycle_review` uses `write_pool_server()` acquire-then-execute (not spawn_blocking, not analytics drain) | PASS |
| `get_cycle_review` and `pending_cycle_reviews` use `read_pool()` | PASS |
| `SUMMARY_SCHEMA_VERSION` defined only in `cycle_review_index.rs` | PASS |
| 4MB ceiling returns `Err(StoreError::InvalidInput)`, not panic | PASS |
| `raw_signals_available` is `i32` (not `bool`) | PASS |
| No `.unwrap()` in non-test code | PASS |
| No `TODO`, `FIXME`, `unimplemented!()` | PASS |
| File under 500 lines | PASS (477 lines) |

## Commit

`4659ac2` — `impl(cycle_review_index): add CycleReviewRecord store module for crt-033 memoization (#451)`

## Issues Encountered

None blocking. The `col018_topic_signal_null_for_generic_prompt` test failure seen in full workspace runs is a pre-existing timing-sensitive flake — confirmed by isolated rerun.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001 (#3793), ADR-004 (#3796), ADR-002 (#3794), and dual-pool patterns (#2147, #2153). Applied: write_pool_server acquire-first pattern, read_pool for all read methods.
- Stored: entry #3799 "Acquire write connection before execute — don't pass write_pool_server() directly to .execute()" via `/uni-store-pattern`. This is the non-obvious distinction between `write_pool_server().acquire().await` + execute-on-conn vs passing the pool directly to `.execute()`, which behaves differently under contention with a 2-connection write pool.
