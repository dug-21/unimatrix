# Component: Report Aggregation

File: `eval/report/aggregate.rs`

## Purpose

Add `compute_phase_stats` — a pure synchronous function that groups `ScenarioResult`
records by phase and computes per-phase mean metrics. Returns an empty `Vec` when all
phases are `None` (section 6 is then omitted by the renderer).

---

## File Size Check

Current `aggregate.rs` is approximately 395 lines. `compute_phase_stats` will add
approximately 65-80 lines, bringing the total to ~460-475 lines — within the 500-line
limit. No split into `aggregate_phase.rs` is required unless the implementation grows
beyond this estimate. If the implementation agent finds the file exceeds 490 lines after
adding the function, extract `compute_phase_stats` to `aggregate_phase.rs` per
Constraint 7 and NFR-04.

---

## New Function: `compute_phase_stats`

### Signature (already in integration surface)

```
pub(super) fn compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>
```

Visibility: `pub(super)` — used by `mod.rs` only.
Synchronous: no async, no tokio, no database access (Constraint 4, NFR-03).

### Algorithm

```
FUNCTION compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>

    IF results is empty THEN
        RETURN empty vec   // EC-01: no panic on empty input
    END IF

    // Step 1: Collect all distinct phase keys present in results.
    // None is treated as a distinct key for grouping purposes.
    // Key type: Option<String>

    // Step 2: Check if ANY result has a non-null phase.
    // If ALL phases are None, return empty vec (R-07 guard).
    has_non_null_phase = any result where result.phase is Some(_)
    IF NOT has_non_null_phase THEN
        RETURN empty vec
    END IF

    // Step 3: Group results by phase.
    // Use a HashMap<Option<String>, Vec<metrics>> or accumulator struct.
    // For each group, accumulate: count, sum_p_at_k, sum_mrr, sum_cc_at_k, sum_icd.

    groups: HashMap<Option<String>, PhaseAccumulator>

    FOR EACH result IN results DO
        key = result.phase.clone()    // None or Some("delivery") etc.

        // Extract baseline profile metrics (first profile = baseline by convention)
        // For per-phase aggregation, use the BASELINE profile metrics.
        // Rationale: phase stratification measures the baseline retrieval quality
        // per phase. For single-profile runs, only one profile exists — use it.
        // For multi-profile runs, use the profile named "baseline" (matching
        // existing convention in compute_aggregate_stats).

        baseline_metrics = get_baseline_profile_metrics(result)
        // Returns Option<(p_at_k, mrr, cc_at_k, icd)>
        // If result has no profiles, skip this result

        IF baseline_metrics is None THEN
            CONTINUE
        END IF

        (p_at_k, mrr, cc_at_k, icd) = baseline_metrics

        accumulator = groups.entry(key).or_insert(PhaseAccumulator::new())
        accumulator.count += 1
        accumulator.sum_p_at_k += p_at_k
        accumulator.sum_mrr += mrr
        accumulator.sum_cc_at_k += cc_at_k
        accumulator.sum_icd += icd
    END FOR

    // Step 4: Convert accumulators to PhaseAggregateStats.
    stats: Vec<PhaseAggregateStats>

    FOR EACH (phase_key, acc) IN groups DO
        IF acc.count == 0 THEN CONTINUE END IF

        phase_label = match phase_key {
            None        => "(unset)".to_string(),    // ADR-003; MUST be "(unset)" not "(none)"
            Some(s)     => s.clone(),
        }

        stats.push(PhaseAggregateStats {
            phase_label,
            scenario_count: acc.count,
            mean_p_at_k:  acc.sum_p_at_k  / acc.count as f64,
            mean_mrr:     acc.sum_mrr      / acc.count as f64,
            mean_cc_at_k: acc.sum_cc_at_k  / acc.count as f64,
            mean_icd:     acc.sum_icd       / acc.count as f64,
        })
    END FOR

    // Step 5: Sort — alphabetical ascending for named phases; "(unset)" last.
    // MUST NOT use a plain lexicographic sort: '(' (ASCII 40) < 'a' (ASCII 97),
    // so "(unset)" would sort BEFORE alphabetic phase names. ADR-003 requires it last.
    stats.sort_by(|a, b| {
        match (&a.phase_label[..], &b.phase_label[..]) {
            ("(unset)", "(unset)") => Equal,
            ("(unset)", _)         => Greater,   // "(unset)" always last
            (_, "(unset)")         => Less,       // named phases before "(unset)"
            (x, y)                 => x.cmp(y),  // alphabetical for named phases
        }
    })

    RETURN stats
```

