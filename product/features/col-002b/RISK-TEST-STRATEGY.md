# Risk-Based Test Strategy: col-002b

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Detection rules silently produce no findings due to incorrect record field access patterns | High | Med | High |
| R-02 | Baseline stddev computation produces NaN/Inf with degenerate data (all identical, all zero) | Med | Med | Medium |
| R-03 | Phase duration outlier rule fails when historical MetricVectors have no matching phase names | Med | Med | Medium |
| R-04 | Regex patterns in rules (compile cycles, search-via-bash, output parsing) miss valid command variations | Med | High | Medium |
| R-05 | detection.rs refactor into submodules breaks col-002's existing 3 rules | High | Low | Medium |
| R-06 | RetrospectiveReport serde(default) does not cover new baseline_comparison field, breaking cached report deserialization | Med | Low | Medium |
| R-07 | default_rules() signature change breaks server handler call site | Low | High | Low |
| R-08 | Cold restart detection false positives from normal session pauses (lunch break, meeting) | Low | Med | Low |
| R-09 | Post-completion boundary detection fails when TaskUpdate records use non-standard status strings | Med | Med | Medium |
| R-10 | Baseline comparison excludes current feature but includes previous runs of the same feature (re-analysis) | Med | Low | Medium |
| R-11 | Output parsing struggle rule false positives from legitimately different cargo commands in sequence | Low | Med | Low |
| R-12 | ObservationRecord.input field structure varies across tool types, rules must handle all shapes | High | Med | High |

## Risk-to-Scenario Mapping

### R-01: Rules Produce No Findings Due to Incorrect Field Access
**Severity**: High
**Likelihood**: Med
**Impact**: An entire hotspot category returns empty results. Metrics look clean when problems exist. False confidence in project health.

**Test Scenarios**:
1. For each of the 18 rules: provide synthetic records designed to exceed the threshold, verify a finding is produced
2. For each rule: provide synthetic records below threshold, verify no false positive
3. For each rule: provide records with the exact boundary value, verify threshold comparison direction (> vs >=)
4. For each rule: provide empty record set, verify no crash and empty findings

**Coverage Requirement**: Per-rule unit tests with synthetic records. Every rule must have both a "fires" and "does not fire" test case.

### R-02: Baseline Arithmetic Degenerate Cases
**Severity**: Med
**Likelihood**: Med
**Impact**: NaN/Inf in MCP tool response. JSON serialization failure. Report is unusable.

**Test Scenarios**:
1. Three MetricVectors with identical values for all metrics — verify stddev is 0.0, no outlier flags, status is "no_variance"
2. Three MetricVectors where all values are 0.0 — current has non-zero value — verify status is "new_signal"
3. Three MetricVectors with normal distribution — verify mean and stddev match expected values
4. Exactly 3 MetricVectors (minimum) — verify baselines compute correctly
5. Verify no f64 value in BaselineComparison is NaN or Inf (explicit assertion)

**Coverage Requirement**: Unit tests in baseline module. Explicit NaN/Inf guards in compute_baselines().

### R-03: Phase Duration Outlier with Mismatched Phase Names
**Severity**: Med
**Likelihood**: Med
**Impact**: Rule never fires because historical phase names differ from current (e.g., "3a" vs "stage-3a").

**Test Scenarios**:
1. Historical data has phase "3a", current has phase "3a" — outlier detection works
2. Historical data has phase "3a", current has phase "design" — no baseline available, falls back to absolute threshold
3. Historical data has fewer than 3 entries for a phase name — falls back to absolute threshold
4. Current feature introduces a new phase name not in history — absolute threshold fallback

**Coverage Requirement**: Unit tests for PhaseDurationOutlierRule with various phase name combinations.

### R-04: Regex Patterns Miss Command Variations
**Severity**: Med
**Likelihood**: High
**Impact**: Compile cycles, search-via-bash, and output parsing rules undercount events.

**Test Scenarios**:
1. Compile cycles: `cargo test`, `cargo test --workspace`, `cargo test -p unimatrix-store`, `RUSTFLAGS=... cargo check` — verify all matched
2. Compile cycles: `cargo add serde`, `cargo fmt` — verify NOT matched (not compile commands)
3. Search-via-bash: `find . -name "*.rs"`, `grep -r "pattern"`, `rg "pattern"`, `grep pattern file.rs` — verify all matched
4. Search-via-bash: `echo "finding things"`, `cat grep_results.txt` — verify NOT matched
5. Output parsing: `cargo test 2>&1 | grep FAIL`, `cargo test 2>&1 | tail -20`, `cargo test 2>&1 | head -5` — within 3 min, verify detected as struggle

