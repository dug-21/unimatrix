# Agent Report: nan-008-agent-5-runner-metrics

## Task

Implement `compute_cc_at_k` and `compute_icd` pure metric functions in
`runner/metrics.rs`, extend `compute_comparison` to populate `cc_at_k_delta`
and `icd_delta` from real values (replacing Wave 1 stubs), and add unit tests
covering AC-10 to `runner/tests_metrics.rs`.

## Files Modified

- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/runner/metrics.rs`
- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/runner/tests_metrics.rs`

## Implementation Summary

### metrics.rs

**`compute_cc_at_k`** — intersection semantics per ADR-004 nan-008 and WARN-2
resolution. Builds a `HashSet<&String>` from `configured_categories`, then
collects distinct `entry.category` values that are present in that set. Emits
`tracing::warn!` and returns `0.0` when `configured_categories` is empty.
Range is naturally capped at `[0.0, 1.0]` because the intersection numerator
cannot exceed the configured denominator.

**`compute_icd`** — raw Shannon entropy using `f64::ln` per ADR-002 nan-008.
Returns `0.0` for empty entries. Builds a `HashMap<&str, usize>` count map,
then computes `entropy -= p * p.ln()` for each non-zero count. The
`if count == 0 { continue }` NaN guard is explicit defense-in-depth against
`0.0 * f64::ln(0.0) = NaN`.

**`compute_comparison`** — replaced stub `cc_at_k_delta: 0.0` and
`icd_delta: 0.0` with `candidate.cc_at_k - baseline.cc_at_k` and
`candidate.icd - baseline.icd`. Sign convention matches `mrr_delta` (positive
= candidate improved over baseline).

### tests_metrics.rs

Added imports for `compute_cc_at_k`, `compute_comparison`, `compute_icd`,
`HashMap`, and `ProfileResult`. Added helper `make_entries_with_categories` and
`make_profile_result_for_comparison`. Added 16 new tests:

- AC-10 boundary: `test_cc_at_k_all_categories_present`, `test_cc_at_k_one_category_present`, `test_icd_maximum_entropy`, `test_icd_single_category`
- Guard: `test_cc_at_k_empty_configured_categories_returns_zero`, `test_icd_nan_guard`
- Intersection semantics (WARN-2): `test_cc_at_k_intersection_semantics_category_outside_configured_not_counted`
- Edge cases: `test_cc_at_k_entries_empty_configured_non_empty`, `test_icd_empty_entries_returns_zero`, `test_icd_two_entries_one_category_each`
- Delta signs: `test_compute_comparison_delta_positive`, `test_compute_comparison_delta_negative`
- Determinism: `test_cc_at_k_deterministic`, `test_icd_deterministic`

## Test Results

```
running 47 tests
... (all pass)
test result: ok. 47 passed; 0 failed; 0 ignored
```

31 pre-existing tests + 16 new tests. All pass.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test -p unimatrix-server eval::runner` passes (47/47)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files are within scope defined in the brief
- [x] No `.unwrap()` in non-test code
- [x] New structs — N/A (no new structs added)
- [x] Code follows validated pseudocode — no deviations
- [x] Test cases match component test plan expectations
- [x] No source file exceeds 500 lines (metrics.rs: ~290 lines, tests_metrics.rs: ~470 lines)

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "pure function metric computation eval harness" — returned entry #1042 (Pure Computation Engine Module Pattern) and #3472 (pre-post differential atomic update pattern). Neither was directly applicable to Shannon entropy NaN guards but confirmed the zero-I/O, zero-async constraint is an established pattern for this crate.
- Stored: entry #3528 "Shannon entropy ICD pattern: skip zero-count categories to avoid 0.0 * ln(0.0) = NaN" via `/uni-store-pattern` — this is a non-obvious trap: the HashMap construction from observed data never inserts zeros, but without the explicit guard a future refactor pre-populating from `configured_categories` would silently produce NaN in the entropy sum.
