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
| Alignment Report | product/features/col-029/ALIGNMENT-REPORT.md |

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
| Which pool to use — `read_pool()` vs. `write_pool_server()` | `read_pool()` for both queries. `compute_status_aggregates` (the direct precedent) uses `read_pool()` at lines 959 and 983 of `read.rs`. Cohesion queries are read-only aggregates that require no write serialisation; `read_pool()` is the correct pool for read-only queries. Note: earlier design drafts (SCOPE.md background, ARCHITECTURE.md) incorrectly stated `write_pool_server()` — ADR-003 documents the correction. | ADR-003 | `architecture/ADR-003-write-pool-server-for-cohesion-queries.md` |
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

Both queries execute against `read_pool()` (ADR-003).

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
- **`read_pool()` for all SQL in `compute_graph_cohesion_metrics()`**: consistent with `compute_status_aggregates` (the direct precedent), which uses `read_pool()` at lines 959 and 983 of `read.rs`. Cohesion queries are read-only aggregates and do not require write-pool serialisation (ADR-003).
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
| `read_pool()` | Pool handle (existing) | In `unimatrix-store/src/db.rs`; used by `compute_status_aggregates` precedent |
| `open_test_store()` | Test helper (existing) | Used by all new unit tests |
| `create_graph_edges_table()` | Test helper (existing, `read.rs` `#[cfg(test)]`) | Used by unit tests to populate edges |
| `compute_status_aggregates` | Pattern reference | New function follows the same structure; confirmed to use `read_pool()` |
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

ALIGNMENT-REPORT.md was produced and reviewed on 2026-03-26. Overall status: **PASS with two WARNs**.

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly enables W3-1 `graph_degree` feature vector and W1-4 NLI observability |
| Milestone Fit | PASS | Wave 1 / Wave 1A support work; no future-milestone capabilities built |
| Scope Gaps | WARN | Spec FR-11 error propagation diverges from architecture's non-fatal pattern |
| Scope Additions | WARN | `EDGE_SOURCE_NLI` constant and `lib.rs` re-export not in SCOPE.md, but low-risk and well-justified |
| Architecture Consistency | PASS | All four layers internally consistent |
| Risk Completeness | PASS | 10 risks, 27 scenarios; all critical paths covered |

### WARN 1 — FR-11 Error Propagation Model (Resolved: use architecture's non-fatal pattern)

SPECIFICATION FR-11 specifies fatal error propagation (`ServiceError::Core(CoreError::Store(e))`),
while the architecture (Layer 3), risk-test strategy (R-07 failure modes), and SCOPE.md all
describe a non-fatal pattern (`tracing::warn!` + skip with cohesion fields at zero). Three of
four documents align on non-fatal. The implementation must follow the non-fatal architecture
model. The service call site in the Function Signatures section above reflects this resolution.

### WARN 2 — EDGE_SOURCE_NLI Scope Addition (Accepted)

The `EDGE_SOURCE_NLI` constant and `lib.rs` re-export are not listed in SCOPE.md but are
required by ADR-001 to resolve SR-01 (string coupling with GH #412). This addition is
non-breaking, well-documented, and accepted. It is included in the Files to Create / Modify
table above.

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
| R-11 — WAL staleness with read_pool() | Low/Medium | Under SQLite WAL mode, `read_pool()` may return a snapshot that does not include the most recent NLI inference writes until the next WAL checkpoint. Risk is Low severity given that `context_status` is a diagnostic tool where a slightly stale read (seconds to minutes) is acceptable; operators re-invoke after a checkpoint if precision is needed. No test required — accepted trade-off documented in ADR-003. |