**Coverage Requirement**: Regex test cases with diverse command formats. Boundary cases for each regex pattern.

### R-05: Detection Submodule Refactor Breaks Existing Rules
**Severity**: High
**Likelihood**: Low
**Impact**: col-002's 3 pipeline-proof rules stop working. Regression in existing functionality.

**Test Scenarios**:
1. After refactor, run col-002's existing detection rule unit tests — all pass
2. Verify permission retries, session timeout, and sleep workarounds produce identical findings as before refactor
3. Verify detect_hotspots() with all 21 rules returns findings from both col-002 and col-002b rules

**Coverage Requirement**: col-002's existing detection tests must pass unchanged. No behavioral regression.

### R-06: RetrospectiveReport Deserialization Compatibility
**Severity**: Med
**Likelihood**: Low
**Impact**: If RetrospectiveReport is ever serialized (e.g., for caching), the new field breaks deserialization of old reports.

**Test Scenarios**:
1. Verify `baseline_comparison` field has `#[serde(default)]` annotation
2. Create a RetrospectiveReport without baseline_comparison, serialize, deserialize — verify baseline_comparison is None
3. Create a RetrospectiveReport with baseline_comparison, serialize, deserialize — verify roundtrip

**Coverage Requirement**: Unit test for serde(default) contract on the new field.

### R-07: default_rules() Signature Change
**Severity**: Low
**Likelihood**: High
**Impact**: Compilation failure at server call site. Known churn, not a logic bug.

**Test Scenarios**:
1. Server handler calls default_rules(Some(&history)) — compiles and returns 21 rules
2. Server handler calls default_rules(None) — compiles and returns 21 rules with absolute thresholds

**Coverage Requirement**: Compile-time verification. Integration test exercises both call paths.

### R-08: Cold Restart False Positives
**Severity**: Low
**Likelihood**: Med
**Impact**: Noisy findings. LLM discusses normal breaks as problems.

**Test Scenarios**:
1. 35-minute gap followed by reads to NEW files (not previously read) — verify NOT detected
2. 35-minute gap followed by reads to previously-read files — verify detected
3. 25-minute gap (below threshold) followed by re-reads — verify NOT detected

**Coverage Requirement**: Unit test with precise timing and file path scenarios.

### R-09: Post-Completion Boundary Detection Failure
**Severity**: Med
**Likelihood**: Med
**Impact**: Post-completion work percentage is wrong. Post-delivery issues rule misidentifies boundary.

**Test Scenarios**:
1. Records contain TaskUpdate with `{"status": "completed"}` in input — verify boundary detected
2. Records contain no TaskUpdate — post-completion rules return no findings (no boundary = no detection)
3. Multiple TaskUpdate completions — use the LAST one as boundary

**Coverage Requirement**: Unit tests for boundary detection logic shared by post-completion work and post-delivery issues rules.

### R-10: Self-Comparison in Baseline
**Severity**: Med
**Likelihood**: Low
**Impact**: Current feature's own metrics inflate the baseline, reducing outlier sensitivity.

**Test Scenarios**:
1. History includes 4 MetricVectors, one has same feature_cycle as current — verify it is excluded
2. History has only 3 MetricVectors, excluding current leaves 2 — verify baseline returns None

**Coverage Requirement**: Server handler test verifying current feature exclusion from baseline history.

### R-11: Output Parsing Struggle False Positives
**Severity**: Low
**Likelihood**: Med
**Impact**: Normal command variation flagged as struggle.

**Test Scenarios**:
1. `cargo test | grep test_parse` then `cargo test | grep test_attr` within 3 min — detected (same base command, different filter)
2. `cargo test` then `cargo build` within 3 min — NOT detected (different base commands)
3. `cargo test | grep test_parse` then same command repeated — NOT detected (same filter, not variation)

**Coverage Requirement**: Unit test with timestamp and command variation edge cases.

### R-12: ObservationRecord.input Field Variations
**Severity**: High
**Likelihood**: Med
**Impact**: Rules that parse `input` field crash or return wrong results when field structure differs from expectation.

