## ADR-008: EDGE_SOURCE_AGENT Constant Placement and created_by Convention

### Context

`write_graph_edge` binds the `source` parameter to BOTH the `source` column AND the `created_by`
column in the INSERT statement (confirmed `nli_detection.rs:78-118`, ?6 bound twice):
```sql
INSERT OR IGNORE INTO graph_edges
(source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0, ?7)
```

The established pattern for edge source constants is:
- `EDGE_SOURCE_NLI = "nli"` — defined in `unimatrix-store/src/read.rs`, re-exported from lib.rs (col-029, entry #3591)
- `EDGE_SOURCE_CO_ACCESS` — same pattern (crt-034)
- `EDGE_SOURCE_S1/S2/S8` — same pattern (crt-041)

Agent-declared edges need a new constant. The `write_graph_edge` function will set
`created_by = source = "agent"` for all agent-declared edges, regardless of which specific agent
declared the edge.

A tension exists: SCOPE.md AC-18 states "Edge attribution uses `created_by` (agent_id)." If
this means the specific agent's ID should appear in `created_by`, the current `write_graph_edge`
signature does not support it — there is no separate `created_by` parameter. Passing the actual
agent_id as `source` would break the EDGE_SOURCE_* naming convention and would be inconsistent
with all other sources.

Two options:
- Option A: `EDGE_SOURCE_AGENT = "agent"` — consistent with convention, `created_by = "agent"`
  for all agent-declared edges. Individual agent attribution is not available at the edge level.
- Option B: Pass `agent_id` as `source` — breaks the EDGE_SOURCE_* convention, makes each agent
  its own source, complicates graph cohesion metrics and SQL filters.

### Decision

Use `EDGE_SOURCE_AGENT = "agent"` as the constant (Option A). `created_by` will be `"agent"` for
all agent-declared edges, consistent with the `write_graph_edge` signature that binds `source`
to both columns.

Individual agent attribution for edges is deferred. The entry-level `created_by` field already
records which agent created the source entry; this provides sufficient audit traceability. If
per-edge agent attribution becomes necessary, the `write_graph_edge` signature would need a
separate `created_by` parameter — that is a future enhancement separate from vnc-015.

SCOPE.md AC-18 is interpreted as: "edge attribution uses agent identity context" (meaning the
source entry's `created_by` attribute is the agent, not that the edge row itself stores
the agent_id). This interpretation is consistent with the existing `write_graph_edge` design.

`EDGE_SOURCE_AGENT` is defined in `edge_write.rs` (ADR-005) and re-exported from
`unimatrix-server`'s lib.rs for external visibility (following the EDGE_SOURCE_* pattern from
`unimatrix-store`). It does NOT go in `unimatrix-store/src/read.rs` because agent-declared
edges originate from the MCP server layer, not the store layer. The constant belongs in the
layer where it is used.

### Consequences

Easier: consistent with all existing EDGE_SOURCE_* constants. SQL filter for agent-declared edges
is simple: `WHERE source = 'agent'`. Graph cohesion metrics that count by edge source can
include agent-declared edges in a single named bucket.

Harder: individual agent attribution is not available at the edge level. Audit tracing of
"which agent declared this specific edge" requires a JOIN to the source entry's `created_by`
field, which may not be straightforward for edges between entries created by different agents.

Supersedes: none.
Related: entry #3591 (EDGE_SOURCE_NLI constant placement), pattern #4056 (write_graph_edge source convention).
