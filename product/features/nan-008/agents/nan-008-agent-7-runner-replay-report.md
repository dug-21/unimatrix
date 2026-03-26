# Agent Report: nan-008-agent-7-runner-replay

## Task
Wire the new metric functions into the replay orchestration in `runner/replay.rs`.

## Changes Made

**File modified:** `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/runner/replay.rs`

Changes (18 insertions, 6 deletions across one file):

1. Extended the metrics import line to include `compute_cc_at_k` and `compute_icd`.
2. Added `configured_categories: &[String]` parameter to `run_single_profile`.
3. Verified `category: se.entry.category.clone()` was already correctly populated by Wave 1 (not a stub).
4. Added step 6 after P@K/MRR computation: calls `compute_cc_at_k(&entries, configured_categories)` and `compute_icd(&entries)`.
5. Replaced `cc_at_k: 0.0` and `icd: 0.0` stubs in `ProfileResult` construction with the computed values.
6. Updated `replay_scenario` call site to pass `&profile.config_overrides.knowledge.categories` to `run_single_profile`.

**Phase snapshot (ADR-001):** No snapshot was required. `configured_categories` is a `&[String]` borrowed from `&profile.config_overrides.knowledge.categories` — a shared borrow of `EvalProfile` from `profiles: &[EvalProfile]`. No lock guard is held; no move occurs across the await boundary. This is consistent with SR-07 resolution documented in the pseudocode.

## Test Results

```
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured
```

All 47 `eval::runner` tests pass. No new failures.

## Issues

None. All five required changes were applied cleanly. The `category` field was already populated from `se.entry.category` by a prior wave — no stub removal needed for that field.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `nan-008 architectural decisions`, `eval harness replay orchestration async spawn` — found ADRs #3520–#3524, dual-type pattern #3512, phase-snapshot pattern #3027, round-trip test strategy #3526.
- Stored: nothing novel to store — the borrow-across-await safety for `&[String]` from a `&[EvalProfile]` slice is already captured by ADR-001 / pattern #3027. No new crate-specific gotcha emerged; the implementation was a straightforward parameter threading exercise.
