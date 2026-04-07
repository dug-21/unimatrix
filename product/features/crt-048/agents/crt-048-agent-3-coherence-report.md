# Agent Report: crt-048-agent-3-coherence

## Component
`crates/unimatrix-server/src/infra/coherence.rs` (Component A)

## Status
COMPLETE

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/coherence.rs`

## Changes Implemented

### Struct / Constants
- Removed `confidence_freshness: f64` field from `CoherenceWeights` (3 fields remain)
- Updated `DEFAULT_WEIGHTS` to `{ graph_quality: 0.46, embedding_consistency: 0.23, contradiction_density: 0.31 }`
- Updated `DEFAULT_STALENESS_THRESHOLD_SECS` doc comment per ADR-002 (constant value unchanged)
- Removed `use unimatrix_store::EntryRecord` import (no longer needed after function deletions)
- Updated module doc comment: "four dimension scores" → "three dimension scores"

### Functions Deleted
- `confidence_freshness_score()` — deleted entirely (no replacement)
- `oldest_stale_age()` — deleted entirely (no replacement)

### Functions Updated
- `compute_lambda()` — removed `freshness: f64` first parameter; both `Some` and `None` arms updated to remove freshness term; `None` re-normalization now over `graph_quality + contradiction_density` only
- `generate_recommendations()` — removed `stale_confidence_count: u64` and `oldest_stale_age_secs: u64` parameters; deleted stale-confidence `if` branch

### Tests Deleted (~11)
`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

Also deleted: `make_entry_with_timestamps()` helper (no retained test uses it).

### Tests Updated / Renamed (~11)
- `lambda_all_ones` — removed freshness arg
- `lambda_all_zeros` — removed freshness arg
- `lambda_weighted_sum` — updated to `compute_lambda(0.6, Some(0.7), 0.4)`, expected 0.561 per test plan
- `lambda_specific_four_dimensions` → **renamed** `lambda_specific_three_dimensions` — inputs `(0.8, Some(0.5), 0.3)`, expected 0.576
- `lambda_single_dimension_deviation` — expanded to 3 sub-cases; all three per-dimension deviations asserted distinct
- `lambda_weight_sum_invariant` — removed `confidence_freshness`; uses `f64::EPSILON` guard (NFR-04)
- `lambda_renormalization_without_embedding` — expanded: trivial all-ones case + non-trivial `(0.8, None, 0.6)` case (R-07)
- `lambda_renormalization_partial` — updated to `(0.4, None, 0.9)` with formula-derived expected
- `lambda_renormalized_weights_sum_to_one` — removed freshness from sum; 2-of-3 re-norm
- `lambda_embedding_excluded_specific` — updated to `(0.7, None, 0.8)` with formula-derived expected
- `lambda_custom_weights_zero_embedding` — removed `confidence_freshness` from struct literal; call updated to `(0.6, None, 0.4)`, expected 0.52

### Tests Updated (arg-count change only)
- `recommendations_above_threshold_empty` — 7 args → 5 args
- `recommendations_at_threshold_empty` — 7 args → 5 args
- `recommendations_below_threshold_high_stale_ratio` — 7 args → 5 args
- `recommendations_below_threshold_all_issues` — expected len 4 → 3 (stale-confidence branch removed)
- `recommendations_below_threshold_embedding_inconsistencies` — 7 args → 5 args
- `recommendations_below_threshold_quarantined` — 7 args → 5 args

## Test Results

The coherence module tests cannot be run in isolation because the crate fails to
compile due to downstream callers in `services/status.rs` that still reference the
old 5-parameter `compute_lambda()` and deleted functions. This is the expected Wave 1
compile state documented in the spawn prompt.

All 6 compile errors are in `services/status.rs` (Wave 2 scope):
- `coherence::confidence_freshness_score` not found (lines 695, 793)
- `coherence::oldest_stale_age` not found (line 766)
- `compute_lambda()` 5 args → 4 args (lines 771, 798)
- `generate_recommendations()` 7 args → 5 args (line 811)

Zero errors originate in `infra/coherence.rs` itself (verified by error location grep).

## Key Decisions Applied
- Weight literals 0.46/0.31/0.23 locked per OQ-1 / ADR-001 (#4199)
- `DEFAULT_STALENESS_THRESHOLD_SECS` retained per ADR-002 (#4193)
- `lambda_weight_sum_invariant` uses `f64::EPSILON` guard (NFR-04)
- `lambda_renormalization_without_embedding` includes non-trivial R-07 case
- `lambda_specific_three_dimensions` uses distinct per-dimension values (R-01)

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #4193 (ADR-002 staleness retention), #4189 (drop time-based dimensions pattern), #4199 (ADR-001 3-dim weights). All directly applicable; no surprises.
- Stored: nothing novel to store — the patterns were pre-existing (#4189: drop time-based Lambda dimensions when lifecycle assumption is invalidated) and the ADRs were already recorded (#4192/#4193). The implementation followed them without deviation.
