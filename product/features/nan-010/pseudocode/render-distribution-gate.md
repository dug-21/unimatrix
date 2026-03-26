# Component 5: Distribution Gate Renderer

**File**: `eval/report/render_distribution_gate.rs` (new)
**Pattern**: Follows `render_phase.rs` — a sibling module extracted for the 500-line limit.

---

## Purpose

Render Section 5 "Distribution Gate" as a Markdown string for one candidate profile that
declares `distribution_change = true`. Called from `render_report` in `render.rs`.

This module contains exactly one public function: `render_distribution_gate_section`.
All Distribution Gate render logic lives here; `render.rs` must not grow beyond 500 lines
(ADR-001, SR-03).

---

## Module-Level Comment

```
//! Distribution Gate section renderer (nan-010).
//!
//! Extracted into this sibling module to keep render.rs within the 500-line limit (ADR-001).
//! Renders Section 5 for candidate profiles with distribution_change = true.
//! Follows the render_phase.rs extraction pattern.
```

---

## Types Imported

```
use super::AggregateStats;
use super::aggregate::distribution::{DistributionGateResult, MetricGateRow};
```

`HeadingLevel` is defined in this file or passed as a parameter. See below.

---

## `HeadingLevel` Enum

Defined locally in this file (or in `render.rs` and imported here — either works, but
local is cleaner for a pure rendering module):

```
// Controls whether this profile's Section 5 uses a top-level or sub-level heading.
// Single: exactly one non-baseline candidate → "## 5. Distribution Gate"
// Multi { index }: multiple non-baseline candidates → "### 5.N Distribution Gate — {name}"
pub(super) enum HeadingLevel {
    Single,
    Multi { index: usize },  // 1-based index (first candidate is 5.1)
}
```

---

## Function: `render_distribution_gate_section`

**Signature**:
```
pub(super) fn render_distribution_gate_section(
    profile_name: &str,
    gate: &DistributionGateResult,
    baseline_stats: &AggregateStats,
    heading_level: HeadingLevel,
) -> String
```

**Pseudocode**:

```
fn render_distribution_gate_section(profile_name, gate, baseline_stats, heading_level):
    out = String::new()

    // ---- Heading ----
    match heading_level:
        Single =>
            out.push_str("## 5. Distribution Gate\n\n")
        Multi { index } =>
            // Note: parent "## 5. Distribution Gate" heading is written by render.rs
            // before the loop. Each profile gets a sub-heading.
            out.push_str(&format!("### 5.{index} Distribution Gate — {profile_name}\n\n"))

    // ---- Declaration notice ----
    out.push_str(
        "Distribution change declared. Evaluating against CC@k and ICD targets.\n\n"
    )

    // ---- Diversity target table (CC@k and ICD rows) ----
    out.push_str("| Metric | Target | Actual | Result |\n")
    out.push_str("|--------|--------|--------|--------|\n")
    out.push_str(&format!(
        "| CC@k | ≥ {:.4} | {:.4} | {} |\n",
        gate.cc_at_k.target,
        gate.cc_at_k.actual,
        pass_fail_label(gate.cc_at_k.passed),
    ))
    out.push_str(&format!(
        "| ICD | ≥ {:.4} | {:.4} | {} |\n",
        gate.icd.target,
        gate.icd.actual,
        pass_fail_label(gate.icd.passed),
    ))
    out.push('\n')

    // ---- Diversity gate verdict ----
    if gate.diversity_passed:
        out.push_str("**Diversity gate: PASSED**\n\n")
    else:
        out.push_str("**Diversity gate: FAILED** — Diversity targets not met.\n\n")

    // ---- MRR floor table ----
    out.push_str("MRR floor (veto):\n\n")
    out.push_str("| Metric | Floor | Actual | Result |\n")
    out.push_str("|--------|-------|--------|--------|\n")
    out.push_str(&format!(
        "| MRR | ≥ {:.4} | {:.4} | {} |\n",
        gate.mrr_floor.target,
        gate.mrr_floor.actual,
        pass_fail_label(gate.mrr_floor.passed),
    ))
    // Informational row: baseline MRR reference (AC-08, SCOPE.md design decision #5)
    // Not a gate criterion — no pass/fail column.
    out.push_str(&format!(
        "| Baseline MRR (reference) | — | {:.4} | — |\n",
        baseline_stats.mean_mrr,
    ))
    out.push('\n')

    // ---- MRR floor verdict ----
    if gate.mrr_floor_passed:
        out.push_str("**MRR floor: PASSED**\n\n")
    else:
        out.push_str("**MRR floor: FAILED**\n\n")

    // ---- Overall verdict with distinguishable failure modes (ADR-003, AC-10) ----
    if gate.overall_passed:
        out.push_str("**Overall: PASSED**\n\n")
    else:
        // Determine which failure mode to report
        match (gate.diversity_passed, gate.mrr_floor_passed):
            (false, true) =>
                out.push_str("**Overall: FAILED** — Diversity targets not met.\n\n")
            (true, false) =>
                out.push_str(
                    "**Overall: FAILED** — Diversity targets met, but ranking floor breached.\n\n"
                )
            (false, false) =>
                out.push_str(
                    "**Overall: FAILED** — Diversity targets not met. Ranking floor breached.\n\n"
                )
            (true, true) =>
                // Cannot reach this branch: overall_passed would be true
                // Defensive: treat as passed
                out.push_str("**Overall: PASSED**\n\n")

    out
```

