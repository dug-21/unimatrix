# nan-010 Test Plan — Overview

GH Issue: #402

---

## Test Strategy

nan-010 spans three subsystems: profile parsing (`eval/profile/`), runner sidecar output
(`eval/runner/`), and report rendering (`eval/report/`). The test strategy mirrors that
subsystem decomposition.

**Unit tests** cover the logic within each component: TOML parse-time validation, sidecar
JSON schema, aggregation gate logic, and rendered Markdown output. All unit tests are in Rust
using `#[test]` (sync) or `#[tokio::test]` (async, if needed).

**Integration tests** (infra-001 harness) are evaluated for applicability below; no new
integration scenarios are required — all behavior change is in the eval binary, not in the
MCP server's JSON-RPC tools.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk Description | Test(s) | File |
|---------|----------|-----------------|---------|------|
| R-01 | Critical | Module pre-split executed in wrong order — 500-line breach | Static: `wc -l` check pre-gate, compile-time enforcement | Gate-3b |
| R-02 | High | Extraction after `[profile]` strip silently drops new fields | `test_parse_distribution_change_profile_valid`, `test_parse_distribution_change_missing_targets` | `eval/profile/tests.rs` |
| R-03 | High | Baseline profile with `distribution_change = true` silently accepted | `test_distribution_gate_baseline_rejected` | `eval/report/tests_distribution_gate.rs` |
| R-04 | High | Non-atomic sidecar write leaves corrupt state | `test_write_profile_meta_schema` (atomic path); tmp-not-read scenario | `eval/report/tests_distribution_gate.rs` |
| R-05 | High | `overall_passed` conflates diversity failure and MRR veto | `test_check_distribution_targets_all_pass`, `test_check_distribution_targets_cc_at_k_fail`, `test_check_distribution_targets_icd_fail`, `test_check_distribution_targets_mrr_floor_fail`, `test_distribution_gate_mrr_floor_veto` | `eval/report/tests_distribution_gate.rs` |
| R-06 | High | Rendered failure messages indistinguishable | `test_distribution_gate_distinct_failure_modes` | `eval/report/tests_distribution_gate.rs` |
| R-07 | High | Corrupt sidecar falls back silently instead of aborting | `test_distribution_gate_corrupt_sidecar_aborts` | `eval/report/tests_distribution_gate.rs` |
| R-08 | Med | `AggregateStats` fields renamed or absent | `test_check_distribution_targets_all_pass` (explicit field assertions) | `eval/report/tests_distribution_gate.rs` |
| R-09 | Med | Section 5 heading level mismatch single vs. multi-profile | `test_distribution_gate_section_header` (heading assertions) | `eval/report/tests_distribution_gate.rs` |
| R-10 | Med | `profile-meta.json` schema mismatch between writer and reader | `test_write_profile_meta_schema` (write + round-trip parse); `test_distribution_gate_table_content` | `eval/report/tests_distribution_gate.rs` |
| R-11 | High | Mandatory test modules absent at delivery | Gate-3b grep for all non-negotiable test names | Gate-3b |
| R-12 | Med | `eval report` exits non-zero on Distribution Gate failure | `test_distribution_gate_exit_code_zero` | `eval/report/tests_distribution_gate.rs` |
| R-13 | Med | `find_regressions` bleeds into Distribution Gate render | `test_distribution_gate_table_content` (assert no regression rows) | `eval/report/tests_distribution_gate.rs` |
| R-14 | Med | `mrr_floor` compared against baseline MRR instead of candidate | `test_check_distribution_targets_mrr_floor_fail` (fixture: candidate 0.40, baseline 0.60, floor 0.35) | `eval/report/tests_distribution_gate.rs` |
| R-15 | Med | Dual-type constraint violated — field added to `ScenarioResult` | `test_report_without_profile_meta_json` (pre-nan-010 result JSON), gate-3b field-count audit | `eval/report/tests_distribution_gate.rs` |

---

## Non-Negotiable Test Names

Gate-3b verifies by grep. All names must exist exactly as listed.

### `eval/profile/tests.rs`
- `test_parse_distribution_change_profile_valid` (AC-01, R-02)
- `test_parse_distribution_change_missing_targets` (AC-02, R-02)
- `test_parse_distribution_change_missing_cc_at_k` (AC-03)
- `test_parse_distribution_change_missing_icd` (AC-03)
- `test_parse_distribution_change_missing_mrr_floor` (AC-03)
- `test_parse_no_distribution_change_flag` (AC-04)

