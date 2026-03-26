# nan-008 Pseudocode: runner/replay.rs

## Purpose

Scenario replay orchestration. Loads scenarios from JSONL, calls the service
layer per profile, and assembles per-scenario result files. Two functions change
in nan-008: `replay_scenario` and `run_single_profile`. The outer loop
`run_replay_loop` changes only if `replay_scenario` changes signature; see below.

## Existing Code (context — do not remove)

`load_scenarios`, `run_replay_loop` — unchanged.
`replay_scenario` — gains one call-site change (passes categories to `run_single_profile`).
`run_single_profile` — gains one parameter and two additional lines.

## Import Changes

Add to existing imports:
```
use super::metrics::{compute_cc_at_k, compute_icd, ...};
```

The existing import line already includes `compute_comparison`, `compute_mrr`,
`compute_p_at_k`, `determine_ground_truth`. Extend it to also import
`compute_cc_at_k` and `compute_icd`.

## Modified Functions

### replay_scenario — add categories pass-through

```
pub(super) async fn replay_scenario(
    record: &ScenarioRecord,
    profiles: &[EvalProfile],
    layers: &[EvalServiceLayer],
    k: usize,
) -> Result<ScenarioResult, Box<dyn std::error::Error>>

Changes:
  Line that calls run_single_profile changes from:
      let result = run_single_profile(record, layer, k).await?;
  to:
      let result = run_single_profile(
          record,
          layer,
          k,
          &profile.config_overrides.knowledge.categories,
      ).await?;

  The `profile` variable is already in scope from the zip iterator:
      for (profile, layer) in profiles.iter().zip(layers.iter())

  Borrow analysis (SR-07 resolution from ARCHITECTURE.md):
  - `profiles` is borrowed as `&[EvalProfile]` throughout this function.
  - `profile` is a `&EvalProfile` from the slice borrow.
  - `&profile.config_overrides.knowledge.categories` is a `&Vec<String>` from
    that shared borrow, coerced to `&[String]` at the call site.
  - No move of `profile` occurs in this scope. No lifetime conflict.
  - The async block does not capture `profile` beyond the single `await` call.

All other logic unchanged.
```

### run_single_profile — add configured_categories parameter, populate category, call metrics

```
async fn run_single_profile(
    record: &ScenarioRecord,
    layer: &EvalServiceLayer,
    k: usize,
    configured_categories: &[String],    // NEW parameter
) -> Result<ProfileResult, Box<dyn std::error::Error>>

Step 1 — Build search params: unchanged.
Step 2 — Time the search: unchanged.

Step 3 — Build ScoredEntry list: ADD category field population

    Before:
        .map(|se| ScoredEntry {
            id: se.entry.id as u64,
            title: se.entry.title.clone(),
            final_score: se.final_score,
            similarity: se.similarity,
            confidence: se.entry.confidence,
            status: se.entry.status.to_string(),
            nli_rerank_delta: None,
        })

    After:
        .map(|se| ScoredEntry {
            id: se.entry.id as u64,
            title: se.entry.title.clone(),
            category: se.entry.category.clone(),    // NEW — from EntryRecord.category
            final_score: se.final_score,
            similarity: se.similarity,
            confidence: se.entry.confidence,
            status: se.entry.status.to_string(),
            nli_rerank_delta: None,
        })

    `se.entry.category` is the `category: String` field on `EntryRecord` from
    `unimatrix-store`. No new imports needed — `se.entry` already provides this.

Step 4 — Determine ground truth: unchanged.
Step 5 — Compute P@K and MRR: unchanged.

Step 6 — NEW: Compute CC@k and ICD (insert after step 5, before building ProfileResult)

    let cc_at_k = compute_cc_at_k(&entries, configured_categories);
    let icd = compute_icd(&entries);

    `entries` is the assembled Vec<ScoredEntry> from step 3 (with category populated).
    `configured_categories` is the new parameter passed from replay_scenario.

Step 7 — Build and return ProfileResult: ADD new fields

    Before:
        Ok(ProfileResult {
            entries,
            latency_ms,
            p_at_k,
            mrr,
        })

    After:
        Ok(ProfileResult {
            entries,
            latency_ms,
            p_at_k,
            mrr,
            cc_at_k,    // NEW
            icd,        // NEW
        })
```

## No Changes to run_replay_loop

`run_replay_loop` calls `replay_scenario` which calls `run_single_profile`.
The `configured_categories` parameter flows inward: `replay_scenario` reads it
from `&profile.config_overrides.knowledge.categories` and passes it down. The
outer loop signature is unchanged.

## Data Flow Summary

```
EvalProfile.config_overrides.knowledge.categories: Vec<String>
    |
    | replay_scenario borrows as &[String]
    |
    v
run_single_profile(..., configured_categories: &[String])
    |
    +-- maps se.entry.category -> ScoredEntry.category
    |
    +-- compute_cc_at_k(&entries, configured_categories) -> cc_at_k: f64
    +-- compute_icd(&entries) -> icd: f64
    |
    v
ProfileResult { entries, latency_ms, p_at_k, mrr, cc_at_k, icd }
```

## Error Handling

- `compute_cc_at_k` and `compute_icd` are pure functions that never fail. They
  return `f64` values; no `?` operator needed.
- If `se.entry.category` is an empty string (the `EntryRecord` category was never
  set), the entry maps to `category: ""`. The metric functions handle this correctly:
  `compute_cc_at_k` counts `""` as a category, which will not match any non-empty
  `configured_categories` entry under intersection semantics. `compute_icd`
  treats `""` as one category. The resulting metric values are technically correct
  but indicate a data-quality issue; the delivery agent's integration test (R-08)
  must verify that categories are non-empty in practice.
- No new `Err` paths are introduced.

## Key Test Scenarios

1. Integration test — `run_single_profile` with fixture layer returning entries
   from two distinct categories (e.g., "decision" and "lesson-learned"):
   - Assert `result.cc_at_k > 0.0`
   - Assert `result.icd > 0.0`
   - Assert per-entry `category` fields are non-empty and match expected values

2. `replay_scenario` with two profiles (baseline and candidate) where candidate
   has higher CC@k:
   - Assert `comparison.cc_at_k_delta > 0.0`
   - Assert `comparison.icd_delta` has the correct sign

3. Single-profile run: `replay_scenario` with one profile:
   - Assert `comparison.cc_at_k_delta == 0.0` (self-comparison)
   - Assert `comparison.icd_delta == 0.0`

4. Profile with empty `configured_categories` (R-09 / R-03 guard):
   - Assert `result.cc_at_k == 0.0`
   - Assert no panic
   - Verify `tracing::warn!` is reachable in the call path (code-inspection level)
