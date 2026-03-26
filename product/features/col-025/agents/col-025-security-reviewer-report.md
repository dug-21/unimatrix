# Security Review: col-025-security-reviewer

---

## Bugfix Review: PR #393 — GH #391 (set_current_goal None guard)

**Date**: 2026-03-26
**Branch**: bugfix/391-set-current-goal-none-guard
**Review scope**: `git diff main...HEAD` on PR #393, cold read from fresh context.

---

## Risk Level: low

## Summary

The diff is a single-file, minimal guard fix in `crates/unimatrix-server/src/uds/listener.rs`. The change wraps one unconditional call to `session_registry.set_current_goal(...)` in an `if goal.is_some()` block so that a `cycle_start` event with no `goal` key does not overwrite an existing session goal. No new dependencies are introduced. No inputs, deserialization paths, access control surfaces, or secrets are affected. The fix is safe.

---

## Findings

### Finding 1: Guard placement is correct and consistent with session-resume call site

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2461-2463`
- **Description**: The guard `if goal.is_some()` is applied only at the `cycle_start` event call site. The session-resume call site at ~L588 remains intentionally unconditional, as mandated by ADR-004 (deterministic initialization on resume, even when the DB lookup returns `None`). The two call sites have different contracts and the fix correctly differentiates them.
- **Recommendation**: None. The guard placement matches the existing `set_current_phase` pattern cited in the gate report, and the intentional difference between the two call sites is documented in inline comments.
- **Blocking**: no

### Finding 2: No new input validation surface introduced

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2436-2468`
- **Description**: The fix does not add, remove, or bypass any input validation. The `goal` value is still extracted from `event.payload`, still run through the `MAX_GOAL_BYTES` truncation guard (`truncate_at_utf8_boundary`), and the resulting `Option<String>` is what the guard checks before calling `set_current_goal`. The validation chain is intact and unchanged.
- **Recommendation**: None.
- **Blocking**: no

### Finding 3: No injection risk

