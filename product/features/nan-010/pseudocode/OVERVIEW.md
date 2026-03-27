# nan-010 Pseudocode Overview

## Purpose

Distribution Change Profile Flag for Eval Harness. Adds `distribution_change = true` to
profile TOMLs so profiles that intentionally shift retrieval distribution can replace the
"Zero-Regression Check" (Section 5) with a "Distribution Gate" that evaluates mean CC@k,
mean ICD, and an absolute MRR floor. When absent or false all existing behavior is unchanged.

---

## Module Pre-Split Steps (FIRST — before any feature code)

These are not feature components. They are structural prerequisites that must be committed
and building before any other change in this feature.

### Pre-split A: `render_distribution_gate.rs` boundary stub

`eval/report/render.rs` is at 499 lines. Any addition — including a `mod` declaration or
`use` line — breaches the 500-line limit (ADR-001, SR-03).

Step: Create `eval/report/render_distribution_gate.rs` as an empty boundary file containing
only a `// Distribution Gate renderer (nan-010)` comment. Add to `render.rs`:
```
mod render_distribution_gate;
use render_distribution_gate::render_distribution_gate_section;
```
Confirm `cargo build` passes before proceeding.

### Pre-split B: `aggregate.rs` → `aggregate/mod.rs`

`eval/report/aggregate.rs` is at 488 lines. The new aggregation code would breach 500.

Step: Rename `eval/report/aggregate.rs` to `eval/report/aggregate/mod.rs`. No logic changes.
All existing public symbols re-exported unchanged. Create empty
`eval/report/aggregate/distribution.rs` placeholder. Confirm `cargo build` passes.

---

## Components Involved

| # | Component | File | Status |
|---|-----------|------|--------|
| 1 | Profile Types | `eval/profile/types.rs` | Modify |
| 2 | Profile Validation | `eval/profile/validation.rs` | Modify |
| 3 | Runner Profile Meta Sidecar | `eval/runner/profile_meta.rs` (new) + `eval/runner/mod.rs` | Create + Modify |
| 4 | Distribution Gate Aggregation | `eval/report/aggregate/distribution.rs` (new) | Create |
| 5 | Distribution Gate Renderer | `eval/report/render_distribution_gate.rs` (new) | Create |
| 6 | Section 5 Dispatch | `eval/report/render.rs` | Modify |
| 7 | Report Sidecar Load | `eval/report/mod.rs` | Modify |

---

## Data Flow

```
eval run:
  TOML file
    → parse_profile_toml()           [profile/validation.rs]
         extracts distribution_change + distribution_targets from raw TOML
         before [profile] section stripped for UnimatrixConfig
         returns EvalProfile { name, description, config_overrides,
                               distribution_change, distribution_targets }
    → run_eval_async()               [runner/mod.rs]
         after run_replay_loop() completes:
         → write_profile_meta(profiles, out)   [runner/profile_meta.rs]
              builds ProfileMetaFile { version:1, profiles:HashMap }
              writes to {out}/profile-meta.json.tmp
              renames to {out}/profile-meta.json

eval report:
  results directory
    → run_report()                   [report/mod.rs]
         Step 1-3: existing JSON load + query map (unchanged)
         Step 3.5: load_profile_meta(results)   → HashMap<String, ProfileMetaEntry>
                   absent → Ok(HashMap::new())
                   corrupt → Err(EvalError::...) → abort non-zero
         Step 4: aggregate unchanged; check_distribution_targets called per profile
                 [report/aggregate/distribution.rs]
                   takes AggregateStats + DistributionTargets from profile_meta
                   returns DistributionGateResult
         Step 5: render_report(..., profile_meta)  [report/render.rs]
                   Section 5 dispatch per non-baseline profile:
                     distribution_change=true → render_distribution_gate_section()
                     distribution_change=false → existing zero-regression block
```

---

## Shared Types Introduced

All types defined in their owning component file. This table shows what crosses module
boundaries and where each type is declared vs. consumed.

| Type | Declared In | Consumed In |
|------|------------|-------------|
| `DistributionTargets` | `profile/types.rs` | `profile/validation.rs`, `runner/profile_meta.rs`, `report/aggregate/distribution.rs` |
| `EvalProfile` (extended) | `profile/types.rs` | `runner/mod.rs`, `runner/profile_meta.rs` |
| `ProfileMetaFile` | `runner/profile_meta.rs` | `runner/profile_meta.rs` (write only) |
| `ProfileMetaEntry` | `runner/profile_meta.rs` | `report/mod.rs`, `report/render.rs` |
| `DistributionTargetsJson` | `runner/profile_meta.rs` | `runner/profile_meta.rs`, `report/mod.rs` |
| `MetricGateRow` | `report/aggregate/distribution.rs` | `report/render_distribution_gate.rs` |
| `DistributionGateResult` | `report/aggregate/distribution.rs` | `report/render.rs`, `report/render_distribution_gate.rs` |
| `HeadingLevel` | `report/render.rs` | `report/render_distribution_gate.rs` |

`ProfileMetaEntry` and `DistributionTargetsJson` must be re-exported from
`eval/runner/profile_meta.rs` so `report/mod.rs` can import them.

---

## Shared Enum: `HeadingLevel`

Defined in `report/render.rs` (or as a small local type), passed to
`render_distribution_gate_section` to control whether Section 5 uses `## 5.` or `### 5.N`.

```
enum HeadingLevel {
    Single,             // "## 5."  — exactly one non-baseline candidate
    Multi { index: usize },  // "### 5.N" — multiple non-baseline candidates
}
```

---

## Sequencing Constraints (hard order)

1. Pre-split A and B (boundary stubs) — before any feature code in render.rs or aggregate.rs
2. Component 1 (profile/types.rs) — DistributionTargets must exist before validation, runner, report
3. Component 2 (profile/validation.rs) — parse_profile_toml extended after types exist
4. Component 3 (runner/profile_meta.rs + runner/mod.rs) — depends on EvalProfile having new fields
5. Component 4 (aggregate/distribution.rs) — depends on AggregateStats shape + DistributionTargets
6. Component 5 (render_distribution_gate.rs) — depends on DistributionGateResult, AggregateStats
7. Component 6 (render.rs dispatch) — depends on Components 4 and 5
8. Component 7 (report/mod.rs sidecar load) — depends on ProfileMetaEntry, Component 6

---

## Key Invariants Across All Components

- Zero fields added to `ScenarioResult` in either `runner/output.rs` or `report/mod.rs` (ADR-002)
- `eval report` exits 0 regardless of Distribution Gate outcome (C-07, FR-29)
- Absent `profile-meta.json` → backward-compat, no error
- Corrupt `profile-meta.json` → abort with non-zero exit, message "profile-meta.json is malformed — re-run eval to regenerate"
- `mrr_floor` compared against candidate mean MRR only, never baseline MRR (ADR-003)
- Baseline profile with `distribution_change = true` → `EvalError::ConfigInvariant` at parse time
