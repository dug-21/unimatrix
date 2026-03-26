# SPECIFICATION — nan-010: Distribution Change Profile Flag for Eval Harness

GH Issue: #402

---

## Objective

Some eval profiles are designed to shift the retrieval result distribution rather than preserve
it. Running the zero-regression check against these profiles produces false positives — every
intended distribution shift appears as a regression — making the gate meaningless for that
class of feature. This feature adds a `distribution_change` boolean to the profile TOML and,
when set, replaces Section 5 "Zero-Regression Check" with a "Distribution Gate" that evaluates
CC@k, ICD, and an MRR floor instead.

---

## Functional Requirements

### FR-01 — `distribution_change` flag in profile TOML
A profile TOML may include `distribution_change = true` (boolean) in the `[profile]` section.
When absent or `false`, all existing behavior is unchanged.

**Testable by**: AC-01, AC-04.

### FR-02 — `[profile.distribution_targets]` sub-table required when flag is set
When `distribution_change = true`, the `[profile]` section must contain a
`[profile.distribution_targets]` sub-table with exactly three required fields:
`cc_at_k_min: f64`, `icd_min: f64`, `mrr_floor: f64`. All three are mandatory; no defaults
are inferred.

**Testable by**: AC-01, AC-02, AC-03.

### FR-03 — Parse-time validation, not report-time
`parse_profile_toml` (in `eval/profile/validation.rs`) must validate the presence and
completeness of `[profile.distribution_targets]` when `distribution_change = true`. If the
sub-table is absent, or any of the three required fields is missing, the function must return
`EvalError::ConfigInvariant` with a message identifying the missing section or field. Validation
must occur before any `EvalServiceLayer` is constructed.

**Testable by**: AC-02, AC-03, SR-07 regression.

### FR-04 — Extraction before `[profile]` stripping
`distribution_change` and `distribution_targets` must be extracted from the raw TOML value
before the `[profile]` section is removed for `UnimatrixConfig` deserialization. This follows
the same pattern as `name` and `description` extraction (lines 66–80 of `validation.rs`).

**Testable by**: AC-01 (parse-round-trip), AC-03 (validates extraction is complete).

### FR-05 — `EvalProfile` type extension
`eval/profile/types.rs` must define:
- `DistributionTargets { cc_at_k_min: f64, icd_min: f64, mrr_floor: f64 }` — a new struct.
- `EvalProfile.distribution_change: bool` — defaults to `false`.
- `EvalProfile.distribution_targets: Option<DistributionTargets>` — `None` when flag is false.

**Testable by**: AC-01, AC-04 (field presence on parsed struct).

### FR-06 — `profile-meta.json` written by `eval run`
After all scenario results are written, `eval run` must write a `profile-meta.json` file to
the `--out` directory. The file contains a JSON object keyed by profile name, where each value
is `{ "version": 1, "distribution_change": bool, "distribution_targets": <object or null> }`.
The write must be atomic: write to a `.tmp` sibling file first, then rename to
`profile-meta.json`.

**Testable by**: AC-05.

### FR-07 — `profile-meta.json` schema
The `profile-meta.json` file conforms to the following schema:

```json
{
  "<profile-name>": {
    "version": 1,
    "distribution_change": true,
    "distribution_targets": {
      "cc_at_k_min": 0.60,
      "icd_min": 1.20,
      "mrr_floor": 0.35
    }
  },
  "<other-profile-name>": {
    "version": 1,
    "distribution_change": false,
    "distribution_targets": null
  }
}
```

`"version": 1` is required in every entry. It is not inferred from field absence.

**Testable by**: AC-05 (file content inspection).

### FR-08 — `eval report` reads `profile-meta.json` from the results directory
`eval report` (via `run_report`) derives the `profile-meta.json` path from the results
directory path. It reads this file itself — the path is not passed from a calling layer.
This keeps the results directory self-contained and reproducible from CI artifacts.

**Testable by**: AC-06 (absent file fallback), AC-11 (backward compat).

### FR-09 — Backward compatibility: absent `profile-meta.json`
When `profile-meta.json` is absent from the results directory, `eval report` must treat all
profiles as `distribution_change = false` and render Section 5 as "Zero-Regression Check".
No error is emitted. This covers all result sets produced before nan-010.

