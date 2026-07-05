## ADR-002: Audit Removed Edge Tuples, Not Just a Count

### Context

The eager delete is an **irreversible** removal of hand-declared graph relationships at the source event — no soft-delete, no undo, no successor repoint (unlike `context_correct`, which repoints) (SR-01). If the predicate ever mis-keys or an operator later questions a removal, a bare count (`N edges removed`) gives no way to reconstruct *which* relationships were destroyed. The caller-facing advisory (option a) needs only a count, but the audit record (option b) is the durable diagnostic surface.

SQLite (via sqlx) supports `DELETE ... RETURNING`, already used in the codebase (`analytics.rs`). This lets one statement both delete and return the removed rows atomically — no separate SELECT-then-DELETE race.

### Decision

The eager delete uses `DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type`. The helper returns `Vec<RemovedEdge>`; `count = tuples.len()`.

The audit record captures the **tuples**, not just the count. One fire-and-forget `AuditEvent` (via `audit_fire_and_forget`, `server.rs:650`), emitted only on `Ok` with a non-empty result:
- `operation`: `"context_deprecate.edge_cleanup"` (distinct from the flip's `"context_deprecate"` audit event)
- `target_ids`: `[entry_id]`
- `detail`: human-readable count summary
- `metadata`: JSON array of `{source_id, target_id, relation_type}` for each removed edge — the reconstructable record.

The caller advisory (AC-02) carries only the count (ADR-004); tuple detail lives in the audit log, not the tool response.

The predicate is **LOCKED** to exactly `(source_id=? OR target_id=?) AND source='agent'` — never widened by relation-type, never a relation-type blocklist (F2 discipline, `background.rs:849`). Provenance is filtered on the `source` column to the single value `EDGE_SOURCE_AGENT` (`edge_write.rs:28`).

### Consequences

Easier: a wrongful or surprising eager delete is fully diagnosable and reconstructable from the audit log (SR-01). `RETURNING` gives delete + tuple capture in one atomic statement — no SELECT/DELETE race, no extra round-trip.

Harder: audit metadata rows grow with edge count for high-degree entries (bounded by an entry's agent-edge degree — small in practice). The relation_type is captured but not interpreted — the delete applies no relation semantics.
