# Test Plan: Distribution Gate Renderer (`eval/report/render_distribution_gate.rs`)

Component 5 of 7.

---

## Scope

New module `eval/report/render_distribution_gate.rs`. Exposes
`render_distribution_gate_section(profile_name, gate, baseline_stats, heading_level) -> String`.

This function produces a self-contained Markdown block for Section 5. It receives a fully
computed `DistributionGateResult` (from Component 4) and the baseline `AggregateStats` (for
the "Baseline MRR (reference)" informational row). It does not perform gate logic — all
pass/fail is pre-computed.

All tests are in `eval/report/tests_distribution_gate.rs`.

---

## Pre-Split Prerequisite (R-01, R-03)

Before any code is added to `render.rs`, the boundary module must exist:
- `eval/report/render_distribution_gate.rs` exists with correct module declaration.
- `render.rs` has `mod render_distribution_gate;` and
  `use render_distribution_gate::render_distribution_gate_section;` added.
- `cargo build` passes.
- `wc -l render.rs` <= 500.

Gate-3b static check, not a unit test.

---

## Unit Test Expectations

### `test_distribution_gate_section_header` (AC-07, R-09)

- Arrange: construct a passing `DistributionGateResult` (all fields passed), baseline stats,
  heading level `Single` (maps to `## 5.`).
- Act: `let output = render_distribution_gate_section("ppr-candidate", &gate, &baseline, Single);`
- Assert:
  - `output.contains("## 5. Distribution Gate")` is `true`
  - `output.contains("Distribution change declared")` is `true`
  - `output.contains("Evaluating against CC@k and ICD targets")` is `true`
  - `output.contains("### 5.")` is `false` (single-profile must not use sub-heading)

For multi-profile heading:
- Arrange: heading level `Multi(1)` (maps to `### 5.1`).
- Assert:
  - `output.contains("### 5.1 Distribution Gate")` is `true` (or contains profile_name)
  - `output.contains("## 5. Distribution Gate")` is `false` (no top-level heading in multi)

---

### `test_distribution_gate_table_content` (AC-08, R-13)

Validates diversity table structure, MRR floor table, and "Baseline MRR (reference)" row.
Also guards against regression bleed (R-13).

- Arrange:
  - `DistributionGateResult` with `cc_at_k: { target: 0.60, actual: 0.6234, passed: true }`,
    `icd: { target: 1.20, actual: 1.3101, passed: true }`,
    `mrr_floor: { target: 0.35, actual: 0.3812, passed: true }`.
  - Baseline stats: `AggregateStats { mean_mrr: 0.5103, ... }`.
- Act: call `render_distribution_gate_section`.
- Assert (rendered string):
  - Contains `"CC@k"` and `"0.60"` and `"0.6234"` in the same row context
  - Contains `"ICD"` and `"1.20"` and `"1.3101"` in the same row context
  - Contains `"Baseline MRR (reference)"` row
  - Contains `"0.5103"` (baseline MRR value in reference row)
  - Contains `"0.35"` (MRR floor target) and `"0.3812"` (candidate MRR actual)
  - Does NOT contain any regression-related text (`"Regressions"`, `"PASSED/FAILED"` in a
    regression-row context) — guards R-13
  - Contains table header row: `| Metric | Target | Actual | Result |`
  - The reference row has `"—"` in the Result column (informational, not a gate criterion)

---

### `test_distribution_gate_pass_condition` (AC-09)

- Arrange: `DistributionGateResult` with `diversity_passed = true`, `mrr_floor_passed = true`,
  `overall_passed = true`.
- Assert:
  - Output contains `"**Overall: PASSED**"` (or equivalent pass verdict)
  - Output contains `"**Diversity gate: PASSED**"` (or equivalent)
  - Output does NOT contain `"FAILED"`

---

### `test_distribution_gate_mrr_floor_veto` (AC-09, R-05)

The critical veto semantics test. CC@k and ICD pass; MRR floor fails.

- Arrange: `DistributionGateResult`:
  - `cc_at_k.passed = true`, `icd.passed = true`, `diversity_passed = true`
  - `mrr_floor.passed = false`, `mrr_floor_passed = false`
  - `overall_passed = false`
- Assert:
  - Output contains `"**Diversity gate: PASSED**"`
  - Output contains text indicating MRR floor failure
  - Output contains `"FAILED"` in the overall verdict
  - Output does NOT contain `"**Overall: PASSED**"`

---

### `test_distribution_gate_distinct_failure_modes` (AC-10, R-06)

Two sub-cases within one test (or two separate test functions with this combined name is
acceptable):

**Case A — diversity fails, MRR passes:**
- `diversity_passed = false`, `mrr_floor_passed = true`, `overall_passed = false`
- Assert: output contains `"Diversity targets not met"` and does NOT contain
  `"ranking floor breached"`

**Case B — diversity passes, MRR fails:**
- `diversity_passed = true`, `mrr_floor_passed = false`, `overall_passed = false`
- Assert: output contains `"ranking floor breached"` and does NOT contain
  `"Diversity targets not met"`

These distinct message assertions are the primary test for R-06.

---

## Rendered Output Structure Validation

The expected rendered output shape for a single-profile passing run (reference for implementer):

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

Tests do not need to assert exact whitespace, but must assert all structural elements above.

---

## Heading Level Variants (ADR-005, R-09)

| Scenario | HeadingLevel | Expected heading |
|----------|-------------|-----------------|
| Single non-baseline candidate | `Single` | `## 5. Distribution Gate` |
| Multiple candidates, this one is N=1 | `Multi(1)` | `### 5.1 Distribution Gate — {profile_name}` |
| Multiple candidates, this one is N=2 | `Multi(2)` | `### 5.2 Distribution Gate — {profile_name}` |

Test `test_distribution_gate_section_header` must cover at least Single and Multi(1).

---

## Risks Covered

| Risk | Test |
|------|------|
| R-06 (indistinguishable failure messages) | `test_distribution_gate_distinct_failure_modes` |
| R-09 (heading level wrong) | `test_distribution_gate_section_header` heading assertions |
| R-13 (regressions bleed into render) | `test_distribution_gate_table_content` negative assertion |
| R-01 (pre-split) | Gate-3b: `render.rs` <= 500 lines after boundary established |
| AC-08 (reference row) | `test_distribution_gate_table_content` reference row assertion |
