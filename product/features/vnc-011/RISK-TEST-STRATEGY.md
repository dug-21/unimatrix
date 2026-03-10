# Risk-Based Test Strategy: vnc-011

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Finding collapse produces incorrect severity when grouping findings with mixed severities (e.g., Info + Critical in same rule_name group) | High | Med | High |
| R-02 | evidence_limit default change from 3 to 0 silently inflates JSON response size for existing consumers | High | Med | High |
| R-03 | Formatter panics or produces malformed markdown when all Optional fields are None (minimal report) | High | Med | High |
| R-04 | Narrative-to-finding matching fails when HotspotNarrative.hotspot_type does not exactly match HotspotFinding.rule_name | Med | High | High |
| R-05 | Timestamp-based k=3 example selection produces duplicate or empty evidence when evidence pool has identical timestamps or is empty | Med | Med | Med |
| R-06 | Session table rendering breaks with edge-case SessionSummary data (empty tool_distribution, zero duration, missing outcome) | Med | Med | Med |
| R-07 | Baseline outlier filtering omits the section header or sample count when all comparisons are Normal/NoVariance | Med | Med | Med |
| R-08 | Recommendation deduplication by hotspot_type silently drops recommendations with distinct actions but same hotspot_type | Med | Med | Med |
| R-09 | Zero-activity phase suppression heuristic hides phases with tool_call_count=1 and duration_secs=0 (a legitimate single-call phase) | Low | Med | Low |
| R-10 | Duration formatting produces incorrect output for edge cases (0 seconds, >24 hours, fractional seconds from f64 conversion) | Low | Med | Low |
| R-11 | Markdown table alignment breaks when metric names or values contain pipe characters or are extremely long | Low | Low | Low |
| R-12 | CollapsedFinding total_events sum of f64 measured values produces floating-point artifacts in rendered output | Low | Med | Low |
| R-13 | Format parameter accepts arbitrary strings beyond "markdown"/"json", leading to undefined behavior | Med | Med | Med |
| R-14 | Phase outlier rendering does not apply zero-activity suppression, showing outlier rows for suppressed phases | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Finding collapse severity selection
**Severity**: High
**Likelihood**: Med
**Impact**: Findings are mis-prioritized in output. A Critical finding could be rendered as Info if the severity comparison is inverted.

**Test Scenarios**:
1. Group 3 findings with same rule_name: Info, Warning, Critical. Assert collapsed finding shows Critical.
2. Group findings where all share the same severity. Assert that severity is preserved exactly.
3. Verify F-XX ordering: Critical groups sort before Warning, which sort before Info.

**Coverage Requirement**: Unit test with explicit Severity ordering assertion on CollapsedFinding output.

### R-02: evidence_limit default change inflates JSON
**Severity**: High
**Likelihood**: Med
**Impact**: JSON consumers receive unbounded evidence arrays by default. For reports with many evidence records, response size could grow significantly.

**Test Scenarios**:
1. Call with format "json" and no evidence_limit. Assert all evidence records are present (unwrap_or(0) means no truncation).
2. Call with format "json" and evidence_limit=3. Assert evidence is truncated to 3.
3. Call with format "markdown" and any evidence_limit. Assert evidence_limit is ignored (formatter uses k=3 timestamp selection).

**Coverage Requirement**: Unit test verifying both default paths and explicit override.

### R-03: All-None optional fields
**Severity**: High
**Likelihood**: Med
**Impact**: Formatter panic on unwrap of None field, or malformed markdown with dangling section headers.

**Test Scenarios**:
1. Build RetrospectiveReport with all Optional fields as None, empty hotspots, empty recommendations. Assert valid markdown with header only.
2. Build report with only session_summaries as Some, all others None. Assert Sessions section present, all others absent.
3. Iterate through each Optional field: set one to Some while all others are None. Assert the corresponding section renders and all others are omitted.

**Coverage Requirement**: Exhaustive None-combination test covering all 8 Optional fields from the Architecture None-handling table.

### R-04: Narrative-to-finding matching
**Severity**: Med
**Likelihood**: High
**Impact**: Narrative enrichment silently fails. Findings render without cluster counts or sequence patterns even when narratives exist.

