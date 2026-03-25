# Test Plan Overview: col-027 — PostToolUseFailure Hook Support

## Overall Test Strategy

col-027 spans three crates and one configuration file. The test approach is:

1. **Unit tests** — per-function, per-module coverage for all changed and new logic. Primary vehicle
   for AC coverage. Located in-module (`#[cfg(test)]`) following existing project patterns.
2. **Integration smoke gate** — infra-001 smoke suite run against the compiled binary to verify
   existing behaviour is not regressed.
3. **Feature-level integration** — AC-12 binary exit-code verification against the compiled
   `unimatrix hook PostToolUseFailure` command path.
4. **Structural inspection tests** — AC-01 (settings.json JSON structure), AC-11a (grep for match
   arm existence). These are shell/cargo-independent and run during Stage 3c.

### Test Baseline

- 2169 unit + 16 migration + 185 infra integration tests (pre-col-027)
- All new tests are additive. No existing tests may be deleted or modified.
- The post-col-027 total must exceed the baseline.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Description | Component | Test Location | Test Names (expected) |
|---------|----------|-------------|-----------|---------------|----------------------|
| R-01 | Critical | Wrong extractor called: `extract_response_fields()` on failure payload | observation-storage | `listener.rs` tests | `test_extract_error_field_present`, `test_extract_error_field_absent`, `test_extract_error_field_truncation`, `test_extract_observation_fields_posttoolusefailure_snippet` |
| R-02 | Critical | Partial two-site differential fix: metrics and friction diverge | friction-metrics | `friction.rs` + `metrics.rs` (same function) | `test_two_site_agreement_balanced_failure_and_post`, `test_two_site_agreement_genuine_imbalance`, `test_two_site_agreement_failure_only` |
| R-03 | High | Wildcard fall-through stores `tool = None` | observation-storage | `listener.rs` tests | `test_extract_observation_fields_posttoolusefailure_tool_some` |
| R-04 | High | `PermissionRetriesRule` fires for failure-only imbalance | friction-metrics | `friction.rs` tests | `test_permission_retries_failure_as_terminal_no_finding`, `test_permission_retries_failure_partial_imbalance` |
| R-05 | Med | `build_request()` wildcard: `tool_name` not extracted | hook-dispatcher | `hook.rs` tests | `test_build_request_posttoolusefailure_explicit_arm`, `test_build_request_posttoolusefailure_empty_extra` |
| R-06 | Med | `ToolFailureRule` threshold boundary error | friction-metrics | `friction.rs` tests | `test_tool_failure_rule_at_threshold_no_finding`, `test_tool_failure_rule_above_threshold_fires` |
| R-07 | Med | `ToolFailureRule` missing `source_domain` guard | friction-metrics | `friction.rs` tests | `test_tool_failure_rule_non_claude_code_excluded`, `test_tool_failure_rule_mixed_domains` |
| R-08 | Med | Hook exits non-zero on malformed payload | hook-dispatcher | `hook.rs` tests + binary integration | `test_build_request_posttoolusefailure_empty_extra`, `test_build_request_posttoolusefailure_missing_tool_name`, `test_build_request_posttoolusefailure_null_error` |
| R-09 | Low | `extract_event_topic_signal()` falls through for failure events | hook-dispatcher | `hook.rs` tests | `test_extract_event_topic_signal_posttoolusefailure` |
| R-10 | Low | `response_size` set for failure records | observation-storage | `listener.rs` tests | assertion in `test_extract_observation_fields_posttoolusefailure_snippet` |
| R-11 | Med | `POSTTOOLUSEFAILURE` constant value misspelled | core-constants | `observation.rs` tests | `test_posttoolusefailure_constant_value` |
| R-12 | Low | Negative `permission_friction_events` from signed subtraction | friction-metrics | `metrics.rs` tests | `test_permission_friction_events_saturating_sub_no_negative` |
| R-13 | Med | `ToolFailureRule` not registered in `default_rules()` | friction-metrics | `mod.rs` tests | `test_default_rules_contains_tool_failure_hotspot`, `test_default_rules_count_is_22` |
| R-14 | High | settings.json registration wrong pattern or casing | hook-registration | shell inspection (Stage 3c) | `grep -A3 '"PostToolUseFailure"' .claude/settings.json` structural assertion |

---

## Component Boundaries and Test File Mapping

| Component | Files Modified | Test Plan File |
|-----------|---------------|----------------|
| core-constants | `crates/unimatrix-core/src/observation.rs` | `test-plan/core-constants.md` |
| hook-registration | `.claude/settings.json` | `test-plan/hook-registration.md` |
| hook-dispatcher | `crates/unimatrix-server/src/uds/hook.rs` | `test-plan/hook-dispatcher.md` |
| observation-storage | `crates/unimatrix-server/src/uds/listener.rs` | `test-plan/observation-storage.md` |
| friction-metrics | `crates/unimatrix-observe/src/detection/friction.rs` + `metrics.rs` + `detection/mod.rs` | `test-plan/friction-metrics.md` |

---

## Critical Coupling Constraints

### R-02: Two-Site Atomicity (must be tested in a single function)

ADR-004 requires the AC-05 / AC-07 coupled test to assert both `compute_universal()` and
`PermissionRetriesRule::detect()` on the **same observation set** in the **same test function**.
This is the only enforcement mechanism for the two-site atomicity requirement.

The R-02 tests must live in `friction.rs` (or a dedicated coupling test module) and must import
and call `compute_universal()` directly — not via separate test files.

