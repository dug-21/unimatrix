# Agent Report: crt-033-agent-6-tools-handler

## Task

Implement the `context_cycle_review` handler modifications in `crates/unimatrix-server/src/mcp/tools.rs` per the pseudocode and test plan for crt-033.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/validation.rs`

## Changes Summary

### RetrospectiveParams (5th field)

Added `pub force: Option<bool>` as the fifth field. Absent JSON field deserializes to `None` (equivalent to `false`).

### Step 2.5 — Memoization check (after three-path load, before step 4)

- `force.unwrap_or(false)` controls branching.
- `force=false`: calls `store.get_cycle_review()`:
  - `Ok(Some(record))`: calls `check_stored_review()`; on `Ok((report, advisory))` dispatches via `dispatch_review_with_advisory()` and returns; on `Err` (corrupt JSON) falls through to full pipeline (ADR-003).
  - `Ok(None)`: proceeds to full pipeline.
  - `Err`: logs `tracing::warn`, treats as cache miss, proceeds to full pipeline (ADR-003).
- `force=true + attributed.is_empty()`: calls `get_cycle_review()` as sole discriminator (OQ-01):
  - `Ok(Some(record))`: returns stored record with purged-signals note.
  - `Ok(None)` or `Err`: returns `ERROR_NO_OBSERVATION_DATA`.
- `force=true + attributed non-empty`: skips step 2.5 entirely, falls through to full pipeline.

### Step 8a — Memoization store (after full pipeline, before audit)

- Calls `build_cycle_review_record(&feature_cycle, &report)`.
- On `Ok(record)`: calls `store.store_cycle_review(&record).await`; on failure logs `tracing::warn` and continues (no error return to caller — avoids breaking retrospective over a cache write failure, consistent with GH #409 gate note in comments).
- On serialization `Err`: logs `tracing::warn`, continues.
- No `spawn_blocking` — direct `.await` in the handler's async context (ADR-001).
- Evidence-limit truncation is NOT applied before `serde_json::to_string` (C-03).

### Helper functions (free functions outside `impl UnimatrixServer`)

- `check_stored_review(record, current_version) -> Result<(RetrospectiveReport, Option<String>), serde_json::Error>`: deserializes `summary_json`; builds advisory when `schema_version` differs.
- `build_cycle_review_record(feature_cycle, report) -> Result<CycleReviewRecord, serde_json::Error>`: serializes report; sets `SUMMARY_SCHEMA_VERSION`, `raw_signals_available=1`, `computed_at` from `SystemTime::now()`.
- `dispatch_review_with_advisory(report, format, evidence_limit, advisory) -> Result<CallToolResult, ErrorData>`: mirrors step 12 format dispatch; appends advisory as additional text content item; applies `evidence_limit` truncation for JSON format only.

### validation.rs

Added `force: None` to three existing `RetrospectiveParams` struct literal instantiations in `validate_retrospective_params` tests so they compile after the new field was added.

## Tests: 8 passed / 0 failed

New unit tests added (all in `mcp::tools::tests`):

| Test | AC | Result |
|------|----|--------|
| `test_retrospective_params_force_absent_is_none` | TH-U-01, AC-12 | ok |
| `test_retrospective_params_force_true` | TH-U-02a, AC-12 | ok |
| `test_retrospective_params_force_false` | TH-U-02b, AC-12 | ok |
| `test_check_stored_review_matching_version_no_advisory` | TH-U-03, R-08 | ok |
| `test_check_stored_review_mismatched_version_produces_advisory` | TH-U-04, AC-04b | ok |
| `test_check_stored_review_future_version_produces_advisory` | TH-U-05, R-08 | ok |
| `test_check_stored_review_corrupted_json_returns_err` | TH-U-06, R-06-3 | ok |
| `test_build_cycle_review_record_sets_correct_fields` | TH-U-07, AC-03 | ok |

Full suite: 2399 lib tests + 46 + 16 + 16 + 7 = 2484 total — all pass, zero new failures.

## Design Deviations from Pseudocode

The pseudocode defines `handle_memoization_hit` and `handle_purged_signals_hit` as separate named functions returning `Result<CallToolResult, ServerError>` and introduces a `ServerError::MemoizationDeserError` variant. The implementation inlines the `handle_*` call sites into the step 2.5 match arms and uses the `dispatch_review_with_advisory` helper instead, avoiding a new `ServerError` variant. The externally observable behavior is identical. The pseudocode itself acknowledges this as one of two valid implementation approaches ("Alternative: return Result<Option<CallToolResult>, ServerError>").

The step 8a pseudocode says "return Err on store failure". The implementation instead logs a warning and continues (does not error-return). This deviates from the pseudocode but matches the spawn prompt instruction ("log tracing::warn, continue — don't fail the handler") and is the safer behavior given that GH #409 gate handles missing rows gracefully.

## Issues / Blockers

None. All constraints met:
- No `spawn_blocking` around `store_cycle_review` or `get_cycle_review` (TH-G-01).
- `evidence_limit` not applied before `serde_json::to_string` in step 8a (TH-G-02 / C-03).
- `SUMMARY_SCHEMA_VERSION` imported from `unimatrix_store` — not re-defined in tools.rs (C-04).
- `raw_signals_available` is `i32` in `CycleReviewRecord` (confirmed from store module).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR-001 (#3793), ADR-002 (#3794), ADR-003 (#3795), ADR-004 (#3796). All four crt-033 ADRs surfaced. Applied ADR-001 (synchronous write), ADR-003 (deserialization fallthrough as cache miss).
- Stored: entry via `/uni-store-pattern` — "crt-033 memoization: check_stored_review returns Result, not Option — callers handle cache-miss via Err branch" + gotcha about pre-baked JSON for test CycleReviewRecord.