**Test Scenarios**:
1. Build report with narratives where hotspot_type matches rule_name exactly. Assert cluster count and summary appear in output.
2. Build report with narratives where hotspot_type does NOT match any finding's rule_name. Assert findings render from hotspot data alone without error.
3. Build report with narratives containing sequence_pattern. Assert pattern string appears inline in the finding.

**Coverage Requirement**: Unit test with matching and non-matching narrative data.

### R-05: Evidence example selection edge cases
**Severity**: Med
**Likelihood**: Med
**Impact**: Panic on empty slice, or misleading duplicate examples.

**Test Scenarios**:
1. Finding group with 0 evidence records across all findings. Assert no "Examples:" section or an empty examples block.
2. Finding group with exactly 1 evidence record. Assert 1 example bullet.
3. Finding group with exactly 3 records. Assert all 3 rendered.
4. Finding group with 10 records having distinct timestamps. Assert exactly 3 rendered, sorted by ts ascending (earliest 3).
5. Finding group with all records sharing the same timestamp. Assert 3 rendered without panic (stable selection by input order).

**Coverage Requirement**: Unit test per edge case.

### R-06: Session table edge cases
**Severity**: Med
**Likelihood**: Med
**Impact**: Malformed table rows, panic on HashMap access, or incorrect summary counts.

**Test Scenarios**:
1. SessionSummary with empty tool_distribution HashMap. Assert Calls column shows 0.
2. SessionSummary with duration_secs=0. Assert Window column renders start time with "(0m)" or similar.
3. SessionSummary with outcome as None (if applicable) or empty string. Assert "-" or graceful fallback.
4. Two sessions with normal data. Assert table has correct row count and column alignment.

**Coverage Requirement**: Unit test with boundary SessionSummary values.

### R-07: Baseline outlier filtering produces empty section
**Severity**: Med
**Likelihood**: Med
**Impact**: Empty "## Outliers" heading with no table, or section not omitted when it should be.

**Test Scenarios**:
1. All baseline comparisons are Normal. Assert "## Outliers" section is absent from output.
2. Mix of Normal, Outlier, NewSignal, NoVariance. Assert only Outlier and NewSignal rows appear.
3. baseline_comparison is Some but empty Vec. Assert section is omitted.
4. Single Outlier entry. Assert section renders with one data row.

**Coverage Requirement**: Unit test per filtering scenario.

### R-08: Recommendation dedup drops distinct actions
**Severity**: Med
**Likelihood**: Med
**Impact**: User sees fewer recommendations than intended. Two different actions for the same hotspot_type are collapsed to one.

**Test Scenarios**:
1. Two recommendations with same hotspot_type but different action strings. Assert only first action is rendered.
2. Three recommendations with distinct hotspot_types. Assert all three rendered.
3. Empty recommendations Vec. Assert "## Recommendations" section is omitted.

**Coverage Requirement**: Unit test verifying dedup semantics match spec (first occurrence wins).

### R-13: Invalid format parameter
**Severity**: Med
**Likelihood**: Med
**Impact**: Unrecognized format string could panic, return empty response, or fall through to wrong path.

**Test Scenarios**:
1. format = Some("markdown") -- assert markdown output.
2. format = Some("json") -- assert JSON output.
3. format = None -- assert markdown output (default).
4. format = Some("xml") -- assert graceful error or fallback to markdown (spec does not define behavior for unknown formats).

**Coverage Requirement**: Unit test for each format variant including invalid input.

## Integration Risks

| Risk ID | Risk | Component Boundary | Test Approach |
|---------|------|--------------------|---------------|
| IR-01 | Formatter reads RetrospectiveReport types from unimatrix-observe. If any field type changes in observe (e.g., Severity enum variant rename), formatter compiles but produces wrong output. | observe -> server | Compilation catches type mismatches. Snapshot tests catch semantic drift. |
| IR-02 | Handler dispatch in tools.rs must correctly route to format_retrospective_markdown vs format_retrospective_report. Incorrect conditional means wrong format returned. | tools.rs -> response/ | Integration test calling handler with both format values. |
| IR-03 | The clone-and-truncate step (col-010b) must only apply to JSON path. If applied to markdown path, evidence is truncated before formatter sees it. | tools.rs evidence_limit logic | Unit test: markdown path receives full report regardless of evidence_limit param. |
| IR-04 | New retrospective.rs module must be gated behind #[cfg(feature = "mcp-briefing")]. Missing gate means compilation fails when feature is disabled. | response/mod.rs | Compile test with feature disabled (cargo test without mcp-briefing feature). |