**Test Scenarios**:
1. Read tool: input is `{"file_path": "/path/to/file"}` — file_breadth rule extracts path correctly
2. Bash tool: input is `{"command": "cargo test"}` — compile cycles rule extracts command correctly
3. Write tool: input is `{"file_path": "/path", "content": "..."}` — source file count extracts path correctly
4. Edit tool: input is `{"file_path": "/path", "old_string": "...", "new_string": "..."}` — mutation spread extracts path correctly
5. Input is None (SubagentStop) — all rules handle gracefully, no crash
6. Input has unexpected structure (missing expected key) — rule skips record, no crash

**Coverage Requirement**: Per-rule unit tests with realistic input JSON structures. Defensive parsing tests.

## Integration Risks

- **detection.rs refactor**: Moving col-002's existing rules into submodules could break import paths in col-002's tests. Mitigation: ensure mod.rs re-exports everything from the old public API.
- **default_rules() signature**: Server handler must update from `default_rules()` to `default_rules(Some(&history))`. Mitigation: single call-site change, caught at compile time.
- **build_report() signature**: Adding baseline parameter changes the function signature. Mitigation: single call-site in server handler.
- **Baseline history loading**: Server must deserialize potentially many MetricVectors. If any deserialization fails (corrupted data), the entire baseline computation fails. Mitigation: skip individual failures with logging, proceed with available data.

## Edge Cases

- Zero records attributed to feature — detection rules return empty, no baselines computed
- Exactly 3 historical MetricVectors (minimum) — baselines compute but with high variance
- Historical MetricVector with empty phases map — phase-specific baselines have no data for those phases
- Phase name is empty string (from malformed task subject) — baseline groups by empty string, treated as valid phase
- All 18 rules fire simultaneously on a pathological record set — report may be large but functional
- Bash command contains newlines — regex matching should use single-line mode or handle multi-line input
- File paths with spaces, special characters — path-based rules must handle without crash

## Security Risks

- **No new security surface.** col-002b adds pure computation logic within the existing observe crate. No new external inputs, no new MCP parameters, no new file I/O paths. All data flows through col-002's existing security boundaries (hook input sanitization, session_id validation, observation directory permissions).
- **Regex denial of service**: Complex regex patterns on untrusted input (Bash command strings from hooks) could cause catastrophic backtracking. Mitigation: use simple, non-nested regex patterns. Avoid `(.*)` patterns. Test with adversarial inputs.

## Failure Modes

- **Individual rule crash**: If one rule panics, `detect_hotspots()` propagates the panic. Mitigation: consider catch_unwind or Result-returning detect(), but the simpler approach is thorough testing — these are pure functions on known data shapes.
- **Baseline computation with corrupted history**: Deserialization failure on one MetricVector. Mitigation: server handler filters out deserialization failures, computes baseline from remaining valid vectors.
- **All rules return empty**: Legitimate scenario for a clean feature. Not a failure — the report shows zero hotspots and (if available) the baseline comparison.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (baseline stddev edge cases) | R-02 | ADR-003 defines explicit guards: zero-stddev = "no_variance", zero-mean + zero-stddev = "new_signal", no NaN/Inf propagation |
| SR-02 (MetricVector deserialization) | R-06 | col-002b does not modify MetricVector. RetrospectiveReport extension uses serde(default). No stored data compatibility impact. |
| SR-03 (performance with 18 rule passes) | — | Accepted. 18 rules on 5K records is <2s (NFR-01). Each rule is a single pass. No optimization needed. |
| SR-04 (phase duration outlier ordering) | R-03 | ADR-001: baseline data injected via constructor. Rule works with or without history. No ordering dependency within col-002b implementation. |
| SR-05 (UniversalMetrics field coverage) | R-01 | Architecture confirms all 18 rules map to existing fields or produce findings (not new metric fields). Verified via record access pattern table in spec. |
| SR-06 (insufficient baseline history) | — | Accepted. Minimum 3 vectors enforced. "Insufficient history" message when unmet. Not a risk — designed behavior. |
| SR-07 (col-002 not yet implemented) | R-05, R-07, R-12 | Design against col-002's specified interfaces. Detection submodule refactor is backwards-compatible. Field access patterns documented per rule. |
| SR-08 (record field patterns) | R-12 | Each rule's expected record field access documented in specification. Defensive parsing required — rules skip records with unexpected structure. |
| SR-09 (baseline deserialization) | R-10 | Server uses same serialize/deserialize helpers from col-002 ADR-002. Current feature excluded from own baseline. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-05, R-12) | 14 scenarios |
| Medium | 5 (R-02, R-03, R-04, R-09, R-10) | 19 scenarios |
| Low | 4 (R-06, R-07, R-08, R-11) | 11 scenarios |
