# nan-010: Distribution Change Profile Flag for Eval Harness — Implementation Brief

GH Issue: #402

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-010/SCOPE.md |
| Architecture | product/features/nan-010/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-010/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-010/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-010/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Profile Types | pseudocode/profile-types.md | test-plan/profile-types.md |
| Profile Validation | pseudocode/profile-validation.md | test-plan/profile-validation.md |
| Runner Profile Meta Sidecar | pseudocode/runner-profile-meta.md | test-plan/runner-profile-meta.md |
| Distribution Gate Aggregation | pseudocode/aggregate-distribution.md | test-plan/aggregate-distribution.md |
| Distribution Gate Renderer | pseudocode/render-distribution-gate.md | test-plan/render-distribution-gate.md |
| Section 5 Dispatch | pseudocode/section5-dispatch.md | test-plan/section5-dispatch.md |
| Report Sidecar Load | pseudocode/report-sidecar-load.md | test-plan/report-sidecar-load.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files produced in Stage 3a. Paths confirmed.

---

## Goal

Add a `distribution_change` boolean flag to eval profile TOMLs so that profiles designed to intentionally shift the retrieval result distribution can replace the "Zero-Regression Check" (Section 5) with a "Distribution Gate" that evaluates mean CC@k, mean ICD, and an absolute MRR floor instead. When the flag is absent or false all existing behavior is unchanged, preserving full backward compatibility with pre-nan-010 result directories.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Where to carry profile metadata so `eval report` can gate correctly without re-reading TOMLs | Sidecar `profile-meta.json` written by `eval run` to the output directory; zero changes to `ScenarioResult` dual-type copies | SCOPE.md §Design Decisions #4, SR-06 | product/features/nan-010/architecture/ADR-002-sidecar-file-zero-scenarioresult-changes.md |
| How to prevent a partial `eval run` from producing a corrupt sidecar | Atomic write via `profile-meta.json.tmp` → `fs::rename`; absent sidecar = backward-compat fallback; corrupt sidecar = abort with non-zero exit | SCOPE.md §Design Decisions #8, SR-01 | product/features/nan-010/architecture/ADR-004-atomic-sidecar-write.md |
| Should `aggregate.rs` and `render.rs` module splits precede feature code | Yes — module boundaries must be established as the first implementation step before any line is added to either file | SR-02, SR-03 | product/features/nan-010/architecture/ADR-001-module-pre-split-boundary.md |
| Is `mrr_floor` a co-equal diversity target or a veto | `mrr_floor` is a veto: evaluated and rendered independently; `diversity_passed` (CC@k + ICD) and `mrr_floor_passed` are separate booleans in `DistributionGateResult`; four distinct states | SCOPE.md §Design Decisions #3 | product/features/nan-010/architecture/ADR-003-mrr-floor-as-veto.md |
| How to render Section 5 when multiple candidate profiles mix `distribution_change` values | Per-profile independent gating: `## 5.` heading for single-profile, `### 5.N` sub-blocks for multi-profile | SCOPE.md §Design Decisions #1, #6, SR-05 | product/features/nan-010/architecture/ADR-005-per-profile-section5-rendering.md |
| What happens when baseline profile declares `distribution_change = true` | Hard `ConfigInvariant` at parse time: "baseline profile must not declare `distribution_change = true`"; silent ignore is ruled out | SCOPE.md §Design Decisions #7, ALIGNMENT-REPORT.md Variance #1 (resolved) | product/features/nan-010/architecture/ADR-001-module-pre-split-boundary.md |
| Include "Baseline MRR (reference)" row in Distribution Gate table | Yes — informational row, not a gate criterion; `render_distribution_gate_section` receives baseline `AggregateStats` as a second parameter | SCOPE.md §Design Decisions #5, ALIGNMENT-REPORT.md Variance #2 (resolved) | product/features/nan-010/architecture/ADR-003-mrr-floor-as-veto.md |
| `profile-meta.json` version field placement | Top-level field in `ProfileMetaFile` struct (not per-entry); version 1 from the start | ADR-002, SCOPE.md §Design Decisions #2, ALIGNMENT-REPORT.md (spec/arch schema inconsistency resolved) | product/features/nan-010/architecture/ADR-002-sidecar-file-zero-scenarioresult-changes.md |

---

## Files to Create or Modify