### R-01 and R-03: Compound Assertion

The AC-03 test function must assert all four of these in one block:
- `obs.hook == "PostToolUseFailure"` (no normalization, R-03/R-11)
- `obs.tool.is_some()` (not None, R-03)
- `obs.response_snippet == Some("some error message")` (R-01)
- `obs.response_size == None` (R-10)

Splitting these across separate test functions risks one masking another.

---

## Cross-Component Integration Dependencies

| Integration | Components | Scenario |
|-------------|------------|----------|
| settings.json key → `build_request()` match arm | hook-registration → hook-dispatcher | The string `"PostToolUseFailure"` must match exactly in both; casing mismatch is silent |
| `build_request()` payload → `extract_observation_fields()` | hook-dispatcher → observation-storage | `payload["error"]` must survive intact; no intermediate transformation must occur |
| `hook_type::POSTTOOLUSEFAILURE` → all string comparisons | core-constants → hook-dispatcher, observation-storage, friction-metrics | Constant value must match the exact string produced by Claude Code |
| `PermissionRetriesRule` + `compute_universal()` | friction-metrics (two sites) | Must produce consistent signals from same data — R-02 |

---

## Integration Harness Plan (infra-001)

### Existing Suite Coverage Assessment

| Suite | Relevance | Coverage for col-027 |
|-------|-----------|----------------------|
| `protocol` | Low | Protocol compliance is unchanged; no new tool |
| `tools` | None | No new MCP tool; existing tool signatures unchanged |
| `lifecycle` | Low | No new lifecycle flow through MCP interface |
| `volume` | None | No schema change; volume behaviour unchanged |
| `security` | Partial | `payload["error"]` ingest security; 500-char truncation limit |
| `confidence` | None | Confidence system unchanged |
| `contradiction` | None | No new detection signals via MCP interface |
| `edge_cases` | None | Failure edge cases are unit-testable |
| **smoke** | **Required** | **Mandatory gate — verify existing behaviour not regressed** |

The `PostToolUseFailure` hook is fired by Claude Code externally, not through the MCP JSON-RPC
interface. Its storage path (`RecordEvent` fire-and-forget) is tested by existing lifecycle and
tools suites incidentally. No new infra-001 suite test is needed for the primary col-027 change.

### New Integration Tests Needed (not in existing suites)

One new integration test is required that is **not testable via unit tests alone**: the binary
exit-code path for AC-12.

This test runs against the compiled `unimatrix` binary and checks that `unimatrix hook
PostToolUseFailure` exits 0 for malformed and empty inputs:

```
# AC-12 binary integration tests (shell, Stage 3c)
echo '{}' | unimatrix hook PostToolUseFailure; echo $?          → 0
echo 'not-json' | unimatrix hook PostToolUseFailure; echo $?    → 0
echo '' | unimatrix hook PostToolUseFailure; echo $?            → 0
```

These are **shell-level integration tests** run during Stage 3c, not additions to the infra-001
pytest suite. The infra-001 harness exercises MCP JSON-RPC; these tests exercise the hook binary's
stdin/exit-code contract, which is a different interface.

No additions to `suites/test_tools.py`, `suites/test_lifecycle.py`, or any other infra-001 suite
file are planned for col-027. The feature is not visible through the MCP interface.

### Suites to Execute During Stage 3c

| Suite | When | Reason |
|-------|------|--------|
| `smoke` | Always — mandatory gate | Regression guard for all existing capability |
| `tools` | If time permits | Verify existing tool signatures unaffected |

Full suite run (`pytest suites/ -v`) is optional but recommended if the smoke gate passes cleanly.

---

## Edge Cases Requiring Test Coverage

- Empty payload (`{}`): no panic, valid `RecordEvent` returned
- `error` field absent or null: `response_snippet = None`, no panic
- `error` field is a non-string type (array, object): `as_str()` returns `None`, `response_snippet = None`
- `error` field exactly 500 chars: no truncation
- `error` field exactly 501 chars: truncated at valid UTF-8 boundary
- `tool_name` absent: `obs.tool = None`; `ToolFailureRule` skips None-tool records gracefully
- `is_interrupt` absent: no panic, no effect on any stored field
- `PostToolUseFailure` records with `source_domain != "claude-code"`: excluded from all friction rules
- `PermissionRetriesRule` with only failure records and pre == failure count: zero retries
- `ToolFailureRule` with empty record set: empty `Vec<HotspotFinding>`, no panic
- `ToolFailureRule` with exactly 3 failures: no finding (threshold is strictly greater than 3)
- Multiple tools each at exactly 4 failures: two separate findings, one per tool

---

## AC Coverage Summary

| AC-ID | Component | Type | Priority |
|-------|-----------|------|----------|
| AC-01 | hook-registration | shell inspection | High |
| AC-02 | core-constants | unit test | Med |
| AC-03 | observation-storage | unit test | Critical |
| AC-04 | observation-storage | unit test (same as AC-03) | Critical |
| AC-05 | friction-metrics | unit test (coupled) | Critical |
| AC-06 | friction-metrics | unit test (regression) | Critical |
| AC-07 | friction-metrics | unit test (coupled with AC-05) | Critical |
| AC-08 | friction-metrics | unit test | Med |
| AC-09 | friction-metrics | unit test | Med |
| AC-10 | friction-metrics | grep inspection | Med |
| AC-11 | hook-dispatcher | grep + unit test | Med |
| AC-12 | hook-dispatcher + binary | unit + shell integration | Med |