**Testable by**: AC-11, AC-14.

### FR-10 — `check_distribution_targets` aggregation function
`eval/report/aggregate.rs` (or a new `eval/report/aggregate/distribution.rs` if the 500-line
limit is reached) must expose a function `check_distribution_targets(stats, targets)` that
accepts a per-profile `AggregateStats` (already containing `mean_cc_at_k`, `mean_icd`,
`mean_mrr`) and a `DistributionTargets`, and returns a `DistributionGateResult`.

`DistributionGateResult` must carry:
- For each of `cc_at_k`, `icd`: target value, actual value (`mean` over all scenarios), and
  individual pass/fail.
- Distribution gate pass/fail: `mean(cc_at_k) >= cc_at_k_min` AND `mean(icd) >= icd_min`.
- For `mrr_floor`: target value, actual `mean(mrr)`, and individual pass/fail
  (`mean(mrr) >= mrr_floor`).
- Overall pass/fail: distribution gate AND mrr_floor both pass.

**Testable by**: AC-08, AC-09, AC-13.

### FR-11 — Section 5 conditional rendering — per profile
Section 5 is rendered once per candidate profile. When a candidate profile has
`distribution_change = false` (or absent), Section 5 for that profile renders as
"Zero-Regression Check" using existing logic. When a candidate profile has
`distribution_change = true`, Section 5 for that profile renders as "Distribution Gate"
using the new render path. The two modes co-exist in the same report when multiple candidate
profiles are present.

**Testable by**: AC-06, AC-07, AC-08.

### FR-12 — Distribution Gate Section 5 render content
When `distribution_change = true`, Section 5 for that profile must:
1. Be titled "Distribution Gate".
2. Print the declaration notice: "Distribution change declared. Evaluating against CC@k and
   ICD targets."
3. Show a target-vs-actual table with rows for `cc_at_k` and `icd`. Each row contains:
   metric name, target value, actual value (mean over all scenarios), and pass/fail.
4. Include a "Baseline MRR (reference)" row in the MRR floor table. This row is informational
   only — not a gate criterion. It shows the baseline profile's `mean_mrr` so users can
   calibrate `mrr_floor` values. Labelled clearly as "(reference)".
5. Render the distribution gate result (CC@k + ICD) first.
6. Render the MRR floor as a separate, clearly labelled line after the distribution gate
   result. The MRR floor is a veto on the overall outcome, not a co-equal diversity target.
7. Show final overall PASSED/FAILED verdict. "Diversity targets met, but ranking floor
   breached" must be distinguishable from "Diversity targets not met."

**Testable by**: AC-07, AC-08, AC-09, AC-10.

### FR-13 — MRR floor is absolute, not relative
The MRR floor is compared against `mean(mrr)` of the candidate profile directly. It is not
compared against baseline MRR. A distribution-change feature may have lower MRR than the
baseline (expected) but must not fall below the configured absolute floor.

**Testable by**: AC-09 (absolute comparison verified in unit test), AC-13.

### FR-14 — `eval report` exit code invariant unchanged
`eval report` exits 0 regardless of Distribution Gate outcome. PASSED/FAILED is rendered in
the report body only. This is invariant C-07/FR-29 and is not relaxed by this feature.

**Testable by**: AC-06 (existing suite), post-condition of all report tests.

### FR-15 — Distribution Gate render code in `render_distribution_gate.rs`
All Distribution Gate render logic must be placed in a new
`eval/report/render_distribution_gate.rs` module. `eval/report/render.rs` must not grow
beyond 500 lines. The new module is called from `render.rs` via a function invocation,
following the same pattern as `render_phase.rs` for Section 6.

**Testable by**: Static: line count check on `render.rs` after implementation.

### FR-16 — Documentation update (`docs/testing/eval-harness.md`)
`docs/testing/eval-harness.md` must be updated to document:
1. The `distribution_change` flag and its purpose.
2. The `[profile.distribution_targets]` sub-table: fields, types, and all three required.
3. An example profile TOML for PPR-class features.
4. Section 5 behavior in both modes (Zero-Regression Check vs. Distribution Gate).
5. How to choose `cc_at_k_min`, `icd_min`, and `mrr_floor` values, including guidance
   that the actual baseline MRR is a useful reference point for the floor value.
