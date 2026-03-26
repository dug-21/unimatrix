# col-029: Graph Cohesion Metrics in context_status — Architecture

GH Issue: #413

## System Overview

`context_status` is the primary health-reporting tool in the MCP server. It already
reports coherence (lambda), HNSW vector index staleness, co-access patterns, and
contradiction density. It has no visibility into `GRAPH_EDGES` topology — the table
that drives PPR (Personalized PageRank) and phase-conditioned retrieval.

This feature adds six GRAPH_EDGES-derived metrics to `StatusReport`, making the graph
inference work from #412 observable. The metrics answer three operational questions:

1. Are entries connected or isolated? (connectivity rate, isolated entry count)
2. Is the graph cross-category or homogeneous? (cross-category edge count)
3. How many edges are NLI-inferred vs. explicitly written? (inferred edge count,
   supports edge count, mean degree)

No schema changes. No lambda changes. Diagnostic only.

## Component Breakdown

### Layer 1 — Store Query Function (`unimatrix-store`)

**File**: `crates/unimatrix-store/src/read.rs`

New items:
- `pub const EDGE_SOURCE_NLI: &str = "nli"` — named constant shared with GH #412
  implementation to prevent silent string mismatch (addresses SR-01)
- `pub struct GraphCohesionMetrics` — typed output struct (six fields)
- `pub async fn Store::compute_graph_cohesion_metrics() -> Result<GraphCohesionMetrics>`
  — two SQL queries via `write_pool_server()`, following the `compute_status_aggregates`
  pattern

The constant is placed in `read.rs` alongside `ENTRY_COLUMNS` and near the graph row
types. It is re-exported from `lib.rs` so `nli_detection.rs` in the server crate can
import it instead of using a bare string literal.

Responsibilities:
- Execute SQL against live store state
- Return typed, validated aggregate values
- Apply `bootstrap_only = 0` filter to match TypedRelationGraph semantics
- Active-only join: `entries.status = 0`

### Layer 2 — Status Report Struct (`unimatrix-server`)

**File**: `crates/unimatrix-server/src/mcp/response/status.rs`

Six fields appended to `StatusReport`:
```rust
pub graph_connectivity_rate: f64,   // fraction of active entries with ≥1 edge
pub isolated_entry_count: u64,      // active entries with zero non-bootstrap edges
pub cross_category_edge_count: u64, // edges where src.category != tgt.category
pub supports_edge_count: u64,       // edges with relation_type = 'Supports'
pub mean_entry_degree: f64,         // (2 * edge_count) / active_entry_count
pub inferred_edge_count: u64,       // edges with source = EDGE_SOURCE_NLI
```

Default values in `StatusReport::default()`:
- `graph_connectivity_rate`: `0.0`
- `isolated_entry_count`: `0`
- `cross_category_edge_count`: `0`
- `supports_edge_count`: `0`
- `mean_entry_degree`: `0.0`
- `inferred_edge_count`: `0`

Format additions:
- **Summary**: one line appended after the existing graph stale ratio line, present
  only when `isolated_entry_count + cross_category_edge_count + inferred_edge_count > 0`
  (avoids noise on empty stores)
- **Markdown**: new `#### Graph Cohesion` sub-section inside the existing
  `### Coherence` block, after the HNSW graph stale ratio lines

### Layer 3 — Status Service (`unimatrix-server`)

**File**: `crates/unimatrix-server/src/services/status.rs`

Call site in `compute_report()` Phase 5, after the HNSW stale ratio is assigned
(line ~669) and before the lambda computation:

```rust
// Graph cohesion metrics (col-029)
match self.store.compute_graph_cohesion_metrics().await {
    Ok(gcm) => {
        report.graph_connectivity_rate = gcm.connectivity_rate;
        report.isolated_entry_count = gcm.isolated_entry_count;
        report.cross_category_edge_count = gcm.cross_category_edge_count;
        report.supports_edge_count = gcm.supports_edge_count;
        report.mean_entry_degree = gcm.mean_entry_degree;
        report.inferred_edge_count = gcm.inferred_edge_count;
    }
    Err(e) => tracing::warn!("graph cohesion metrics failed: {e}"),
}
```

Failure is non-fatal (warn + skip), consistent with how Phase 4 co-access stats errors
are handled. The report is still returned; cohesion fields default to zero.

`load_maintenance_snapshot()` is NOT modified — the snapshot already skips Phases 2,
4, and the maintenance-tick path does not invoke cohesion metrics (diagnostic only).

