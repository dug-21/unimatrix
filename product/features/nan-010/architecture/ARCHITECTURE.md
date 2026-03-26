# nan-010: Distribution Change Profile Flag — Architecture

## System Overview

The eval harness (`eval/`) in `unimatrix-server` is a two-command system: `eval run` replays
scenarios and writes per-scenario JSON files; `eval report` reads those files and produces a
Markdown report. The commands are decoupled — `eval report` operates purely from the result
directory with no runtime connection to `eval run`.

nan-010 adds the ability for a candidate profile TOML to declare `distribution_change = true`,
which replaces Section 5 "Zero-Regression Check" with a "Distribution Gate" that evaluates
mean CC@k, mean ICD, and mean MRR against human-specified floor values. When
`distribution_change` is absent or false, no behavior changes.

The feature spans three subsystems: profile parsing (`eval/profile/`), the runner output path
(`eval/runner/`), and the report rendering pipeline (`eval/report/`).

## Component Breakdown

### Component 1 — Profile Types (`eval/profile/types.rs`)

Adds two new types and extends `EvalProfile`:

- **`DistributionTargets`** — three `f64` floor values: `cc_at_k_min`, `icd_min`, `mrr_floor`.
- **`EvalProfile`** — gains `distribution_change: bool` (default `false`) and
  `distribution_targets: Option<DistributionTargets>`.

`DistributionTargets` carries no serde derives — it lives only in memory and in
`profile-meta.json` (written by a distinct `ProfileMeta` type in the runner; see Component 3).
Keeping the profile types free of serde is consistent with the existing pattern where
`UnimatrixConfig` is parsed from TOML but not written back.

### Component 2 — Profile Validation (`eval/profile/validation.rs`)

Extends `parse_profile_toml` to extract the two new fields from the raw TOML *before* the
`[profile]` section is stripped. Extraction follows the identical pattern already used for
`name` and `description` (lines 66–80 today): read from `raw.get("profile")`, validate, then
strip `[profile]` before deserializing the remainder as `UnimatrixConfig`.

Validation rules:
- `distribution_change = true` with no `[profile.distribution_targets]` → `EvalError::ConfigInvariant`
- `distribution_change = true` with any of the three target fields missing →
  `EvalError::ConfigInvariant` naming the missing field.
- `distribution_change` absent or `false` → `distribution_change: false`,
  `distribution_targets: None`, no error.

### Component 3 — Profile Metadata Sidecar (`eval/runner/profile_meta.rs`, new)

A new module in `eval/runner/` responsible for producing and writing `profile-meta.json`.

Types (serde-enabled, separate from `EvalProfile`):

```rust
#[derive(Serialize, Deserialize)]
pub struct DistributionTargetsJson {
    pub cc_at_k_min: f64,
    pub icd_min: f64,
    pub mrr_floor: f64,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileMetaEntry {
    pub distribution_change: bool,
    pub distribution_targets: Option<DistributionTargetsJson>,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileMetaFile {
    pub version: u32,  // always 1 (ADR-002)
    pub profiles: HashMap<String, ProfileMetaEntry>,
}
```

Responsibility: given the list of `EvalProfile` instances (post-parse, pre-run), produce a
`ProfileMetaFile`, serialize to JSON, and write atomically to `{out}/profile-meta.json`.

Atomic write protocol (SR-01 mitigation): write to `{out}/profile-meta.json.tmp`, then
`fs::rename` to `{out}/profile-meta.json`. If `rename` fails (e.g., cross-device), fall back
to `fs::copy` + `fs::remove_file`. This ensures a partial run (crash after result files, before
meta flush) leaves no corrupt sidecar — the old file is absent, and `eval report` falls back
to backward-compat mode (AC-11).

Write location: after all scenario replay completes in `run_eval_async`, before returning.
This ensures the file is written only when the run reaches completion. A partially written
`profile-meta.json.tmp` that was never renamed is ignored by `eval report` (the report reads
only `profile-meta.json`, not `.tmp`).

### Component 4 — Distribution Gate Aggregation (`eval/report/aggregate/distribution.rs`, new)

The 500-line constraint on `aggregate.rs` (currently 488 lines) requires a module split before
adding new code. Pre-split plan (SR-02 mitigation, ADR-001):

- `eval/report/aggregate/mod.rs` — re-exports everything currently public; contains or delegates
  to the four existing functions: `compute_aggregate_stats`, `find_regressions`,
  `compute_latency_buckets`, `compute_entry_rank_changes`, `compute_cc_at_k_scenario_rows`,
  `compute_phase_stats`, and the private helper `baseline_metrics`.
