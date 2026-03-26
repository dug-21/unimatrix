# col-029: Graph Cohesion Metrics in context_status — Implementation Brief

GH Issue: #413

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-029/SCOPE.md |
| Architecture | product/features/col-029/architecture/ARCHITECTURE.md |
| Specification | product/features/col-029/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/col-029/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-029/ALIGNMENT-REPORT.md _(pending — not yet produced)_ |

---

## Goal

Add six graph cohesion metrics to `StatusReport` so operators can observe whether
automated NLI-based edge inference (GH #412) is producing a connected, cross-category
graph that PPR can exploit. Metrics are computed per-call via two SQL queries over
`GRAPH_EDGES` joined to `entries`; no schema migration, no lambda change, no
background-tick caching.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| `Store::compute_graph_cohesion_metrics` | pseudocode/store-cohesion-query.md | test-plan/store-cohesion-query.md |
| `StatusReport` six new fields | pseudocode/status-report-fields.md | test-plan/status-report-fields.md |
| `StatusService::compute_report` Phase 5 call site | pseudocode/service-call-site.md | test-plan/service-call-site.md |
| `format_status_report` Summary + Markdown output | pseudocode/format-output.md | test-plan/format-output.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Where to define the `"nli"` source constant to prevent SR-01 string divergence | `pub const EDGE_SOURCE_NLI: &str = "nli"` in `unimatrix-store/src/read.rs`, re-exported from `lib.rs`; bare literals in `nli_detection.rs` are candidates for follow-up migration | ADR-001 | `architecture/ADR-001-edge-source-nli-constant.md` |
| One query vs. two queries vs. six queries for the six metrics | Two SQL queries: Query 1 is pure GRAPH_EDGES aggregates (no JOIN); Query 2 joins `entries` for connectivity and cross-category. `mean_entry_degree`, `connectivity_rate`, and `isolated_entry_count` are derived in Rust from those two query results | ADR-002 | `architecture/ADR-002-two-sql-queries.md` |
| Which pool to use — `read_pool()` vs. `write_pool_server()` | `write_pool_server()` for both queries, matching all other GRAPH_EDGES readers (`query_bootstrap_contradicts`, `find_nli_count_for_circuit_breaker`). Note: SCOPE.md incorrectly cited `compute_status_aggregates` as using `write_pool_server()`; it uses `read_pool()`. Cohesion uses write pool because GRAPH_EDGES is write-active during NLI inference and freshness matters | ADR-003 | `architecture/ADR-003-write-pool-server-for-cohesion-queries.md` |
| How to count cross-category edges without a cartesian product (SR-04) | Query 2 uses explicit `LEFT JOIN entries src_e ON src_e.id = ge.source_id AND src_e.status = 0` and `LEFT JOIN entries tgt_e ON tgt_e.id = ge.target_id AND tgt_e.status = 0` with a four-condition CASE guard (`ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL AND src_e.category != tgt_e.category`) | ADR-004 | `architecture/ADR-004-cross-category-sql-no-cartesian-product.md` |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/read.rs` | Modify | Add `EDGE_SOURCE_NLI` constant, `GraphCohesionMetrics` struct, and `compute_graph_cohesion_metrics()` async fn; add seven unit tests in existing `#[cfg(test)]` block |
| `crates/unimatrix-store/src/lib.rs` | Modify | Re-export `EDGE_SOURCE_NLI` and `GraphCohesionMetrics` |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Modify | Append six fields to `StatusReport` struct and `StatusReport::default()`; add one-line Summary graph cohesion output; add `#### Graph Cohesion` Markdown sub-section inside the Coherence block |
| `crates/unimatrix-server/src/services/status.rs` | Modify | Call `self.store.compute_graph_cohesion_metrics().await` in `compute_report()` Phase 5, after HNSW stale ratio, with non-fatal error handling (`warn + skip`) |

---

## Data Structures

### `GraphCohesionMetrics` (new — `unimatrix-store/src/read.rs`)

```rust
pub const EDGE_SOURCE_NLI: &str = "nli";

pub struct GraphCohesionMetrics {
    pub connectivity_rate: f64,          // connected_active / total_active; 0.0 if total=0
    pub isolated_entry_count: u64,       // total_active - connected_active
    pub cross_category_edge_count: u64,  // edges with active endpoints of different category
    pub supports_edge_count: u64,        // edges with relation_type='Supports', bootstrap_only=0
    pub mean_entry_degree: f64,          // (2 * non_bootstrap_edges) / total_active; 0.0 if total=0
    pub inferred_edge_count: u64,        // edges with source=EDGE_SOURCE_NLI, bootstrap_only=0
}
```

### `StatusReport` additions (existing struct — `unimatrix-server/src/mcp/response/status.rs`)

Six fields appended after `graph_compacted: bool`:

```rust
pub graph_connectivity_rate: f64,   // default 0.0
pub isolated_entry_count: u64,      // default 0
pub cross_category_edge_count: u64, // default 0
pub supports_edge_count: u64,       // default 0
pub mean_entry_degree: f64,         // default 0.0
pub inferred_edge_count: u64,       // default 0
```

All six must also appear in `StatusReport::default()` (struct has a hand-written `Default` impl — omitting any field is a compile error, see R-04).

---

## Function Signatures

### Store layer (new)

```rust
// crates/unimatrix-store/src/read.rs
pub const EDGE_SOURCE_NLI: &str = "nli";

pub struct GraphCohesionMetrics { /* six fields above */ }

impl Store {
    pub async fn compute_graph_cohesion_metrics(&self) -> Result<GraphCohesionMetrics>;
}
```

Internal SQL — Query 1 (pure `graph_edges`, `bootstrap_only = 0`):
```sql
SELECT
    COUNT(*)                                                              AS total_edges,
    COALESCE(SUM(CASE WHEN relation_type = 'Supports' THEN 1 ELSE 0 END), 0)
                                                                          AS supports_edge_count,
    COALESCE(SUM(CASE WHEN source = 'nli'             THEN 1 ELSE 0 END), 0)
                                                                          AS inferred_edge_count
FROM graph_edges
WHERE bootstrap_only = 0
```

Internal SQL — Query 2 (entries JOIN, `status = 0`):
```sql
SELECT
    COUNT(DISTINCT e.id)  AS active_entry_count,
    (SELECT COUNT(*) FROM (
         SELECT source_id AS id FROM graph_edges WHERE bootstrap_only = 0
         UNION
         SELECT target_id AS id FROM graph_edges WHERE bootstrap_only = 0
     ) AS connected_ids
     JOIN entries ce ON ce.id = connected_ids.id AND ce.status = 0
    )                     AS connected_entry_count,
    COALESCE(SUM(
        CASE WHEN ge.id IS NOT NULL
             AND src_e.category IS NOT NULL
             AND tgt_e.category IS NOT NULL
             AND src_e.category != tgt_e.category
        THEN 1 ELSE 0 END
    ), 0)                 AS cross_category_edge_count
FROM entries e
LEFT JOIN graph_edges ge
       ON ge.bootstrap_only = 0
      AND (ge.source_id = e.id OR ge.target_id = e.id)
LEFT JOIN entries src_e ON src_e.id = ge.source_id AND src_e.status = 0
LEFT JOIN entries tgt_e ON tgt_e.id = ge.target_id AND tgt_e.status = 0
WHERE e.status = 0
```

Rust-side derivation (division guards required):
```rust
let connectivity_rate = if active > 0 { connected as f64 / active as f64 } else { 0.0 };
let mean_entry_degree = if active > 0 { (2.0 * total_edges as f64) / active as f64 } else { 0.0 };
let isolated_entry_count = active.saturating_sub(connected);
```

### Service call site (Phase 5 addition — `services/status.rs`)

```rust
// Graph cohesion metrics (col-029)
match self.store.compute_graph_cohesion_metrics().await {
    Ok(gcm) => {
        report.graph_connectivity_rate    = gcm.connectivity_rate;
        report.isolated_entry_count       = gcm.isolated_entry_count;
        report.cross_category_edge_count  = gcm.cross_category_edge_count;
        report.supports_edge_count        = gcm.supports_edge_count;
        report.mean_entry_degree          = gcm.mean_entry_degree;
        report.inferred_edge_count        = gcm.inferred_edge_count;
    }
    Err(e) => tracing::warn!("graph cohesion metrics failed: {e}"),
}
```

### Format output additions (`mcp/response/status.rs`)

Summary (conditional on `isolated + cross_category + inferred > 0`):
```
Graph cohesion: {:.1}% connected, {} isolated, {} cross-category, {} inferred
```

Markdown (always present inside `### Coherence` block):
```markdown
#### Graph Cohesion
- Connectivity: {:.1}% ({}/{} active entries connected)
- Isolated entries: {}
- Cross-category edges: {}
- Supports edges: {}
- Mean entry degree: {:.2}
- Inferred (NLI) edges: {}
```

---

## Constraints

- **No schema migration**: all data exists in `GRAPH_EDGES` (v13) and `entries` (v17). No new columns, tables, or indexes.
- **`bootstrap_only = 0` filter mandatory on all six metrics**: matches `TypedRelationGraph.inner` semantics. Bootstrap-only edges are not real inference edges.
- **Active-only entry join (`status = 0`)**: deprecated (status=1) and quarantined (status=3) entries are invisible to PPR and must be invisible to cohesion metrics.
- **`write_pool_server()` for all SQL in `compute_graph_cohesion_metrics()`**: matches all other GRAPH_EDGES readers; ensures freshness against NLI write activity (ADR-003).
- **Two-query maximum**: Query 1 (pure GRAPH_EDGES), Query 2 (entries JOIN). A UNION scalar sub-query for `connected_entry_count` is embedded in Query 2 (ADR-002). Rust-side HashSet is an acceptable alternative.
- **Function + struct ≤ 50 lines** in `read.rs`. Note: `read.rs` is already 1570 lines (exceeds the 500-line housekeeping rule). Splitting is out of scope; annotate as a future housekeeping concern.
- **No new crate dependency**: uses only `sqlx` and scalar arithmetic already present in `unimatrix-store`.
- **No lambda / coherence scalar change**: `coherence` (lambda), `graph_quality_score`, and the four coherence dimension scores are untouched.
- **Not called from maintenance tick**: `compute_graph_cohesion_metrics()` must have exactly one call site — in `compute_report()` Phase 5. Never in `load_maintenance_snapshot()` or `maintenance_tick()` (NFR-01, AC-15).
- **Non-fatal error handling**: on `Err`, log `tracing::warn!` and leave all six cohesion fields at their default zero values. Do not propagate to the MCP caller.
- **`StatusReport::default()` is hand-written** — all six fields must be explicitly listed. Missing any field is a compile error.

---

## Dependencies

| Dependency | Kind | Notes |
|------------|------|-------|
| `sqlx` | Crate (existing) | All SQL queries use existing pool handles |
| `GRAPH_EDGES` table | Schema (existing, v13) | `bootstrap_only`, `source`, `relation_type` columns used |
| `entries` table | Schema (existing, v17) | Joined on `status = 0` and `category` |
| `write_pool_server()` | Pool handle (existing) | In `unimatrix-store/src/db.rs` |
| `open_test_store()` | Test helper (existing) | Used by all new unit tests |
| `create_graph_edges_table()` | Test helper (existing, `read.rs` `#[cfg(test)]`) | Used by unit tests to populate edges |
| `compute_status_aggregates` | Pattern reference | New function follows the same structure |
| GH #412 (NLI inference) | Integration dependency | Defines `source='nli'`; `EDGE_SOURCE_NLI` constant coordinates the string |

---

## NOT in Scope

- Changes to the lambda (coherence) scalar or its four dimension weights
- Replacement or modification of `graph_quality_score` / `graph_stale_ratio`
- Caching cohesion metrics in the background maintenance tick or `MaintenanceDataSnapshot`
- Per-category or per-relation-type breakdowns beyond the six specified metrics
- Distinguishing Supports vs. Contradicts among NLI-inferred edges in `inferred_edge_count`
- Alerting thresholds or automated maintenance triggered by cohesion metric values
- Changes to `TypedRelationGraph` in-memory structure or `build_typed_relation_graph`
- Migrating bare `"nli"` literals in `nli_detection.rs` to use `EDGE_SOURCE_NLI` (this is a GH #412 follow-up)
- Per-entry degree distribution (histogram); only the mean is exposed
- Integration tests beyond unit tests for `compute_graph_cohesion_metrics`

---

## Alignment Status

ALIGNMENT-REPORT.md was not produced by Session 1 (no vision guardian artifact found at
`product/features/col-029/ALIGNMENT-REPORT.md`). Alignment status is **pending**.

No variances have been flagged. The feature is narrowly scoped (read-only SQL, display-only
metrics, no lambda change, no schema migration) and poses low vision alignment risk. The
implementation brief proceeds on that basis; any variances identified in a subsequent
alignment pass should be reconciled before Stage 3b begins.

---

## Critical Risk Callouts for Implementer

These risks from RISK-TEST-STRATEGY.md require explicit test coverage:

| Risk | Priority | What to Watch For |
|------|----------|-------------------|
| R-01 — connected_entry_count double-count | Critical | `COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)` overcounts overlap; use UNION sub-query or Rust HashSet dedup. Test with chain topology (A→B→C where B is both source and target). Assert `connectivity_rate ≤ 1.0`. |
| R-02 — cross_category NULL guard | High | LEFT JOIN on deprecated endpoint produces NULL `src_e.category`; NULL != NULL evaluates to NULL (not FALSE), silently counting the edge. CASE guard must include `IS NOT NULL` checks on both category values. |
| R-03 — bootstrap_only=1 NLI edge leak | High | An edge with `source='nli'` AND `bootstrap_only=1` must not appear in `inferred_edge_count`. The `bootstrap_only = 0` filter in the WHERE clause is the guard. Test explicitly (AC-16). |
| R-05 — division by zero | High | `mean_entry_degree` and `connectivity_rate` must return `0.0` (not `NaN`/`inf`) when `active_entry_count = 0`. |
| R-04 — `StatusReport::default()` missing field | Medium | Hand-written `Default` impl; failing to add any of the six fields causes a compile error. Confirmed by `cargo check`. |