### Helper Function: `pass_fail_label`

```
fn pass_fail_label(passed: bool) -> &'static str:
    if passed { "PASSED" } else { "FAILED" }
```

---

## Integration with `render.rs`

`render.rs` must declare and import this module:
```
mod render_distribution_gate;
use render_distribution_gate::{render_distribution_gate_section, HeadingLevel};
```

These two lines are the only additions to `render.rs` from this module (added as part of
Pre-split A). No other logic in this file lives in `render.rs`.

---

## Rendered Output Examples

### Single-profile, all pass

```markdown
## 5. Distribution Gate

Distribution change declared. Evaluating against CC@k and ICD targets.

| Metric | Target | Actual | Result |
|--------|--------|--------|--------|
| CC@k   | ≥ 0.6000 | 0.6234 | PASSED |
| ICD    | ≥ 1.2000 | 1.3101 | PASSED |

**Diversity gate: PASSED**

MRR floor (veto):

| Metric | Floor | Actual | Result |
|--------|-------|--------|--------|
| MRR    | ≥ 0.3500 | 0.3812 | PASSED |
| Baseline MRR (reference) | — | 0.5103 | — |

**MRR floor: PASSED**

**Overall: PASSED**
```

### Single-profile, diversity passed, MRR floor failed

```markdown
**Diversity gate: PASSED**

...

**MRR floor: FAILED**

**Overall: FAILED** — Diversity targets met, but ranking floor breached.
```

### Single-profile, diversity failed

```markdown
**Diversity gate: FAILED** — Diversity targets not met.

...

**Overall: FAILED** — Diversity targets not met.
```

### Multi-profile sub-heading (index=1)

```markdown
### 5.1 Distribution Gate — ppr-candidate

Distribution change declared. ...
```

---

## Data Flow

Inputs:
- `profile_name: &str` — for multi-profile sub-heading label
- `gate: &DistributionGateResult` — from `check_distribution_targets` (Component 4)
- `baseline_stats: &AggregateStats` — for the "Baseline MRR (reference)" informational row
- `heading_level: HeadingLevel` — determined by caller (count of non-baseline candidates)

Outputs:
- `String` — Markdown fragment for Section 5 of this profile's block

---

## Error Handling

This function is infallible — pure string formatting. No `Result` return type.

---

## Key Test Scenarios

Tests in `eval/report/tests_distribution_gate.rs`:

```
test_distribution_gate_section_header:
    Call with HeadingLevel::Single
    Assert: output starts with "## 5. Distribution Gate\n"
    Call with HeadingLevel::Multi { index: 1 } and profile_name="ppr-candidate"
    Assert: output contains "### 5.1 Distribution Gate — ppr-candidate"

test_distribution_gate_table_content:
    gate: cc_at_k target=0.60, actual=0.62; icd target=1.20, actual=1.31
    baseline_stats: mean_mrr=0.51
    Assert: rendered table contains "≥ 0.6000" and "0.6234" (or whatever actual is)
    Assert: "Baseline MRR (reference)" row present with "0.5103" (or fixture value)
    Assert: "— | — |" pattern in reference row (no pass/fail)

test_distribution_gate_pass_condition:
    gate: diversity_passed=true, mrr_floor_passed=true, overall_passed=true
    Assert: contains "**Diversity gate: PASSED**"
    Assert: contains "**MRR floor: PASSED**"
    Assert: contains "**Overall: PASSED**"

test_distribution_gate_mrr_floor_veto:
    gate: diversity_passed=true, mrr_floor_passed=false, overall_passed=false
    Assert: contains "**Diversity gate: PASSED**"
    Assert: contains "**MRR floor: FAILED**"
    Assert: contains "ranking floor breached"
    Assert: does NOT contain "Diversity targets not met"

test_distribution_gate_distinct_failure_modes:
    Mode A: diversity_passed=false, mrr_floor_passed=true
    Assert: contains "Diversity targets not met"
    Assert: does NOT contain "ranking floor breached"
    Mode B: diversity_passed=true, mrr_floor_passed=false
    Assert: contains "ranking floor breached"
    Assert: does NOT contain "Diversity targets not met"
    Mode C: diversity_passed=false, mrr_floor_passed=false
    Assert: contains "Diversity targets not met"
    Assert: contains "Ranking floor breached" (or equivalent)
    Assert: overall FAILED
```

---

## Notes

- `render_phase.rs` is the direct pattern reference. This file has the same structure:
  module-level comment, single exported function, `pub(super)` visibility.
- Numeric formatting: 4 decimal places (`{:.4}`) for all metric values, matching the
  established pattern in `render.rs` and `render_phase.rs`.
- The "Baseline MRR (reference)" row uses `—` (em-dash, U+2014) for Floor and Result
  columns. This matches the existing em-dash pattern in `render.rs` for zero deltas.
- `render_distribution_gate_section` does NOT accept a regressions parameter (R-13
  mitigation: no regression rows appear in Distribution Gate output).
- `find_regressions` is still computed in `run_report` for all profiles; its output is
  simply not passed to this function. The existing zero-regression block still uses it
  for profiles with `distribution_change=false`.