- `eval/report/aggregate/distribution.rs` — new file, contains only
  `check_distribution_targets`.

`check_distribution_targets` signature:

```rust
pub(super) fn check_distribution_targets(
    stats: &AggregateStats,
    targets: &DistributionTargets,
) -> DistributionGateResult
```

`DistributionGateResult` (defined in `aggregate/distribution.rs`, re-exported via
`aggregate/mod.rs`):

```rust
pub(super) struct MetricGateRow {
    pub target: f64,
    pub actual: f64,
    pub passed: bool,
}

pub(super) struct DistributionGateResult {
    pub cc_at_k: MetricGateRow,
    pub icd: MetricGateRow,
    pub mrr_floor: MetricGateRow,  // veto — evaluated separately (ADR-003)
    pub diversity_passed: bool,    // cc_at_k.passed && icd.passed
    pub mrr_floor_passed: bool,    // mrr_floor.passed
    pub overall_passed: bool,      // diversity_passed && mrr_floor_passed
}
```

The `mrr_floor` row is a structurally separate field (not folded into the same pass/fail as
CC@k and ICD), reflecting ADR-003's veto semantics. `check_distribution_targets` takes
`mean_mrr`, `mean_cc_at_k`, and `mean_icd` directly from `AggregateStats` — no new
computation.

### Component 5 — Distribution Gate Renderer (`eval/report/render_distribution_gate.rs`, new)

Follows the same split pattern as `render_phase.rs` (SR-03 mitigation, ADR-001).

`render.rs` is at 499 lines — no new code may be added to that file before extracting the
boundary. The extraction is: add `mod render_distribution_gate;` and
`use render_distribution_gate::render_distribution_gate_section;` to `render.rs`, then
implement `render_distribution_gate_section` in the new sibling module.

`render_distribution_gate_section` signature (includes baseline stats for the reference row):

```rust
pub(super) fn render_distribution_gate_section(
    profile_name: &str,
    gate: &DistributionGateResult,
    baseline_stats: &AggregateStats,
) -> String
```

Rendered output structure for Section 5 — single-profile run (`## 5.`):

```
## 5. Distribution Gate

Distribution change declared. Evaluating against CC@k and ICD targets.

| Metric | Target | Actual | Result |
|--------|--------|--------|--------|
| CC@k   | ≥ 0.60 | 0.6234 | PASSED |
| ICD    | ≥ 1.20 | 1.3101 | PASSED |

**Diversity gate: PASSED**

MRR floor (veto):

| Metric | Floor | Actual | Result |
|--------|-------|--------|--------|
| MRR    | ≥ 0.35 | 0.3812 | PASSED |
| Baseline MRR (reference) | — | 0.5103 | — |

**Overall: PASSED**
```

Multi-profile run uses `## 5. Distribution Gate` as the section anchor, with `### 5.N — {profile_name}` children for each candidate profile. Single-profile runs omit the sub-heading — the `## 5.` heading is the only heading for that section.

When diversity fails: "Diversity targets not met." When MRR floor fails independently:
"Diversity targets met, but ranking floor breached." Both failure modes are distinguishable
in the output (AC-10).

### Component 6 — Section 5 Dispatch in `render_report` (`eval/report/render.rs`)

`render_report` receives a new parameter:

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

Section 5 render loop: for each non-baseline profile in `stats`, check
`profile_meta.get(profile_name)`. If `distribution_change = true` and targets are present,
call `render_distribution_gate_section`. Otherwise, render the existing zero-regression block.

The zero-regression block is unchanged for profiles that do not declare `distribution_change`.
`find_regressions` output is still computed for all profiles (no change to existing logic).
For distribution-change profiles, the regressions slice is not used in Section 5 but remains
available (it does not cause harm to compute it).

For the multi-profile case (SR-05): Section 5 is rendered once per non-baseline profile, each
independently gated. The section header becomes `### 5. Distribution Gate — {profile_name}`
or `### 5. Zero-Regression Check — {profile_name}` when multiple candidates are present. When
only one candidate profile exists, the header is `## 5. Distribution Gate` or
`## 5. Zero-Regression Check` (consistent with current single-profile format).

### Component 7 — `run_report` Sidecar Load (`eval/report/mod.rs`)

