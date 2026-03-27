# Component 4: Distribution Gate Aggregation

**File**: `eval/report/aggregate/distribution.rs` (new)
**Prerequisite**: Pre-split B must be complete (`aggregate.rs` → `aggregate/mod.rs`) before
this file is created.

---

## Purpose

Compute whether a distribution-change candidate profile's mean metrics meet the declared
targets. Returns a `DistributionGateResult` that carries per-metric detail for rendering
and two independent pass/fail booleans — one for diversity (CC@k + ICD) and one for the
MRR floor veto (ADR-003).

No new metric computation — reads `mean_cc_at_k`, `mean_icd`, `mean_mrr` from the existing
`AggregateStats` struct (fields added in nan-008, confirmed present by inspection).

---

## New Types in This File

```
// Per-metric gate detail: what was the target, what was the actual value, did it pass.
pub(super) struct MetricGateRow {
    pub target: f64,
    pub actual: f64,
    pub passed: bool,   // actual >= target
}

// Full result of check_distribution_targets for one profile.
// ADR-003: mrr_floor is a veto, structurally separate from diversity targets.
pub(super) struct DistributionGateResult {
    pub cc_at_k: MetricGateRow,
    pub icd: MetricGateRow,
    pub mrr_floor: MetricGateRow,      // veto — evaluated independently
    pub diversity_passed: bool,        // cc_at_k.passed && icd.passed
    pub mrr_floor_passed: bool,        // mrr_floor.passed
    pub overall_passed: bool,          // diversity_passed && mrr_floor_passed
}
```

Derive `Debug` on both. No `Clone` required (these are produced fresh per call).

---

## Function: `check_distribution_targets`

```
pub(super) fn check_distribution_targets(
    stats: &AggregateStats,        // candidate profile stats — NEVER baseline stats
    targets: &DistributionTargets,
) -> DistributionGateResult

    // All comparisons use >= (edge case: exact equality passes, ADR-003)
    cc_row = MetricGateRow {
        target: targets.cc_at_k_min,
        actual: stats.mean_cc_at_k,
        passed: stats.mean_cc_at_k >= targets.cc_at_k_min,
    }

    icd_row = MetricGateRow {
        target: targets.icd_min,
        actual: stats.mean_icd,
        passed: stats.mean_icd >= targets.icd_min,
    }

    mrr_row = MetricGateRow {
        target: targets.mrr_floor,
        actual: stats.mean_mrr,
        passed: stats.mean_mrr >= targets.mrr_floor,
    }

    diversity_passed = cc_row.passed && icd_row.passed
    mrr_floor_passed = mrr_row.passed
    overall_passed   = diversity_passed && mrr_floor_passed

    DistributionGateResult {
        cc_at_k: cc_row,
        icd: icd_row,
        mrr_floor: mrr_row,
        diversity_passed,
        mrr_floor_passed,
        overall_passed,
    }
```

No error path — this function is infallible. All inputs are well-typed f64 values.
NaN handling: Rust's `>=` on NaN returns false, so NaN actual values fail the gate.
This is correct behavior — a NaN mean indicates a computation error upstream, not a valid
pass. No special NaN handling is added here.

---

## Integration with `aggregate/mod.rs`

After the pre-split, `aggregate/mod.rs` contains all existing functions unchanged.
`distribution.rs` is a sibling. `mod.rs` must declare the submodule and re-export the
new types:

```
// In aggregate/mod.rs (additions only):
mod distribution;
pub(super) use distribution::{check_distribution_targets, DistributionGateResult, MetricGateRow};
```

The re-export uses `pub(super)` to match the existing visibility pattern in `aggregate/mod.rs`
(all existing pub functions there are `pub(super)`). This keeps the types invisible outside
`report/`.

`DistributionTargets` is imported via:
```
use crate::eval::profile::DistributionTargets;
```

`AggregateStats` is imported via:
```
use super::super::AggregateStats;  // report/mod.rs defines AggregateStats
```
or equivalently:
```
use crate::eval::report::AggregateStats;
```
The exact path depends on the module hierarchy after the split; the implementation agent
must verify the correct relative path.

---

## Data Flow

Inputs:
- `stats: &AggregateStats` — candidate profile aggregate stats; reads `mean_cc_at_k`,
  `mean_icd`, `mean_mrr`; no other fields touched
- `targets: &DistributionTargets` — from `ProfileMetaEntry.distribution_targets` (loaded
  by `report/mod.rs` from the sidecar and converted from `DistributionTargetsJson`)

