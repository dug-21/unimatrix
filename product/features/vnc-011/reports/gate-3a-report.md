# Gate 3a Report: vnc-011

> Gate: 3a (Design Review)
> Date: 2026-03-10
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | One open question (baseline sample count) is a pragmatic omission, acceptable |
| Specification coverage | FAIL | FR-09 narrative summary not captured in pseudocode; evidence_limit default contradicts human override |
| Risk coverage | PASS | All 14 risks + 4 integration risks mapped to test scenarios |
| Interface consistency | WARN | CollapsedFinding struct omits narrative summary field; context_reload_pct scale mismatch between pseudocode and test plan |

## Detailed Findings

### 1. Architecture Alignment
**Status**: WARN

**Evidence**: Component boundaries match architecture exactly:
- C1 (params-extension) in `tools.rs` -- matches Architecture C1
- C2 (retrospective-formatter) in `response/retrospective.rs` -- matches Architecture C2 and ADR-003
- C3 (handler-dispatch) in `tools.rs` -- matches Architecture C3

Feature gate `#[cfg(feature = "mcp-briefing")]` correctly specified in OVERVIEW.md module registration and handler-dispatch pseudocode.

Technology choices (no new crate deps, `std::fmt::Write`, deterministic timestamp selection per ADR-002) are consistent.

**Minor gap**: OVERVIEW.md open question #1 notes that `BaselineComparison` does not carry `sample_count`, so the FR-06 heading `vs {N}-feature baseline` cannot be rendered. The pseudocode omits it. This is a pragmatic decision -- the data is not available at the formatter's consumption point. WARN, not FAIL: the architecture's Integration Surface table does not list `sample_count` as available to the formatter either.

### 2. Specification Coverage
**Status**: FAIL

**Issue 1 -- FR-09 narrative summary not rendered (FAIL)**:
FR-09 states: "render the `summary` as the finding's description line." The pseudocode's `CollapsedFinding` struct does not store the narrative `summary` field. The `collapse_findings` function extracts `cluster_count` and `sequence_pattern` from matched narratives but ignores `summary`. The `render_findings` function renders `finding.claims[0]` as the description line regardless of whether a matching narrative exists.

The architecture also specifies this: "render the `summary` as the finding's description line" is in the Architecture C2 sub-responsibilities for Finding collapse.

Fix: Add `narrative_summary: Option<String>` to `CollapsedFinding`. In `collapse_findings`, populate it from `narrative.map(|n| n.summary.clone())`. In `render_findings`, when `narrative_summary` is `Some`, use it instead of `claims[0]`.

**Issue 2 -- evidence_limit default contradicts human override (FAIL)**:
The IMPLEMENTATION-BRIEF Resolved Decisions table states: "evidence_limit for JSON path: NO change -- JSON path keeps its existing `unwrap_or(3)` default." The spawn prompt confirms this: "evidence_limit: JSON path keeps existing unwrap_or(3). Markdown path ignores evidence_limit entirely."

However, the handler-dispatch pseudocode line 40 uses `params.evidence_limit.unwrap_or(0)`, and the OVERVIEW data flow (line 25) also says `unwrap_or(0)`. These contradict the human override.

The handler-dispatch TEST PLAN correctly reflects the human override: `test_json_evidence_limit_default_3` asserts `unwrap_or(3)`. But the pseudocode itself is wrong.

Additionally, the test-plan OVERVIEW risk-to-test mapping for R-02 lists `test_json_no_evidence_limit_returns_all` which expects no truncation by default -- contradicting the human override that says JSON keeps `unwrap_or(3)`.

Fix: In handler-dispatch pseudocode, change `params.evidence_limit.unwrap_or(0)` to `params.evidence_limit.unwrap_or(3)`. In OVERVIEW data flow, change `evidence_limit.unwrap_or(0)` to `evidence_limit.unwrap_or(3)`. In test-plan OVERVIEW, update `test_json_no_evidence_limit_returns_all` to expect truncation to 3 by default.

**Issue 3 -- params-extension doc comment is misleading (WARN)**:
The params-extension pseudocode line 20 says `Maximum evidence items per hotspot (default: 0 = unlimited, JSON path only)`. Per the human override, the default is 3, not 0. The doc comment should say `(default: 3, JSON path only)`.

**All other FRs covered**:
- FR-01 (format parameter): params-extension adds `format: Option<String>`, handler-dispatch routes on it
- FR-02 (evidence_limit): addressed above
- FR-03 (header): render_header produces correct format
- FR-04 (session table): render_sessions matches spec columns
- FR-05 (attribution note): render_attribution_note matches spec format
- FR-06 (baseline filtering): render_baseline_outliers filters to Outlier/NewSignal (minus sample count, see WARN above)
- FR-07 (finding collapse): collapse_findings groups by rule_name, picks highest severity
- FR-08 (k=3 examples): deterministic earliest-by-timestamp per ADR-002
- FR-10 (recommendation dedup): render_recommendations deduplicates by hotspot_type
- FR-11 (zero-activity suppression): is_zero_activity_phase applied in phase outlier filtering
- FR-12 (knowledge reuse): render_knowledge_reuse matches spec
- FR-13 (rework/reload): render_rework_reload present, IN SCOPE per human override
- FR-14 (formatter data-driven): all logic in formatter module, no observe changes
- NFR-01 through NFR-04: addressed by design (pure string building, no new deps, immutable report consumption)

