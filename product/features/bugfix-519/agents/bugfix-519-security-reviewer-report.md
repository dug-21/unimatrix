# Security Review: bugfix-519-security-reviewer

## Risk Level: low

## Summary

The fix correctly addresses GH #519 with a minimal two-part change: pre-registering evicted
sessions in `handle_cycle_event` (guarded by `get_state().is_none()`) and adding
`sanitize_session_id` guards to the `RecordEvent` and `RecordEvents` dispatch arms. All
OWASP-relevant concerns are handled. No blocking findings. One pre-existing gap (rework arm
missing `sanitize_session_id`) is noted as a non-blocking follow-up.

---

## Findings

### F-01: Pre-existing — rework arm lacks sanitize_session_id guard

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:657` (`post_tool_use_rework_candidate` arm)
- **Description**: The `post_tool_use_rework_candidate` arm writes to `session_registry.record_rework_event`
  and `session_registry.record_topic_signal` using an unvalidated `session_id`. This arm predates this PR
  and is identical on `main`. It does NOT call `register_session` and cannot reach the GH #519 re-registration
  path. However, the Unimatrix lesson #3902 states: "adding a session registry call triggers
  sanitize_session_id audit" and the consistency pattern #3921 requires all dispatch arms that use
  session_id to carry the guard.
- **Recommendation**: File a follow-up GH issue to add `sanitize_session_id` to the
  `post_tool_use_rework_candidate` arm for consistency with the established pattern. This is pre-existing
  debt and not introduced by this PR.
- **Blocking**: no

### F-02: Reviewed — sanitize_session_id placement is correct and load-bearing

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:731` and `863`
- **Description**: The `sanitize_session_id` check is placed immediately after the capability gate and
  before any registry mutation, including before `handle_cycle_event` is called. This correctly prevents
  malformed session IDs from reaching `register_session` via the new evicted-session re-registration path.
  The guard at line 731 gates the entire `RecordEvent` arm including the cycle routing block at line 751.
  The batch guard at line 862 correctly fails fast on the first invalid ID, preventing partial writes.
- **Recommendation**: No action needed. Guard placement is correct.
- **Blocking**: no

### F-03: Reviewed — get_state().is_none() correctly protects live sessions

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2380`
- **Description**: The three-condition guard (`CycleLifecycle::Start && !feature_cycle.is_empty() &&
  session_registry.get_state(&event.session_id).is_none()`) ensures `register_session` is only called
  when the session is absent. Live sessions with accumulated state (injection_history, coaccess_seen,
  topic_signals, category_counts) are not overwritten. There is no TOCTOU issue: the lock is
  re-acquired by `register_session`, which inserts unconditionally (overwrites). In the worst case a
  session could be re-registered by another concurrent event between the `get_state` check and the
  `register_session` call. However: (a) Claude Code hook events are serialized per session per the
  session.rs doc comment, and (b) even if concurrent re-registration occurred, the data overwritten is
  in-memory session state — no privilege escalation path exists because `role` is not used for
  capability gating (capabilities are enforced at the UDS connection level, not per-session role).
- **Recommendation**: No action needed. The guard is correct for its purpose.
- **Blocking**: no

### F-04: Reviewed — register_session with None role is not a privilege escalation vector

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2382-2386`
- **Description**: `register_session` is called with `role = None`. The `role` field on `SessionState`
  is used only in `format_compaction_payload` to populate a display header ("Role: ...") in the
  briefing text. It is NOT used in any capability check, trust-level decision, or access control
  evaluation anywhere in the codebase. The actual capability enforcement for UDS connections is
  performed by `uds_has_capability(Capability::SessionWrite)` at the transport level, before any
  session-specific logic. Passing `None` for role on re-registration means compaction output omits
  the "Role:" header for this session — this is cosmetic and functionally equivalent to `SessionRegister`
  arriving before the role is known.
- **Recommendation**: No action needed. None role carries no privilege escalation risk.
- **Blocking**: no

