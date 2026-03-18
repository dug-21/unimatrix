# Security Review: bugfix-308-security-reviewer

## Risk Level: low

## Summary

The PR fixes 5 remaining call sites where `AuditLog::log_event()` was called from async
context, starving the tokio write pool. All changes follow an already-established pattern
(GH #302) and are limited to async scheduling of audit writes. No new inputs, no new
deserialization, no new trust boundaries, no new dependencies, and no hardcoded secrets.
One informational finding is noted regarding two parallel audit-dispatch strategies
co-existing in the codebase.

---

## Findings

### Finding 1: Silent Audit Loss on All Five Fixed Sites (server.rs)

- **Severity**: low
- **Location**: `server.rs:461`, `server.rs:518`, `server.rs:821`
- **Description**: All three server.rs fire-and-forget spawns silently discard errors:
  `let _ = audit.log_event_async(...).await;`. If the audit write fails (e.g., write pool
  exhausted, DB closed during shutdown), no warning is emitted. The two `background.rs`
  sites do emit a `tracing::warn!` on failure, which is the correct pattern.
  This is not new behaviour for the server.rs sites — the previous code also used `map_err`
  only to propagate to the caller, but with fire-and-forget there is no caller to propagate
  to. The risk is that audit gaps during normal error conditions are now silent rather than
  logged. In adversarial terms, an operator monitoring audit completeness cannot distinguish
  "no write operation occurred" from "write occurred but audit write silently failed".
- **Recommendation**: Mirror the `background.rs` pattern in server.rs spawns:
  `if let Err(e) = audit.log_event_async(...).await { tracing::warn!(...) }`.
- **Blocking**: no — the original server.rs sites already had no guaranteed audit delivery;
  this is a marginal regression in observability, not a security gate issue.

### Finding 2: Two Co-Existing Audit Dispatch Strategies

- **Severity**: low (informational)
- **Location**: `server.rs:399–408` (`audit_fire_and_forget`), `mcp/tools.rs` (7 callers)
- **Description**: After this fix, two fire-and-forget audit mechanisms co-exist:
  1. `audit_fire_and_forget()` — used by 7 sites in `mcp/tools.rs`. Uses
     `spawn_blocking` + `log_event()` (which internally calls `block_in_place`).
     `spawn_blocking` moves work off the async thread, so `block_in_place` runs inside
     a blocking thread pool thread. This is different from but not equivalent to the new
     pattern, and may still contend on the write pool connection.
  2. `tokio::spawn(async move { log_event_async() })` — new pattern from GH #308,
     5 sites across `server.rs` and `background.rs`.
  This inconsistency is not introduced by this PR (the helper predates it), but it means
  the original starvation bug may be incompletely fixed — if the 7 `mcp/tools.rs` sites
  remain on the old `spawn_blocking` path, they can still run `block_in_place` on the
  write pool during the analytics drain window. This is outside the stated scope of this
  PR (which targets 5 specific missed sites), but it is a residual starvation risk.
- **Recommendation**: Track conversion of the 7 `audit_fire_and_forget` callers in
  `mcp/tools.rs` to the async pattern in a follow-up issue. The helper itself could also
  be updated to use `log_event_async`.
- **Blocking**: no — this is pre-existing behaviour not introduced by this PR.

### Finding 3: Test Timing Dependency on sleep(50ms)

- **Severity**: low (informational)
- **Location**: `server.rs:1545`, `server.rs:1855`, `background.rs:1898`, `background.rs:1953`
- **Description**: Four tests that verify audit event presence after fire-and-forget calls
  now insert a `tokio::time::sleep(Duration::from_millis(50))` to wait for the spawned
  task to commit. This is a time-based synchronisation pattern, not a deterministic one.
  On a heavily loaded CI runner or a slow SQLite write, 50ms may be insufficient. A
  flaking test would create a false negative — the audit event would appear "missing" in
  test but be present in production. The regression tests (`test_insert_with_audit_does_not_block_under_concurrent_writes`)
  correctly use `yield_now()` rather than sleep, but the four existing tests use sleep.
- **Recommendation**: Replace sleep with polling/retry (e.g., `tokio::time::timeout`
  wrapping a loop) or inject a completion channel. Acceptable as-is for now given 50ms
  is well above typical SQLite write latency; track as a test-reliability improvement.
- **Blocking**: no.

---

## OWASP Evaluation

| Category | Assessment |
|----------|-----------|
| Injection (SQL, command) | No new SQL construction. `log_event_async` delegates to existing `SqlxStore::log_audit_event` — no change to query parameterisation. |
| Broken access control | No change to trust boundaries, capability checks, or agent identity verification. Audit writes remain internal-only. |
| Security misconfiguration | No configuration changes. No new flags, env vars, or defaults. |
| Vulnerable components | No new crate dependencies introduced (`Cargo.toml` and `Cargo.lock` not changed). |
| Data integrity failures | The data transaction (insert/correct/quarantine) commits before the audit task is spawned. The main operation is atomic; audit is explicitly best-effort (pre-existing design). No integrity regression. |
| Deserialization | No new deserialization. `AuditEvent` structs are constructed in-process from already-validated fields. |
| Input validation | No new external inputs. The changed functions receive `AuditEvent` structs constructed from previously validated data; no raw user input reaches the spawned async block. |
| Secrets / credentials | No hardcoded secrets, API keys, tokens, or credentials anywhere in the diff. |

---

## Blast Radius Assessment

**Worst case if this fix has a subtle bug:**

The spawned audit task runs after the data commit. If `log_event_async` panics (rather
than returning Err), the spawned task is silently dropped — tokio catches the panic but
the audit record is lost. A panic inside an audit write does not propagate to the
caller; the data operation has already succeeded and been returned to the client. The
failure mode is: **audit gap without data corruption or service disruption**. There is no
path from an audit write failure to data loss, access control bypass, or denial of service.

A more subtle risk: if the spawned task is scheduled after the runtime begins shutdown,
tokio will abort it. This means audit events for operations performed at server shutdown
time may be silently lost. This is a pre-existing risk with the `audit_fire_and_forget`
pattern, now extended to 5 more sites. Not a new regression.

---

## Regression Risk

- **Audit completeness**: marginal regression. server.rs sites previously returned errors
  to the caller on audit failure; now failures are silently dropped. The background.rs
  sites correctly log warnings. Risk of undetected audit gaps is slightly elevated.
- **Data operations**: no regression risk. Data commits are unaffected by the audit
  spawning strategy. The order (data commit → spawn audit task) is preserved at all sites.
- **Write pool starvation**: the fix reduces contention. No regression in normal operation.
- **Test stability**: 4 tests now depend on sleep(50ms) for audit timing. These could
  produce spurious failures under severe load; 50ms is conservative for a local SQLite
  write.
- **Existing functionality**: all 1359 passing tests continue to pass; the 10 pre-existing
  failures (GH #303) are unrelated.

---

## PR Comments

- Posted 1 comment on PR #310.
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the finding that silent audit loss is worse than logged
  audit loss in fire-and-forget patterns is a code quality observation, not a new security
  anti-pattern specific to this project. The inconsistency between `audit_fire_and_forget`
  (spawn_blocking) and the new async pattern is already implied by GH #308 scope
  boundaries. Nothing generalizable beyond what the investigator and developer already captured.