## Edge Cases

- **Empty hotspots Vec**: No findings to collapse. Assert "## Findings (0)" or section omitted.
- **Single finding with single evidence record**: Collapsed finding is trivially the finding itself. No grouping needed.
- **Very large report**: 50 hotspot findings across 10 rule_names, 100 baseline comparisons. Assert formatter completes in <5ms (NFR-03).
- **Unicode in claim/description strings**: Metric names or evidence descriptions containing non-ASCII characters. Assert markdown renders correctly.
- **Negative or NaN measured values**: f64 measured could theoretically be NaN or negative. Assert no panic in formatting.
- **BaselineComparison with stddev=0.0**: Division by stddev for z-score rendering could produce infinity. Formatter should render raw values.
- **Duplicate rule_names across different categories**: Two findings with same rule_name but different HotspotCategory. Spec groups by rule_name only -- verify this is intentional.

## Security Risks

This feature introduces no new attack surface. The formatter is a pure function consuming an in-memory struct and producing a string. No external input beyond the existing `format` and `evidence_limit` parameters (already validated strings/integers). No file I/O, no network calls, no deserialization of untrusted data beyond what `RetrospectiveParams` already handles via serde.

- **format parameter injection**: The `format` field is an `Option<String>` parsed from JSON. It is matched against known values ("markdown", "json"). No injection risk -- the string is compared, never interpolated into commands or queries.
- **Markdown injection via report data**: Evidence descriptions and claims originate from the observation pipeline (trusted internal data, not user input). If a claim contained markdown control characters (e.g., `|`, `#`), the table could be visually disrupted but there is no security impact since markdown is consumed by LLMs, not rendered in a browser.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Formatter encounters unexpected None on non-Optional field | Cannot happen -- non-Optional fields are guaranteed by RetrospectiveReport struct. Compiler enforces. | N/A |
| Formatter encounters empty evidence pool during k=3 selection | Render finding without "Examples:" section. No panic. | Defensive check: `if evidence_pool.is_empty() { skip examples }` |
| Format parameter is unrecognized string | Return error or fall back to markdown default. Handler must not silently succeed with wrong format. | Match arm with explicit default or error. |
| evidence_limit=0 on JSON path with very large evidence arrays | Large JSON response. No crash. Server returns full data as designed (ADR-001). | Caller can pass evidence_limit explicitly. |
| Narrative matching finds no matching narrative for a finding | Finding renders from hotspot data alone. No cluster count, no sequence pattern. Degraded but valid output. | Fallback path in render_findings. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Optional field None handling) | R-03 | Architecture enumerates all 8 Optional fields with explicit None behavior table. Formatter omits sections for None fields. |
| SR-02 (Non-deterministic k=3 selection) | R-05 | ADR-002 resolves: deterministic selection by timestamp ascending. Testing is now straightforward with assert_eq. |
| SR-03 (evidence_limit default change) | R-02 | ADR-001 resolves: global change to 0 is intentional. JSON gets full data by default. Markdown ignores evidence_limit entirely. |
| SR-04 (Scope/vision mismatch on actionability) | -- | Acknowledged in architecture. Deferred from this feature. No architecture risk. |
| SR-05 (Formatter-only constraint limits future refactoring) | -- | Accepted for MVP. CollapsedFinding internal struct is the right abstraction layer. |
| SR-06 (Zero-activity suppression heuristic) | R-09 | Heuristic documented. Low risk accepted. |
| SR-07 (col-020b type instability) | R-04 | Formatter handles FeatureKnowledgeReuse as Option. If type shape changes, compilation catches it. Narrative matching by string is the real risk (R-04). |
| SR-08 (Cross-crate boundary) | IR-01 | Formatter consumes observe types read-only. No cross-crate mutation. Module gated behind mcp-briefing feature (IR-04). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-02, R-03, R-04) | 12 scenarios |
| Medium | 5 (R-05, R-06, R-07, R-08, R-13) | 16 scenarios |
| Low | 4 (R-09, R-10, R-11, R-12) | 4 scenarios (basic coverage) |
| Integration | 4 (IR-01 through IR-04) | 4 scenarios |
| **Total** | **17** | **36 scenarios** |
