# Risk-Based Test Strategy: nan-010

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Module pre-split for `aggregate.rs` and `render.rs` executed in wrong order, causing 500-line breach before boundary module exists | High | High | Critical |
| R-02 | `distribution_change` and `distribution_targets` extracted after `[profile]` strip, silently producing `None` with no error for a `distribution_change = true` profile | High | Med | High |
| R-03 | Baseline profile with `distribution_change = true` silently accepted (no `ConfigInvariant`), wrong gate mode applied | High | Med | High |
| R-04 | `profile-meta.json` not written atomically: partial-write leaves a corrupt sidecar that causes `eval report` to silently fall back to zero-regression mode for a distribution-change profile | High | Med | High |
| R-05 | `check_distribution_targets` computes `overall_passed` incorrectly: MRR floor treated as co-equal target rather than independent veto, conflating two distinct failure modes | High | Med | High |
| R-06 | `render_distribution_gate_section` produces undistinguishable failure messages: "diversity failed" and "MRR floor failed" rendered identically, breaking AC-10 | Med | Med | High |
| R-07 | Corrupt `profile-meta.json` (non-JSON, truncated) handled by WARN+fallback in `run_report`, silently applying wrong gate to a distribution-change profile (ADR-002 changed this to abort; implementation regresses to fallback) | High | Med | High |
| R-08 | `AggregateStats` fields `mean_cc_at_k`, `mean_icd`, `mean_mrr` assumed present from nan-008; if renamed or absent, `check_distribution_targets` panics or returns wrong values at runtime | Med | Low | Med |
| R-09 | Multi-profile Section 5 heading level mismatch: `## 5.` rendered for multi-profile (should be `### 5.N`) or `### 5.N` rendered for single-profile (should be `## 5.`), breaking CI tooling that anchors on heading level | Med | Med | Med |
| R-10 | `profile-meta.json` schema divergence: `ProfileMetaFile` (runner) and deserialization in `load_profile_meta` (report) produced by independent types; a field name mismatch silently produces `distribution_change = false` for all profiles | Med | Med | Med |
| R-11 | Mandatory test modules absent at delivery: `eval/profile/tests.rs` additions and new `tests_distribution_gate.rs` not delivered, causing gate-3b failure (prior pattern in nan-009, entry #3579) | High | Med | High |
| R-12 | `eval report` exit code changes to non-zero on Distribution Gate failure — violates invariant C-07/FR-29 | High | Low | Med |
| R-13 | `find_regressions` output passed to Distribution Gate render path, causing spurious regression rows to appear in Section 5 for a `distribution_change = true` profile | Med | Low | Med |
| R-14 | `mrr_floor` compared against baseline MRR instead of candidate mean MRR, producing wrong pass/fail verdict | High | Low | Med |
| R-15 | Dual-type constraint violated: a field silently added to `ScenarioResult` in `runner/output.rs` without the matching `#[serde(default)]` in `report/mod.rs`, causing deserialization failures on pre-nan-010 result sets | High | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Module pre-split executed in wrong order
**Severity**: High
**Likelihood**: High
**Impact**: Any import line or doc comment added to `render.rs` before `render_distribution_gate.rs` exists breaches the 500-line limit. The workspace fails to compile. Entry #3580 (nan-009) confirms this happened in a prior feature.

**Test Scenarios**:
1. After the pre-split step (and before any feature code), run `wc -l` on `render.rs` and `aggregate.rs`; assert both are <= 500 lines and the boundary modules (`render_distribution_gate.rs`, `aggregate/distribution.rs`) exist and compile cleanly.
2. CI line-count check: the workspace rule (entry #161) must catch a breach at build time — verify the check runs before feature code is added.

**Coverage Requirement**: Static verification that pre-split is the first committed change; compile-time line count enforcement.

---

### R-02: Extraction after `[profile]` strip silently drops new fields
**Severity**: High
**Likelihood**: Med
**Impact**: A TOML with `distribution_change = true` and valid targets is parsed as `distribution_change = false`, `distribution_targets = None`. No error is returned. Wrong gate mode applied silently.

**Test Scenarios**:
1. AC-01 round-trip: parse a TOML with `distribution_change = true` and valid targets; assert `EvalProfile.distribution_change == true` and `distribution_targets == Some(...)`.
2. AC-02 rejection: parse a TOML with `distribution_change = true` and no targets table; assert `EvalError::ConfigInvariant` is returned (not `Ok(EvalProfile { distribution_change: false, ... })`).
3. Ordering regression: verify `parse_profile_toml` reads from `raw.get("profile")` before calling any function that strips `[profile]`; inspect the code path or trace test failure to the silent-drop scenario.

**Coverage Requirement**: AC-01, AC-02, AC-03 all exercise the extraction-before-strip path; all three must pass.

---

### R-03: Baseline profile with `distribution_change = true` accepted silently
**Severity**: High
**Likelihood**: Med
**Impact**: Baseline profile gets Distribution Gate applied instead of serving as the reference distribution. Report output is semantically wrong. Resolved as hard `ConfigInvariant` (SCOPE.md §Design Decisions #7), but implementation may miss the baseline check.

**Test Scenarios**:
1. Parse a TOML that is identified as the baseline profile and has `distribution_change = true`; assert `EvalError::ConfigInvariant` with message containing "baseline profile must not declare `distribution_change = true`".
2. Parse a non-baseline TOML with `distribution_change = true`; assert no error (baseline check does not over-fire).

**Coverage Requirement**: Explicit test for baseline-as-distribution-change rejection; error message must be present in assertion (NFR-06).

---

### R-04: Non-atomic sidecar write leaves corrupt state
**Severity**: High
**Likelihood**: Med
**Impact**: A crash between writing JSON and rename leaves a `.tmp` artifact that `eval report` ignores — clean. But if the rename path is not used (plain `write`), a partial file exists as `profile-meta.json`, which `run_report` parses as malformed and (per ADR-002) must abort. If the implementation uses a plain write instead, the fallback to zero-regression mode silently applies wrong gate.

**Test Scenarios**:
1. AC-05: Inspect `profile-meta.json` written by `write_profile_meta`; assert it is fully valid JSON and contains all expected profiles with correct schema.
2. Atomic path test: simulate a write of `profile-meta.json.tmp` followed by rename; verify only `profile-meta.json` exists (no orphan `.tmp` on success).
3. Leftover `.tmp` ignored: create a `profile-meta.json.tmp` in the results directory with invalid content; run `eval report`; assert it reads `profile-meta.json`, not `.tmp`, and produces correct output.

**Coverage Requirement**: AC-05; explicit test that `.tmp` is not read by `eval report`.

---

### R-05: `overall_passed` conflates diversity failure and MRR floor failure
**Severity**: High
**Likelihood**: Med
**Impact**: AC-10 requires distinguishable failure modes. If `DistributionGateResult` computes a single `overall_passed` bool without separately tracking `diversity_passed` and `mrr_floor_passed`, the renderer cannot distinguish them. ADR-003 defines four states (pass/pass, pass/fail, fail/pass, fail/fail).

**Test Scenarios**:
1. `check_distribution_targets` with CC@k passing, ICD passing, MRR floor failing: assert `diversity_passed = true`, `mrr_floor_passed = false`, `overall_passed = false`.
2. `check_distribution_targets` with CC@k failing, ICD passing, MRR floor passing: assert `diversity_passed = false`, `mrr_floor_passed = true`, `overall_passed = false`.
3. `check_distribution_targets` with all three passing: assert `diversity_passed = true`, `mrr_floor_passed = true`, `overall_passed = true`.
4. `check_distribution_targets` with CC@k and MRR both failing: assert both `diversity_passed` and `mrr_floor_passed` false, `overall_passed = false`.

**Coverage Requirement**: All four gate states covered in unit tests. AC-09, AC-13.

---

### R-06: Indistinguishable rendered failure messages
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-10 requires the rendered report to distinguish "Diversity targets not met" from "Diversity targets met, but ranking floor breached." If the renderer emits a generic "FAILED" for both, operators cannot diagnose the failure mode without re-running with raw stats.

**Test Scenarios**:
1. Render `DistributionGateResult` with `diversity_passed = false`, `mrr_floor_passed = true`; assert rendered string contains "Diversity targets not met" and does not contain "ranking floor breached".
2. Render `DistributionGateResult` with `diversity_passed = true`, `mrr_floor_passed = false`; assert rendered string contains "ranking floor breached" and does not contain "Diversity targets not met".
3. Render with both failing; assert both messages present.

**Coverage Requirement**: `render_distribution_gate_section` tested in `tests_distribution_gate.rs` with explicit string assertions on all three failure states (AC-10).

---

### R-07: Corrupt `profile-meta.json` abort semantics drift back to fallback during implementation
**Severity**: High
**Likelihood**: Med
**Impact**: The decision is resolved — ARCHITECTURE.md Component 7 and SCOPE.md §Design Decision #8 both specify abort + exit non-zero on corrupt sidecar (conflict resolved during design review). The risk is that the delivery agent implements WARN+fallback anyway, either by misreading an earlier version of the architecture or by reaching for the "safe" fallback pattern. Without an explicit test, a fallback regression would not be caught at gate.

**Test Scenarios**:
1. Write a malformed `profile-meta.json` (truncated JSON) to a results directory; run `eval report`; assert the process exits non-zero and emits an error message containing "profile-meta.json is malformed".
2. Write a structurally valid but semantically wrong `profile-meta.json` (version field missing); run `eval report`; assert process exits non-zero with a clear message.
3. Absent `profile-meta.json` (not corrupt — simply missing): run `eval report`; assert it renders Zero-Regression Check with no error (AC-11 backward compat, separate from the corrupt case).

**Coverage Requirement**: Corrupt vs. absent sidecar must be tested separately. AC-14 covers absent; the corrupt-abort case needs an explicit test in `tests_distribution_gate.rs`.

---

### R-08: `AggregateStats` fields renamed or absent from nan-008
**Severity**: Med
**Likelihood**: Low
**Impact**: `check_distribution_targets` reads `mean_cc_at_k`, `mean_icd`, `mean_mrr` from `AggregateStats`. If these were renamed (e.g., `avg_cc_at_k`) in a subsequent feature, the distribution gate reads zero or panics.

**Test Scenarios**:
1. AC-13: Provide a fixture `AggregateStats` with known `mean_cc_at_k`, `mean_icd`, `mean_mrr` values; verify `check_distribution_targets` produces correct `MetricGateRow.actual` values matching the fixture.
2. Compile-time: the Rust compiler catches field name mismatches — no additional runtime test needed beyond AC-13 passing.

**Coverage Requirement**: AC-13 with explicit field value assertions (not just pass/fail boolean) validates the field access path.

---

### R-09: Section 5 heading level wrong for single vs. multi-profile
**Severity**: Med
**Likelihood**: Med
**Impact**: CI tooling and downstream consumers (PR bots, documentation links) anchor on `## 5.` for single-profile reports. If the renderer always uses `### 5.N` regardless of profile count, the heading level breaks stable anchors. ADR-005 defines the distinction explicitly.

**Test Scenarios**:
1. Render Section 5 for a single non-baseline candidate with `distribution_change = true`; assert heading is `## 5. Distribution Gate` (not `### 5.1`).
2. Render Section 5 for two non-baseline candidates (one distribution-change, one not); assert headings are `### 5.1 Distribution Gate — {name}` and `### 5.2 Zero-Regression Check — {name}`.
3. Render Section 5 for a single non-baseline candidate with `distribution_change = false`; assert heading is `## 5. Zero-Regression Check` (existing behavior unchanged, AC-06).

**Coverage Requirement**: All three heading variants tested; existing single-profile zero-regression heading must not regress (AC-06).

---

### R-10: `profile-meta.json` schema mismatch between writer and reader types
**Severity**: Med
**Likelihood**: Med
**Impact**: `ProfileMetaFile` (written by runner) and the deserialization type used in `load_profile_meta` (in `report/mod.rs`) are independent types without a shared schema contract enforced at compile time. A field name typo in either silently deserializes as `distribution_change = false`.

**Test Scenarios**:
1. AC-05 + AC-07 round-trip: `eval run` writes `profile-meta.json`; `eval report` reads it from the same directory; assert Section 5 renders as "Distribution Gate" (not "Zero-Regression Check") for a distribution-change profile. This round-trip test catches schema mismatch at the integration boundary.
2. Direct JSON parse test: construct the exact JSON string matching the AC-07 schema; deserialize into `ProfileMetaEntry`; assert `distribution_change = true` and targets are populated.

**Coverage Requirement**: Round-trip integration test (runner writes, report reads) is the primary defense against schema divergence.

---

### R-11: Mandatory test modules absent at delivery
**Severity**: High
**Likelihood**: Med
**Impact**: nan-009 (entry #3579) had delivery wave produce production code with zero mandatory tests — entire test modules absent. For nan-010, the required test files are: extended `eval/profile/tests.rs` (AC-01 through AC-04) and new `eval/report/tests_distribution_gate.rs` (AC-05 through AC-14). If the latter is absent, gate-3b will reject the delivery.

**Test Scenarios**:
1. Gate-3b check: verify `eval/report/tests_distribution_gate.rs` exists as a file with non-trivial content covering AC-07 through AC-14.
2. Gate-3b check: verify `eval/profile/tests.rs` contains tests for AC-01, AC-02, AC-03 (one test per missing field), and AC-04.
3. Test names pre-declared: the tester must list required test function names in the coverage plan so gate-3b can verify by grepping, not by reading (entry #2758).

**Coverage Requirement**: Both test files must exist and contain named tests for every AC before gate-3b is claimed. Non-negotiable test names should be listed in the test plan.

---

### R-12: `eval report` exit code changes to non-zero on gate failure
**Severity**: High
**Likelihood**: Low
**Impact**: Invariant C-07/FR-29 is well-specified and tested. The risk is implementation error (e.g., the distribution gate result is wired to process exit by mistake). Any CI pipeline that relies on `eval report` exiting 0 breaks silently.

**Test Scenarios**:
1. Run `eval report` against a results directory where the Distribution Gate fails (CC@k below target); assert process exits 0.
2. Run `eval report` against a results directory where MRR floor is breached; assert process exits 0.
3. Both tests must check the actual process exit code, not just the report string.

**Coverage Requirement**: Exit code assertions added to at least one distribution-gate test scenario (AC-13, post-condition).

---

### R-13: `find_regressions` output bleeds into Distribution Gate render path
**Severity**: Med
**Likelihood**: Low
**Impact**: ADR-005 states `find_regressions` still runs for all profiles (unchanged), but its output must not appear in Section 5 for `distribution_change = true` profiles. If `render_distribution_gate_section` accidentally receives the regressions slice, regression rows appear in the Distribution Gate table.

**Test Scenarios**:
1. Render Section 5 for a `distribution_change = true` profile where `find_regressions` would return a non-empty slice (if applied); assert rendered output contains no regression rows and no "Regressions found" text.
2. `render_distribution_gate_section` signature review: function must not accept a regressions parameter.

**Coverage Requirement**: Section 5 render test with a fixture that would produce regressions under zero-regression mode; assert clean Distribution Gate output.

---

### R-14: `mrr_floor` compared against baseline MRR instead of candidate mean MRR
**Severity**: High
**Likelihood**: Low
**Impact**: FR-13 and ADR-003 explicitly require comparison against candidate mean MRR only. If the implementation mistakenly passes `baseline_stats.mean_mrr` as the `actual` value in `check_distribution_targets`, the veto fires incorrectly for every distribution-change feature.

**Test Scenarios**:
1. AC-13 with explicit MRR values: fixture where candidate `mean_mrr = 0.40`, baseline `mean_mrr = 0.60`, `mrr_floor = 0.35`; assert `mrr_floor.actual == 0.40` (not 0.60) and `mrr_floor.passed = true`.
2. Verify `check_distribution_targets` signature accepts `stats: &AggregateStats` (candidate stats only) — baseline stats must not be a parameter.

**Coverage Requirement**: AC-13 MRR fixture uses values where baseline MRR != candidate MRR to distinguish the two code paths.

---

### R-15: Dual-type constraint violated silently
**Severity**: High
**Likelihood**: Low
**Impact**: The sidecar approach (ADR-002) is designed to prevent touching `ScenarioResult`. If the implementation adds a convenience field to `ScenarioResult` during development (e.g., for logging), the second copy in `report/mod.rs` may be missed, causing deserialization failures on pre-nan-010 result sets. Entry #3574 documents this caused rework in nan-007, nan-008, nan-009.

**Test Scenarios**:
1. Static check: `ScenarioResult` field count in `runner/output.rs` and `report/mod.rs` must be identical before and after nan-010 (zero net change).
2. AC-14 backward-compat test: run `eval report` against a results directory containing pre-nan-010 `ScenarioResult` JSON (no new fields); assert no deserialization error and Section 5 renders as Zero-Regression Check.

**Coverage Requirement**: AC-14 backward-compat test exercises the deserialization path; explicit field-count audit at gate-3b.

---

## Integration Risks

### Profile parsing → Runner metadata write
The runner calls `write_profile_meta` after `parse_profile_toml` populates `EvalProfile`. The
`DistributionTargets` in-memory type must map correctly to `DistributionTargetsJson` serde
type. A field order mismatch in the JSON struct (e.g., `cc_at_k_min` serialized as
`"cc_at_k"`) is caught only by a round-trip test, not by the Rust compiler.

**Mitigated by**: R-10 round-trip test; AC-05 schema validation.

### Runner output directory → Report input directory
The results directory is the sole artifact boundary. `eval report` derives the
`profile-meta.json` path from the `--results` argument. If the report is ever invoked with
individual file arguments rather than a directory path (SCOPE.md assumption §Assumptions),
`load_profile_meta` fails silently (returns empty map) and applies wrong gate. This is a
latent scope assumption, not a current breakage.

**Mitigated by**: AC-11 backward-compat test; documented in SCOPE.md §Assumptions.

### `render_report` new parameter threading
`render_report` gains a `profile_meta: &HashMap<String, ProfileMetaEntry>` parameter
(Architecture Component 6). Every call site in `report/mod.rs` must be updated. If a call
site passes an empty map by mistake, the report silently renders Zero-Regression Check for
all profiles — the same as backward-compat fallback, no compile error.

**Mitigated by**: R-10 round-trip test catches this; entry #3529 pattern for parameter threading.

---

## Edge Cases

| Edge Case | Risk | Scenario |
|-----------|------|----------|
| `cc_at_k_min = 0.0` | Gate trivially passes for any candidate | Assert `check_distribution_targets` accepts `0.0` as a valid floor; document that floor selection is the user's responsibility (AC-12) |
| `mrr_floor > 1.0` | Gate always fails; impossible to pass | No validation prevents this — it is a user configuration error. Assert that `ConfigInvariant` is NOT returned for this (out-of-range values are not rejected at parse time per scope) |
| Single scenario in results | `mean(cc_at_k)` == value of that one scenario; no averaging needed | Assert `check_distribution_targets` handles a single-element mean correctly |
| Empty results directory (zero scenarios) | `mean(cc_at_k)` undefined / zero | `eval report` should fail before reaching distribution gate if no results exist; verify graceful failure, not a divide-by-zero panic |
| Profile name collision in `profile-meta.json` | Two profiles with same name — last writer wins | `HashMap` semantics apply; document that profile names must be unique (already required by profile validation) |
| Multi-profile with all `distribution_change = true` | Every Section 5 block is a Distribution Gate | Assert all blocks render correctly with `### 5.N` sub-headings |
| `mrr_floor` exactly equal to `mean(mrr)` | Boundary condition: `>=` must pass, `>` would fail | Assert `check_distribution_targets` uses `>=` comparison (not `>`) for all three metrics |

---

## Security Risks

### Input surface: profile TOML
- **Untrusted input**: Profile TOML files are author-controlled, not user-controlled at runtime. Risk is low for hostile injection — no shell execution or path traversal from TOML values.
- **Malformed numeric fields**: `cc_at_k_min`, `icd_min`, `mrr_floor` are `f64`. Rust's TOML parser rejects non-numeric values at parse time. NaN/Infinity values are not produced by TOML parsers. No special handling required.
- **Blast radius**: A malformed TOML causes `ConfigInvariant` at parse time, before any scenario replay. No data is corrupted; the harness aborts cleanly.

### Input surface: `profile-meta.json`
- **Trust model**: This file is written by `eval run` to a local output directory. It is not externally supplied in normal use. An adversary who can write arbitrary files to the output directory already has broader filesystem access.
- **Malformed JSON**: `serde_json` deserialization errors are handled by `load_profile_meta` returning an empty map (or aborting, per resolved Design Decision #8). No path traversal risk — the path is derived from the `--results` argument which is already validated.
- **Blast radius**: Corrupt sidecar causes report abort (non-zero exit) per Design Decision #8. Worst case: operator must re-run `eval run` to regenerate. No data loss.

### Input surface: result JSON files (`--results`)
- **Unchanged by nan-010**: `ScenarioResult` deserialization is not modified. Security surface is unchanged from pre-nan-010.

---

## Failure Modes

| Failure Mode | Expected Behavior | Testable By |
|---|---|---|
| `distribution_change = true`, targets absent | `EvalError::ConfigInvariant` at parse time, before any scenario replay; message names missing section | AC-02 |
| `distribution_change = true`, one target field missing | `EvalError::ConfigInvariant` naming the missing field | AC-03 |
| Baseline profile with `distribution_change = true` | `EvalError::ConfigInvariant` with message "baseline profile must not declare `distribution_change = true`" | R-03 test |
| `profile-meta.json` absent | `eval report` silently treats all profiles as `distribution_change = false`; Section 5 renders Zero-Regression Check | AC-11, AC-14 |
| `profile-meta.json` corrupt (non-JSON) | `eval report` emits error, exits non-zero, with message "profile-meta.json is malformed — re-run eval to regenerate" | R-07 test |
| Distribution Gate fails (CC@k or ICD below target) | Section 5 shows FAILED with "Diversity targets not met"; report exits 0 | AC-09, AC-10, R-12 |
| MRR floor breached, diversity targets met | Section 5 shows "Diversity targets met, but ranking floor breached"; report exits 0 | AC-10, R-06 |
| Both diversity and MRR fail | Both failure messages shown; overall FAILED; report exits 0 | R-05, R-06 |
| `write_profile_meta` fails (disk full) | `eval run` returns an error; result files may exist but sidecar absent; `eval report` falls back to backward-compat mode | AC-11 |
| `render.rs` exceeds 500 lines | Workspace build fails immediately (CI line-count check, entry #161) | NFR-01, R-01 |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (partial run, corrupt sidecar) | R-04, R-07 | ADR-004 atomic write mitigates partial-run corrupt state; Design Decision #8 specifies abort + exit non-zero on corrupt sidecar. ARCHITECTURE.md Component 7 updated during design review to match — conflict resolved. |
| SR-02 (`aggregate.rs` line limit) | R-01 | ADR-001 mandates pre-split as the first implementation step; `aggregate/distribution.rs` boundary established before feature code. |
| SR-03 (`render.rs` line limit) | R-01 | ADR-001 mandates `render_distribution_gate.rs` boundary established before any touch to `render.rs`. |
| SR-04 (`mrr_floor` value guidance) | — | Resolved: Architecture mandates a "Baseline MRR (reference)" row in the Distribution Gate table (SCOPE.md Design Decision #5). Not an architecture risk; documentation concern addressed in AC-12 and FR-16. |
| SR-05 (multi-profile Section 5 structure) | R-09 | ADR-005 specifies per-profile Section 5 with `## 5.` for single-profile and `### 5.N` for multi-profile. Render loop must count non-baseline profiles before choosing heading level. |
| SR-06 (dual-type constraint) | R-15 | ADR-002 hard-constraints zero changes to `ScenarioResult`; sidecar pattern (#3582) is the mitigation. Any violation requires three-site sync (runner + report + round-trip tests). |
| SR-07 (extraction before strip) | R-02 | FR-04 specifies extraction before stripping; AC-01 and AC-02 test cases exercise the path; an extraction-order regression would cause AC-02 to pass when it should fail. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 2 scenarios (pre-split static + CI line-count) |
| High | 7 (R-02, R-03, R-04, R-05, R-06, R-07, R-11) | 18+ scenarios across `eval/profile/tests.rs` and `tests_distribution_gate.rs` |
| Med | 5 (R-08, R-09, R-10, R-12, R-13) | 10 scenarios |
| Low | 3 (R-14, R-15, edge cases) | 6 scenarios |

**Non-negotiable test names** (gate-3b must grep for these):
- `test_parse_distribution_change_profile_valid` (AC-01)
- `test_parse_distribution_change_missing_targets` (AC-02)
- `test_parse_distribution_change_missing_cc_at_k` (AC-03)
- `test_parse_distribution_change_missing_icd` (AC-03)
- `test_parse_distribution_change_missing_mrr_floor` (AC-03)
- `test_parse_no_distribution_change_flag` (AC-04)
- `test_write_profile_meta_schema` (AC-05)
- `test_distribution_gate_section_header` (AC-07)
- `test_distribution_gate_table_content` (AC-08)
- `test_distribution_gate_pass_condition` (AC-09)
- `test_distribution_gate_mrr_floor_veto` (AC-09)
- `test_distribution_gate_distinct_failure_modes` (AC-10)
- `test_report_without_profile_meta_json` (AC-11, AC-14)
- `test_check_distribution_targets_all_pass` (AC-13)
- `test_check_distribution_targets_cc_at_k_fail` (AC-13)
- `test_check_distribution_targets_icd_fail` (AC-13)
- `test_check_distribution_targets_mrr_floor_fail` (AC-13)
- `test_distribution_gate_baseline_rejected` (R-03)
- `test_distribution_gate_corrupt_sidecar_aborts` (R-07)
- `test_distribution_gate_exit_code_zero` (R-12, C-07)

---

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found entries #3579 (nan-009 gate-3b: entire test modules absent), #3580 (nan-009 gate-3b: 500-line violation not self-checked), #2758 (gate-3c: non-negotiable test name pre-declaration)
- Queried: `/uni-knowledge-search` for risk pattern eval harness render dual-type — found entries #3583 (render.rs 500-line split pattern), #3585 (atomic sidecar write pattern), #3582 (sidecar metadata file pattern), #3574 (dual-type constraint)
- Queried: `/uni-knowledge-search` for ScenarioResult dual-type sidecar — confirmed #3587 (ADR-002 nan-010 sidecar decision stored), #3582
- Stored: nothing novel to store — all patterns and ADRs are already stored in Unimatrix from this feature's architecture phase (#3582, #3583, #3585, #3586, #3587). No cross-feature pattern emerged that isn't already captured.