### Layer 4 — Unit Tests (`unimatrix-store`)

**File**: `crates/unimatrix-store/src/read.rs` (existing `#[cfg(test)]` block)

Seven test cases using `open_test_store()` and the existing
`create_graph_edges_table()` helper:

| Test | Scenario | Key assertion |
|------|----------|---------------|
| `all_isolated` | Active entries, no edges | connectivity_rate=0.0, isolated=N |
| `all_connected` | All entries have ≥1 edge | connectivity_rate=1.0, isolated=0 |
| `mixed_connectivity` | Half connected, half isolated | connectivity_rate=0.5 |
| `cross_category_edges` | Edges between different categories | cross_category_edge_count correct |
| `same_category_only` | Edges within same category | cross_category_edge_count=0 |
| `nli_source_edges` | Edges with source='nli' | inferred_edge_count correct |
| `bootstrap_excluded` | bootstrap_only=1 edges only | isolated_entry_count=active_count |

## Component Interactions

```
context_status MCP call
    │
    ▼
StatusService::compute_report()   (services/status.rs)
    │
    ├── Phase 1: store.compute_status_aggregates()          [existing]
    ├── Phase 2: contradiction scan                          [existing]
    ├── Phase 3: observation stats                           [existing]
    ├── Phase 4: co-access stats                             [existing]
    ├── Phase 5: coherence dimensions
    │       ├── HNSW stale ratio / graph_quality_score       [existing]
    │       └── store.compute_graph_cohesion_metrics()       [NEW col-029]
    ├── Phase 5b: lambda + recommendations                   [existing]
    ├── Phase 7: maintenance operations                      [existing]
    └── Phase 8: effectiveness analysis                      [existing]
    │
    ▼
format_status_report()            (mcp/response/status.rs)
    │
    ├── Summary: one-line graph cohesion annotation          [NEW]
    └── Markdown: #### Graph Cohesion sub-section            [NEW]
```

Data flow for `compute_graph_cohesion_metrics()`:

```
write_pool_server()
    │
    ├── SQL Query 1 (pure GRAPH_EDGES, bootstrap_only=0):
    │       → total_edges: i64
    │       → supports_edge_count: i64
    │       → inferred_edge_count: i64
    │
    └── SQL Query 2 (GRAPH_EDGES LEFT JOIN entries, status=0):
            → active_entry_count: i64
            → connected_entry_count: i64
            → cross_category_edge_count: i64
    │
    ▼
GraphCohesionMetrics (computed):
    connectivity_rate = connected / active  (0.0 if active=0)
    isolated_entry_count = active - connected
    mean_entry_degree = (2 * total_edges) / active  (0.0 if active=0)
```

## Technology Decisions

See ADR files. Summary:
- ADR-001: `EDGE_SOURCE_NLI` constant in `unimatrix-store/src/read.rs` (SR-01)
- ADR-002: Two SQL queries (not one mega-query) for cohesion metrics
- ADR-003: `write_pool_server()` for both queries (not `read_pool()`)
- ADR-004: Exact SQL design for cross-category edge count (SR-04 — no cartesian product)

## Integration Points