### F-05: Reviewed — feature_cycle value in register_session is sanitized

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2347, 2385`
- **Description**: The `feature_cycle` value passed to `register_session` in Step 1b has already been
  through `sanitize_metadata_field` (line 2347: ASCII printable, truncated at 128 chars) before
  reaching the Step 1b guard. No raw payload value reaches the registry.
- **Recommendation**: No action needed.
- **Blocking**: no

---

## OWASP Assessment

| Category | Status | Notes |
|----------|--------|-------|
| Injection (session_id) | Clear | sanitize_session_id blocks non-alphanumeric chars including path separators, SQL metacharacters, shell metacharacters |
| Injection (feature_cycle) | Clear | sanitize_metadata_field strips non-printable ASCII and truncates |
| Broken access control | Clear | role=None not used in capability gating; UDS capability enforcement precedes all session ops |
| Security misconfiguration | Clear | No new configuration, env vars, or transport changes |
| Input validation gaps | Partial (pre-existing) | rework arm missing sanitize_session_id — pre-existing on main, not introduced by this PR |
| Deserialization | Clear | No new deserialization paths; existing serde boundary unchanged |
| Error handling | Clear | Errors returned as HookResponse::Error; no internal state leaked in messages |
| Secrets | Clear | No hardcoded credentials or keys in the diff |

---

## Blast Radius Assessment

Worst case scenario if the fix has a subtle bug:

**Scenario A — get_state().is_none() check races with eviction:**
If a session is evicted between the `get_state` check and `register_session`, the re-registration
would overwrite the newly-evicted (empty) entry, resetting in-memory state for that session.
Impact: loss of in-memory session state (injection_history, topic signals). Data integrity failure
(topic_signal = NULL for observations) — the same class of failure as the original bug.
No silent data corruption of stored entries; observations already written to DB are unaffected.

**Scenario B — register_session is reached with invalid session_id (guard bypass):**
Not possible: `sanitize_session_id` blocks all non-alphanumeric session IDs at line 731, which
gates the entire `RecordEvent` arm including the cycle routing block. There is no code path that
calls `handle_cycle_event` without passing through the line 731 guard first.

**Scenario C — feature_cycle=empty string bypasses !feature_cycle.is_empty() guard:**
If `feature_cycle` is empty after sanitization, the Step 1b block is skipped entirely. No
re-registration occurs. `set_feature_force` hits the None arm (silent no-op) as before. Effect:
topic_signal remains NULL — the original bug behavior, but only for the degenerate case of
empty feature_cycle, not the normal eviction scenario.

Overall blast radius: bounded to in-memory session state loss. No persistent data corruption.
No privilege escalation. Failure mode is safe (returns Ack; topic_signal = NULL, observable in DB).

---

## Regression Risk

**Low.** The three-condition guard ensures the new code path fires only when:
1. The event is a `cycle_start`
2. `feature_cycle` is non-empty
3. The session is absent from the registry

All other event types, empty feature_cycle cases, and live-session cases are unaffected. The
2734-test workspace suite passed and the regression test verifies the complete causal chain
from eviction through to per-observation DB attribution.

The one area not covered by integration tests is the UDS path through the stdio-transport
integration harness — this is a pre-existing gap acknowledged in the gate report. The unit
test exercises the UDS dispatch path directly and is sufficient.

---

## PR Comments

Posted 1 comment on PR #521 (non-blocking findings summary).
Blocking findings: no.

---

## Knowledge Stewardship

- Queried: Unimatrix entries #3902 (lesson: adding a session registry call triggers sanitize_session_id audit), #3921 (pattern: all UDS arms with session_id must carry sanitize_session_id), #4135 (lesson: set_feature_force silently no-ops for absent sessions), #4136 (pattern: pre-register absent sessions in handle_cycle_event before set_feature_force).
- Stored: nothing novel to store — the relevant patterns (#3902, #3921, #4135, #4136) already exist and this review produced no new recurring security anti-patterns beyond what is already captured.
