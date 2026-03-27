# col-029 Researcher Agent Report

## Summary

SCOPE.md written to `product/features/col-029/SCOPE.md`. All required sections
present. 13 acceptance criteria defined with AC-IDs. 4 open questions surfaced.

## Key Findings

### GRAPH_EDGES Schema (confirmed from `crates/unimatrix-store/src/db.rs`)

Table has: `source_id`, `target_id`, `relation_type` (TEXT), `weight`, `created_at`,
`created_by`, `source` (TEXT), `bootstrap_only` (INTEGER, 0=false), `metadata`.
UNIQUE constraint on `(source_id, target_id, relation_type)`.
Three indexes: source_id, target_id, relation_type.

Schema is at v17 (migration.rs CURRENT_SCHEMA_VERSION=17). GRAPH_EDGES was added at
v13 (crt-021). No schema change required.

### Inferred Edge Convention (confirmed from `nli_detection.rs`)

NLI-inferred edges use `source='nli'`, `bootstrap_only=0`. Integration tests
explicitly assert `WHERE source='nli'`. This is the right filter for `inferred_edge_count`.

### StatusReport Integration Point (`mcp/response/status.rs`)

`StatusReport` is a plain struct with manual `Default` impl. Six new fields append
cleanly. Current "graph" fields (`graph_stale_ratio`, `graph_quality_score`,
`graph_compacted`) measure HNSW vector index health — completely separate from
GRAPH_EDGES topology. No naming conflict.

### Computation Pattern (`crates/unimatrix-store/src/read.rs`)

`compute_status_aggregates()` is the direct precedent: SQL aggregate query, returns
a typed struct, called from `StatusService::compute_report()` Phase 1. The new
`compute_graph_cohesion_metrics()` follows identical pattern, called in Phase 5.

All store-layer raw SQL uses `write_pool_server()` — not `read_pool()`. This is
consistent across all server-facing query methods in `read.rs`.

### Phase Placement

`compute_report()` has 8 phases. Phase 5 (coherence dimensions) is where
`graph_quality_score` is computed from VectorIndex atomics. The new graph cohesion
SQL call fits in Phase 5, after the HNSW stale ratio computation.

`MaintenanceDataSnapshot` (the tick-optimized snapshot) explicitly skips Phases 2,
4, 6, 7 to avoid expensive ONNX calls. The new metrics are fast SQL aggregates but
are diagnostic-only — not consumed by any maintenance action — so they should also
be skipped in the tick snapshot. No change to `load_maintenance_snapshot()`.

### TypedRelationGraph vs. GRAPH_EDGES

The in-memory `TypedRelationGraph` already excludes `bootstrap_only=1` rows
structurally (SPECIFICATION: "bootstrap_only=true rows are excluded from inner
during rebuild"). The six cohesion metrics must apply the same `bootstrap_only=0`
filter to reflect actual graph state as seen by PPR.

## Integration Touch Points

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/read.rs` | Add `GraphCohesionMetrics` struct + `compute_graph_cohesion_metrics()` |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Add 6 fields to `StatusReport` + format output |
| `crates/unimatrix-server/src/services/status.rs` | Call `compute_graph_cohesion_metrics()` in Phase 5, assign fields |

## No New Migration Required

All data is in existing tables (GRAPH_EDGES, entries). Schema v17 is sufficient.

## Open Questions for Human

1. **mean_entry_degree direction**: in-degree + out-degree (2x per edge, treating
   connectivity as undirected), or out-degree only? The proposed approach uses 2x
   (undirected view) as the more informative health metric.

2. **bootstrap_only edges in connectivity**: an entry connected only via `bootstrap_only=1`
   edges is structurally isolated from TypedRelationGraph. Should those entries count
   as "connected" or "isolated"? Proposed: they count as isolated (bootstrap_only=0
   filter on all metrics).

3. **Format placement**: graph cohesion sub-section within the existing Coherence
   section (Markdown), or a new "Graph Topology" top-level section?

4. **Phase 5 vs. caching in tick**: is one additional async SQL call per `context_status`
   invocation acceptable, or should the metrics be cached in `MaintenanceDataSnapshot`
   and served from the tick cache instead?

## Knowledge Stewardship

- Queried: /uni-query-patterns for "graph cohesion metrics context_status maintenance report" -- MCP k parameter type mismatch prevented search (string vs i64). Queries returned errors. Queried without category filter — also errored. Pattern not found; proceeding from codebase research.
- Stored: nothing — Write capability unavailable for anonymous agent.