| File | Action | Summary |
|------|--------|---------|
| `eval/profile/types.rs` | Modify | Add `DistributionTargets` struct; extend `EvalProfile` with `distribution_change: bool` and `distribution_targets: Option<DistributionTargets>` |
| `eval/profile/validation.rs` | Modify | Extract `distribution_change` and `distribution_targets` from raw TOML before `[profile]` strip; validate completeness; return `EvalError::ConfigInvariant` on violation |
| `eval/runner/profile_meta.rs` | Create | New module: `ProfileMetaFile`, `ProfileMetaEntry`, `DistributionTargetsJson` serde types; `write_profile_meta` function with atomic write |
| `eval/runner/mod.rs` | Modify | Call `write_profile_meta` after scenario replay completes in `run_eval_async` |
| `eval/report/aggregate.rs` → `eval/report/aggregate/mod.rs` | Pre-split | Rename file to `aggregate/mod.rs`; re-export all existing public symbols; no logic changes |
| `eval/report/aggregate/distribution.rs` | Create | New submodule: `MetricGateRow`, `DistributionGateResult`, `check_distribution_targets` |
| `eval/report/render_distribution_gate.rs` | Create | New sibling module: `render_distribution_gate_section` rendering Section 5 Distribution Gate; receives `DistributionGateResult` and baseline `AggregateStats` |
| `eval/report/render.rs` | Modify | Add `mod render_distribution_gate;`; add `profile_meta` parameter to `render_report`; Section 5 dispatch per profile |
| `eval/report/mod.rs` | Modify | Add `load_profile_meta` private helper; declare new modules; pass `profile_meta` to `render_report` |
| `eval/profile/tests.rs` | Modify | Add parse tests for AC-01 through AC-04 (valid profile, missing section, missing fields, no flag) |
| `eval/report/tests_distribution_gate.rs` | Create | New test module: Distribution Gate render and aggregation tests for AC-05 through AC-14 plus R-03, R-07, R-12 scenarios |
| `docs/testing/eval-harness.md` | Modify | Document `distribution_change` flag, `[profile.distribution_targets]` sub-table, Distribution Gate Section 5 behavior, example TOML for PPR-class features, target value guidance |

### Implementation Order (hard constraint)

1. Create `eval/report/render_distribution_gate.rs` (empty boundary stub) — before any touch to `render.rs`
2. Pre-split `eval/report/aggregate.rs` → `eval/report/aggregate/mod.rs` — before creating `distribution.rs`
3. Add `DistributionTargets` and extend `EvalProfile` in `types.rs`; extend `parse_profile_toml` in `validation.rs`
4. Create `eval/runner/profile_meta.rs`; extend `eval/runner/mod.rs`
5. Create `eval/report/aggregate/distribution.rs`
6. Implement `render_distribution_gate_section` in `render_distribution_gate.rs`
7. Wire `render_report` dispatch and `load_profile_meta` in `report/mod.rs` and `render.rs`
8. Tests and documentation

---

## Data Structures

### `DistributionTargets` (`eval/profile/types.rs`)

Human-specified floor values. All three fields are required when `distribution_change = true`. No serde derives — in-memory type only.

```rust
pub struct DistributionTargets {
    pub cc_at_k_min: f64,
    pub icd_min: f64,
    pub mrr_floor: f64,
}
```

### `EvalProfile` extension (`eval/profile/types.rs`)

Two new fields appended to the existing struct:

```rust
pub distribution_change: bool,             // default false
pub distribution_targets: Option<DistributionTargets>,  // None when flag is false
```

### `ProfileMetaFile` / `ProfileMetaEntry` / `DistributionTargetsJson` (`eval/runner/profile_meta.rs`)

Serde-enabled types for `profile-meta.json`. Separate from `EvalProfile` / `DistributionTargets`.

```rust
#[derive(Serialize, Deserialize)]
pub struct ProfileMetaFile {
    pub version: u32,   // always 1; top-level field
    pub profiles: HashMap<String, ProfileMetaEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileMetaEntry {
    pub distribution_change: bool,
    pub distribution_targets: Option<DistributionTargetsJson>,
}

#[derive(Serialize, Deserialize)]
pub struct DistributionTargetsJson {
    pub cc_at_k_min: f64,
    pub icd_min: f64,
    pub mrr_floor: f64,
}
```

### `MetricGateRow` and `DistributionGateResult` (`eval/report/aggregate/distribution.rs`)