6. A "Safety constraints" table entry for the Distribution Gate invariant.

**Testable by**: AC-12.

---

## Non-Functional Requirements

### NFR-01 — 500-line file limit
No source file in the eval crate may exceed 500 lines after this feature. `render.rs` is
currently at 499 lines; any addition there (including imports) must be offset by extraction.
`aggregate.rs` is at 488 lines; `check_distribution_targets` must be placed where it does
not breach the limit, either inline if room permits or in a new `aggregate/distribution.rs`.

### NFR-02 — Atomic write for `profile-meta.json`
`profile-meta.json` must be written atomically: write to `profile-meta.json.tmp` in the
output directory, then `fs::rename` to `profile-meta.json`. A partial run (crash after result
files but before meta flush) leaves no `profile-meta.json`, which is the defined
backward-compatible fallback (FR-09). A corrupt or truncated file (non-JSON) must surface
as an error at `eval report` time, not silently fall back.

### NFR-03 — No changes to `ScenarioResult`
The dual-type constraint (pattern #3574, #3550) means that any field added to
`ScenarioResult` in `runner/output.rs` must also be added to the independent copy in
`report/mod.rs` with `#[serde(default)]`. This feature avoids this cost entirely by using
the sidecar file pattern (pattern #3582). Zero fields may be added to `ScenarioResult`.

### NFR-04 — No changes to eval runner replay logic
The `distribution_change` flag has no effect on how scenarios are replayed or how CC@k and
ICD are computed during `eval run`. Both metrics are already computed for every profile. The
runner change is limited to writing `profile-meta.json`.

### NFR-05 — Compatibility with pre-nan-010 result directories
`eval report` must produce identical output to its current behavior when run against result
directories that contain no `profile-meta.json`. No new mandatory arguments or file reads
may be added to `eval report` invocation.

### NFR-06 — Parse error messages are human-readable
`EvalError::ConfigInvariant` messages emitted for missing distribution targets must name
the missing section or field explicitly so the user can fix the TOML without reading the
source.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-01 | A profile TOML with `distribution_change = true` and a valid `[profile.distribution_targets]` table is parsed successfully by `parse_profile_toml`, producing an `EvalProfile` with `distribution_change = true` and `distribution_targets = Some(DistributionTargets { ... })`. | Unit test in `eval/profile/tests.rs`. |
| AC-02 | A profile TOML with `distribution_change = true` and no `[profile.distribution_targets]` table is rejected by `parse_profile_toml` with `EvalError::ConfigInvariant` naming the missing section. | Unit test asserting error variant and message content. |
| AC-03 | A profile TOML with `distribution_change = true` and a `[profile.distribution_targets]` table missing any of the three required fields (`cc_at_k_min`, `icd_min`, `mrr_floor`) is rejected with `EvalError::ConfigInvariant` naming the missing field. | Three separate unit tests (one per missing field). |
| AC-04 | A profile TOML with no `distribution_change` key (or `distribution_change = false`) is parsed with `distribution_change = false` and `distribution_targets = None`; all existing behavior is unchanged. | Unit test; existing profile parse tests continue to pass. |
| AC-05 | `eval run` writes a `profile-meta.json` file to the `--out` directory containing the `distribution_change` flag, `"version": 1`, and targets (or `null`) for each profile name. | Unit or integration test inspecting written file content against schema. |
| AC-06 | When `distribution_change = false` (or absent), `eval report` Section 5 renders as "Zero-Regression Check" with existing regression logic — no behavioral change. | Existing regression-check render tests continue to pass. |
| AC-07 | When `distribution_change = true` for the candidate profile, `eval report` Section 5 is titled "Distribution Gate" and prints: "Distribution change declared. Evaluating against CC@k and ICD targets." | Unit test in `eval/report/tests_distribution_gate.rs` asserting header and notice text. |
| AC-08 | The Distribution Gate render shows: (a) a diversity target table with `cc_at_k` and `icd` rows (metric name, target value, actual value, pass/fail); (b) an MRR floor table with the `mrr` gate row and an additional "Baseline MRR (reference)" row showing the baseline profile's `mean_mrr` (labelled as informational, no pass/fail column). | Unit test asserting rendered table structure, per-row values, and presence of the reference row. |
| AC-09 | Distribution gate pass condition is `mean(cc_at_k) >= cc_at_k_min` AND `mean(icd) >= icd_min`. The MRR floor (`mean(mrr) >= mrr_floor`) is a separate veto rendered as its own line after the distribution gate result. The report shows "PASSED" only when both the distribution gate and the MRR floor pass. | Unit test with a fixture that passes CC@k and ICD but fails MRR floor; assert overall FAILED. |
| AC-10 | Fail condition distinguishes "Diversity targets not met" from "Diversity targets met, but ranking floor breached". The two failure modes are reported separately in the rendered output. | Unit tests: one fixture where CC@k fails (diversity not met), one where CC@k/ICD pass but MRR floor fails; assert distinct rendered messages. |
| AC-11 | When `profile-meta.json` is absent from the results directory (pre-nan-010 result sets), `eval report` treats all profiles as `distribution_change = false` and renders Section 5 as "Zero-Regression Check". No error is emitted. | Unit test pointing `eval report` at a results directory with no `profile-meta.json`. |
| AC-12 | `docs/testing/eval-harness.md` documents: the `distribution_change` flag; the `[profile.distribution_targets]` sub-table; Distribution Gate Section 5 behavior; example TOML for PPR-class features; guidance on choosing `cc_at_k_min`/`icd_min`/`mrr_floor` values. | Manual review; doc must include all six items from FR-16. |
| AC-13 | Unit tests cover: successful parse of a distribution-change profile; rejection when targets section is missing; rejection when a required target field is missing; `check_distribution_targets` returns pass when all targets met; returns fail with correct per-metric detail when any target is missed; MRR floor fail case. | Test suite in `eval/profile/tests.rs` and `eval/report/tests_distribution_gate.rs`. |
| AC-14 | Unit tests cover the backward-compatibility path: `eval report` against a results directory with no `profile-meta.json` renders Section 5 as zero-regression check. | Test in `eval/report/tests_distribution_gate.rs`. |

---

## Domain Models

### `DistributionTargets`
Human-specified floor and minimum values for the distribution gate. All three fields are
required together; there is no partial specification.

| Field | Type | Semantics |
|-------|------|-----------|
| `cc_at_k_min` | `f64` | Minimum acceptable mean CC@k across all scenarios. A diversity target. |
| `icd_min` | `f64` | Minimum acceptable mean ICD across all scenarios. A diversity target. |
| `mrr_floor` | `f64` | Absolute minimum acceptable mean MRR. A ranking quality veto, not a diversity target. |

### `EvalProfile` (extended)
The parsed representation of a profile TOML file. Fields added by this feature:

| Field | Type | Default | Semantics |
|-------|------|---------|-----------|
| `distribution_change` | `bool` | `false` | Declares that this profile intentionally shifts the result distribution. |
| `distribution_targets` | `Option<DistributionTargets>` | `None` | Required and populated when `distribution_change = true`; `None` otherwise. |

### `DistributionGateResult`
The output of `check_distribution_targets`. Carries pass/fail and per-metric detail for
rendering Section 5.

| Field | Semantics |
|-------|-----------|
| `cc_at_k` | `(target: f64, actual: f64, passed: bool)` |
| `icd` | `(target: f64, actual: f64, passed: bool)` |
| `mrr_floor` | `(target: f64, actual: f64, passed: bool)` — veto, evaluated independently |
| `distribution_gate_passed` | `true` iff `cc_at_k.passed && icd.passed` |
| `overall_passed` | `true` iff `distribution_gate_passed && mrr_floor.passed` |

### `ProfileMeta` (new sidecar type)
The per-profile entry in `profile-meta.json`. Used by `eval report` to determine gating mode.

| Field | Type | Semantics |
|-------|------|-----------|
| `version` | `u32` (always `1`) | Schema version. Required; not inferred from field absence. |
| `distribution_change` | `bool` | Mirrors `EvalProfile.distribution_change` at run time. |
| `distribution_targets` | `Option<DistributionTargets>` | Mirrors `EvalProfile.distribution_targets` at run time. |

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| Distribution-change feature | A feature intentionally designed to move retrieval result distribution (re-ranking, suppression, phase-boosting). Contrast with distribution-preserving features. |
| Distribution Gate | The Section 5 gate mode active when `distribution_change = true`. Evaluates CC@k and ICD targets, with MRR floor as a veto. |
| Zero-Regression Check | The existing Section 5 gate mode. Active when `distribution_change = false` or absent. |
| MRR floor | An absolute minimum for `mean(mrr)` of the candidate profile. A veto on distribution-gate pass, not a diversity target. Compared against candidate MRR only, not baseline MRR. |
| Diversity targets | `cc_at_k_min` and `icd_min`. These two together constitute the distribution gate pass condition. |
| Sidecar file | `profile-meta.json` in the results directory. Written by `eval run`; read by `eval report`. The results directory is self-contained: re-running the report from CI artifacts requires only this directory. |
| Veto | A pass/fail condition that is evaluated and rendered independently of the primary gate. The MRR floor is a veto: it can fail while diversity targets pass, or pass while diversity targets fail. Both outcomes are distinctly reported. |

---

## User Workflows

### Workflow 1 — Authoring a distribution-change profile
1. User writes a profile TOML and sets `distribution_change = true` in `[profile]`.
2. User adds `[profile.distribution_targets]` with `cc_at_k_min`, `icd_min`, `mrr_floor`.
3. User runs `eval run --profile ppr-candidate.toml --out results/ppr-run/`.
4. If the TOML is invalid (missing targets or fields), `eval run` fails immediately with
   `EvalError::ConfigInvariant` and a message naming the missing item. No scenarios are
   replayed.
5. On success, `eval run` writes scenario result files and `profile-meta.json` to
   `results/ppr-run/`.

### Workflow 2 — Reading the Distribution Gate in the report
1. User runs `eval report --results results/ppr-run/`.
2. `eval report` reads `profile-meta.json` from `results/ppr-run/`.
3. For the candidate profile with `distribution_change = true`, Section 5 renders as
   "Distribution Gate" with the declaration notice and target-vs-actual table.
4. The table shows CC@k and ICD rows (diversity targets) and a separate MRR floor row
   (veto).
5. The report prints overall PASSED or FAILED with distinguishable failure modes.
6. Process exits 0 regardless.

### Workflow 3 — Backward compatibility (pre-nan-010 results)
1. User runs `eval report --results results/old-run/` against a directory produced before
   nan-010.
2. `eval report` finds no `profile-meta.json`. It treats all profiles as
   `distribution_change = false`.
3. Section 5 renders as "Zero-Regression Check" — identical to pre-nan-010 behavior.

### Workflow 4 — Mixed-profile report (one distribution-change, one zero-regression)
1. User runs a report against results containing a `distribution_change = true` candidate
   (e.g., `ppr-candidate`) and a `distribution_change = false` candidate (e.g., `baseline`).
2. Section 5 for `ppr-candidate` renders as "Distribution Gate".
3. Section 5 for `baseline` renders as "Zero-Regression Check".
4. Each profile gate is evaluated independently on its own terms.

---

## Constraints

1. **Dual-type constraint (pattern #3574, #3550).** Zero fields may be added to
   `ScenarioResult` (runner/output.rs or report/mod.rs). Profile metadata is carried
   exclusively via `profile-meta.json`. If this constraint is violated, both copies of all
   changed types must be updated in sync with `#[serde(default)]` on new fields.

2. **`render.rs` 500-line limit (pattern #3583).** `render.rs` is at 499 lines. Distribution
   Gate render code must be placed in a new `eval/report/render_distribution_gate.rs` module.
   The module boundary must be established before any other change to `render.rs`.

3. **`aggregate.rs` 500-line limit.** `aggregate.rs` is at 488 lines. `check_distribution_targets`
   must be placed such that the file remains at or below 500 lines. If the addition would
   breach the limit, extract into `eval/report/aggregate/distribution.rs` before adding code.

4. **Parse-time validation only.** Distribution target validation must occur in
   `parse_profile_toml`, before `EvalServiceLayer` construction. Report-time validation
   is not acceptable.

5. **`[profile]` stripping order.** `distribution_change` and `distribution_targets` must be
   extracted from the raw TOML before the `[profile]` section is removed for
   `UnimatrixConfig` deserialization. Changing this order silently drops the new fields.

6. **`eval report` exit code invariant (C-07, FR-29).** The process exits 0 always.
   Distribution Gate PASSED/FAILED appears in the report body only.

7. **`mrr_floor` is absolute.** The MRR floor is compared against `mean(mrr)` of the candidate
   profile. It is never compared against baseline MRR.

8. **No `--distribution-change` CLI flag.** The declaration lives in the TOML exclusively.
   There is no command-line override.

9. **Baseline profile with `distribution_change = true` → `ConfigInvariant` at parse time.**
   The distribution gate is a candidate-only concept. A baseline profile with this flag set
   must be rejected by `parse_profile_toml` with `EvalError::ConfigInvariant` and message
   "baseline profile must not declare `distribution_change = true`". Silent ignore is ruled
   out — it would apply the wrong gate without signalling the misconfiguration.

10. **Atomic write for `profile-meta.json`.** A crash between result files and the metadata
    flush must leave `eval report` in the defined backward-compatible fallback state (no file =
    treat as `distribution_change = false`), not in a corrupt state.

---

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `eval/profile/types.rs` | Workspace | Extended with `DistributionTargets` and new `EvalProfile` fields. |
| `eval/profile/validation.rs` | Workspace | `parse_profile_toml` extended to extract and validate new fields. |
| `eval/runner/mod.rs` or new `eval/runner/profile_meta.rs` | Workspace | Writes `profile-meta.json` after scenario results. |
| `eval/report/aggregate.rs` | Workspace | New `check_distribution_targets` function. |
| `eval/report/render.rs` | Workspace | Conditional dispatch to distribution gate render path. Must not grow past 500 lines. |
| `eval/report/render_distribution_gate.rs` | New module | All Distribution Gate Section 5 render logic. |
| `eval/report/mod.rs` | Workspace | Passes profile metadata map to `render_report`. |
| `eval/profile/tests.rs` | Workspace | Existing test file extended for new parse cases. |
| `eval/report/tests_distribution_gate.rs` | New test module | Distribution Gate render and aggregation tests. |
| `docs/testing/eval-harness.md` | Workspace | Documentation update (AC-12). |
| `mean_cc_at_k`, `mean_icd`, `mean_mrr` in `AggregateStats` | `aggregate.rs` | Pre-existing fields from nan-008. Distribution gate reads these; does not compute new metrics. |
| `serde_json` | Cargo.toml | Serialization of `profile-meta.json`. |

---

## NOT in Scope

- Changes to eval runner replay logic or how CC@k and ICD are computed.
- Changes to the baseline profile definition or validation.
- Auto-derivation of `cc_at_k_min`, `icd_min`, or `mrr_floor` from historical data.
- Per-scenario distribution gate evaluation (gate is over means only).
- Cross-profile distribution gate comparison.
- Changes to Section 7 Distribution Analysis.
- A `--distribution-change` CLI flag or any command-line override of TOML declarations.
- Changes to `eval report` exit code.
- Changes to `ScenarioResult` fields in `runner/output.rs` or `report/mod.rs`.

---

## Open Questions

1. **OQ-01 (architect): Baseline profile with `distribution_change = true`.** The scope
   states the baseline never uses this flag but does not specify whether to error or silently
   ignore if encountered. The architect should define the exact behavior (emit
   `ConfigInvariant`, emit a warning, or ignore silently) to avoid ambiguity in
   `parse_profile_toml`.

2. **OQ-02 (architect): Corrupt `profile-meta.json` (non-JSON or truncated).**
   NFR-02 states this must surface as an error at `eval report` time. The architect should
   specify the error type, message, and whether the report aborts entirely or falls back
   to zero-regression mode.

3. **OQ-03 (architect): `aggregate.rs` line budget.** Confirm whether the 12 lines
   available in `aggregate.rs` (500 - 488) are sufficient for `check_distribution_targets`
   plus imports, or whether a pre-split into `aggregate/distribution.rs` is required before
   implementation begins. Risk SR-02 makes this a day-one decision.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for eval-harness, profile-TOML, dual-type, render-split -- results: #3582 (sidecar pattern, nan-010), #3574/#3550/#3563 (dual-type constraint), #3583 (render.rs 500-line split), #3529 (render_report parameter passing), #2806 (eval harness TOML pattern). Established conventions confirmed and applied.
