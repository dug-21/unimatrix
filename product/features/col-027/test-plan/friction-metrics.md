# Test Plan: friction-metrics (friction.rs + metrics.rs + mod.rs)

**Files:**
- `crates/unimatrix-observe/src/detection/friction.rs` — `PermissionRetriesRule` fix, `ToolFailureRule` new rule
- `crates/unimatrix-observe/src/metrics.rs` — `compute_universal()` fix
- `crates/unimatrix-observe/src/detection/mod.rs` — `default_rules()` registration

**Risks covered:** R-02 (two-site divergence), R-04 (PermissionRetriesRule fires for failures),
R-06 (ToolFailureRule threshold boundary), R-07 (source_domain guard), R-12 (negative metric),
R-13 (not registered in default_rules)

---

## Test Infrastructure: `make_failure` Helper (AC-10, NFR-04)

A `make_failure(ts: u64, tool: &str) -> ObservationRecord` helper must be added to the
`#[cfg(test)]` block in `friction.rs` alongside the existing `make_pre` and `make_post` helpers.

Expected structure (mirrors `make_post`):
```rust
fn make_failure(ts: u64, tool: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }
}
```

All `PermissionRetriesRule` and `ToolFailureRule` tests in `friction.rs` must use this helper.
Inline struct construction is not permitted (AC-10, NFR-04).

The `metrics.rs` test module has its own `make_pre` and `make_post` helpers with a different
signature (`make_post(ts, tool, session, response_size)`). A `make_failure` helper with matching
signature must also be added to `metrics.rs` for the R-02 and AC-07 tests:

```rust
fn make_failure_m(ts: u64, tool: &str, session: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session.to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }
}
```

---

## Unit Tests: `PermissionRetriesRule` Fix (friction.rs)

### T-FM-01: `test_permission_retries_failure_as_terminal_no_finding` (AC-05)
**AC:** AC-05
**Risk:** R-04

Arrange: `records = vec![make_pre×5 for "Bash", make_failure×5 for "Bash"]` (all failures match all pre).
Act: `PermissionRetriesRule.detect(&records)`.
Assert: `findings.is_empty()` — 5 pre, 5 terminal (all failure), retries = 0.

**Why this is AC-05**: The entire motivation for col-027's friction fix — a tool fails 5 times,
`PermissionRetriesRule` must not fire because pre == terminal.

---

### T-FM-02: `test_permission_retries_mixed_post_and_failure_balanced` (AC-05 extension)
**Risk:** R-04

Arrange: 4 `make_pre` + 2 `make_post` + 2 `make_failure` for "Read".
Assert: `findings.is_empty()` — 4 pre, 4 terminal (2 post + 2 failure), retries = 0.

---

### T-FM-03: `test_permission_retries_genuine_imbalance_with_failures` (AC-06 new case)
**AC:** AC-06 (new assertion)
**Risk:** R-04

Arrange: 5 `make_pre` + 2 `make_post` + 0 `make_failure` for "Write".
Assert: 1 finding fires with `measured == 3.0` (5 pre, 2 terminal, retries = 3 > threshold 2).

This is the AC-06 explicit regression case — genuine imbalance must still fire after the fix.

---

### T-FM-04: Existing tests pass unchanged (AC-06 regression guard)
**AC:** AC-06
**Risk:** R-04

These existing tests must pass without any modification to their fixture data:
- `test_permission_retries_exceeds_threshold` (5 pre, 2 post → `measured == 3.0`)
- `test_permission_retries_equal_pre_post` (3 pre, 3 post → empty findings)
- `test_permission_retries_empty_records` (empty → empty findings)

These are non-modification constraints. The Stage 3c executor must run these and confirm they pass
as-is. If any fails after the col-027 fix, the fix introduced a regression (a test must not be
modified to make it pass).

---

## Unit Tests: `compute_universal()` Fix (metrics.rs)

### T-FM-05: `test_permission_friction_events_failure_as_terminal_zero` (AC-07)
**AC:** AC-07
**Risk:** R-02

Arrange: `records = [make_pre×4 "Bash", make_post×2 "Bash", make_failure_m×2 "Bash"]`
(using session `"s1"` or similar).
Act: `compute_universal(&records)`.
Assert: `metrics.permission_friction_events == 0.0` (4 pre, 4 terminal, 0 friction events).

---