Outputs:
- `DistributionGateResult` — consumed by `render_distribution_gate_section` in Component 5

Caller (in `report/mod.rs`, step 4 aggregate block):
```
// For each non-baseline profile in aggregate_stats:
if let Some(entry) = profile_meta.get(&stat.profile_name):
    if entry.distribution_change:
        if let Some(ref targets_json) = entry.distribution_targets:
            // Convert DistributionTargetsJson → DistributionTargets
            targets = DistributionTargets {
                cc_at_k_min: targets_json.cc_at_k_min,
                icd_min: targets_json.icd_min,
                mrr_floor: targets_json.mrr_floor,
            }
            gate_result = check_distribution_targets(&stat, &targets)
            // gate_result passed to render_report via some mechanism — see Component 6+7
```

The exact mechanism for passing `gate_result` to `render_report` is handled in Components
6 and 7. Options: compute in `run_report` and pass in a `HashMap<String, DistributionGateResult>`,
or compute inside `render_report`. Recommendation: compute in `run_report` before calling
`render_report`, pass as a `HashMap<String, DistributionGateResult>` parameter. This keeps
`render_report` pure and testable.

---

## Error Handling

`check_distribution_targets` is infallible. No `Result` return type.

Caller must handle the case where `profile_meta.get(name)` returns `None` (absent sidecar
or profile not in sidecar). In that case, `distribution_change` is treated as false
(backward-compat). This logic lives in the caller, not here.

---

## Key Test Scenarios

Tests in `eval/report/tests_distribution_gate.rs`:

```
test_check_distribution_targets_all_pass:
    stats: mean_cc_at_k=0.70, mean_icd=1.50, mean_mrr=0.45
    targets: cc_at_k_min=0.60, icd_min=1.20, mrr_floor=0.35
    Assert: diversity_passed=true, mrr_floor_passed=true, overall_passed=true
            cc_at_k.passed=true, icd.passed=true, mrr_floor.passed=true

test_check_distribution_targets_cc_at_k_fail:
    stats: mean_cc_at_k=0.55, mean_icd=1.50, mean_mrr=0.45
    targets: cc_at_k_min=0.60, icd_min=1.20, mrr_floor=0.35
    Assert: cc_at_k.passed=false, diversity_passed=false, overall_passed=false
            icd.passed=true, mrr_floor.passed=true

test_check_distribution_targets_icd_fail:
    stats: mean_cc_at_k=0.70, mean_icd=1.10, mean_mrr=0.45
    targets: cc_at_k_min=0.60, icd_min=1.20, mrr_floor=0.35
    Assert: icd.passed=false, diversity_passed=false, overall_passed=false
            cc_at_k.passed=true, mrr_floor.passed=true

test_check_distribution_targets_mrr_floor_fail:
    stats: mean_cc_at_k=0.70, mean_icd=1.50, mean_mrr=0.30
    targets: cc_at_k_min=0.60, icd_min=1.20, mrr_floor=0.35
    Assert: mrr_floor.passed=false, mrr_floor_passed=false, overall_passed=false
            cc_at_k.passed=true, icd.passed=true, diversity_passed=true
    (R-14 coverage: assert mrr_floor.actual==0.30, NOT baseline_stats.mean_mrr)

Boundary condition (edge case from RISK-TEST-STRATEGY):
    mrr_floor exactly equal to mean_mrr:
    stats: mean_mrr=0.35, targets: mrr_floor=0.35
    Assert: mrr_floor.passed=true   (>= semantics)

Four-state coverage (R-05):
    State pass/pass: all three pass → diversity_passed=true, mrr_floor_passed=true
    State pass/fail: diversity pass, mrr fail → diversity_passed=true, mrr_floor_passed=false
    State fail/pass: diversity fail, mrr pass → diversity_passed=false, mrr_floor_passed=true
    State fail/fail: both fail → diversity_passed=false, mrr_floor_passed=false
```

---

## Notes

- `DistributionTargets` is defined in `profile/types.rs` and imported here. The sidecar
  types `DistributionTargetsJson` in `runner/profile_meta.rs` are separate — the caller
  in `report/mod.rs` converts between them before passing to this function.
- `pub(super)` visibility on both types and the function means they are usable within
  `report/` (via the re-export from `aggregate/mod.rs`) but invisible outside the crate.
- The function takes `&AggregateStats` not `&[ScenarioResult]` — the averaging is
  already done by `compute_aggregate_stats`. This function does only comparison.