```rust
pub(super) struct MetricGateRow {
    pub target: f64,
    pub actual: f64,
    pub passed: bool,
}

pub(super) struct DistributionGateResult {
    pub cc_at_k: MetricGateRow,
    pub icd: MetricGateRow,
    pub mrr_floor: MetricGateRow,      // veto — separate from diversity
    pub diversity_passed: bool,        // cc_at_k.passed && icd.passed
    pub mrr_floor_passed: bool,        // mrr_floor.passed
    pub overall_passed: bool,          // diversity_passed && mrr_floor_passed
}
```

---

## Function Signatures

### `write_profile_meta` (`eval/runner/profile_meta.rs`)

```rust
pub fn write_profile_meta(profiles: &[EvalProfile], out: &Path) -> Result<(), EvalError>
```

Atomic write: serialize to `{out}/profile-meta.json.tmp`, then `fs::rename` to `profile-meta.json`. Falls back to `fs::copy` + `fs::remove_file` on cross-device rename failure.

### `check_distribution_targets` (`eval/report/aggregate/distribution.rs`)

```rust
pub(super) fn check_distribution_targets(
    stats: &AggregateStats,
    targets: &DistributionTargets,
) -> DistributionGateResult
```

Reads `stats.mean_cc_at_k`, `stats.mean_icd`, `stats.mean_mrr`. Compares against targets using `>=`. All comparisons against candidate stats — never baseline stats.

### `render_distribution_gate_section` (`eval/report/render_distribution_gate.rs`)

```rust
pub(super) fn render_distribution_gate_section(
    profile_name: &str,
    gate: &DistributionGateResult,
    baseline_stats: &AggregateStats,
    heading_level: HeadingLevel,   // Single (## 5.) or Multi (### 5.N)
) -> String
```

Emits: declaration notice, diversity target table (CC@k and ICD rows), diversity gate verdict, MRR floor table (gate row + "Baseline MRR (reference)" informational row from `baseline_stats.mean_mrr`), MRR floor verdict, overall PASSED/FAILED with distinguishable failure messages.

### `load_profile_meta` (`eval/report/mod.rs`, private)

```rust
fn load_profile_meta(dir: &Path) -> Result<HashMap<String, ProfileMetaEntry>, EvalError>
```

Returns `Ok(HashMap::new())` when `profile-meta.json` is absent (backward compat). Returns `Err(EvalError::...)` with message "profile-meta.json is malformed — re-run eval to regenerate" when the file is present but non-JSON or structurally invalid. Does not silently fall back on corrupt sidecar.

### `render_report` signature extension (`eval/report/render.rs`)

```rust
pub(super) fn render_report(
    stats: &[AggregateStats],
    phase_stats: &[PhaseAggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],
    profile_meta: &HashMap<String, ProfileMetaEntry>,  // new
) -> String
```

---

## Constraints