### T-FM-06: `test_permission_friction_events_genuine_imbalance` (AC-07 extension)
**Risk:** R-02

Arrange: `records = [make_pre×5 "Bash", make_post×2 "Bash", make_failure_m×1 "Bash"]`.
Assert: `metrics.permission_friction_events == 2.0` (5 pre, 3 terminal, 2 friction events).

---

### T-FM-07: `test_permission_friction_events_saturating_sub_no_negative` (R-12)
**AC:** AC-07 (extension)
**Risk:** R-12

Arrange: `records = [make_pre×1 "Bash", make_failure_m×5 "Bash"]`
(failure count >> pre count — pathological case).
Assert: `metrics.permission_friction_events >= 0.0` — `saturating_sub` must prevent negative value.
Specifically: `== 0.0` (1 pre, 5+ terminal, saturates at 0).

---

## Unit Tests: Two-Site Atomicity (R-02 — CRITICAL REQUIREMENT)

### T-FM-08: `test_two_site_agreement_balanced_failure_and_post` (R-02, AC-05+AC-07)
**AC:** AC-05, AC-07 (coupled — R-02 requires both in same function)
**Risk:** R-02

This is the mandatory coupling test from ADR-004. It must call **both** `compute_universal()` and
`PermissionRetriesRule::detect()` on the **same `records` slice** within a **single test function**.

```rust
#[test]
fn test_two_site_agreement_balanced_failure_and_post() {
    let records = vec![
        make_pre(1000, "Bash"),   make_pre(2000, "Bash"),
        make_pre(3000, "Bash"),   make_pre(4000, "Bash"),
        make_post(1500, "Bash"),  make_post(2500, "Bash"),
        make_failure(3500, "Bash"), make_failure(4500, "Bash"),
    ];
    // Site 1: metrics
    let metrics = compute_universal(&records);  // or use appropriate metrics helper
    assert_eq!(metrics.permission_friction_events, 0.0);
    // Site 2: rule
    let findings = PermissionRetriesRule.detect(&records);
    assert!(findings.is_empty());
}
```

Both assertions in one function is the only enforcement mechanism for the two-site atomicity
requirement. If they are in separate functions, a partial fix (one site updated, the other not)
could still pass both tests individually.

**Note on imports**: `compute_universal` is in `metrics.rs`. This test must be in a location
that can import both `compute_universal` (from `metrics`) and `PermissionRetriesRule` (from
`friction`). The most practical location is a new test module in `friction.rs` that imports
`compute_universal` with a `use crate::metrics::compute_universal;` import, or alternatively
in an integration-style test module at `crates/unimatrix-observe/tests/`. Either location is
acceptable. The test must be in one function.

---

### T-FM-09: `test_two_site_agreement_genuine_imbalance`
**Risk:** R-02

Same structure as T-FM-08, but with records that produce non-zero friction:
- 5 pre, 2 post, 1 failure → terminal = 3, retries = 2 (at threshold, no finding)
- Both sites must agree: `permission_friction_events == 2.0` AND `findings.is_empty()`
  (threshold for rule is > 2, metric is just the count)

---

### T-FM-10: `test_two_site_agreement_failure_only_no_post`
**Risk:** R-02

- 5 pre, 0 post, 5 failure for "Read" → terminal = 5, retries = 0
- Assert: `permission_friction_events == 0.0` AND `PermissionRetriesRule findings.is_empty()`

---

## Unit Tests: `ToolFailureRule` (friction.rs)

### T-FM-11: `test_tool_failure_rule_at_threshold_no_finding` (AC-09)
**AC:** AC-09
**Risk:** R-06

Arrange: exactly 3 `make_failure(_, "Read")`.
Act: `ToolFailureRule.detect(&records)`.
Assert: `findings.is_empty()` — 3 is at-threshold, not above it. Threshold fires at `count > 3`.

---

### T-FM-12: `test_tool_failure_rule_above_threshold_fires` (AC-08)
**AC:** AC-08
**Risk:** R-06

Arrange: exactly 4 `make_failure(_, "Bash")`.
Act: `ToolFailureRule.detect(&records)`.
Assert:
- `findings.len() == 1`
- `findings[0].rule_name == "tool_failure_hotspot"`
- `findings[0].measured == 4.0`
- `findings[0].threshold == 3.0`
- `findings[0].claim == "Tool 'Bash' failed 4 times"`
- `findings[0].category == HotspotCategory::Friction`
- `findings[0].severity == Severity::Warning`

