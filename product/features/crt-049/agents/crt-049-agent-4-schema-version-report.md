# Agent Report: crt-049-agent-4-schema-version

**Component**: Component 6 — SUMMARY_SCHEMA_VERSION bump
**File**: `crates/unimatrix-store/src/cycle_review_index.rs`
**Commit**: `813c4801`

## Changes Made

### 1. `crates/unimatrix-store/src/cycle_review_index.rs`

- Bumped `SUMMARY_SCHEMA_VERSION` from `2` to `3`
- Added doc comment entry: `crt-049: bumped 2 → 3; adding explicit_read_count, explicit_read_by_category, and redefining total_served (search exposures no longer contribute).`
- Renamed test `test_summary_schema_version_is_two` → `test_summary_schema_version_is_three`
- Updated test assertion from `2u32` to `3u32` with updated message citing crt-049 changes
- Updated comment `CRS-V24-U-01 (replaces CRS-U-02): SUMMARY_SCHEMA_VERSION is 3`

### 2. `crates/unimatrix-server/src/mcp/tools.rs`

- Updated advisory message in `check_stored_review` from generic version mismatch text to:
  `"Stored review has schema_version {stored} (current: {current}). schema_version 2 predates the explicit read signal and total_served redefinition (search exposures no longer contribute to total_served); use force=true to recompute."`
- Advisory message location confirmed in `tools.rs`, not `cycle_review_index.rs`

## Test Results

- `cargo test -p unimatrix-store`: **263 passed, 0 failed**
- `test_summary_schema_version_is_three`: PASS (CRS-V24-U-01)
- `unimatrix-store` builds cleanly: `cargo build -p unimatrix-store` — no errors

Note: `cargo build --workspace` shows 7 pre-existing errors in `unimatrix-server` from other
agents' in-progress work (renaming `delivery_count` to `search_exposure_count` in
`unimatrix-observe` without callers updated yet). These are not in this component's scope.

Existing TH-U-04 test in `tools.rs` (`test_check_stored_review_mismatched_version_produces_advisory`)
continues to pass — the new advisory message satisfies all three assertions it checks
(contains "use force=true to recompute", stored version number, current version number).

## Issues

None. All three sub-changes landed cleanly.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — skipped (used context_search per spawn prompt);
  found pattern #4153 (schema version bump requires three paths: constant + migration assertion +
  advisory message — aligns exactly with this task). ADR entries #4215, #4216, #4218 confirmed
  crt-049 design decisions.
- Stored: nothing novel to store — the three-path bump pattern is already captured in #4153.
  The advisory message location being in `tools.rs` rather than `cycle_review_index.rs` is
  already visible from reading the codebase and is not a runtime trap.