1. **Dual-type constraint (pattern #3574, #3550).** Zero fields added to `ScenarioResult` in `runner/output.rs` or `report/mod.rs`. Profile metadata lives exclusively in `profile-meta.json` sidecar.

2. **500-line file limit.** `render.rs` is at 499 lines; `aggregate.rs` is at 488 lines. Both module boundaries must be established before any feature code is added. New render code goes in `render_distribution_gate.rs`; new aggregation code goes in `aggregate/distribution.rs`.

3. **`[profile]` stripping order.** `distribution_change` and `distribution_targets` must be extracted from the raw TOML value before the `[profile]` section is removed for `UnimatrixConfig` deserialization.

4. **Parse-time validation only.** Validation of `distribution_targets` completeness must occur in `parse_profile_toml` before any `EvalServiceLayer` is constructed.

5. **`eval report` exits 0 always (C-07, FR-29).** Distribution Gate PASSED/FAILED appears in the report body only; it has no effect on the process exit code.

6. **`mrr_floor` is absolute.** Compared against `mean(mrr)` of the candidate profile, never against baseline MRR.

7. **Backward compatibility.** `eval report` against a results directory with no `profile-meta.json` must render Section 5 as "Zero-Regression Check" with no error. Absent file is the defined fallback; corrupt file is an abort.

8. **Baseline profile `distribution_change = true` is rejected.** `parse_profile_toml` must return `EvalError::ConfigInvariant` with message "baseline profile must not declare `distribution_change = true`".

9. **No `--distribution-change` CLI flag.** The declaration lives in the TOML exclusively.

---

## Dependencies

| Dependency | Kind | Role |
|------------|------|------|
| `serde_json` | Crate (already in workspace) | Serialize/deserialize `profile-meta.json` |
| `serde` | Crate (already in workspace) | Derive `Serialize`, `Deserialize` on sidecar types |
| `mean_cc_at_k`, `mean_icd`, `mean_mrr` in `AggregateStats` | Existing fields from nan-008 | Read by `check_distribution_targets`; no new computation |
| `EvalError::ConfigInvariant` | Existing error variant | Returned for invalid TOML profiles |
| `render_phase.rs` / `render_phase_section` | Existing module | Pattern reference for the new `render_distribution_gate.rs` split |
| `eval/profile/tests.rs` | Existing test file | Extended with parse tests for new fields |

---

## NOT in Scope

- Changes to eval runner replay logic or how CC@k and ICD are computed
- Changes to the baseline profile definition beyond the baseline-rejection error
- Auto-derivation of `cc_at_k_min`, `icd_min`, or `mrr_floor` from historical data
- Per-scenario distribution gate evaluation (gate operates on means only)
- Cross-profile distribution gate comparison
- Changes to Section 7 Distribution Analysis
- A `--distribution-change` CLI flag or any command-line override of TOML declarations
- Changes to `eval report` exit code semantics
- Changes to `ScenarioResult` fields in `runner/output.rs` or `report/mod.rs`

---

## Alignment Status

**Overall: 4 WARNs — all resolved during design review. No open variances.**

| Variance | Status | Resolution |
|----------|--------|------------|
| WARN-1: Baseline profile `distribution_change = true` error behavior left open in spec (OQ-01) | RESOLVED | Hard `ConfigInvariant` with message "baseline profile must not declare `distribution_change = true`". SPECIFICATION.md constraint 9 hedging language superseded by SCOPE.md Design Decision #7. Non-negotiable test `test_distribution_gate_baseline_rejected` asserts this exact behavior. |
| WARN-2: Baseline MRR reference row: scoped in SCOPE.md Design Decision #5 but absent from FR-12 and AC-08 | RESOLVED | Reference row is required. `render_distribution_gate_section` receives baseline `AggregateStats` as a second parameter. AC-08 verification detail updated to include the reference row assertion. |
| WARN-3: R-07 in RISK-TEST-STRATEGY contained a factual mischaracterisation of ARCHITECTURE.md Component 7 (described WARN+fallback where the architecture specifies abort) | RESOLVED | ARCHITECTURE.md Component 7 specifies abort + non-zero exit for corrupt sidecar. R-07 test scenarios are correct in intent; the "phantom conflict" language was noted as misleading. Implementation must follow the architecture: corrupt sidecar aborts, absent sidecar falls back. |
| WARN-4: Component 5 heading level example used `### 5.` unconditionally, contradicting the `## 5.` single-profile rule in Component 6 | RESOLVED | Single-profile: `## 5. Distribution Gate`. Multi-profile: `### 5.N Distribution Gate — {profile_name}`. Render loop must count non-baseline profiles before choosing heading level. |

---

## Non-Negotiable Test Names (gate-3b checklist)

The following test function names must exist in the delivered test files. Gate-3b verifies by grep.

In `eval/profile/tests.rs`:
- `test_parse_distribution_change_profile_valid`
- `test_parse_distribution_change_missing_targets`
- `test_parse_distribution_change_missing_cc_at_k`
- `test_parse_distribution_change_missing_icd`
- `test_parse_distribution_change_missing_mrr_floor`
- `test_parse_no_distribution_change_flag`

In `eval/report/tests_distribution_gate.rs`:
- `test_write_profile_meta_schema`
- `test_distribution_gate_section_header`
- `test_distribution_gate_table_content`
- `test_distribution_gate_pass_condition`
- `test_distribution_gate_mrr_floor_veto`
- `test_distribution_gate_distinct_failure_modes`
- `test_report_without_profile_meta_json`
- `test_check_distribution_targets_all_pass`
- `test_check_distribution_targets_cc_at_k_fail`
- `test_check_distribution_targets_icd_fail`
- `test_check_distribution_targets_mrr_floor_fail`
- `test_distribution_gate_baseline_rejected`
- `test_distribution_gate_corrupt_sidecar_aborts`
- `test_distribution_gate_exit_code_zero`
