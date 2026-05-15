# Component 5: stale_dependency_edges (read.rs)

## Purpose

Add a `stale_dependency_edges: u64` field to `GraphCohesionMetrics` and compute it in
`compute_graph_cohesion_metrics()` via one additional SQL JOIN query against `read_pool()`.

This count surfaces deprecated-source Prerequisite edges — signaling that declared dependencies
may need review. The value flows through to `context_status` output and feeds the
`DependencyOnDeprecated` detection rule via `context_cycle_review`.

## File

`crates/unimatrix-store/src/read.rs` (modified)

## Struct Modification: GraphCohesionMetrics

```
// Current struct (before vnc-015):
pub struct GraphCohesionMetrics {
    // ... existing fields ...
}

// After vnc-015 — add field:
pub struct GraphCohesionMetrics {
    // ... existing fields unchanged ...
    pub stale_dependency_edges: u64,
    // Count of GRAPH_EDGES rows where:
    //   - relation_type = 'Prerequisite'
    //   - source entry has status = 1 (Deprecated)
    // Zero when no stale edges exist — non-nullable
}
```

## SQL Query

```sql
-- Named: stale_dependency_count_query
-- Follows existing JOIN-to-entries style in compute_graph_cohesion_metrics()
SELECT COUNT(*) AS stale_count
FROM graph_edges ge
JOIN entries e ON e.id = ge.source_id
WHERE ge.relation_type = 'Prerequisite'
  AND e.status = 1
```

Explanation:
- `relation_type = 'Prerequisite'` — hardcoded string literal (NOT a format-string interpolation;
  SQL injection is not possible here and must not be introduced by refactoring — RISK-TEST-STRATEGY.md Security section)
- `e.status = 1` — integer literal for Deprecated status (0=Active, 1=Deprecated, 2=Quarantined)
- Intentionally Prerequisite-only: `Advances`, `Motivates`, and other new variants are not
  dependency-assertion edges; stale detection for them is deferred to Phase 2

## Function Modification: compute_graph_cohesion_metrics

```
ASYNC FUNCTION compute_graph_cohesion_metrics(store: &Store) -> Result<GraphCohesionMetrics, StoreError>
    LET pool = store.read_pool()

    // ... existing queries (unchanged) ...

    // [NEW] Stale dependency edges query
    LET stale_count_row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM graph_edges ge
         JOIN entries e ON e.id = ge.source_id
         WHERE ge.relation_type = 'Prerequisite'
           AND e.status = 1"
    )
    .fetch_one(pool)
    .await?

    LET stale_dependency_edges: u64 = stale_count_row.0.max(0) as u64
    // COUNT(*) is non-negative; the max(0) cast is defensive

    RETURN Ok(GraphCohesionMetrics {
        // ... existing fields populated as before ...
        stale_dependency_edges,  // new field
    })
END FUNCTION
```

## stale_dependency_edges for DependencyOnDeprecated Rule

The `context_cycle_review` handler needs a DIFFERENT query — per-cycle scoped Prerequisite
edges, not the global count. The `stale_dependency_edges` count in `GraphCohesionMetrics`
is the global metric (used by `context_status`). The detection rule needs pairs.

```
// In context_cycle_review handler (tools.rs) — pre-query stale pairs for this cycle
// This is a separate query from compute_graph_cohesion_metrics

ASYNC FUNCTION query_stale_prerequisite_edges_for_cycle(
    store:         &Store,
    feature_cycle: &str,
) -> Result<Vec<(u64, u64)>, StoreError>
    // Find all GRAPH_EDGES rows where:
    //   - relation_type = 'Prerequisite'
    //   - source entry is in the current feature_cycle
    //   - source entry has status = Deprecated
    // Returns (source_id, target_id) pairs

    LET rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT ge.source_id, ge.target_id
         FROM graph_edges ge
         JOIN entries e ON e.id = ge.source_id
         JOIN feature_entries fe ON fe.entry_id = ge.source_id
         WHERE ge.relation_type = 'Prerequisite'
           AND e.status = 1
           AND fe.feature_cycle = ?1"
    )
    .bind(feature_cycle)
    .fetch_all(store.read_pool())
    .await?

    LET pairs: Vec<(u64, u64)> = rows
        .into_iter()
        .map(|(src, tgt)| (src as u64, tgt as u64))
        .collect()

    RETURN Ok(pairs)
END FUNCTION
```

Note: This per-cycle query is defined in `read.rs` and called from `context_cycle_review`
in `tools.rs`. The function is a new addition to `read.rs` for the detection rule injection.
If `feature_entries` is the join table linking entries to cycles, verify the exact table name
and column names match the current schema.

## Integration Points

- `GraphCohesionMetrics.stale_dependency_edges` is consumed by the `context_status` handler
  in `tools.rs` — the field must be serialized into the status response JSON.
- `query_stale_prerequisite_edges_for_cycle` is consumed by `context_cycle_review` in `tools.rs`
  to produce the `Vec<(u64, u64)>` passed to `default_rules()`.

## Error Handling

| Error | Source | Behavior |
|-------|--------|----------|
| `StoreError` from COUNT query | pool connectivity | Propagates from `compute_graph_cohesion_metrics` — context_status returns error |
| `StoreError` from cycle query | pool connectivity | Propagates from `context_cycle_review` — review returns error |

## Key Test Scenarios

1. Zero stale edges → `stale_dependency_edges = 0` in context_status output
2. Write Prerequisite edge A→B; deprecate A; call context_status → `stale_dependency_edges >= 1` (AC-11)
3. Write Prerequisite edge but DO NOT deprecate source → count unchanged (Active source not counted)
4. Deprecate source entry without a Prerequisite edge → count unchanged (only Prerequisite counts)
5. SQL correctness: status literal = 1 (not 0 or 2) — catches R-14 risk
6. Per-cycle query: only returns pairs where source is in the given feature_cycle, not globally
7. `query_stale_prerequisite_edges_for_cycle(cycle)` with no stale edges → returns empty vec
   (DependencyOnDeprecated must not fire false-positive when vec is empty)
