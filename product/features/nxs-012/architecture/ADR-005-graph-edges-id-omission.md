## ADR-005: graph_edges.id Omitted from Export

### Context

`graph_edges.id` is an INTEGER PRIMARY KEY AUTOINCREMENT. It serves only as a synthetic row identifier -- no other table references it via FOREIGN KEY or any application logic. The UNIQUE constraint on (source_id, target_id, relation_type) is the real identity of an edge.

Three options:
- Option A: Export id, import with explicit id (like audit_log and observations). Preserves exact row identity.
- Option B: Omit id from export, let SQLite assign fresh ids on import. Simpler, no risk of id conflicts.
- Option C: Export id but let SQLite reassign (import without explicit id). Confusing -- exported data has ids that are not preserved.

### Decision

Option B. Omit `graph_edges.id` from export. Export 9 columns (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata). The `insert_graph_edge` INSERT statement does not include `id` -- SQLite AUTOINCREMENT assigns fresh values.

This differs from `observations.id` and `cycle_events.id` (ADR-006) because those ids serve as watermarks and ordering keys used by extraction ticks and cycle event sequencing. `graph_edges.id` has no such downstream usage.

### Consequences

- Exported graph_edges are identified by their natural key (source_id, target_id, relation_type), not a synthetic id
- Import is simpler -- no risk of AUTOINCREMENT sequence conflicts
- The UNIQUE constraint on (source_id, target_id, relation_type) catches duplicates during import without needing id-based dedup
- If a future feature adds FK references to graph_edges.id, this decision would need to be revisited (currently no such references exist)
