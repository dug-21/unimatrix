# store-cohesion-query — Pseudocode

Component: `Store::compute_graph_cohesion_metrics`
File: `crates/unimatrix-store/src/read.rs` + `crates/unimatrix-store/src/lib.rs`

---

## Purpose

Introduce the `EDGE_SOURCE_NLI` named constant, the `GraphCohesionMetrics` output
struct, and the `compute_graph_cohesion_metrics()` async method on `Store`. Execute
two SQL queries against `read_pool()` (ADR-003) and derive the three Rust-computed
fields. Add seven unit tests in the existing `#[cfg(test)]` block.

The constant, struct, and function are placed near the existing `GraphEdgeRow` and
`ContradictEdgeRow` type definitions (~line 1379) and the `StatusAggregates` struct
(~line 1418). This is the natural grouping for graph-adjacent output types.

---

## New Constant

```
// Placed near GraphEdgeRow definition (~line 1379 in read.rs)
// ADR-001 col-029: named constant prevents silent divergence with nli_detection.rs.
// NOTE: read.rs is already 1570 lines (exceeds 500-line housekeeping rule). Splitting
// to read_graph.rs is out of scope for col-029 but should be addressed in a future cycle.
pub const EDGE_SOURCE_NLI: &str = "nli"
```

---

## New Struct

```
// Placed after StatusAggregates definition (~line 1424 in read.rs)
/// Six graph topology metrics derived from GRAPH_EDGES joined to entries (col-029).
/// All metrics exclude bootstrap_only=1 edges. All entry joins restrict to status=0.
pub struct GraphCohesionMetrics {
    /// Fraction of active entries with at least one non-bootstrap edge. Range [0.0, 1.0].
    pub connectivity_rate: f64,
    /// Active entries with zero non-bootstrap edges on either endpoint.
    pub isolated_entry_count: u64,
    /// Non-bootstrap edges where both active endpoints have different category values.
    pub cross_category_edge_count: u64,
    /// Non-bootstrap edges with relation_type = 'Supports'.
    pub supports_edge_count: u64,
    /// Average in+out degree: (2 * non_bootstrap_edge_count) / active_entry_count.
    /// 0.0 when active_entry_count = 0.
    pub mean_entry_degree: f64,
    /// Non-bootstrap edges with source = EDGE_SOURCE_NLI ('nli').
    pub inferred_edge_count: u64,
}
```

---

## New Function

