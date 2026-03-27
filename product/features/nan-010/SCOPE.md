# nan-010: Distribution Change Profile Flag for Eval Harness

## Problem Statement

The eval harness Section 5 "Zero-Regression Check" compares candidate MRR and P@K against
the baseline profile using OR semantics: a scenario is flagged as a regression when either
MRR or P@K drops. This is the right gate for features that preserve or improve the existing
result distribution.

However, some features are *designed* to shift the result distribution rather than
preserve it. PPR (#398) re-ranks entries over positive relationship edges. Phase-conditioned
retrieval boosts phase-relevant categories. Contradicts suppression (#395) demotes entries
that contradict accepted knowledge. All three are expected to move MRR and P@K relative to
the distribution-preserving baseline — but that movement is the *intended effect*, not a
regression. Running the zero-regression check on these features produces false positives:
every intended distribution shift appears as a regression, drowning out any genuine
signal.

There is currently no mechanism to declare in a profile TOML that the candidate is
intentionally changing the distribution, and to substitute a distribution-quality gate
(CC@k and ICD targets) for the zero-regression check in that case. The harness therefore
gives incorrect guidance for an entire category of features.

## Goals

1. Add `distribution_change = true` boolean flag to the `[profile]` TOML section. When
   absent or `false`, existing behavior is unchanged.
2. Add a `[profile.distribution_targets]` TOML sub-table with three fields: `cc_at_k_min`,
   `icd_min`, and `mrr_floor`. All three are required when `distribution_change = true`
   (parse-time validation).
3. Extend `EvalProfile` struct in `eval/profile/types.rs` to carry the new fields:
   `distribution_change: bool` and `distribution_targets: Option<DistributionTargets>`.
4. Extend `parse_profile_toml` in `eval/profile/validation.rs` to extract and validate
   the new fields. Return `EvalError::ConfigInvariant` when `distribution_change = true`
   but `[profile.distribution_targets]` is absent or any required field is missing.
5. In `eval/report/aggregate.rs`, add `check_distribution_targets(results, targets)` that
   returns a `DistributionGateResult`: pass/fail, and per-metric actual vs. target values.
6. In `eval/report/render.rs`, replace the Section 5 render path when the candidate profile
   has `distribution_change = true`: emit "Distribution Gate" instead of "Zero-Regression
   Check", print the declaration notice, and show a target-vs-actual table.
7. Thread the profile metadata (specifically whether `distribution_change = true` and its
   targets) from the profile TOML through `eval run` into the result JSON, so `eval report`
   can read the intent without needing to re-parse the original TOML files.
8. Update `docs/testing/eval-harness.md` to document the `distribution_change` flag,
   `[profile.distribution_targets]` sub-table, the Distribution Gate behavior, and example
   profile TOML for PPR-class features.

## Non-Goals

- **No changes to eval runner replay logic.** The `distribution_change` flag is a reporting
  and gate-selection concern. It has no effect on how scenarios are replayed or how CC@k and
  ICD are computed during `eval run`. Those metrics are already computed for every profile.
- **No change to the baseline profile.** The baseline profile never has `distribution_change
  = true`. The distribution gate is a candidate-only concept. Applying it to the baseline
  would be semantically meaningless and is not supported.
- **No change to `eval report` exit code.** The report exits 0 regardless of Distribution
  Gate outcome. This is an existing invariant (C-07, FR-29) that is not relaxed by this
  feature.
- **No target auto-derivation.** The `cc_at_k_min`, `icd_min`, and `mrr_floor` values are
  human-specified in the TOML. The harness does not compute or suggest target values.
- **No per-scenario target evaluation.** The distribution gate evaluates `mean(cc_at_k)`,
  `mean(icd)`, and `mean(mrr)` over all scenarios, not per-scenario. Per-scenario
  distribution gates are a future concern.
- **No multi-profile distribution gates.** When multiple candidate profiles are present, the
  distribution gate is applied independently per profile that declares `distribution_change
  = true`. Cross-profile comparison is not added.
- **No changes to Section 7 Distribution Analysis.** Section 7 already shows CC@k and ICD
  range tables. The Distribution Gate in Section 5 is a new pass/fail gate, not a
  replacement for Section 7.
- **No `--distribution-change` CLI flag.** The declaration lives in the TOML; there is no
  command-line override.

## Background Research

### CC@k and ICD are already fully computed (nan-008)

`eval/runner/metrics.rs` contains `compute_cc_at_k` and `compute_icd`. Both are called
from `replay.rs` for every profile on every scenario. The results flow into `ProfileResult`
as `cc_at_k: f64` and `icd: f64`, are serialized into the per-scenario JSON by
`eval run`, and are deserialized by `eval report`. Mean CC@k and mean ICD are already
computed in `compute_aggregate_stats` (aggregate.rs) and rendered in Section 1 Summary and
Section 7. The distribution gate needs only to compare those already-computed means against
human-specified floor values. No new computation is required in the runner.

### Section 5 render path is a standalone block in render.rs

`eval/report/render.rs`, lines 182–212: Section 5 is rendered as a single `if/else` block
on `regressions.is_empty()`. The block is self-contained and does not share logic with
other sections. Replacing it conditionally — based on whether the candidate has
`distribution_change = true` — is a low-risk, localized change.

`find_regressions` in `aggregate.rs` must still run when `distribution_change = false`
(unchanged behavior). When `distribution_change = true`, `find_regressions` output is
ignored for Section 5 rendering (the distribution gate is shown instead).

### EvalProfile currently carries no distribution metadata

`eval/profile/types.rs` — `EvalProfile` has three fields: `name: String`,
`description: Option<String>`, `config_overrides: UnimatrixConfig`. There is no flag for
distribution intent.

### parse_profile_toml strips the [profile] section before deserializing UnimatrixConfig

`eval/profile/validation.rs` lines 85–102: `parse_profile_toml` removes `[profile]` from
the raw TOML value, then deserializes the remainder into `UnimatrixConfig`. The
`[profile.distribution_targets]` sub-table is nested under `[profile]`, so it is removed
along with `[profile].name`, `[profile].description`, etc. The new fields must be extracted
from the raw TOML *before* the `[profile]` section is stripped, exactly like `name` and
`description` are today (lines 66–80).

### Profile metadata must reach eval report without re-reading TOML files

`eval report` reads only the per-scenario JSON result files in `--results`. It does not
have access to the original profile TOML paths. The `distribution_change` flag and targets
must be embedded in the result JSON so the report can gate correctly. The cleanest place
is a top-level metadata field in `ScenarioResult` (or a new per-run metadata file in the
output directory).

There are two viable approaches:
1. Add `profile_metadata: HashMap<String, ProfileMeta>` to `ScenarioResult` — metadata
   per profile keyed by profile name.
2. Write a separate `profile-meta.json` file to the output directory during `eval run`,
   which `eval report` reads alongside the result files.

Option 2 (separate metadata file) avoids touching the dual-type constraint on
`ScenarioResult` for this new concern, and keeps `ScenarioResult` focused on per-scenario
data. It introduces a new file the report must load — but the report already does optional
JSONL loading (`--scenarios`). This is the recommended approach.

### Dual-type constraint (pattern #3574)

`runner/output.rs` and `report/mod.rs` maintain independent copies of result types
(`ScenarioResult`, `ProfileResult`, `ComparisonMetrics`, `RankChange`, `ScoredEntry`).
Both copies must be kept in sync when fields are added. If the profile metadata is carried
via a separate file (option 2 above), the dual-type constraint is not triggered by this
feature.

### The [profile.distribution_targets] TOML nesting

The proposed TOML shape nests `distribution_targets` under `[profile]`:

```toml
[profile]
name = "ppr-candidate"
description = "Full PPR over positive edges"
distribution_change = true

[profile.distribution_targets]
cc_at_k_min = 0.60
icd_min = 1.20
mrr_floor = 0.35
```

In TOML, `[profile.distribution_targets]` is a sub-table of `[profile]`. When
`parse_profile_toml` reads `raw.get("profile")`, it gets a `toml::Value::Table` containing
`name`, `description`, `distribution_change`, and `distribution_targets` (which is itself
a sub-table). The extraction follows the same pattern already used for `name` and
`description`.

### Existing test infrastructure

`eval/profile/tests.rs` tests `parse_profile_toml` with TOML written to a `TempDir`. The
same pattern applies for testing the new fields. Tests for the distribution gate check
belong in a new `report/tests_distribution_gate.rs` (following the pattern established by
`tests_distribution.rs`, `tests_phase.rs`, etc.).

### 500-line file limit

`eval/report/render.rs` is currently 499 lines. Adding the Distribution Gate render path
to this file would exceed the limit. The distribution gate rendering should go in a new
`render_distribution_gate.rs` sibling module, following the same split used for
`render_phase.rs` (section 6 rendering).

## Proposed Approach

### Change 1 — New types

**`eval/profile/types.rs`**: Add `DistributionTargets { cc_at_k_min: f64, icd_min: f64,
mrr_floor: f64 }` struct. Extend `EvalProfile` with `distribution_change: bool` (default
`false`) and `distribution_targets: Option<DistributionTargets>`.

### Change 2 — Profile TOML parsing

**`eval/profile/validation.rs`**: In `parse_profile_toml`, after extracting `name` and
`description`, extract `distribution_change` and `distribution_targets` from the raw
`[profile]` table. Return `EvalError::ConfigInvariant` if `distribution_change = true` and
`[profile.distribution_targets]` is absent or missing any required field.

### Change 3 — Profile metadata file

**`eval/runner/mod.rs`** (or a new `eval/runner/profile_meta.rs`): After layer construction
in `run_eval_async`, write a `profile-meta.json` file to the output directory containing a
map of profile name to `{ distribution_change: bool, distribution_targets: Option<...> }`.
This file is read by `eval report` to determine gating mode per profile.

### Change 4 — Distribution gate aggregation

**`eval/report/aggregate.rs`**: Add `check_distribution_targets(stats, targets)` that takes
the per-profile `AggregateStats` (which already contains `mean_cc_at_k`, `mean_icd`,
`mean_mrr`) and a `DistributionTargets`, and returns a `DistributionGateResult` with:
pass/fail boolean, and per-metric `(target, actual, passed)` triples.

### Change 5 — Section 5 conditional render

**`eval/report/render_distribution_gate.rs`** (new file): `render_distribution_gate_section`
function. Renders Section 5 "Distribution Gate" with declaration notice and
target-vs-actual table.

**`eval/report/render.rs`**: In `render_report`, load `profile-meta.json` from the results
directory (or receive it as a parameter). For each candidate profile, check whether
`distribution_change = true`. If so, delegate Section 5 to `render_distribution_gate_section`
instead of the zero-regression path.

**`eval/report/mod.rs`**: Pass the profile metadata map to `render_report`.

### Change 6 — Documentation

**`docs/testing/eval-harness.md`**: Add a new subsection to "Writing profile TOMLs"
documenting `distribution_change` and `[profile.distribution_targets]`. Extend "Reading
the report" Section 5 entry to describe the two possible modes (Zero-Regression Check vs.
Distribution Gate). Add a table entry to "Safety constraints".

## Acceptance Criteria

- AC-01: A profile TOML with `distribution_change = true` and a valid
  `[profile.distribution_targets]` table is parsed successfully by `parse_profile_toml`,
  producing an `EvalProfile` with `distribution_change = true` and
  `distribution_targets = Some(DistributionTargets { ... })`.
- AC-02: A profile TOML with `distribution_change = true` and no
  `[profile.distribution_targets]` table is rejected by `parse_profile_toml` with
  `EvalError::ConfigInvariant` naming the missing section.
- AC-03: A profile TOML with `distribution_change = true` and a
  `[profile.distribution_targets]` table missing any of the three required fields
  (`cc_at_k_min`, `icd_min`, `mrr_floor`) is rejected with `EvalError::ConfigInvariant`
  naming the missing field.
- AC-04: A profile TOML with no `distribution_change` key (or `distribution_change =
  false`) is parsed with `distribution_change = false` and `distribution_targets = None`,
  and all existing behavior is unchanged.
- AC-05: `eval run` writes a `profile-meta.json` file to the `--out` directory containing
  the `distribution_change` flag and targets (or `null`) for each profile name.
- AC-06: When `distribution_change = false` (or absent), `eval report` Section 5 renders
  as "Zero-Regression Check" with existing regression logic — no behavioral change.
- AC-07: When `distribution_change = true` for the candidate profile, `eval report`
  Section 5 is titled "Distribution Gate" and prints: "Distribution change declared.
  Evaluating against CC@k and ICD targets."
- AC-08: The Distribution Gate table shows `cc_at_k`, `icd`, and `mrr` rows, each with
  columns: metric name, target value, actual value (mean over all scenarios), and pass/fail.
- AC-09: Distribution gate pass condition: `mean(cc_at_k) >= cc_at_k_min` AND
  `mean(icd) >= icd_min`. The MRR floor (`mean(mrr) >= mrr_floor`) is a separate veto
  rendered as its own line. The report shows "PASSED" only when both the distribution gate
  and the MRR floor pass.
- AC-10: Fail condition: distribution gate fails (CC@k or ICD target missed) OR MRR floor
  breached. The two failure modes are reported separately — "Diversity targets met, but
  ranking floor breached" is distinguishable from "Diversity targets not met."
- AC-11: When `profile-meta.json` is absent from the results directory (pre-nan-010 result
  sets), `eval report` treats all profiles as `distribution_change = false` and renders
  Section 5 as "Zero-Regression Check" — fully backward-compatible.
- AC-12: `docs/testing/eval-harness.md` documents the `distribution_change` flag, the
  `[profile.distribution_targets]` sub-table, the Distribution Gate Section 5 behavior,
  the example TOML for PPR-class features, and how to choose `cc_at_k_min`/`icd_min`/
  `mrr_floor` values.
- AC-13: Unit tests cover: successful parse of a distribution-change profile; rejection
  when targets section is missing; rejection when a required target field is missing;
  `check_distribution_targets` returns pass when all targets met; returns fail with correct
  per-metric detail when any target is missed; MRR floor fail case.
- AC-14: Unit tests cover the backward-compatibility path: `eval report` against a results
  directory with no `profile-meta.json` renders Section 5 as zero-regression check.

## Constraints

1. **Dual-type constraint (pattern #3574).** If any field is added to `ScenarioResult`
   (runner/output.rs), the corresponding field must be added to report/mod.rs's local
   copy with `#[serde(default)]`. Profile metadata carried via a separate `profile-meta.json`
   file avoids triggering this constraint. If the design changes and metadata is embedded
   in `ScenarioResult`, both copies must be updated in sync.

2. **Report exits 0 always (C-07, FR-29).** The Distribution Gate does not change this
   invariant. PASSED/FAILED is printed in the report body; the process exit code remains 0.

3. **500-line file limit (Rust workspace rule).** `render.rs` is currently at 499 lines.
   Distribution Gate rendering must go in a new `render_distribution_gate.rs` module.
   `aggregate.rs` is 488 lines; the new aggregation helper must respect the limit or force
   a module split.

4. **`[profile]` stripping in parse_profile_toml.** The parser removes the `[profile]`
   section before deserializing `UnimatrixConfig`. `distribution_change` and
   `distribution_targets` must be extracted from the raw TOML value *before* the `[profile]`
   section is removed, following the existing pattern for `name` and `description`.

5. **Backward compatibility for result directories.** `eval report` must not fail or change
   behavior when run against result directories produced before nan-010 (no
   `profile-meta.json`). All new behavior is gated on the presence and content of that file.

6. **Parse-time validation.** All distribution target validation (missing section, missing
   fields) must occur in `parse_profile_toml`, not at report time. If a TOML is invalid,
   `eval run` must fail immediately with a user-readable message before constructing any
   `EvalServiceLayer`.

7. **`mrr_floor` is an absolute floor, not a delta.** The MRR floor in the distribution
   targets is compared against `mean(mrr)` of the candidate profile directly. It is not
   compared against baseline MRR. This is intentional: a distribution-change feature may
   have lower MRR than the baseline (expected) but must not fall below an absolute quality
   floor.

## Design Decisions (resolved)

### Architecture decisions (Phase 2a)

5. **Distribution Gate table includes a "Baseline MRR (reference)" row.**
   Without it, `mrr_floor` values are set blind. Label clearly as "Baseline MRR (reference)"
   — informational only, not a gate criterion. `render_distribution_gate_section` takes
   baseline `AggregateStats` as a second parameter.

6. **Multi-candidate Section 5 heading structure: `## 5.` parent + `### 5.N` children.**
   The `## 5. Distribution Gate` parent heading is always present as the section anchor
   (stable for CI tooling and deep links). `### 5.N — {profile_name}` sub-blocks appear
   as children in multi-candidate runs. Single-candidate runs omit the sub-heading.

7. **Baseline profile with `distribution_change = true` → `ConfigInvariant` at parse time.**
   Silent ignore is the wrong tradeoff. If it lands on the baseline by mistake, the user
   gets the wrong gate silently. Hard error with message "baseline profile must not declare
   `distribution_change = true`" — clear, immediate, fixable.

8. **Corrupt `profile-meta.json` → surface as error, abort, exit non-zero.**
   Silent fallback to zero-regression is a correctness hazard: you don't know whether the
   run was distribution-gated or not. The report fails with "profile-meta.json is malformed
   — re-run eval to regenerate" and exits non-zero. The results directory is the artifact
   boundary; a corrupt meta file means the artifact is invalid.

### Scope decisions (Phase 1)

1. **Section 5 rendering with multiple candidate profiles — per-profile independence.**
   Each profile gets its own Section 5 gate on its own terms. A profile declaring
   `distribution_change = true` gets a Distribution Gate table; one that does not gets
   zero-regression rows. A global Section 5 that flattens both modes hides whether the
   distribution-changing profile passed — per-profile is the honest rendering.

2. **`profile-meta.json` version field — include `"version": 1` from the start.**
   Costs nothing now; avoids the messier "absent field = version 1" inference later.

3. **`mrr_floor` is a veto, not a co-equal target.**
   CC@k and ICD measure whether diversity improved. `mrr_floor` measures whether ranking
   quality didn't collapse. They are orthogonal: diversity can improve by shuffling the
   ranking and tanking MRR. The floor semantics — "distribution gate passes only if MRR
   doesn't fall below threshold" — are the accurate ones. The report renders the
   distribution gate result (CC@k + ICD pass/fail) first, then the MRR floor as a separate
   line that can independently block. "Diversity targets met, but ranking regressed" is a
   different failure mode than "diversity targets not met."

4. **`run_report` reads `profile-meta.json` itself from the results directory.**
   The results directory is the artifact boundary — everything needed to reproduce the
   report must live there. Passing it via `run_eval_command` creates a coupling where the
   report can't be re-rendered from the results dir alone (e.g., CI artifacts, post-hoc
   analysis). Option (a) keeps the report self-contained.

## Tracking

GH Issue: #402