- **Severity**: informational (positive finding)
- **Description**: The changed lines involve only in-memory registry state writes. No SQL query construction, no shell execution, no deserialization of untrusted data was modified. The SQL persistence path for `goal` (the `insert_cycle_event` call site downstream in a `tokio::spawn`) is not touched by this diff.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: Test assertion flip is semantically correct

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:6411-6417`
- **Description**: The test previously pinned the buggy behavior (`current_goal == None`). The new assertion (`current_goal.as_deref() == Some("existing goal")`) correctly describes the intended post-fix behavior. The test name and doc comments are updated consistently. Companion test `test_uds_cycle_start_no_goal_sets_none` (which verifies a fresh session receiving a bare `cycle_start` still correctly results in `current_goal = None`) was also confirmed passing by the verification agent — this is the key regression check for the guard.
- **Recommendation**: None.
- **Blocking**: no

### Finding 5: No hardcoded secrets in diff

- **Severity**: informational (positive finding)
- **Description**: The diff contains no API keys, tokens, passwords, or credentials. The only string literals added are doc comment text.
- **Blocking**: no

### Finding 6: No unsafe code

- **Severity**: informational (positive finding)
- **Description**: The diff introduces zero lines containing `unsafe`. The gate report confirms this and the diff itself shows only safe Rust.
- **Blocking**: no

### Finding 7: Pre-existing clippy warning in unrelated crate

- **Severity**: low (pre-existing, not introduced by this fix)
- **Location**: `crates/unimatrix-engine/src/auth.rs:113`
- **Description**: `cargo clippy --workspace -- -D warnings` reports a `collapsible_if` error. The gate report correctly identifies this as pre-existing — the file is untouched by this PR. It is not a security concern.
- **Recommendation**: Track and fix in a separate PR. Not blocking for this fix.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if the guard has a subtle bug:** The only failure mode from this change is behavioral, not security-class. If `goal.is_some()` somehow fails to evaluate correctly (impossible for a standard Rust `Option` check), the worst outcome is that a previously-set goal is either incorrectly overwritten (returning to the pre-fix bug) or never updated when it should be (if someone passes an explicit new goal on a second `cycle_start`). Both are data-quality failures, not security failures.

The second scenario — a user intentionally sending a new goal on a second `cycle_start` — is explicitly handled: because the second `cycle_start` payload would contain a `goal` key, `goal` would be `Some(...)`, and the guard would permit the write. This is correct behavior.

No data corruption, no silent data loss, no privilege escalation, no information disclosure is possible from this change.

---

## Regression Risk

**Well-covered by tests:**
- `test_cycle_start_missing_goal_does_not_overwrite_existing` — directly exercises the fixed path (second bare `cycle_start` preserves goal).
- `test_uds_cycle_start_no_goal_sets_none` — covers the AC-02 regression case (fresh session, bare `cycle_start`, goal stays `None`).
- 20/20 smoke, 37+2xfail lifecycle, 94+1xfail tools, 13/13 protocol integration tests all pass.

**Risk areas not newly introduced:**
- The session-resume path at ~L588 is unconditional and untouched. No risk introduced there.
- The `set_current_goal` function in `session.rs` accepts `None` and correctly writes it (Passing `None` resets to "no goal"). The guard in `listener.rs` is the correct place to block that write — the underlying function signature is not changed and remains fully general.

Overall regression risk: low.

---

## OWASP Assessment

| Concern | Status |
|---------|--------|
| Injection (SQL, command, path traversal) | Not applicable to this change |
| Broken access control | Not applicable — no access control paths modified |
| Security misconfiguration | Not applicable — no config changes |
| Input validation gaps | None introduced; existing validation preserved |
| Deserialization risks | Not applicable — no serialization/deserialization changed |
| Vulnerable components | No new dependencies introduced |
| Secrets/credentials | None present in diff |

---

## PR Comments

- Posted 1 comment on PR #393 summarizing findings.
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the `if Some` guard pattern for session state writes is well-established in this codebase and not a new anti-pattern. The bugfix is a correct application of an existing known pattern, not a new generalizable discovery.

---

## Prior Review (PR #375 — original col-025 feature)

The content below documents the prior security review for the col-025 feature landing PR. It is retained for traceability.

---

## Risk Level (PR #375): medium

## Summary (PR #375)

col-025 adds a `goal` field to the feature cycle lifecycle — stored in `cycle_events` (schema v16), cached in `SessionState`, and used as the retrieval query for `IndexBriefingService`. The diff is well-structured: parameterized SQL binds prevent injection, input validation is layered correctly across MCP and UDS paths, and the test suite directly addresses the risk register items. One medium-severity finding requires a non-blocking fix before next merge: a raw byte slice on the `goal_text` value in a `tracing::debug!` call (`listener.rs:936`) is not char-boundary-safe and will panic on any non-ASCII goal string whose 50th byte falls in the middle of a multi-byte UTF-8 sequence.

### Finding 1 (PR #375): Unsafe byte-index slice on `goal_text` in debug log (listener.rs:936)

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:936`
- **Description**: `&goal_text[..goal_text.len().min(50)]` slices at a raw byte offset. Non-ASCII goals can panic.
- **Recommendation**: Use `truncate_at_utf8_boundary(goal_text, 50)` which is already in scope.
- **Blocking**: no (debug-level only, but panic-class defect)

### Finding 2 (PR #375): Goal text reflected verbatim in MCP response string

- **Severity**: low
- **Blocking**: no (intentional acknowledgment, input sanitized upstream)

### Finding 3–9 (PR #375): Positive findings

SQL injection: parameterized binds. Migration idempotency: pragma pre-check. Audit log: records `"goal=present"` not goal content. Adversarial embedding input: bounded by `MAX_GOAL_BYTES`. No hardcoded secrets.

PR #375 comment posted (1 comment, no blocking findings).
