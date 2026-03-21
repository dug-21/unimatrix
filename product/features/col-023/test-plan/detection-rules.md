# Test Plan: detection-rules

**Component**: `crates/unimatrix-observe/src/detection/{agent,friction,session,scope}.rs` + `mod.rs`
**AC Coverage**: AC-02, AC-04, AC-05
**Risk Coverage**: R-01 (CRITICAL — cross-domain false findings), R-02 (CRITICAL — backward compat), R-13 (HookType constants misuse)

---

## Overview

This component has the highest test obligation in col-023. Every one of the 21 rules
must be individually tested with a mixed-domain slice (R-01), and an end-to-end
snapshot test must validate backward compatibility (R-02).

The primary new test file is:
```
crates/unimatrix-observe/tests/detection_isolation.rs
```

---

## Unit Test Expectations

### Location: `crates/unimatrix-observe/tests/detection_isolation.rs` (new)

---

### Mixed-Domain Isolation Tests (R-01, AC-05) — One per rule, 21 total

For each rule `{rule_name}` in the 21 claude-code detection rules:

```rust
// test_{rule_name}_no_findings_for_unknown_domain
//
// Arrange: supply a mixed ObservationRecord slice:
//   - N records with source_domain = "claude-code" and event_type values that
//     WOULD trigger this rule
//   - N records with source_domain = "unknown" and IDENTICAL event_type and tool
//     values (to simulate the worst case: unknown domain events that look exactly
//     like claude-code rule triggers)
// Act: rule.detect(&mixed_slice)
// Assert:
//   (a) findings count == number expected from claude-code records only
//   (b) NO finding in the result has a source attribution pointing to
//       source_domain = "unknown" records
//
// Naming pattern: test_{rule_module}_{brief_description}_no_cross_domain_findings
```

The 21 rules span four modules. Approximate distribution:
- `agent.rs`: rules related to agent spawning, delegation patterns (~5 rules)
- `friction.rs`: rules related to tool call failures, retries, errors (~6 rules)
- `session.rs`: rules related to session duration, call volumes (~5 rules)
- `scope.rs`: rules related to search patterns, knowledge gaps (~5 rules)

Each test must construct records with explicit `source_domain` and `event_type` string
values (not `HookType` enum variants — those no longer exist post-Wave 3).

### Full Unknown-Only Slice Test (R-01, R-06)

```rust
// test_all_21_rules_produce_no_findings_for_unknown_only_slice
// Arrange: 100 records all with source_domain = "unknown", event_type = "PostToolUse"
//          (exact string match for a claude-code rule trigger)
// Act: run default_rules() — all 21 rules — against this slice
// Assert: total finding count across all rules == 0
// This is the strongest form of R-01 isolation test.
```

### SRE Domain Pack Isolation Test (AC-05, R-01)

```rust
// test_sre_domain_events_trigger_sre_rule_not_claude_code_rules
// Arrange:
//   - Synthetic "sre" DomainPack with one ThresholdRule { source_domain: "sre",
//     event_type_filter: ["incident_opened"], threshold: 1.0 }
//   - 3 records { source_domain: "sre", event_type: "incident_opened", tool: Some("Bash") }
//     (tool="Bash" chosen to overlap with claude-code rule trigger patterns)
//   - Run all_rules = default_rules() + domain_rules(&sre_pack)
// Act: detect_hotspots(&records, &all_rules)
// Assert: at least 1 finding from the sre rule
// Assert: 0 findings from any of the 21 claude-code rules
// This validates AC-05 end-to-end.
```

---

### Backward Compatibility Snapshot Tests (R-02, AC-04)

### T-DET-COMPAT-01: Snapshot test — per-rule regression fixture

For each of the 21 rules, a regression fixture that was valid pre-feature must
produce the same finding post-feature:

```rust
// test_{rule_name}_backward_compat_fires_for_claude_code_fixture
// Arrange: a representative ObservationRecord slice that was known to trigger
//          this rule before the HookType → String refactor
//          Now uses event_type: "PreToolUse" etc. and source_domain: "claude-code"
// Act: rule.detect(&records)
// Assert: returns at least one HotspotFinding
// Assert: finding.severity matches expected value from pre-refactor behavior
// R-02: same records that fired the rule before must still fire it after.
```

### T-DET-COMPAT-02: End-to-end RetrospectiveReport snapshot (AC-04)

This is the highest-priority R-02 test. It lives in:
```
crates/unimatrix-observe/tests/detection_isolation.rs
```

```rust
// test_retrospective_report_backward_compat_claude_code_fixture
//
// Arrange: a fixed, representative ObservationRecord slice corresponding to a
//          claude-code session (2–4 agent spawns, ~50 tool calls, some failures).
//          This fixture must be hardcoded in the test — not loaded from disk —
//          to ensure determinism.
//          All records: source_domain = "claude-code", event_type = "PostToolUse" etc.
//
// Act: run the full retrospective pipeline:
//   1. detect_hotspots(&records, &default_rules())
//   2. compute_metric_vector(&session, &records)
//
// Assert (field-by-field):
//   (a) findings count == EXPECTED_FINDINGS_COUNT (hardcoded baseline)
//   (b) finding types == EXPECTED_FINDING_TYPES (hardcoded list)
//   (c) All 21 UniversalMetrics fields == EXPECTED_METRIC_VALUES (hardcoded baseline)
//
// This test MUST capture a known-good baseline from the pre-feature codebase
// before Stage 3b begins. The Stage 3b implementor is responsible for running
// the current (pre-refactor) pipeline against the fixture and recording the
// output as the expected values.
```

**Implementation note for Stage 3b**: The backward-compat test fixture values must
be captured BEFORE making any changes. Record the fixture output by running:
```bash
cargo test -p unimatrix-observe -- test_retrospective_report_backward_compat 2>&1
```
on the pre-feature main branch, then hardcode the output as expectations.

---

### `default_rules()` Structural Test

```rust
// test_default_rules_returns_21_rules
// Assert: default_rules().len() == 21
// AC-02: rule count must not decrease.
```

```rust
// test_domain_rules_appended_not_replacing_default_rules
// Arrange: sre pack with 2 RuleDescriptors
// Act: all_rules = default_rules() + domain_rules(&sre_pack)
// Assert: all_rules.len() == 23 (21 + 2)
// FR-04.7: domain rules are appended, not replacing.
```

---

## Static Verification (R-13)

Post-Wave-3, run:

```bash
grep -rn "HookType::" crates/unimatrix-observe/src/detection/
# Must return zero matches
grep -rn "HookType::" crates/unimatrix-server/src/
# Must return zero matches
```

Document in the gate report that this check passed.

---

## Test Count Non-Regression (AC-02, R-03)

Before Stage 3b begins, record the baseline test count:
```bash
cargo test -p unimatrix-observe -- --list 2>&1 | grep "test$" | wc -l
```

After Stage 3c, assert the count is >= baseline. This is checked in the
RISK-COVERAGE-REPORT.md.

---

## Edge Cases

- Empty `ObservationRecord` slice to all 21 rules: zero findings, no panic (EC-06).
- Single record slice: rules requiring multiple events (temporal window logic) must
  not panic; they simply produce no finding.
- `tool = None` records: rules that match on `tool` must handle `None` gracefully.
- `source_domain = "claude-code"` with an unrecognized `event_type` (e.g., "CustomEvent"):
  rules that filter on specific event types must not fire; rules that do not filter on
  event type must evaluate normally.