```
impl Store {
    /// Compute six graph cohesion metrics from GRAPH_EDGES and entries.
    ///
    /// Uses read_pool() — consistent with compute_status_aggregates() (ADR-003 col-029).
    /// WAL snapshot semantics are intentional: bounded staleness is acceptable for this
    /// diagnostic aggregate. Routing through write_pool_server() would contend with NLI
    /// inference writes on its single-connection serialization point.
    ///
    /// Called only from StatusService::compute_report() Phase 5. Must NOT be called
    /// from the background maintenance tick (NFR-01 col-029).
    pub async fn compute_graph_cohesion_metrics(&self) -> Result<GraphCohesionMetrics> {

        // --- Query 1: pure graph_edges aggregates (no JOIN) ---
        // Counts total non-bootstrap edges, Supports edges, and NLI-inferred edges.
        // Single-row result; COUNT(*) never returns NULL, COALESCE guards the SUM columns.
        let sql_q1 =
            "SELECT \
                COUNT(*) AS total_edges, \
                COALESCE(SUM(CASE WHEN relation_type = 'Supports' THEN 1 ELSE 0 END), 0) \
                    AS supports_edge_count, \
                COALESCE(SUM(CASE WHEN source = 'nli' THEN 1 ELSE 0 END), 0) \
                    AS inferred_edge_count \
             FROM graph_edges \
             WHERE bootstrap_only = 0"

        let row1 = sqlx::query(sql_q1)
            .fetch_one(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?

        let total_edges: i64      = row1.try_get(0).map_err(|e| StoreError::Database(e.into()))?
        let supports_count: i64   = row1.try_get(1).map_err(|e| StoreError::Database(e.into()))?
        let inferred_count: i64   = row1.try_get(2).map_err(|e| StoreError::Database(e.into()))?

        // --- Query 2: entries LEFT JOIN graph_edges for connectivity + cross-category ---
        //
        // Outer FROM is `entries WHERE status = 0` — active_entry_count is the denominator.
        //
        // connected_entry_count uses a UNION scalar sub-query (ADR-002) to avoid
        // double-counting entries that appear as both source_id and target_id in
        // different edges (R-01 critical risk). The UNION deduplicates the ID set before
        // counting, then JOINs to entries to enforce the active-only filter.
        //
        // cross_category_edge_count uses explicit LEFT JOINs keyed on indexed columns
        // (idx_graph_edges_source_id, idx_graph_edges_target_id) — ADR-004. The CASE
        // guard checks ge.id IS NOT NULL, src_e.category IS NOT NULL, and
        // tgt_e.category IS NOT NULL before the inequality comparison to prevent
        // NULL != NULL evaluating to NULL (R-02 high risk).
        let sql_q2 =
            "SELECT \
                COUNT(DISTINCT e.id) AS active_entry_count, \
                ( \
                    SELECT COUNT(*) FROM ( \
                        SELECT source_id AS id FROM graph_edges WHERE bootstrap_only = 0 \
                        UNION \
                        SELECT target_id AS id FROM graph_edges WHERE bootstrap_only = 0 \
                    ) AS connected_ids \
                    JOIN entries ce ON ce.id = connected_ids.id AND ce.status = 0 \
                ) AS connected_entry_count, \
                COALESCE(SUM( \
                    CASE WHEN ge.id IS NOT NULL \
                         AND src_e.category IS NOT NULL \
                         AND tgt_e.category IS NOT NULL \
                         AND src_e.category != tgt_e.category \
                    THEN 1 ELSE 0 END \
                ), 0) AS cross_category_edge_count \
             FROM entries e \
             LEFT JOIN graph_edges ge \
                    ON ge.bootstrap_only = 0 \
                   AND (ge.source_id = e.id OR ge.target_id = e.id) \
             LEFT JOIN entries src_e ON src_e.id = ge.source_id AND src_e.status = 0 \
             LEFT JOIN entries tgt_e ON tgt_e.id = ge.target_id AND tgt_e.status = 0 \
             WHERE e.status = 0"

        let row2 = sqlx::query(sql_q2)
            .fetch_one(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?

        let active: i64    = row2.try_get(0).map_err(|e| StoreError::Database(e.into()))?
        let connected: i64 = row2.try_get(1).map_err(|e| StoreError::Database(e.into()))?
        let cross_cat: i64 = row2.try_get(2).map_err(|e| StoreError::Database(e.into()))?

        // --- Rust-side derivation (R-05: division guards required) ---
        let connectivity_rate = if active > 0 {
            connected as f64 / active as f64
        } else {
            0.0
        }

        let mean_entry_degree = if active > 0 {
            (2.0 * total_edges as f64) / active as f64
        } else {
            0.0
        }

        // saturating_sub prevents u64 underflow if connected somehow exceeds active
        // (defensive; the UNION approach ensures connected <= active)
        let isolated = (active as u64).saturating_sub(connected as u64)

        Ok(GraphCohesionMetrics {
            connectivity_rate,
            isolated_entry_count: isolated,
            cross_category_edge_count: cross_cat as u64,
            supports_edge_count: supports_count as u64,
            mean_entry_degree,
            inferred_edge_count: inferred_count as u64,
        })
    }
}
```

---

## lib.rs Re-export

```
// crates/unimatrix-store/src/lib.rs
// Extend the existing pub use read::{...} line:
pub use read::{
    ContradictEdgeRow,
    GraphCohesionMetrics,   // NEW col-029
    GraphEdgeRow,
    StatusAggregates,
    EDGE_SOURCE_NLI,        // NEW col-029
    // ... other existing exports unchanged
}
```

---

## Error Handling

| Error Path | Behavior |
|------------|----------|
| `fetch_one` returns `sqlx::Error` on either query | Propagate as `StoreError::Database(e.into())`. The caller (`compute_report` Phase 5) handles this with `tracing::warn!` + skip — the error does not reach the MCP caller. |
| `try_get` column index mismatch | Same propagation path. Column order in SQL matches index used in `try_get` calls — verified by query structure, not by name. |
| `active = 0` (empty store or all-deprecated) | Not an error; division guard returns `0.0` for `connectivity_rate` and `mean_entry_degree`. `isolated_entry_count = 0`. All counts zero. |
| `connected > active` (should not occur with UNION) | `saturating_sub` returns 0 rather than wrapping to a large u64. |

---

## Unit Tests (in existing `#[cfg(test)]` block in read.rs)

All tests use `open_test_store()` and the existing `create_graph_edges_table()` helper.
Test function names are prefixed `test_graph_cohesion_` per AC-13.

### Test 1 — `test_graph_cohesion_all_isolated`

```
Setup:
  - open_test_store()
  - Insert 3 active entries (status=0, distinct categories)
  - No graph_edges rows

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - connectivity_rate == 0.0
  - isolated_entry_count == 3
  - cross_category_edge_count == 0
  - supports_edge_count == 0
  - mean_entry_degree == 0.0   (R-05: must be 0.0, not NaN)
  - inferred_edge_count == 0
```

