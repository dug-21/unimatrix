# col-029 Architect Agent Report

Agent ID: col-029-agent-1-architect

## Output Files

- `/workspaces/unimatrix/product/features/col-029/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/col-029/architecture/ADR-001-edge-source-nli-constant.md`
- `/workspaces/unimatrix/product/features/col-029/architecture/ADR-002-two-sql-queries.md`
- `/workspaces/unimatrix/product/features/col-029/architecture/ADR-003-write-pool-server-for-cohesion-queries.md`
- `/workspaces/unimatrix/product/features/col-029/architecture/ADR-004-cross-category-sql-no-cartesian-product.md`

## Unimatrix ADR Entries

- #3591 — ADR-001 col-029: EDGE_SOURCE_NLI Named Constant in unimatrix-store
- #3592 — ADR-002 col-029: Two SQL Queries for Graph Cohesion Metrics
- #3593 — ADR-003 col-029: write_pool_server() for Graph Cohesion SQL Queries
- #3594 — ADR-004 col-029: Cross-Category Edge Count SQL — No Cartesian Product

## Key Design Decisions

1. **EDGE_SOURCE_NLI constant** (ADR-001, SR-01): `pub const EDGE_SOURCE_NLI: &str = "nli"` in `unimatrix-store/src/read.rs`, re-exported from `lib.rs`. Eliminates the seventh bare `"nli"` literal. Migration of the six existing literals in `nli_detection.rs` is deferred follow-up work against GH #412.

2. **Two SQL queries** (ADR-002): Query 1 aggregates pure GRAPH_EDGES (total_edges, supports_edge_count, inferred_edge_count). Query 2 joins entries for connectivity and cross-category counts. Mean degree and connectivity rate computed in Rust from the two query outputs.

3. **write_pool_server()** (ADR-003): Both queries use write_pool_server() for GRAPH_EDGES consistency with NLI writes, despite compute_status_aggregates using read_pool() for its own queries.

4. **Cross-category JOIN** (ADR-004, SR-04): Explicit LEFT JOINs with named aliases src_e/tgt_e, tight ON predicates (keyed to indexed columns), and a multi-guard CASE expression. No cartesian product risk.

## Integration Points for Downstream Agents

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/read.rs` | Add `EDGE_SOURCE_NLI` const, `GraphCohesionMetrics` struct, `compute_graph_cohesion_metrics()` fn, unit tests (7 scenarios) |
| `crates/unimatrix-store/src/lib.rs` | Re-export `GraphCohesionMetrics` and `EDGE_SOURCE_NLI` |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Append 6 fields to `StatusReport` + default block; add Summary line + Markdown `#### Graph Cohesion` sub-section inside `### Coherence` |
| `crates/unimatrix-server/src/services/status.rs` | Call `compute_graph_cohesion_metrics()` in Phase 5 after HNSW stale ratio assignment (~line 669); assign 6 fields; error is non-fatal (warn + skip) |

## SQL for the Two Queries

### Query 1 (pure GRAPH_EDGES, write_pool_server)
```sql
SELECT
    COUNT(*)                                                          AS total_edges,
    COALESCE(SUM(CASE WHEN relation_type = 'Supports' THEN 1 ELSE 0 END), 0)
                                                                      AS supports_edge_count,
    COALESCE(SUM(CASE WHEN source = 'nli'             THEN 1 ELSE 0 END), 0)
                                                                      AS inferred_edge_count
FROM graph_edges
WHERE bootstrap_only = 0
```

### Query 2 (entries JOIN, write_pool_server)
```sql
SELECT
    COUNT(DISTINCT e.id)                                              AS active_entry_count,
    (SELECT COUNT(*) FROM (
        SELECT source_id AS id FROM graph_edges WHERE bootstrap_only = 0
        UNION
        SELECT target_id AS id FROM graph_edges WHERE bootstrap_only = 0
    ) AS connected_ids
    JOIN entries ce ON ce.id = connected_ids.id AND ce.status = 0)    AS connected_entry_count,
    COALESCE(SUM(
        CASE WHEN ge.id IS NOT NULL
             AND src_e.category IS NOT NULL
             AND tgt_e.category IS NOT NULL
             AND src_e.category != tgt_e.category
        THEN 1 ELSE 0 END
    ), 0)                                                             AS cross_category_edge_count
FROM entries e
LEFT JOIN graph_edges ge
       ON ge.bootstrap_only = 0
      AND (ge.source_id = e.id OR ge.target_id = e.id)
LEFT JOIN entries src_e
       ON src_e.id = ge.source_id AND src_e.status = 0
LEFT JOIN entries tgt_e
       ON tgt_e.id = ge.target_id AND tgt_e.status = 0
WHERE e.status = 0
```

### Rust-Level Derivations
```rust
let connectivity_rate = if active == 0 { 0.0 } else { connected as f64 / active as f64 };
let isolated = active - connected;
let mean_degree = if active == 0 { 0.0 } else { (2 * total_edges) as f64 / active as f64 };
```

## Open Questions

1. **EDGE_SOURCE_NLI adoption in GH #412**: The constant is defined by col-029. Whether the six existing bare `"nli"` literals in `nli_detection.rs` are replaced in #412 or in a follow-up is a delivery coordination question. Recommend the implementer open a task against #412.

2. **Connected-entry sub-query vs Rust HashSet**: The UNION scalar sub-query approach is cleaner SQL. As an alternative, the implementer may collect source_id and target_id from Query 1's result set into a HashSet and count intersecting active entries. Both produce identical results within the 50-line budget.
