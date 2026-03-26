# Gate 3b Report: nan-009

> Gate: 3b (Code Review)
> Date: 2026-03-26
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions/structs match pseudocode specifications |
| Architecture compliance | PASS | Component boundaries, ADR decisions all followed |
| Interface implementation | PASS | All interfaces implemented as designed; serde annotations correct |
| Test case alignment | FAIL | Mandatory nan-009 tests missing: phase stats unit tests, render tests, round-trip guard, backward-compat tests |
| Code quality — compile | PASS | `cargo check -p unimatrix-server` clean (0 errors, 12 pre-existing warnings) |
| Code quality — stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` found |
| Code quality — unwrap | PASS | No `.unwrap()` in non-test code |
| Code quality — file size | FAIL | `render.rs` = 544 lines, `tests.rs` = 1054 lines — both exceed 500-line limit |
| Security | PASS | No hardcoded secrets; input validated; path traversal guarded via canonicalize |
| Knowledge stewardship | WARN | All agents queried; stores blocked by Write capability restriction (not a stewardship failure by agents) |

---

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence**:

- `ScenarioContext.phase`: `#[serde(default, skip_serializing_if = "Option::is_none")]` — exactly as pseudocode OVERVIEW.md specifies.
- `ScenarioResult` (runner copy, `runner/output.rs`): `#[serde(default)]` only, no `skip_serializing_if` — matches pseudocode spec for the writer side.
- `ScenarioResult` (report copy, `report/mod.rs` line 122-123): `#[serde(default)]` only — matches pseudocode spec for reader side.
- `compute_phase_stats` signature: `pub(super) fn compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>` — exact match.
- `render_phase_section` signature: `pub(super) fn render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String` — exact match.
- `render_report` updated signature includes `phase_stats: &[PhaseAggregateStats]` as second parameter — matches.
- `PhaseAggregateStats` struct defined in `mod.rs` as `pub(super)` with `#[derive(Debug, Default)]` and all six fields — exact match.

### Architecture Compliance

**Status**: PASS

**Evidence**:

- Component 1 (Scenario Extraction): `output.rs` SQL selects `phase`; `extract.rs` reads via `row.try_get("phase")?`; `types.rs` has annotated field. All three files correct.
- Component 2 (Result Passthrough): `replay.rs` line 80: `phase: record.context.phase.clone()` with explicit comment "metadata passthrough only — never forwarded to ServiceSearchParams or AuditContext (R-06)". Confirmed: neither `ServiceSearchParams` nor `AuditContext` receive phase.
- Component 3 (Report Aggregation): `compute_phase_stats` is purely synchronous, no async, no database access. Correct.
- Component 4 (Report Rendering): `render_phase_section` returns empty string for empty input; section 6 rendered conditionally; section 7 heading correctly replaces old section 6 heading.
- Component 5 (Report Entry Point): `run_report` wires `compute_phase_stats` in Step 4 and passes result to `render_report` in Step 5.
- ADR-001 (serde null suppression): `skip_serializing_if` on extraction side only. COMPLIANT.
- ADR-003 (`"(unset)"` canonical): `aggregate.rs` line 447: `key.unwrap_or_else(|| "(unset)".to_string())` with comment `// ADR-003; NOT "(none)"`. COMPLIANT. No `"(none)"` found anywhere in code or docs.
- `"(unset)"` sort-last override: explicit `sort_by` match arms correctly override ASCII ordering.

### Interface Implementation

**Status**: PASS

**Evidence**:

Key check 1 — `types.rs` `ScenarioContext.phase`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase: Option<String>,
```
Confirmed at lines 73-74 of `types.rs`. PASS.

Key check 2 — `runner/output.rs` `ScenarioResult.phase`:
```rust
#[serde(default)]
pub phase: Option<String>,
```
Confirmed at lines 86-87 of `runner/output.rs`. No `skip_serializing_if`. PASS.

Key check 3 — `report/mod.rs` `ScenarioResult.phase`:
```rust
#[serde(default)]
pub phase: Option<String>, // tolerates absent key (pre-nan-009) and explicit null
```
Confirmed at lines 122-123 of `report/mod.rs`. PASS.

Key check 4 — R-06 (phase not forwarded in `replay.rs`): `ServiceSearchParams` at lines 96-108 and `AuditContext` at lines 110-121 of `replay.rs` — neither struct has a phase field set. `phase` is only assigned at the `ScenarioResult` struct literal (line 80). PASS.

Key check 5 — `compute_phase_stats` returns empty `Vec` when ALL phases are `None`: Lines 411-413 of `aggregate.rs`:
```rust
if !results.iter().any(|r| r.phase.is_some()) {
    return Vec::new();
}
```
PASS.

Key check 6 — `"(unset)"` canonical (not `"(none)"`): `aggregate.rs` line 447 uses `"(unset)"`. No `(none)` found in any code or documentation file. PASS.

Key check 7 — `"(unset)"` sorts last despite ASCII ordering: `aggregate.rs` lines 458-463 use an explicit custom sort with match arms overriding `(` < `a`. PASS.

Key check 8 — Section 6 = Phase-Stratified Metrics; `## 7. Distribution Analysis` in `render.rs`:
- `render.rs` line 225: `md.push_str("## 7. Distribution Analysis\n\n");` — PASS.
- `render.rs` line 248: `out.push_str("## 6. Phase-Stratified Metrics\n\n");` — PASS.

Key check 9 — No stubs/placeholders: grep found no `todo!()`, `unimplemented!()`, `TODO`, `FIXME`. PASS.

Key check 10 — `cargo check -p unimatrix-server`: clean compile. PASS.

### Test Case Alignment

**Status**: FAIL

**Evidence**: The test plans specify mandatory tests that are absent from the implementation. Comparing test names in `report/tests.rs` (as listed by grep) against both the report-aggregation and report-rendering test plans:

**Missing tests from `report/tests.rs`** (nan-009 specific):

| Test Name | Plan Reference | Gap |
|-----------|---------------|-----|
| `test_compute_phase_stats_null_bucket_label` | aggregation plan R-01 | MISSING |
| `test_compute_phase_stats_empty_results_returns_empty` | aggregation plan EC-01, FM-03 | MISSING |
| `test_compute_phase_stats_all_null_returns_empty` | aggregation plan R-07, AC-09 item 4 | MISSING |
| `test_compute_phase_stats_null_bucket_sorts_last` | aggregation plan R-08, AC-05 | MISSING |
| `test_compute_phase_stats_mixed_phases_correct_grouping` | aggregation plan AC-09 item 3 | MISSING |
| `test_render_phase_section_empty_input_returns_empty_string` | rendering plan R-09 | MISSING |
| `test_render_phase_section_absent_when_stats_empty` | rendering plan R-07, AC-04, AC-09 item 5 | MISSING |
| `test_report_round_trip_null_phase_only_no_section_6` | rendering plan R-09, AC-04 | MISSING |
| `test_report_round_trip_phase_section_null_label` | rendering plan R-01 (null label in rendered output) | MISSING |
| `test_section_2_phase_label_non_null_present` | rendering plan R-12, AC-08 | MISSING |
| `test_section_2_phase_label_null_absent` | rendering plan R-12, AC-08 | MISSING |
| `test_report_round_trip_phase_section_7_distribution` | **ADR-002 mandatory**, rendering plan, entrypoint plan | MISSING |
| `test_report_deserializes_legacy_result_missing_phase_key` | entrypoint plan AC-06, EC-05, NFR-01 | MISSING |
| `test_report_deserializes_explicit_null_phase_key` | entrypoint plan AC-06, EC-06 | MISSING |
| `test_scenario_result_phase_round_trip_serde` | entrypoint plan R-03 | MISSING |
| Updated `test_report_contains_all_five_sections` to assert section 6 + section 7 ordering | rendering plan R-02, AC-12 | NOT UPDATED |

**Critically missing**: `test_report_round_trip_phase_section_7_distribution` is identified as mandatory in ADR-002 and the Architecture document as the primary dual-type guard. This test exercises the end-to-end path from runner-side serialization through file boundary to report-side deserialization and rendering — the single most important test for this feature.

**Tests that ARE present** (partial coverage only):
- `test_report_contains_all_six_sections`: present, verifies section 7 present and old `## 6. Distribution Analysis` absent, but uses all-null-phase data so section 6 is NOT verified. Comment in test at line 990-993 explicitly acknowledges this gap.
- `test_report_round_trip_cc_at_k_icd_fields_and_section_6`: updated to verify `## 7. Distribution Analysis` and absence of `## 6. Distribution Analysis` — this update was done correctly.
- Scenarios tests (in `eval/scenarios/tests.rs`): 4 nan-009 phase tests present and correct (`test_scenario_context_phase_non_null_present_in_jsonl`, `test_scenario_context_phase_null_absent_from_jsonl`, `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null`).
- Runner tests (in `eval/runner/output.rs`): 2 nan-009 phase tests present (`test_scenario_result_phase_null_serialized_as_null`, `test_scenario_result_phase_non_null_serialized`).

**AC-11 (round-trip integration test)**: Not satisfied. The architecture document calls AC-11 mandatory. No test exists that writes a runner-side `ScenarioResult` with non-null phase to JSON, then calls `run_report` and asserts section 6 renders. This is the primary dual-type guard specified in ADR-002.

### Code Quality — Compile

**Status**: PASS

`cargo check -p unimatrix-server` output: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.21s` — 0 errors. 12 pre-existing warnings (none in nan-009 files).

### Code Quality — File Size

**Status**: FAIL

NFR-04 and Constraint 7 state: "No modified or created file may exceed 500 lines."

| File | Lines | Limit | Status |
|------|-------|-------|--------|
| `eval/scenarios/types.rs` | 87 | 500 | PASS |
| `eval/scenarios/output.rs` | 148 | 500 | PASS |
| `eval/scenarios/extract.rs` | 97 | 500 | PASS |
| `eval/runner/output.rs` | 254 | 500 | PASS |
| `eval/runner/replay.rs` | 189 | 500 | PASS |
| `eval/report/mod.rs` | 317 | 500 | PASS |
| `eval/report/aggregate.rs` | 485 | 500 | PASS (within limit) |
| `eval/report/render.rs` | **544** | 500 | **FAIL** |
| `eval/report/tests.rs` | **1054** | 500 | **FAIL** |
| `docs/testing/eval-harness.md` | 748 | N/A | (docs exempt) |

`render.rs`: The rendering agent acknowledged the 44-line overage (544 vs 500) in its report. The IMPLEMENTATION-BRIEF specified that the 500-line split condition explicitly applies to `aggregate.rs` but the workspace rule applies to all source files. The rendering logic and `render_phase_section` should be moved to a `render_phase.rs` sub-module or the distribution analysis rendering extracted. This is a spec violation requiring a fix.

`tests.rs`: At 1054 lines, the test file significantly exceeds the limit. Adding the required nan-009 tests (approximately 14 additional test functions) will push it further. The tester agent did not split the test file. Test files must be split per the workspace rule. Likely fix: move nan-009 phase-related tests into `report/tests_phase.rs`.

### Security

**Status**: PASS

- No hardcoded secrets, API keys, or credentials found.
- Input validation at system boundary (`do_scenarios`): `ScenarioSource::to_sql_filter()` returns static `&'static str` literals only — no user input reaches SQL directly (documented in inline comment at `output.rs` lines 92-96). PASS.
- Path traversal: live-DB path guard in `output.rs` uses `std::fs::canonicalize` to resolve symlinks before comparison. PASS.
- Serialization: malformed JSON in result files causes `serde_json::from_str` to return `Err`; the `run_report` loop skips with `eprintln!("WARN: ...")` — no panic or state corruption. PASS.
- `cargo audit`: not installed in environment; not verifiable. Noted as unverified rather than FAIL since the dependency footprint is unchanged by nan-009.

### Knowledge Stewardship Compliance

**Status**: WARN

All implementation agents queried Unimatrix before implementing (`/uni-query-patterns`). Reports contain `Queried:` entries with referenced pattern/ADR IDs. All store attempts were blocked by `MCP error -32003: Agent lacks Write capability` — this is an environment constraint, not a stewardship failure by the agents. The agents documented what they would have stored, which satisfies the intent. No agent report is missing the `## Knowledge Stewardship` section.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing nan-009 phase tests in `report/tests.rs` (14 tests) | tester / rust-dev | Add all tests listed in "Test Case Alignment" FAIL section above; most critical is `test_report_round_trip_phase_section_7_distribution` (ADR-002 mandatory dual-type guard) |
| `render.rs` exceeds 500 lines (544) | rust-dev (rendering) | Extract `render_distribution_analysis` or `render_phase_section` to a `render_phase.rs` sub-module, or extract section 7 helpers. Keep `render_report` in `render.rs`. |
| `tests.rs` exceeds 500 lines (1054) | tester / rust-dev | Split: move nan-009 phase tests to `report/tests_phase.rs`; move nan-008 CC@k/ICD tests to `report/tests_distribution.rs`; leave core section/regression tests in `tests.rs`. |
| `test_report_contains_all_five_sections` not updated to assert section 6 | tester | Update test (or add `test_report_contains_all_seven_sections`) with a non-null-phase result so section 6 renders; assert `## 6. Phase-Stratified Metrics` and ordered positions 1–7. |

---

## Knowledge Stewardship

- Queried: nothing novel to store — the test-gap pattern (implementation agents deferring test writing to a "tester wave" then the tester wave not materializing) is already a documented systemic risk. Not feature-specific enough to store as a lesson from this gate alone.
- Stored: nothing novel to store — file-size violations and missing mandatory round-trip tests are both covered by existing project rules and patterns. This gate report documents the feature-specific instance; the lesson is already known.