`run_report` gains a new step between aggregation and rendering: read `profile-meta.json` from
the results directory if it exists. If absent → `HashMap::new()` (backward-compat, AC-11).
If present but malformed JSON → return `EvalError` with message "profile-meta.json is malformed
— re-run eval to regenerate", abort the report, and exit non-zero. Silent fallback on a corrupt
sidecar is a correctness hazard: the operator would see a Zero-Regression Check when a
Distribution Gate was expected, with no indication anything is wrong. (SCOPE.md Design Decision
#8; R-07 resolution.)

```rust
// Step 3.5: Load optional profile-meta.json (nan-010).
let profile_meta = load_profile_meta(results)?;
```

`load_profile_meta(dir: &Path) -> Result<HashMap<String, ProfileMetaEntry>, EvalError>` is a
private helper in `mod.rs`. Returns `Ok(HashMap::new())` when the file is absent (backward
compat). Returns `Err(EvalError::...)` when the file is present but malformed.

## Component Interactions

```
eval run:
  parse_profile_toml()          [profile/validation.rs]
    └─ extracts DistributionTargets, validates, returns EvalProfile
  run_eval_async()              [runner/mod.rs]
    └─ after replay: write_profile_meta(profiles, out)  [runner/profile_meta.rs]
         └─ atomic write → {out}/profile-meta.json

eval report:
  run_report()                  [report/mod.rs]
    ├─ load result JSONs         (no change — ScenarioResult unchanged)
    ├─ load_profile_meta(dir)   → HashMap<String, ProfileMetaEntry>
    ├─ compute_aggregate_stats() [report/aggregate/mod.rs]
    ├─ check_distribution_targets() [report/aggregate/distribution.rs]
    │    └─ takes AggregateStats + DistributionTargets from profile_meta
    └─ render_report(..., profile_meta)  [report/render.rs]
         └─ Section 5 per profile:
              distribution_change=true  → render_distribution_gate_section()
                                            [report/render_distribution_gate.rs]
              distribution_change=false → existing zero-regression block
```

## Technology Decisions

- **Sidecar file over ScenarioResult field** — see ADR-002. Zero changes to the dual-type
  `ScenarioResult` copies in `runner/output.rs` and `report/mod.rs`.
- **Atomic rename for sidecar write** — see ADR-004. Prevents silent backward-compat fallback
  on partial run (SR-01 mitigation).
- **Module pre-split for aggregate.rs and render.rs** — see ADR-001. Both files are at the
  500-line boundary; new code cannot be added inline.
- **`mrr_floor` as veto, not co-equal target** — see ADR-003. Structurally separate in
  `DistributionGateResult`.
- **Per-profile Section 5 rendering** — see ADR-005. Each candidate profile gets its own
  independently gated Section 5 block.
- **`profile-meta.json` version field** — see ADR-002. Always `"version": 1` from the start.

## Integration Points

| Dependency | Nature | Impact |
|------------|--------|--------|
| `EvalProfile` in `eval/profile/types.rs` | Extended with 2 new fields | Profile construction sites must handle new fields |
| `parse_profile_toml` in `eval/profile/validation.rs` | Extended extraction | Existing tests remain valid; new test cases added |
| `run_eval_async` in `eval/runner/mod.rs` | New write step after replay | One new call to `write_profile_meta` |
| `AggregateStats` in `eval/report/mod.rs` | Read-only; no new fields | `check_distribution_targets` reads existing `mean_mrr`, `mean_cc_at_k`, `mean_icd` |
| `ScenarioResult` in both `runner/output.rs` and `report/mod.rs` | ZERO changes | Sidecar approach preserves dual-type invariant |
| `render_report` in `eval/report/render.rs` | New parameter, Section 5 dispatch | One new parameter; no structural change to other sections |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `DistributionTargets` | `{ cc_at_k_min: f64, icd_min: f64, mrr_floor: f64 }` | `eval/profile/types.rs` (new) |
| `EvalProfile::distribution_change` | `bool` (default `false`) | `eval/profile/types.rs` (extended) |
| `EvalProfile::distribution_targets` | `Option<DistributionTargets>` | `eval/profile/types.rs` (extended) |
| `ProfileMetaEntry` | `{ distribution_change: bool, distribution_targets: Option<DistributionTargetsJson> }` | `eval/runner/profile_meta.rs` (new) |
| `ProfileMetaFile` | `{ version: u32, profiles: HashMap<String, ProfileMetaEntry> }` | `eval/runner/profile_meta.rs` (new) |
| `write_profile_meta(profiles, out)` | `fn(&[EvalProfile], &Path) -> Result<(), ...>` | `eval/runner/profile_meta.rs` (new) |
| `DistributionGateResult` | `{ cc_at_k: MetricGateRow, icd: MetricGateRow, mrr_floor: MetricGateRow, diversity_passed: bool, mrr_floor_passed: bool, overall_passed: bool }` | `eval/report/aggregate/distribution.rs` (new) |
| `MetricGateRow` | `{ target: f64, actual: f64, passed: bool }` | `eval/report/aggregate/distribution.rs` (new) |
| `check_distribution_targets(stats, targets)` | `fn(&AggregateStats, &DistributionTargets) -> DistributionGateResult` | `eval/report/aggregate/distribution.rs` (new) |
| `render_distribution_gate_section(profile_name, gate)` | `fn(&str, &DistributionGateResult) -> String` | `eval/report/render_distribution_gate.rs` (new) |
| `load_profile_meta(dir)` | `fn(&Path) -> HashMap<String, ProfileMetaEntry>` | `eval/report/mod.rs` (new private helper) |
| `profile-meta.json` schema | `{ "version": 1, "profiles": { "<name>": { "distribution_change": bool, "distribution_targets": null | { "cc_at_k_min": f64, "icd_min": f64, "mrr_floor": f64 } } } }` | Written by runner, read by report |

## File Change Map

| File | Action | Constraint |
|------|--------|------------|
| `eval/profile/types.rs` | Extend — add `DistributionTargets`, extend `EvalProfile` | — |
| `eval/profile/validation.rs` | Extend — extract new fields before `[profile]` strip | Extract before strip (SR-07) |
| `eval/runner/profile_meta.rs` | New — `ProfileMetaFile`, `ProfileMetaEntry`, `write_profile_meta` | Atomic write required (SR-01) |
| `eval/runner/mod.rs` | Extend — call `write_profile_meta` after replay | One call site |
| `eval/report/aggregate.rs` → `eval/report/aggregate/mod.rs` | Pre-split — rename to mod.rs, no logic change | Must happen before adding new code (SR-02) |
| `eval/report/aggregate/distribution.rs` | New — `check_distribution_targets`, `DistributionGateResult`, `MetricGateRow` | New file only; no changes to mod.rs logic |
| `eval/report/render_distribution_gate.rs` | New — `render_distribution_gate_section` | Created before any changes to render.rs (SR-03) |
| `eval/report/render.rs` | Extend — add `mod`, new param, Section 5 dispatch | No line additions until boundary module exists (SR-03) |
| `eval/report/mod.rs` | Extend — `load_profile_meta`, pass to `render_report`, declare new modules | — |
| `eval/profile/tests.rs` | Extend — new parse cases for AC-01, AC-02, AC-03, AC-04 | — |
| `eval/report/tests_distribution_gate.rs` | New — gate check and render tests (AC-05–AC-14) | New sibling test file pattern |
| `docs/testing/eval-harness.md` | Extend — document `distribution_change`, example TOML | AC-12 |

## Implementation Order Constraints

The following ordering is a hard constraint, not a preference:

1. **Create `render_distribution_gate.rs`** (empty stub with correct module boundary) — before
   touching `render.rs`. Any edit to `render.rs` that adds even one line will breach 500.
2. **Pre-split `aggregate.rs` → `aggregate/mod.rs`** — before writing `distribution.rs`.
3. **Add `DistributionTargets`, extend `EvalProfile`, extend `parse_profile_toml`** — profile
   types must exist before runner or report can reference them.
4. **Add `profile_meta.rs` to runner** — depends on `EvalProfile` having the new fields.
5. **Add `distribution.rs` aggregation** — depends on `AggregateStats` shape (unchanged) and
   `DistributionTargets` type.
6. **Wire `render_report` dispatch and `run_report` sidecar load** — depends on all above.

## Resolved Design Decisions

- **OQ-01 (resolved): Include "Baseline MRR (reference)" row.** The Distribution Gate table
  always includes a "Baseline MRR (reference)" informational row so users can calibrate
  `mrr_floor` values. Clearly labelled as informational — not a gate criterion. This requires
  `render_distribution_gate_section` to receive the baseline `AggregateStats` as a second
  parameter (see Component 5 signature above).

- **OQ-02 (resolved): `## 5.` anchor always present; `### 5.N` children for multi-candidate.**
  The `## 5. Distribution Gate` (or `## 5. Zero-Regression Check`) parent heading appears in
  all runs as the stable section anchor for CI tooling and deep links. `### 5.N — {profile_name}`
  sub-blocks appear as children only in multi-candidate runs. Single-candidate runs omit the
  sub-heading.

- **OQ-03 (resolved): Baseline profile with `distribution_change = true` → `ConfigInvariant`.**
  Hard reject at parse time in `parse_profile_toml`. Error message: "baseline profile must not
  declare `distribution_change = true`". Silent ignore is ruled out — it would apply the wrong
  gate without signalling the misconfiguration.