---

### T-FM-13: `test_tool_failure_rule_multiple_tools_independent` (R-06 + AC-08)
**AC:** AC-08
**Risk:** R-06

Arrange: 4 `make_failure` for "Bash" + 3 `make_failure` for "Read" + 2 `make_failure` for "Write".
Assert:
- `findings.len() == 1` — only "Bash" exceeds threshold
- `findings[0]` is for "Bash" with `measured == 4.0`

---

### T-FM-14: `test_tool_failure_rule_multiple_tools_multiple_findings`
**Risk:** R-06

Arrange: 5 `make_failure` for "Bash" + 4 `make_failure` for "Read".
Assert:
- `findings.len() == 2` — both tools exceed threshold
- One finding for "Bash" with `measured == 5.0`
- One finding for "Read" with `measured == 4.0`
- Findings are per-tool, not an aggregate

---

### T-FM-15: `test_tool_failure_rule_empty_records`
**Risk:** R-06

Arrange: `records = vec![]`.
Assert: `findings.is_empty()` — no panic, empty result.

---

### T-FM-16: `test_tool_failure_rule_non_claude_code_excluded` (R-07)
**AC:** — (source_domain guard)
**Risk:** R-07

Arrange: 5 records with `event_type = "PostToolUseFailure"` and `source_domain = "non-claude-code"`.
Assert: `findings.is_empty()` — source_domain guard excludes non-claude-code records.

---

### T-FM-17: `test_tool_failure_rule_mixed_domains` (R-07)
**Risk:** R-07

Arrange: 4 `make_failure` for "Bash" (`source_domain = "claude-code"`) + 5 records with
`source_domain = "other-agent"` for same tool "Bash".
Assert:
- `findings.len() == 1`
- `findings[0].measured == 4.0` (only claude-code records counted)

---

### T-FM-18: `test_tool_failure_rule_evidence_records`
**Risk:** R-06 (completeness of finding)

Arrange: 4 `make_failure` for "Bash", with `response_snippet = Some("permission denied")` on each.
Assert:
- `findings[0].evidence.len() == 4`
- Each evidence record has `description` containing `"PostToolUseFailure for Bash"`
- Each evidence record's `detail` matches `Some("permission denied")` (or the snippet)

**Note**: `make_failure` produces `response_snippet: None` by default. This test needs a variant
with `response_snippet` set. Either extend `make_failure` to accept an optional snippet parameter,
or construct records directly for this test.

---

## Unit Tests: `default_rules()` Registration (mod.rs)

### T-FM-19: `test_default_rules_contains_tool_failure_hotspot` (R-13)
**AC:** — (registration gate)
**Risk:** R-13

Arrange: call `default_rules()`.
Assert:
- The returned slice contains a rule where `rule.name() == "tool_failure_hotspot"`

---

### T-FM-20: `test_default_rules_count_is_22` (R-13 + FR-07.6)
**AC:** — (count gate)
**Risk:** R-13

Arrange: call `default_rules()`.
Assert: `rules.len() == 22` (21 existing + 1 new `ToolFailureRule`).

The existing doc comment on `default_rules()` must also be updated from 21 to 22.

---

## Integration Test Expectations

These detection rules operate on `ObservationRecord` slices — they do not have a direct MCP
interface. No infra-001 suite directly exercises them. The `lifecycle` suite exercises the
`context_retrospective` tool end-to-end, but its existing tests use fixtures without
`PostToolUseFailure` records, so they will not exercise the new rule paths.

No new infra-001 test is planned. The unit test coverage above is sufficient.

---

## Edge Cases

- `ToolFailureRule` with records where `tool == None`: must be skipped gracefully (not panicked);
  `record.tool.as_ref()` returns `None` and the record contributes to no per-tool bucket.
- `PermissionRetriesRule` with only `PostToolUseFailure` and no `PreToolUse`: `pre_count == 0`,
  `terminal_count > 0`, `retries = saturating_sub(0, N) == 0`, no finding.
- `compute_universal()` with only failure records and no pre/post: `permission_friction_events == 0.0`.
- `ToolFailureRule` with exactly 1 failure for each of 100 tools: no finding (each at 1, well below 3).