### Helper: `get_baseline_profile_metrics`

Used internally — not exported.

```
FUNCTION get_baseline_profile_metrics(result: &ScenarioResult) -> Option<(f64, f64, f64, f64)>
    // Returns (p_at_k, mrr, cc_at_k, icd) from the baseline profile.

    IF result.profiles is empty THEN
        RETURN None
    END IF

    // Determine baseline profile name using same logic as compute_aggregate_stats:
    // "baseline" (case-insensitive) forced first; otherwise alphabetically first.
    profile_names = result.profiles.keys() as sorted Vec<&str>
    IF "baseline" (case-insensitive) in profile_names THEN
        baseline_name = "baseline"
    ELSE
        baseline_name = profile_names[0]
    END IF

    prof = result.profiles.get(baseline_name)?
    RETURN Some((prof.p_at_k, prof.mrr, prof.cc_at_k, prof.icd))
```

Note: Implementation may inline this helper or use the same pattern as
`compute_cc_at_k_scenario_rows` which performs the same baseline selection logic.
Avoid code duplication by extracting a shared helper if the pattern appears a third time.

### Internal accumulator type (private to function, no need to define as a struct)

```
// Private accumulator (can use a local struct or a tuple inside the HashMap)
struct PhaseAccumulator {
    count: usize,
    sum_p_at_k: f64,
    sum_mrr: f64,
    sum_cc_at_k: f64,
    sum_icd: f64,
}
// Initialized as zeros.
```

---

## Imports

`compute_phase_stats` needs to import `PhaseAggregateStats` from `super` (defined in `mod.rs`).
The existing import block in `aggregate.rs` already uses `super::{...}` for similar types.

Add `PhaseAggregateStats` to the existing import from `super`:
```
use super::{
    AggregateStats, CcAtKScenarioRow, EntryRankSummary, LatencyBucket,
    PhaseAggregateStats,        // NEW
    RegressionRecord, ScenarioResult,
};
```

---

## Error Handling

`compute_phase_stats` is infallible — it returns `Vec<PhaseAggregateStats>`, not `Result`.
All inputs are already-loaded in-memory slices. Possible degenerate cases:
- Empty `results`: returns empty vec (guard at top).
- All `phase = None`: returns empty vec (has_non_null_phase guard).
- Results with no profiles: skipped via `get_baseline_profile_metrics` returning `None`.
- Single scenario with one phase: returns a one-element vec.

No panics. Division by zero guarded: `count` is only used as divisor after `count += 1`,
and groups with `count == 0` are skipped.

---

## Key Test Scenarios

Tests live in `eval/report/tests.rs`.

**T1: `test_compute_phase_stats_all_null_returns_empty`** (R-07, AC-04 part 1)
- Input: vec of ScenarioResult where all `phase == None`
- Assert: returned vec is empty

**T2: `test_compute_phase_stats_null_bucket_label`** (R-01, AC-05)
- Input: one result with `phase = None`
- If results had a mix of non-null phases, the None bucket appears
- Assert: `PhaseAggregateStats.phase_label == "(unset)"` for the null bucket
- MUST be exactly `"(unset)"` — canonical string check

**T3: `test_compute_phase_stats_null_bucket_sorts_last`** (R-08, AC-05)
- Input: results with `phase = Some("delivery")`, `phase = Some("design")`,
  `phase = Some("bugfix")`, `phase = None`
- Assert: last element has `phase_label == "(unset)"`
- Assert: first three elements are `["bugfix", "delivery", "design"]` (alphabetical)
- Key invariant: `(` ASCII 40 < `a` ASCII 97 — plain sort would put "(unset)" first

**T4: `test_compute_phase_stats_grouping_and_means`** (AC-05, AC-09 item 3)
- Input: three results — two with `phase = "delivery"`, one with `phase = "design"`
- All with known p_at_k/mrr/cc_at_k/icd values
- Assert:
  - Two groups returned (delivery, design)
  - delivery group: count=2, means are correct averages
  - design group: count=1, means equal the single result's values

**T5: `test_compute_phase_stats_empty_results`** (EC-01)
- Input: empty vec
- Assert: returned vec is empty, no panic

**T6: `test_compute_phase_stats_single_non_null`** (EC-02)
- Input: one result with `phase = Some("delivery")`
- Assert: one-element vec, label="delivery", count=1