### Test 2 — `test_graph_cohesion_all_connected`

```
Setup:
  - 3 active entries: A, B, C
  - Edges: A->B (bootstrap_only=0), B->C (bootstrap_only=0)
  - B appears as both source_id and target_id (R-01 chain topology)

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - connectivity_rate == 1.0   (all 3 entries connected; UNION dedup prevents 4/3)
  - isolated_entry_count == 0
  - mean_entry_degree == (2*2)/3 == 1.333... (approximately)
  - graph_connectivity_rate <= 1.0  (explicit bounds check, R-01)
```

### Test 3 — `test_graph_cohesion_mixed_connectivity`

```
Setup:
  - 4 active entries: A, B, C, D
  - Edges: A->B (bootstrap_only=0)
  - C and D have no edges

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - connectivity_rate == 0.5   (2 connected / 4 active)
  - isolated_entry_count == 2
  - mean_entry_degree == (2*1)/4 == 0.5
```

### Test 4 — `test_graph_cohesion_cross_category`

```
Setup:
  - 3 active entries: A (category="decision"), B (category="pattern"), C (category="decision")
  - 1 deprecated entry: D (category="pattern", status=1)
  - Edges:
      A->B (bootstrap_only=0) — cross-category (decision vs pattern), both active
      A->C (bootstrap_only=0) — same-category (decision vs decision)
      A->D (bootstrap_only=0) — A active, D deprecated (should NOT count as cross-category)

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - cross_category_edge_count == 1  (only A->B; A->C same category; A->D deprecated endpoint)
  - connectivity_rate == 1.0  (A, B, C all reachable via non-bootstrap edges)
  - isolated_entry_count == 0
  -- Note: D is not an active entry; does not affect active_entry_count denominator
```

### Test 5 — `test_graph_cohesion_same_category_only`

```
Setup:
  - 2 active entries: A (category="decision"), B (category="decision")
  - Edge: A->B (bootstrap_only=0, relation_type='Supports')

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - cross_category_edge_count == 0
  - supports_edge_count == 1
  - connectivity_rate == 1.0
```

### Test 6 — `test_graph_cohesion_nli_source`

```
Setup:
  - 2 active entries: A, B
  - Edges:
      A->B (source='nli', bootstrap_only=0)    -- should count
      A->B (source='manual', bootstrap_only=0) -- should NOT count in inferred

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - inferred_edge_count == 1   (only the 'nli' source row)
```

### Test 7 — `test_graph_cohesion_bootstrap_excluded`

```
Setup:
  - 2 active entries: A, B
  - Edges:
      A->B (source='nli',    bootstrap_only=1)  -- bootstrap NLI edge
      A->B (source='manual', bootstrap_only=1)  -- bootstrap manual edge

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - inferred_edge_count == 0    (R-03: bootstrap NLI edge excluded, AC-16)
  - supports_edge_count == 0
  - connectivity_rate == 0.0    (no non-bootstrap edges)
  - isolated_entry_count == 2   (both entries isolated from non-bootstrap perspective)
  - mean_entry_degree == 0.0
```

### Additional edge-case test — `test_graph_cohesion_empty_store`

```
Setup:
  - open_test_store() with no entries and no edges (R-05 denominator=0 case)

Call: store.compute_graph_cohesion_metrics().await

Assert:
  - connectivity_rate == 0.0   (not NaN, not inf)
  - mean_entry_degree == 0.0   (not NaN, not inf)
  - isolated_entry_count == 0
  - all other fields == 0
```

---

## Key Test Scenarios Summary

| Risk | Test | Assertion |
|------|------|-----------|
| R-01 double-count | test_2 (chain A->B->C) | connectivity_rate == 1.0, not > 1.0 |
| R-02 NULL guard | test_4 (deprecated endpoint) | cross_category_edge_count == 1, not 2 |
| R-03 bootstrap NLI leak | test_7 | inferred_edge_count == 0 |
| R-05 division by zero | test_1 + empty_store | mean_entry_degree == 0.0 |
| R-08 HashSet logic | test_4 (active-only) | deprecated endpoint excluded from connected count |

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for `graph cohesion SQL aggregates store layer patterns` — found #726 (SQL Aggregation Struct pattern), #1588 (Active-only query gotcha). Both directly applied.
- Deviations from established patterns: none. Follows `compute_status_aggregates` structure exactly: two `fetch_one` calls on `read_pool()`, `try_get` with column index, `StoreError::Database` wrapping, typed output struct.
