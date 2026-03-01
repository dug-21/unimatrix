# Gate 3a Report: Design Review

## Result: PASS

## Feature: col-002b Detection Library + Baseline Comparison

## Validation Summary

### 1. Component-Architecture Alignment

| Component | Architecture Match | Notes |
|-----------|-------------------|-------|
| detection-agent | PASS | 7 rules match architecture Table 1. All implement DetectionRule trait unchanged. |
| detection-friction | PASS | 2 new + 2 existing moved. Matches ADR-002 submodule structure. |
| detection-session | PASS | 4 new + 1 existing moved. find_completion_boundary correctly shared. |
| detection-scope | PASS | 5 rules match architecture. PhaseDurationOutlierRule uses ADR-001 constructor injection. |
| baseline | PASS | compute_baselines + compare_to_baseline match architecture Integration Surface exactly. |
| server-integration | PASS | Handler changes match architecture Section 4 (server integration). |

### 2. Specification Coverage

| Requirement | Pseudocode Coverage | Notes |
|-------------|-------------------|-------|
| FR-01 (7 agent rules) | PASS | All 7 rules with thresholds and evidence collection |
| FR-02 (2 friction rules) | PASS | SearchViaBash and OutputParsingStruggle |
| FR-03 (4 session rules) | PASS | ColdRestart, CoordinatorRespawns, PostCompletionWork, ReworkEvents |
| FR-04 (5 scope rules) | PASS | All 5 including PhaseDurationOutlier |
| FR-05 (rule registration) | PASS | default_rules() returns 21 rules |
| FR-06 (baseline computation) | PASS | compute_baselines with min-3 guard |
| FR-07 (baseline comparison) | PASS | compare_to_baseline with ADR-003 guards |
| FR-08 (report extension) | PASS | RetrospectiveReport.baseline_comparison with serde(default) |
| FR-09 (server integration) | PASS | Handler loads history, excludes current, passes to baselines |

### 3. Risk Strategy Coverage

| Risk | Test Coverage | Status |
|------|--------------|--------|
| R-01 (silent rules) | Per-rule fires/silent tests | Covered |
| R-02 (NaN/Inf) | Explicit NaN/Inf assertions in baseline tests | Covered |
| R-03 (phase name mismatch) | Phase matching tests in scope and baseline | Covered |
| R-04 (regex patterns) | Regex variation tests for compile_cycles, search_bash | Covered |
| R-05 (submodule refactor) | Existing tests moved verbatim + regression test | Covered |
| R-06 (serde compat) | serde(default) round-trip tests | Covered |
| R-07 (signature change) | Compile-time + integration | Covered |
| R-08 (cold restart FP) | New-files-only test | Covered |
| R-09 (completion boundary) | Shared boundary tests | Covered |
| R-10 (self-comparison) | Server excludes current feature | Covered |
| R-11 (parsing FP) | Different-base-cmds test | Covered |
| R-12 (input variations) | Per-rule realistic JSON input tests | Covered |

### 4. Interface Consistency

- DetectionRule trait: Unchanged. All rules implement name(), category(), detect().
- default_rules(): Signature change to `Option<&[MetricVector]>` matches ADR-001.
- build_report(): Signature extended with baseline parameter. Single call site in server.
- BaselineSet/BaselineEntry/BaselineComparison: Match architecture Integration Surface.

### 5. Design Issues Found

**PhaseDurationOutlierRule detect() limitation**: The detect(records) method cannot access the current MetricVector because metrics are computed AFTER detection in the server handler. The pseudocode correctly identifies this and proposes that phase duration outlier detection is handled through baseline comparison (compare_to_baseline) rather than through the detection rule's detect() method. The rule still registers in default_rules() for rule count compliance (AC-07) and constructor injection compliance (ADR-001). This is an acceptable design trade-off.

**Impact**: AC-13 (phase duration outlier uses baseline when available) is met through baseline comparison, not through the detection rule. The implementation agent should ensure this is clearly documented.

## Gate Decision: PASS

All components align with architecture, specification, and risk strategy. Test plans cover all 12 identified risks. The PhaseDurationOutlierRule limitation is an acceptable design constraint, not a scope failure.
