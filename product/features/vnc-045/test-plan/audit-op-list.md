# Test Plan — Audit op-list update (`unimatrix-store` audit.rs:84)

> Change: add `'context_tag'` to the `audit_write_count_since` op-list (audit.rs:79-92). A **latent** signal (no live throttle consumer). Covers R-07 (op-list inclusion). One-line change → focused coverage.
>
> Seam: `SqlxStore::open` over a temp DB; `audit_write_count_since(agent_id, since)` (audit.rs:79) read directly.

## R-07 — `audit_write_count_since` counts `context_tag` (Low-Med)

1. `test_audit_write_count_includes_context_tag` — log N `AuditEvent`s with `operation="context_tag"` (via `log_audit_event`, audit.rs:19), then assert `audit_write_count_since(agent_id, since)` returns N. Proves `'context_tag'` is in the op-list at audit.rs:84. (FR-09, AC-06b, R-07)
2. `test_audit_write_count_context_tag_since_boundary` — events before `since` are excluded, events at/after `since` counted — proves the op-list addition composes with the existing timestamp filter, not just an unconditional count.
3. `test_audit_write_count_excludes_non_write_ops` — a non-write operation (e.g. a read op NOT in the list) is still excluded after the change — the addition is additive, not a widening that pulls in unrelated ops.

## Notes / Boundaries
- This is a **latent** signal — do NOT assert any throttling/enforcement here. Live throttling is `check_write_rate` (R-07, tested at the service seam in store-tag-service.md).
- No schema change — reuses the existing `audit_log` / `AuditEvent`.
- Full audit-record shape (metadata, `prior_value`, single-event) is R-03 and belongs to the `audit_log` read-back tests in store-tag-service.md, not here — this component only proves op-list membership.
