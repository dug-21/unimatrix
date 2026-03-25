# Security Review: col-027-security-reviewer

## Risk Level: low

## Summary

The col-027 diff adds PostToolUseFailure hook dispatch, observation storage, and a ToolFailureRule
detection rule. All new code paths use defensive Option chaining with no panic-capable unwrap()
calls on untrusted input. Error strings from the hook payload are truncated at a valid UTF-8 boundary
before storage. No new dependencies, no injection surface, no secrets. One dead-code variable was
found (compiler warning level) that is not exploitable but indicates a design artifact worth cleaning.

---

## Findings

### Finding 1 — Unused `tool_name` variable in hook.rs PostToolUseFailure arm

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/hook.rs:495-500`
- **Description**: The new arm extracts `tool_name` into a local binding but never reads it.
  The actual `tool_name` travels to `listener.rs` implicitly via `input.extra.clone()` (the
  full payload). The local binding is a dead variable. Cargo confirms: `warning: unused variable: tool_name`
  (line 495). This is not a security vulnerability, but it represents misleading code — a reader
  following the variable name might incorrectly believe `tool_name` participates in the built event
  struct, when it does not. If future code were to conditionally branch on `tool_name` (e.g., a
  filtering rule) and the author confused this local binding with the payload copy, logic errors
  could result.
- **Recommendation**: Either remove the binding (the payload carries the value correctly without it)
  or rename it to `_tool_name` with a comment explaining it is only present for documentation. The
  dead code should not be merged in its current form as it creates a false signal in future reviews.
- **Blocking**: no — functionally correct, but should be addressed before merge for code hygiene.

### Finding 2 — extract_error_field: 500-byte vs 500-char discrepancy with extract_response_fields

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2692-2694` vs `2654`
- **Description**: `extract_error_field` truncates at 500 **bytes** (via `truncate_at_utf8_boundary`).
  `extract_response_fields` truncates at 500 **chars** (via `.chars().take(500)`). For ASCII content
  these are equivalent, but for multi-byte UTF-8 strings the error snippet may be shorter (in character
  count) than the success snippet. The doc comment on `extract_error_field` says "consistent with
  the 500-char limit in extract_response_fields" — this claim is imprecise for non-ASCII. This is
  not a security risk (truncation in either direction is safe), but the documentation claim could
  confuse a future author extending the error handling path and cause them to introduce an inconsistency
  in the wrong direction (e.g., stripping fewer bytes than intended for multi-byte errors).
- **Recommendation**: Correct the doc comment to say "500 bytes" not "500 chars". Optionally align
  both to byte-based truncation for consistency — but that is outside col-027 scope.
- **Blocking**: no.

### Finding 3 — OWASP A03 (Injection): serde_json::to_string on tool_input payload goes to DB

- **Severity**: low (pass)
- **Location**: `listener.rs:2611-2614`, `hook.rs:299-304`
- **Description**: `serde_json::to_string(v).unwrap_or_default()` on arbitrary JSON values (tool_input)
  is used as the observation `input` column value. This is an evaluation: the value is serialized
  to a JSON string and passed as a bound parameter in a parameterized sqlx query. No string
  interpolation into SQL occurs. The call site at `insert_observation` uses `?1..?8` placeholders
  with `.bind()` throughout. **No injection surface exists.** `unwrap_or_default()` on the
  serialization means pathological JSON that cannot be serialized (cycles are impossible in
  `serde_json::Value`) falls back to an empty string — safe.
- **Recommendation**: No action required.
- **Blocking**: no.

### Finding 4 — OWASP A04 (Insecure Design): error strings from agent tool failures flow into observation DB

- **Severity**: low (by design, accepted)
- **Location**: `extract_error_field`, `ToolFailureRule.detect()`
- **Description**: Error messages produced by Claude Code tool execution (e.g., "permission denied",
  shell error output) are stored verbatim (truncated at 500 bytes) into the `response_snippet`
  column. These strings originate from the OS or tool runtime — they are not end-user input in the
  web-request sense. They are bounded by the 500-byte truncation. The same column already stores
  tool response snippets from PostToolUse (col-018), so the trust boundary is unchanged. The
  `evidence_map` in `ToolFailureRule` further copies `response_snippet` into `EvidenceRecord.detail`
  which is returned in retrospective findings. No sanitization occurs, which is appropriate since
  these findings are consumed by authenticated internal agents, not displayed in a public UI.
