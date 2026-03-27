# Agent Report: nan-010-agent-9-dispatch

Component 6 — Section 5 Dispatch

---

## Summary

Implemented the per-profile Section 5 gate dispatch in `render_report` per pseudocode/section5-dispatch.md and ADR-005.

---

## Files Modified

- `crates/unimatrix-server/src/eval/report/render.rs` — added `profile_meta` parameter, replaced monolithic Section 5 block with per-profile dispatch loop
- `crates/unimatrix-server/src/eval/report/render_zero_regression.rs` — **created** (new file): extracted zero-regression block renderer, called from dispatch loop
- `crates/unimatrix-server/src/eval/report/mod.rs` — `mod render_zero_regression;` declaration (already present in committed state via prior wave)

---

## Changes Made

### render.rs

1. Updated module doc comment (compacted to 6 lines to stay within budget)
2. Added imports: `render_zero_regression_block`, `DistributionTargets`, `ProfileMetaEntry`; removed `#[allow(unused_imports)]` from `HeadingLevel`/`render_distribution_gate_section` import; merged `ScoredEntry` into the `super::{}` block
3. Added `profile_meta: &HashMap<String, ProfileMetaEntry>` as the ninth parameter to `render_report`
4. Replaced old Section 5 (single heading + flat regressions loop) with per-profile dispatch:
   - `non_baseline_stats = stats[1..]` (baseline is always `stats[0]`)
   - `multi_profile` flag controls heading level (ADR-005)
   - Empty `non_baseline_stats` falls back to zero-regression with empty regressions (backward-compat, AC-11, AC-14)
   - Multi-profile: parent heading `## 5. Distribution Gate / Zero-Regression Check` emitted once
   - Each non-baseline profile: `distribution_change=true` → `check_distribution_targets` inline + `render_distribution_gate_section`; `false`/absent → `render_zero_regression_block`
   - Missing targets (unexpected, parse-time validation prevents): emits HTML comment WARN block; report still exits 0 (C-07, FR-29)

### render_zero_regression.rs (new)

Extracted from the old Section 5 inline logic. Accepts `(regressions, profile_name, index, multi_profile)`. Renders `## 5.` or `### 5.N` heading and the regression table (or "No regressions detected" for empty). Extracted to keep `render.rs` within 500 lines (ADR-001).

---

## Final Line Count

`render.rs`: **500 lines** (stable after `cargo fmt`)

---

## Test Results

- Total: 2171 passed, 1 failed
- `test_report_empty_results_dir`: now passing (backward-compat empty-stats fallback)
- `test_report_without_profile_meta_json`, `test_distribution_gate_exit_code_zero`: passing
- All `tests_distribution_gate` module tests: passing
- Pre-existing failure: `test_distribution_gate_table_content` in `render_distribution_gate.rs:275` — introduced in Wave 3 commit `a537cd2` (agent 8), not caused by this component

---

## Deviations from Pseudocode

One deviation from the pseudocode's "only baseline present — no Section 5 content" comment:

**Deviation**: When `non_baseline_stats` is empty (no candidates at all), the code emits a zero-regression block with an empty regressions list rather than emitting nothing.

**Reason**: The existing test `test_report_empty_results_dir` asserts `## 5. Zero-Regression Check` and `No regressions detected` must appear even when the results directory is empty. This is correct backward-compat behavior for pre-nan-010 results with no candidate profiles. The pseudocode comment "no Section 5 content" applies to the iteration loop, not the function as a whole.

---

## Issues / Blockers

None. The pre-existing `test_distribution_gate_table_content` failure is in `render_distribution_gate.rs` (Wave 3 / agent 8's component) and is outside this component's scope.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `crate:unimatrix-server topic:nan-010` — found ADR-005 (#3590) confirming per-profile independence and heading level rules; also ADR-001 (#3586) confirming 500-line pre-split requirement.
- Stored: entry #3605 "render.rs 500-line budget: cargo fmt expands inline calls — write compact then let fmt decide" via `/uni-store-pattern` — documents the iterative fmt-then-trim strategy for staying under budget, specifically the `render_zero_regression_block` 4-arg call expansion trap.