| Boundary | Direction | Detail |
|----------|-----------|--------|
| `StatusService` → `Store` | Call | `store.compute_graph_cohesion_metrics()` |
| `Store` → SQLite | Query | Two queries via `write_pool_server()` |
| `StatusService` → `StatusReport` | Assign | Six field assignments after call |
| `format_status_report` → `StatusReport` | Read | Six new fields in Summary + Markdown |
| `lib.rs` re-export | Public API | `GraphCohesionMetrics`, `EDGE_SOURCE_NLI` |
| `nli_detection.rs` → `EDGE_SOURCE_NLI` | Import | Replaces bare `"nli"` strings (GH #412 follow-up) |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EDGE_SOURCE_NLI` | `pub const &str = "nli"` | `unimatrix-store/src/read.rs` (new) |
| `GraphCohesionMetrics` | `pub struct { connectivity_rate: f64, isolated_entry_count: u64, cross_category_edge_count: u64, supports_edge_count: u64, mean_entry_degree: f64, inferred_edge_count: u64 }` | `unimatrix-store/src/read.rs` (new) |
| `Store::compute_graph_cohesion_metrics` | `pub async fn(&self) -> Result<GraphCohesionMetrics>` | `unimatrix-store/src/read.rs` (new) |
| `StatusReport::graph_connectivity_rate` | `pub f64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `StatusReport::isolated_entry_count` | `pub u64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `StatusReport::cross_category_edge_count` | `pub u64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `StatusReport::supports_edge_count` | `pub u64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `StatusReport::mean_entry_degree` | `pub f64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `StatusReport::inferred_edge_count` | `pub u64` | `unimatrix-server/src/mcp/response/status.rs` (new) |
| `write_pool_server()` | `fn(&self) -> &SqlitePool` | `unimatrix-store/src/db.rs` (existing) |
| `open_test_store()` | test helper | `unimatrix-store/src/test_helpers` (existing) |
| `create_graph_edges_table()` | test helper | `unimatrix-store/src/read.rs` `#[cfg(test)]` (existing) |

## SQL Design

### Query 1 — Edge-Level Counts (pure GRAPH_EDGES)

```sql
SELECT
    COUNT(*)                                                          AS total_edges,
    COALESCE(SUM(CASE WHEN relation_type = 'Supports'  THEN 1 ELSE 0 END), 0)
                                                                      AS supports_edge_count,
    COALESCE(SUM(CASE WHEN source = 'nli'              THEN 1 ELSE 0 END), 0)
                                                                      AS inferred_edge_count
FROM graph_edges
WHERE bootstrap_only = 0
```

Single-table, no JOINs, uses `idx_graph_edges_source_id` for scan.
Returns one row always (COUNT never produces NULL).

### Query 2 — Connectivity and Cross-Category (JOIN to entries)

```sql
SELECT
    COUNT(DISTINCT e.id)                                              AS active_entry_count,
    COUNT(DISTINCT ge.source_id)
      + COUNT(DISTINCT ge.target_id)                                  AS connected_raw,
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

**SR-04 resolution**: the outer entries table drives the FROM clause. The two inner
JOINs to entries (`src_e`, `tgt_e`) use the `ge.source_id`/`ge.target_id` foreign
keys as join predicates — no cartesian product is possible because both JOIN conditions
are equality predicates on indexed columns. SQLite will use `idx_graph_edges_source_id`
and `idx_graph_edges_target_id` for the left-join lookups.

**connected_entry_count**: because `COUNT(DISTINCT ge.source_id)` and
`COUNT(DISTINCT ge.target_id)` can overlap (an entry appears as both source and
target in different edges), the raw sum overcounts. The implementation corrects this
with a Rust-level computation using a UNION approach or a sub-query:

```sql
-- Sub-query approach (cleaner, avoids double-count):
SELECT COUNT(*) FROM (
    SELECT source_id AS id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id AS id FROM graph_edges WHERE bootstrap_only = 0
) AS connected_ids
JOIN entries e ON e.id = connected_ids.id AND e.status = 0
```

This is executed as a scalar sub-query within Query 2, or as a third standalone query.
The implementer chooses the approach that stays within the 50-line budget. Either is
correct. The UNION-based approach is preferred (standard SQL, no Rust post-processing).

**Note on cross_category_edge_count**: the LEFT JOIN to `src_e` and `tgt_e` checks
`status = 0` — an edge where one or both endpoints are deprecated/quarantined does not
count as cross-category (those entries are invisible to PPR). The `ge.id IS NOT NULL`
guard ensures the NULL row from the LEFT JOIN does not trigger a false positive.

## File Size Check

`read.rs` is 1570 lines. The new function + struct + constant should be ≤ 55 lines.
Post-merge estimate: ~1625 lines. The 500-line rule applies per the Rust workspace
rules; `read.rs` already exceeds this and splitting is out of scope. The implementer
should note this as a housekeeping concern for a future cycle.

## Open Questions

1. **EDGE_SOURCE_NLI adoption in #412**: The constant is defined here in col-029 and
   re-exported. Whether the #412 implementation is updated to use it (replacing the
   existing bare `"nli"` literals in `nli_detection.rs`) is a coordination question
   for the delivery agent. The architecture requires the constant to exist; adoption
   in #412 is a follow-up that should be captured as a task in that issue.

2. **Query 2 connectivity implementation choice**: The UNION sub-query approach is
   preferred but adds a third SQL query. The implementer may instead combine into two
   queries using Rust post-processing (track seen IDs in a HashSet). Both produce
   identical results. Either is acceptable; the UNION approach is cleaner for testing.
