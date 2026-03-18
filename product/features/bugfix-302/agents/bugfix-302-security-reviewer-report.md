# Security Review: bugfix-302-security-reviewer

## Risk Level: low

## Summary

The fix converts two synchronous `audit.log_event()` calls (which held the write pool via `block_in_place`) to fire-and-forget async dispatches via `tokio::spawn`. The change is minimal (3 source files), follows an established pattern already used at every other audit site, and introduces no new inputs, trust boundaries, injection surfaces, or dependencies. The audit write is demoted from error-surfaced to best-effort — an explicitly accepted trade-off that matches all other audit call sites in the codebase. No blocking findings.

---

## Findings

### Finding 1 — Audit failure is silently discarded

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/store_ops.rs:218-221`, `store_correct.rs:96-99`
- **Description**: The fire-and-forget pattern (`let _ = audit.log_event_async(...).await`) discards any `Err` returned by `log_audit_event`. A storage error (e.g., disk full, pool timeout, database corruption) after a successful `context_store` or `context_correct` operation will produce a stored entry with no audit record, and no error surfaced to the caller. This is an availability-vs-auditability trade-off, not an injection or access control issue.
- **Recommendation**: This is the same trade-off accepted at every other audit fire-and-forget site in the codebase (documented in entries #2125, #731). The investigator report explicitly acknowledges it. Consider adding a `tracing::warn!` inside the spawned task on `Err` to preserve observability without blocking the caller. This is a suggestion, not a blocker.
- **Blocking**: no

### Finding 2 — Audit write is no longer atomic with the entry insert

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/store_ops.rs`, `store_correct.rs`
- **Description**: Previously, the synchronous `log_event()` call was sequenced after the entry insert committed. With `tokio::spawn`, the audit write is decoupled — a process crash immediately after `context_store` returns success could lose the audit record while the entry is durably stored. This widens a pre-existing window (it existed for other audit sites already) to two additional write operations.
- **Recommendation**: Document this invariant in the `log_event_async` doc comment (currently present in audit.rs:37-50) and in the `AuditLog` struct-level doc. The trade-off is accepted and consistent with the existing design. Not a blocker.
- **Blocking**: no

### Finding 3 — `make_store()` test helper leaks tempdir

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/audit.rs:92`
- **Description**: The test helper calls `std::mem::forget(dir)` to prevent the `TempDir` from being deleted while the store is open. This is an intentional pattern (shared across the existing test suite), but it leaks the temp directory on every test run. This is a test infrastructure concern, not a production security issue.
- **Recommendation**: Pre-existing pattern; no action required for this PR.
- **Blocking**: no

---

## OWASP Evaluation

| Category | Assessment |
|----------|-----------|
| Injection (SQL, command) | No new injection surface. `AuditEvent` fields are constructed from already-validated caller data. The event is passed to `SqlxStore::log_audit_event()`, which uses parameterized queries (established in nxs-011). |
| Broken access control | No change to capability checks, trust level resolution, or registry lookup. The audit write path has no access control gate — correct, it is append-only. |
| Security misconfiguration | No configuration changes. No new env var reads. |
| Vulnerable components | No new dependencies introduced. `tokio::spawn` and `Arc::clone` are stdlib/existing tokio usage. |
| Data integrity failures | See Finding 1 and Finding 2 above. Audit records can be lost on crash. Entry data itself is unaffected. |
| Deserialization risks | No deserialization of untrusted data in the changed code. |
| Input validation gaps | No new external inputs. `AuditEvent` is constructed internally from session-scoped data. |
| Secrets / credentials | No hardcoded secrets, API keys, or tokens. Gate report confirms this. |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: A deadlock or panic inside the spawned task would silently drop the audit event. This is containable — it cannot corrupt entry data, escalate privileges, or affect capability resolution. The production call path (`context_store`, `context_correct`) would continue to succeed. The worst observable outcome is a missing audit record.

If `log_audit_event` itself has a bug that causes it to corrupt the database under concurrent access, that could affect subsequent reads. However, this function is not new — it is the same async path already used at every other audit site, tested by 14 pre-existing audit unit tests plus 2 new regression tests. No new code runs inside `log_audit_event`.

**Scope of impact**: The change touches only audit write paths for `context_store` and `context_correct`. All read operations, search operations, capability resolution, and agent enrollment are unaffected.

---

## Regression Risk

**Low.** The gate report confirms 16/16 audit unit tests pass, 1357 workspace tests pass, and 10 pre-existing failures are tracked under GH#303 (not introduced by this fix). The two new regression tests (`test_log_event_async_concurrent_does_not_starve`, `test_log_event_async_does_not_block_in_place`) mechanistically target the root cause, not just the symptom.

The only behavioral change visible to callers is that audit failures are no longer propagated as errors from `context_store` or `context_correct`. This is a strict improvement in availability (callers no longer fail due to transient audit write errors) at the cost of silent audit loss — a trade-off consistent with the existing design at all other audit sites.

**Not at risk**: entry insertion, vector indexing, duplicate detection, adaptation prototypes, rate limiting, capability checks, agent enrollment.

---

## PR Comments

- Posted 1 comment on PR #304 via `gh pr review`
- Blocking findings: no

---

## Knowledge Stewardship

- nothing novel to store — the fire-and-forget audit pattern and its auditability trade-off are already captured in Unimatrix entries #2125 and #731 as established conventions. No new cross-feature anti-pattern observed.
