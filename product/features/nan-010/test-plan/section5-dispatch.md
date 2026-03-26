# Test Plan: Section 5 Dispatch in `render_report` (`eval/report/render.rs`)

Component 6 of 7.

---

## Scope

`render_report` in `eval/report/render.rs` gains a new parameter
`profile_meta: &HashMap<String, ProfileMetaEntry>` and a Section 5 dispatch loop that
conditionally calls `render_distribution_gate_section` (Component 5) or the existing
zero-regression block for each non-baseline candidate profile.

This component's tests focus on dispatch correctness — which render path is chosen based on
`profile_meta` content — and heading level selection for single vs. multi-profile runs.

All tests are in `eval/report/tests_distribution_gate.rs`.

---

## Unit Test Expectations

### Section 5 Dispatch — Distribution Gate Path

**Within `test_distribution_gate_section_header`** (also covers dispatch):
- Arrange: `profile_meta` map with `"ppr-candidate"` entry having `distribution_change = true`.
  Construct a minimal full render context (one candidate profile, `ppr-candidate`).
- Act: call `render_report` with populated `profile_meta`.
- Assert: rendered output Section 5 contains `"Distribution Gate"`, not `"Zero-Regression Check"`.

---

### Section 5 Dispatch — Zero-Regression Path (AC-06)

**Within `test_report_without_profile_meta_json`** (covers both dispatch and backward compat):
- When `profile_meta` is empty (absent sidecar), all profiles dispatch to zero-regression.
- Assert: rendered output Section 5 contains `"Zero-Regression Check"`, not `"Distribution Gate"`.

---

### `test_distribution_gate_exit_code_zero` (R-12, C-07/FR-29)

Exit code invariant. `eval report` must exit 0 regardless of gate outcome.

- Arrange: results directory where Distribution Gate would fail (CC@k below target).
  Alternatively, this can be a direct test of `run_report`'s return type: `run_report` returns
  `Result<String, EvalError>` — verify it returns `Ok(report_string)` even when gate fails;
  the gate failure is embedded in the report string, not propagated as `Err`.
- Act: run `eval report` against a failing-gate results directory (or call `run_report` directly).
- Assert:
  - Process exit code is `0` (or `run_report` returns `Ok(_)`)
  - The rendered report contains `"FAILED"` in Section 5 (the failure is in the body)
  - The rendered report does NOT propagate to a non-zero exit

Note: if testing via process spawn, use `std::process::Command` to assert `status.success()`.
If testing via direct function call, assert `run_report(...).is_ok()`.

---

## Dispatch Logic Specification

The Section 5 dispatch loop must count non-baseline candidate profiles to determine heading
level before entering the render loop (ADR-005):

```
count = profiles.filter(|p| !p.is_baseline).count()
for (i, profile) in non_baseline_profiles.enumerate():
    meta = profile_meta.get(profile.name)
    if meta.distribution_change:
        heading = if count == 1 { Single } else { Multi(i+1) }
        section5 = render_distribution_gate_section(name, gate, baseline_stats, heading)
    else:
        heading = if count == 1 { "## 5." } else { "### 5.{i+1}" }
        section5 = render_zero_regression_section(regressions, heading)
```

Tests must verify both branches of this dispatch, and both Single and Multi heading modes.

---

## Multi-Profile Dispatch (ADR-005)

**Scenario: one distribution-change candidate, one zero-regression candidate**

This covers the mixed-profile workflow (Workflow 4 from SPECIFICATION.md):
- `profile_meta["ppr-candidate"].distribution_change = true`
- `profile_meta["standard-candidate"].distribution_change = false`
- Two non-baseline candidates → multi-profile heading mode.
- Assert:
  - Section 5.1 contains `"### 5.1 Distribution Gate — ppr-candidate"` (or equivalent)
  - Section 5.2 contains `"### 5.2 Zero-Regression Check — standard-candidate"` (or equivalent)
  - Neither section contains the other's content type

This test may be named `test_distribution_gate_section_header` or a dedicated multi-profile
test. It is not in the non-negotiable list but is required for R-09 coverage.

---

## `render_report` Parameter Addition

- `render_report` gains `profile_meta: &HashMap<String, ProfileMetaEntry>` as the final parameter.
- All existing call sites in `report/mod.rs` must be updated to pass `&profile_meta`.
- Passing an empty `HashMap` produces the backward-compat zero-regression path (covered by
  `test_report_without_profile_meta_json`).
- The compiler catches missing or type-mismatched arguments at build time.

---

## 500-Line Constraint (NFR-01, R-01)

Gate-3b must verify:
- `wc -l eval/report/render.rs` <= 500 after all changes.
- All Distribution Gate render code is in `render_distribution_gate.rs`, not `render.rs`.
- Only the `mod` declaration, `use` statement, and dispatch call are added to `render.rs`.

---

## Risks Covered

| Risk | Test |
|------|------|
| R-09 (heading level wrong) | Multi-profile dispatch test; `test_distribution_gate_section_header` |
| R-12 (exit code non-zero on failure) | `test_distribution_gate_exit_code_zero` |
| R-13 (regressions bleed) | Dispatch logic test: zero-regression path uses regressions; gate path does not pass them to renderer |
| R-01 (500-line breach) | Gate-3b static line count |
| AC-06 (zero-regression path unchanged) | `test_report_without_profile_meta_json` |