### 3. Risk Coverage
**Status**: PASS

All 14 risks from the Risk-Based Test Strategy are mapped to specific test scenarios:

| Risk | Test Plan Coverage |
|------|-------------------|
| R-01 (severity selection) | 3 tests: mixed severity, same severity, ordering |
| R-02 (evidence_limit inflation) | 3 tests: no limit, explicit limit, markdown ignores |
| R-03 (all-None fields) | 3+ tests: all None, single Some, per-field iteration |
| R-04 (narrative matching) | 3 tests: match, no match, sequence pattern |
| R-05 (evidence edge cases) | 5 tests: empty, 1, 3, 10, same timestamp |
| R-06 (session table edges) | 4 tests: empty dist, zero duration, no outcome, normal |
| R-07 (baseline filtering) | 4 tests: all normal, mixed, empty vec, single outlier |
| R-08 (recommendation dedup) | 3 tests: same type, distinct, empty |
| R-09 (zero-activity suppression) | 1 test: phase suppressed |
| R-10 (duration formatting) | 3 tests: zero, over 24h, standard |
| R-11 (pipe in metric name) | 1 test: pipe character |
| R-12 (f64 artifacts) | 1 test: float sum |
| R-13 (invalid format) | 4 tests: markdown, json, none, invalid |
| R-14 (phase outlier suppression) | 1 test: zero-activity phase in outliers |

Integration risks IR-01 through IR-04 are also covered (compile checks, dispatch routing, clone-truncate scoping, feature gate).

Edge cases from the risk strategy (empty hotspots, single finding, large report performance, unicode, NaN, stddev=0, duplicate rule_names) are all present in the test plan.

### 4. Interface Consistency
**Status**: WARN

**CollapsedFinding struct**: The OVERVIEW, architecture, and formatter pseudocode all define `CollapsedFinding` consistently with the same 8 fields. However, as noted in the FR-09 FAIL, the struct is missing a `narrative_summary` field needed to implement the spec.

**format_retrospective_markdown signature**: Consistent across architecture, OVERVIEW, implementation brief, and formatter pseudocode: `pub fn format_retrospective_markdown(report: &RetrospectiveReport) -> CallToolResult`.

**render helper signatures**: All match between architecture internal function table and formatter pseudocode, plus `render_rework_reload` and `is_zero_activity_phase` which are additions beyond the architecture table (architecture did not list these, but they serve FR-13 and FR-11 respectively -- acceptable additions).

**context_reload_pct scale mismatch (WARN)**: The pseudocode `render_rework_reload` multiplies by 100.0 (`pct * 100.0`), treating the value as a fraction (0.0-1.0). Verified against source: `compute_context_reload_pct` in `session_metrics.rs` returns a fraction (e.g., 2.0/3.0 = 0.666). So the pseudocode is correct. However, the test plan for `test_reload_present` uses `Some(34.5)` and expects `"34.5% context reload"` -- this would actually render as `"3450% context reload"` per the pseudocode. The test setup value should be `Some(0.345)` to get `"34% context reload"`, or the assertion should expect `"3450% context reload"`. This is a test plan bug.

**format_duration edge case (WARN)**: Test plan `test_duration_exact_hour` expects `"1h 0m"` but pseudocode's `format_duration(3600)` would produce `"1h"` since the `hours > 0 && minutes > 0` branch fails (minutes is 0), falling to `hours > 0` which returns `format!("{}h", hours)`. The test expectation or pseudocode needs alignment. Minor.

**Data flow coherence**: The params-extension feeds `format: Option<String>` to handler-dispatch, which matches on the string and routes to the correct formatter. The formatter consumes `&RetrospectiveReport` immutably. Data flow is coherent across all three components.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| FR-09: narrative summary not captured or rendered | retrospective-formatter agent | Add `narrative_summary: Option<String>` to `CollapsedFinding`. In `collapse_findings`, set from `narrative.map(|n| n.summary.clone())`. In `render_findings`, use narrative_summary when Some as the description line instead of `claims[0]`. |
| evidence_limit default `unwrap_or(0)` should be `unwrap_or(3)` | handler-dispatch agent | Change `params.evidence_limit.unwrap_or(0)` to `unwrap_or(3)` in handler-dispatch pseudocode. Update OVERVIEW data flow line similarly. |
| Test plan OVERVIEW R-02 test name contradicts human override | handler-dispatch agent | Rename/update `test_json_no_evidence_limit_returns_all` to `test_json_no_evidence_limit_defaults_to_3` and update expected behavior. |
| params-extension doc comment says default 0 | params-extension agent | Change doc comment to `(default: 3, JSON path only)`. |
| Test plan context_reload_pct setup value | retrospective-formatter agent | Change `test_reload_present` setup from `Some(34.5)` to `Some(0.345)` and update assertion to `"34% context reload"` or `"35% context reload"`. |
| Test plan format_duration exact hour expectation | retrospective-formatter agent | Change `test_duration_exact_hour` expected from `"1h 0m"` to `"1h"`, OR update pseudocode to render `"1h 0m"` when minutes == 0 and hours > 0. Recommend updating the test to match pseudocode (`"1h"`). |