- **Recommendation**: No action required for col-027. Document in any future UI layer (Matrix phase)
  that `EvidenceRecord.detail` may contain raw OS error strings and must be escaped on output.
- **Blocking**: no.

### Finding 5 — Integer overflow audit in ToolFailureRule

- **Severity**: low (pass)
- **Location**: `friction.rs:130`, `149`, `155-156`
- **Description**: `failure_counts` accumulates `u64` values. The cast `*count as f64` for
  `measured` and `TOOL_FAILURE_THRESHOLD as f64` for `threshold` are safe: u64 values up to 2^53
  are representable exactly in f64 (IEEE 754). No realistic observation set would reach 9 * 10^15
  records. The `pre_counts.saturating_sub(terminal_counts)` paths in `PermissionRetriesRule` and
  `metrics.rs` already use `saturating_sub`, preventing any underflow. No overflow risk found.
- **Recommendation**: No action required.
- **Blocking**: no.

### Finding 6 — No hardcoded secrets

- **Severity**: pass
- **Description**: Diff contains no API keys, credentials, tokens, or connection strings. All new
  constants are event type name strings and a numeric threshold.
- **Blocking**: no.

### Finding 7 — No new dependencies introduced

- **Severity**: pass
- **Description**: The diff introduces no new Cargo dependencies. All changes use existing crates
  (serde_json, unimatrix_core, unimatrix_engine). No known CVE surface is added.
- **Blocking**: no.

---

## Blast Radius Assessment

The worst-case scenario if this fix has a subtle bug:

**Most dangerous failure mode**: If `extract_error_field` were to return `Some(large_string)` instead
of truncating, the `response_snippet` column could store oversized strings. However, `truncate_at_utf8_boundary`
is a pre-existing, tested utility that applies the same bound as the existing code path. The failure
mode is silent data oversize — not data corruption or privilege escalation.

**Second concern**: If the `x if x == hook_type::POSTTOOLUSEFAILURE` guard arm in
`extract_observation_fields` were silently bypassed and fell to the wildcard `(None, None, None, None)`,
the record would be stored with `tool=NULL` and `response_snippet=NULL`. This is incorrect but safe —
the retrospective finds fewer hotspots, not a security breach. The arm ordering in the match is correct
and the test at T-OS-08 verifies this path.

**ToolFailureRule regression**: If `ToolFailureRule` fires for events from non-claude-code domains
(e.g., "sre"), false findings would appear in retrospectives. The `source_domain == "claude-code"`
pre-filter prevents this and is tested (T-FM-16/17).

**PermissionRetriesRule regression**: The semantic change (treating PostToolUseFailure as a terminal
event, reducing false-positive retries) is the intended fix. Regression risk: previously inflated
`permission_friction_events` counts in features with many failure events would now drop. This is
correct behavior — the two-site agreement tests (T-FM-08/09/10) cover this.

---

## Regression Risk

**Medium** risk to the PermissionRetriesRule path. The change widens `terminal_counts` to include
PostToolUseFailure records. Any historical test fixture that expected a specific `permission_friction_events`
count built solely from PostToolUse records will remain correct (those fixtures have no failure events).
But a fixture that expected a high friction count because failure events were previously not counted
as terminal would now see a lower count. The two-site coherence tests (T-FM-08 through T-FM-10) lock
in the expected behavior and would catch such regressions.

**Low** risk to observation storage. The `hook` column will now contain the new string value
`"PostToolUseFailure"`. Any consumer doing `WHERE hook = 'PostToolUse'` (exact match) is unaffected.
Any consumer doing `WHERE hook LIKE '%ToolUse%'` would newly pick up failure records — this pattern
does not exist in the codebase (verified: no LIKE query on the hook column in the current source).

**Low** risk to detection rule count. Rule count jumps from 21 to 22. Tests assert the exact count
and the new rule name. Any caller that asserts count == 21 (there is one, now updated) would need
updating. The change is covered.

---

## PR Comments

The findings above were posted to PR #388 as a review comment. See below.

---

## Knowledge Stewardship

- nothing novel to store — the doc-comment precision gap (bytes vs chars) and the dead-variable
  pattern are too feature-specific to generalize as lessons at this time. The truncation inconsistency
  is a single-site artifact of two independently written sibling functions; no cross-feature pattern.
