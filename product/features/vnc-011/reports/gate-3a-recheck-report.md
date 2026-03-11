# Gate 3a Recheck Report: vnc-011

> Gate: 3a (Design Review -- Rework Recheck)
> Date: 2026-03-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All three components match architecture C1/C2/C3. Feature gate, no new deps, CollapsedFinding updated for FR-09. |
| Specification coverage | PASS | All FRs covered. FR-09 narrative summary now captured and rendered. evidence_limit default correct. FR-13 in scope. |
| Risk coverage | PASS | All 14 risks + 4 integration risks mapped to test scenarios. R-02 tests corrected. |
| Interface consistency | PASS | CollapsedFinding consistent across OVERVIEW and formatter. context_reload_pct scale correct. format_duration test aligned. |

## Rework Verification

All 6 issues from the original gate-3a-report.md are resolved:

### Issue 1: FR-09 narrative summary not captured or rendered
**Status**: PASS
**Evidence**: `CollapsedFinding` now includes `narrative_summary: Option<String>` (OVERVIEW.md line 42, retrospective-formatter.md line 40). `collapse_findings` populates it via `narrative.map(|n| n.summary.clone())` (retrospective-formatter.md line 367). `render_findings` uses `narrative_summary` when `Some` as the description line, falling back to `claims[0]` (retrospective-formatter.md lines 261-264). Test plan includes `test_collapse_narrative_summary_populated`, `test_collapse_narrative_summary_none_when_no_match`, and `test_findings_narrative_summary_replaces_claim`.

### Issue 2: evidence_limit default unwrap_or(0) should be unwrap_or(3)
**Status**: PASS
**Evidence**: handler-dispatch.md line 40 uses `params.evidence_limit.unwrap_or(3)`. OVERVIEW.md data flow line 25 uses `evidence_limit.unwrap_or(3)`. Both match the human override.

### Issue 3: params-extension doc comment says default 0
**Status**: PASS
**Evidence**: params-extension.md line 20 now reads `/// Maximum evidence items per hotspot (default: 3, JSON path only). (col-010b)`.

### Issue 4: test_reload_present setup value
**Status**: PASS
**Evidence**: test-plan/retrospective-formatter.md line 126 uses `context_reload_pct: Some(0.345)` and asserts `Contains "35% context reload"`. The value is a fraction (0.0-1.0), multiplied by 100.0 in the renderer (`pct * 100.0`), matching the pseudocode in retrospective-formatter.md line 503.

### Issue 5: test_duration_exact_hour expectation
**Status**: PASS
**Evidence**: test-plan/retrospective-formatter.md line 42 expects `"1h"` for input 3600, matching the pseudocode where `hours > 0 && minutes > 0` is false (minutes=0), falling to the `hours > 0` branch returning `format!("{}h", hours)`.

### Issue 6: R-02 test name/expectation contradicts human override
**Status**: PASS
**Evidence**: test-plan/OVERVIEW.md line 16 maps R-02 to `test_json_evidence_limit_default_3`, `test_json_evidence_limit_explicit_5`, `test_markdown_ignores_evidence_limit`. The old `test_json_no_evidence_limit_returns_all` is removed. handler-dispatch test plan line 27 confirms `test_json_evidence_limit_default_3` asserts `unwrap_or(3)` with evidence truncated to 3.

## Full Gate Check Set (re-verified)

### 1. Architecture Alignment
**Status**: PASS
**Evidence**: Component boundaries match architecture exactly: C1 (params-extension) in `tools.rs`, C2 (retrospective-formatter) in `response/retrospective.rs`, C3 (handler-dispatch) in `tools.rs`. Feature gate `#[cfg(feature = "mcp-briefing")]` specified in OVERVIEW.md. No new crate dependencies. Deterministic timestamp selection per ADR-002. `CollapsedFinding` updated with `narrative_summary` field to support FR-09, consistent with architecture C2 sub-responsibilities.

Baseline sample count omission (WARN from original report) remains an acknowledged pragmatic decision -- `BaselineComparison` does not carry `sample_count`. The architecture Integration Surface table does not list it as available to the formatter.

### 2. Specification Coverage
**Status**: PASS
**Evidence**: All 14 functional requirements (FR-01 through FR-14) have corresponding pseudocode. Human overrides are correctly reflected: evidence_limit JSON default unchanged at 3 (FR-02), deterministic earliest-first example selection (FR-08), rework/reload in scope (FR-13). NFR-01 through NFR-04 addressed by design. No scope additions detected.

### 3. Risk Coverage
**Status**: PASS
**Evidence**: All 14 risks (R-01 through R-14) mapped to test scenarios in test-plan/OVERVIEW.md risk-to-test table. Integration risks IR-01 through IR-04 covered. Edge cases (empty hotspots, single finding, large report, unicode, NaN, stddev=0, duplicate rule_names) present in test-plan/retrospective-formatter.md.

### 4. Interface Consistency
**Status**: PASS
**Evidence**: `CollapsedFinding` struct is consistent across OVERVIEW.md (line 35-45) and retrospective-formatter.md (line 33-43) -- both include all 9 fields including `narrative_summary`. `format_retrospective_markdown` signature consistent across architecture, OVERVIEW, implementation brief, and formatter pseudocode. Data flow coherent: params-extension feeds `format: Option<String>` to handler-dispatch, which routes to the correct formatter. The formatter consumes `&RetrospectiveReport` immutably.

## Warnings (non-blocking)

1. **Rounding edge in test_reload_present**: `0.345 * 100.0 = 34.5`, and Rust's `{:.0}` uses round-half-to-even, which would produce `"34"` not `"35"`. The implementer should use a value like `Some(0.35)` (which gives exactly 35.0) to avoid the ambiguity, or the assertion should expect `"34"`. Trivial to fix at implementation time.

2. **Baseline sample count**: FR-06 heading `vs {N}-feature baseline` cannot be rendered because `BaselineComparison` does not carry `sample_count`. The pseudocode renders `## Outliers` without the count. This was accepted in the original report and remains unchanged.
