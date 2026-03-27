# col-029: Graph Cohesion Metrics — Pseudocode Overview

GH Issue: #413

---

## Components Involved

| Component | File | Role |
|-----------|------|------|
| `store-cohesion-query` | `crates/unimatrix-store/src/read.rs` + `lib.rs` | SQL execution, typed result struct, named constant |
| `status-report-fields` | `crates/unimatrix-server/src/mcp/response/status.rs` | Struct fields + hand-written Default additions |
| `service-call-site` | `crates/unimatrix-server/src/services/status.rs` | Phase 5 call, non-fatal error handling, field assignment |
| `format-output` | `crates/unimatrix-server/src/mcp/response/status.rs` | Summary line + Markdown sub-section |

---

## Data Flow

```
context_status MCP call
    |
    v
StatusService::compute_report()         [services/status.rs]
    |
    +-- Phase 1–4: existing (unchanged)
    |
    +-- Phase 5: Coherence dimensions
    |       |
    |       +-- existing: graph_quality_score, graph_stale_ratio
    |       |
    |       +-- NEW: store.compute_graph_cohesion_metrics().await
    |               |
    |               +-- SQL Query 1: graph_edges aggregate (no JOIN)
    |               |       -> total_edges: i64
    |               |       -> supports_edge_count: i64
    |               |       -> inferred_edge_count: i64
    |               |
    |               +-- SQL Query 2: entries LEFT JOIN graph_edges
    |                       -> active_entry_count: i64
    |                       -> connected_entry_count: i64 (UNION sub-query)
    |                       -> cross_category_edge_count: i64
    |               |
    |               v
    |           GraphCohesionMetrics (Rust derivation):
    |               connectivity_rate    = connected / active  (0.0 if active=0)
    |               isolated_entry_count = active - connected  (saturating_sub)
    |               mean_entry_degree    = 2*total / active    (0.0 if active=0)
    |
    +-- Ok(gcm) => assign six fields on StatusReport
    +-- Err(e)  => tracing::warn!, leave fields at default 0
    |
    v
format_status_report(report, format)    [mcp/response/status.rs]
    |
    +-- Summary path: conditional graph cohesion one-liner
    +-- Markdown path: "#### Graph Cohesion" sub-section inside "### Coherence"
```

---

## Shared Types Introduced or Modified

### New — `unimatrix-store/src/read.rs`

```
pub const EDGE_SOURCE_NLI: &str = "nli"

pub struct GraphCohesionMetrics {
    pub connectivity_rate: f64,
    pub isolated_entry_count: u64,
    pub cross_category_edge_count: u64,
    pub supports_edge_count: u64,
    pub mean_entry_degree: f64,
    pub inferred_edge_count: u64,
}
```

Re-exported from `unimatrix-store/src/lib.rs`:
```
pub use read::{..., GraphCohesionMetrics, EDGE_SOURCE_NLI};
```

### Modified — `StatusReport` struct (`unimatrix-server/src/mcp/response/status.rs`)

Six fields appended after `graph_compacted: bool`:
```
pub graph_connectivity_rate: f64,
pub isolated_entry_count: u64,
pub cross_category_edge_count: u64,
pub supports_edge_count: u64,
pub mean_entry_degree: f64,
pub inferred_edge_count: u64,
```

All six also appear in the hand-written `StatusReport::default()` block (missing any
field is a compile error — R-04).

---

## Sequencing Constraints

1. `store-cohesion-query` must be implemented first — it defines `GraphCohesionMetrics`
   which `service-call-site` assigns from and `status-report-fields` stores into.
2. `status-report-fields` must be complete before `service-call-site` compiles — the
   six `StatusReport` fields must exist for the assignment block to type-check.
3. `format-output` depends only on the six `StatusReport` fields being present;
   it can be written in parallel with `service-call-site` once `status-report-fields`
   is done.
4. Unit tests in `store-cohesion-query` are self-contained in `read.rs` and have
   no ordering constraint relative to the other three components.

---

## Key Constraints (cross-cutting)

- `read_pool()` for ALL SQL in `compute_graph_cohesion_metrics()` — ADR-003.
  Never `write_pool_server()`.
- `bootstrap_only = 0` filter mandatory on all six metrics.
- Active-only entry join: `entries.status = 0` everywhere.
- R-01 critical: `connected_entry_count` uses UNION sub-query to avoid double-count
  (an entry appearing as both source_id and target_id must count once, not twice).
- R-02: cross_category CASE guard checks `ge.id IS NOT NULL AND src_e.category IS NOT
  NULL AND tgt_e.category IS NOT NULL` before the inequality comparison.
- Division guards required for `connectivity_rate` and `mean_entry_degree` when
  `active_entry_count = 0`.
- `compute_graph_cohesion_metrics()` has exactly one call site: Phase 5 of
  `compute_report()`. Never in the maintenance tick (NFR-01, AC-15).
- `read.rs` is already 1570 lines. The new struct + constant + function must stay
  within 50 additional lines. Splitting to `read_graph.rs` is out of scope but should
  be noted as a housekeeping concern in a comment.