### `eval/report/tests_distribution_gate.rs`
- `test_write_profile_meta_schema` (AC-05, R-04, R-10)
- `test_distribution_gate_section_header` (AC-07, R-09)
- `test_distribution_gate_table_content` (AC-08, R-13)
- `test_distribution_gate_pass_condition` (AC-09)
- `test_distribution_gate_mrr_floor_veto` (AC-09, R-05)
- `test_distribution_gate_distinct_failure_modes` (AC-10, R-06)
- `test_report_without_profile_meta_json` (AC-11, AC-14, R-15)
- `test_check_distribution_targets_all_pass` (AC-13, R-05, R-08)
- `test_check_distribution_targets_cc_at_k_fail` (AC-13, R-05)
- `test_check_distribution_targets_icd_fail` (AC-13, R-05)
- `test_check_distribution_targets_mrr_floor_fail` (AC-13, R-05, R-14)
- `test_distribution_gate_baseline_rejected` (R-03)
- `test_distribution_gate_corrupt_sidecar_aborts` (R-07)
- `test_distribution_gate_exit_code_zero` (R-12)

---

## Acceptance Criteria Coverage

| AC-ID | Test Name(s) | File |
|-------|-------------|------|
| AC-01 | `test_parse_distribution_change_profile_valid` | `eval/profile/tests.rs` |
| AC-02 | `test_parse_distribution_change_missing_targets` | `eval/profile/tests.rs` |
| AC-03 | `test_parse_distribution_change_missing_cc_at_k`, `_icd`, `_mrr_floor` | `eval/profile/tests.rs` |
| AC-04 | `test_parse_no_distribution_change_flag` | `eval/profile/tests.rs` |
| AC-05 | `test_write_profile_meta_schema` | `eval/report/tests_distribution_gate.rs` |
| AC-06 | Existing render tests; `test_report_without_profile_meta_json` | existing + new |
| AC-07 | `test_distribution_gate_section_header` | `eval/report/tests_distribution_gate.rs` |
| AC-08 | `test_distribution_gate_table_content` | `eval/report/tests_distribution_gate.rs` |
| AC-09 | `test_distribution_gate_pass_condition`, `test_distribution_gate_mrr_floor_veto` | `eval/report/tests_distribution_gate.rs` |
| AC-10 | `test_distribution_gate_distinct_failure_modes` | `eval/report/tests_distribution_gate.rs` |
| AC-11 | `test_report_without_profile_meta_json` | `eval/report/tests_distribution_gate.rs` |
| AC-12 | Manual review of `docs/testing/eval-harness.md` | Manual |
| AC-13 | `test_check_distribution_targets_*` (four tests) | `eval/report/tests_distribution_gate.rs` |
| AC-14 | `test_report_without_profile_meta_json` | `eval/report/tests_distribution_gate.rs` |

---

## Cross-Component Test Dependencies

```
Component 1 (types) → Component 2 (validation): parse tests require both
Component 2 (validation) → Component 3 (runner-sidecar): write_profile_meta uses EvalProfile from validation
Component 3 (runner-sidecar) → Component 7 (report-sidecar-load): round-trip schema test spans both
Component 4 (aggregate-distribution) ← Component 7: check_distribution_targets called from run_report
Component 5 (render-distribution-gate) ← Component 6 (section5-dispatch): dispatch selects render path
```

The round-trip test (`test_write_profile_meta_schema`) spans Components 3 and 7: it calls
`write_profile_meta`, then deserializes the written file as `ProfileMetaFile`. This is the
primary defense against R-10 (schema mismatch between writer and reader types).

---

## Integration Harness Plan (infra-001)

### Applicability Assessment

nan-010 does not add or modify any MCP JSON-RPC tool. The feature is entirely within the eval
binary (`eval run` / `eval report` subcommands). The infra-001 harness exercises the
`unimatrix-server` binary through stdio MCP transport — it does not invoke the eval binary.

**Conclusion**: No existing infra-001 suite covers or needs to cover nan-010 behavior.

### Suite Selection

| Suite | Run? | Rationale |
|-------|------|-----------|
| `smoke` | Yes (mandatory minimum gate) | Always required; verifies server still starts and handshakes cleanly after any change |
| `tools` | No | No MCP tool changes |
| `lifecycle` | No | No store/retrieval behavior changes |
| `confidence` | No | No confidence system changes |
| `contradiction` | No | No contradiction detection changes |
| `security` | No | No security surface changes |
| `volume` | No | No schema or storage changes |
| `edge_cases` | No | No boundary changes in MCP surface |

Smoke tests must pass as the baseline gate confirming the server binary compiles and starts
correctly after the eval harness changes.

### New Integration Tests Required

No new infra-001 integration tests are needed. The JSON schema boundary between `eval run`
(writer) and `eval report` (reader) is internal to the eval binary pair and is validated by
unit tests:
- `test_write_profile_meta_schema` exercises the write path and schema shape.
- `test_report_without_profile_meta_json` exercises the reader fallback path.
- `test_distribution_gate_corrupt_sidecar_aborts` exercises the abort-on-corrupt path.

The round-trip risk (R-10, knowledge package #3526) is fully covered by unit tests because
both writer and reader types are in the same workspace — the round-trip can be executed in
process without a running MCP server.

### When New Integration Tests Would Be Needed

If a future feature adds an MCP tool that triggers eval run or surfaces eval gate results
through the MCP interface, those behaviors would warrant additions to `suites/test_tools.py`
or `suites/test_lifecycle.py`. That is out of scope for nan-010.
