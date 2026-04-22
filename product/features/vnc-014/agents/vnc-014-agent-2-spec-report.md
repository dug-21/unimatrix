# Agent Report: vnc-014-agent-2-spec

## Output

SPECIFICATION.md written to:
`product/features/vnc-014/specification/SPECIFICATION.md`

## Summary

Specification covers all 10 SCOPE.md acceptance criteria (AC-01 through AC-10) plus two
extensions (AC-11, AC-12) addressing SR-04 and SR-05 risk mitigations. 12 functional
requirements, 8 non-functional requirements, full domain model with 12-field AuditEvent,
3 user workflows, constraint set, and dependency inventory.

## Key Decisions Made

1. **AC-11 added**: SR-05 risk (free-form `capability_used` strings diverging across tools)
   is addressed by requiring `Capability::as_str()` — a new method on the existing `Capability`
   enum. This is a spec-level requirement, not an implementation detail.

2. **AC-12 added**: SR-04 risk (missed `build_context()` call sites) is addressed by requiring
   the old function be removed or deprecated, producing compile-time enforcement. Spec mandates
   this explicitly.

3. **FR-11 scope extension**: SR-07 (append-only triggers breaking test DELETEs) is addressed
   by requiring `gc_audit_log` to become a no-op and `import::drop_all_data` to use DROP+recreate
   or a guarded alternative. Three concrete options are described; implementation agent decides.

4. **`metadata` non-nullable confirmed**: Diverges from `graph_edges.metadata` (which is
   nullable). The audit log compliance requirement makes `NOT NULL DEFAULT '{}'` correct here.
   Documented explicitly in C-04.

5. **`ResolvedIdentity` as a stub**: VNC-014 ships a minimal type to satisfy the Seam 2
   signature. The concrete type definition location (server vs core crate) is left as OQ-A
   for the architect.

6. **SR-01 (Mutex contention) accepted**: Spec documents the acceptable concurrency bound
   and requires AC-07 to include a comment noting the DashMap migration path for W2-2.

## Open Questions

- **OQ-A**: Where to define `ResolvedIdentity` — `unimatrix-server` vs `unimatrix-core`.
  Affects crate dependency graph. Architect decision.
- **OQ-B**: Whether `gc_audit_log` callers beyond tests exist in production paths. If none,
  no-op is safe. Implementation agent should grep before deciding.
- **OQ-C**: Whether `import::drop_all_data` can access store DDL initialization, or whether
  a new `SqlxStore::reset_for_import()` method is needed.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 14 entries returned. Entry #4047 (AuditEvent
  5-surface update pattern) directly applicable. Entry #317 (AuditContext construction) and
  entry #296 (transport service extraction) confirm established patterns. Migration idempotency
  pattern confirmed from live migration.rs inspection (pragma_table_info guards at lines
  184, 215, 315, 464, 512, 538, 563).
