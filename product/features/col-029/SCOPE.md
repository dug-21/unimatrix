# col-029: Graph Cohesion Metrics in context_status

## Problem Statement

The `GRAPH_EDGES` table is the foundation of PPR and phase-conditioned retrieval,
but `context_status` has no visibility into graph topology health. The current
`graph_quality_score` field measures HNSW vector index staleness — entirely unrelated
to GRAPH_EDGES structure. As automated NLI-based Supports/Contradicts edge inference
(GH #412) builds the graph, there is no observable signal to answer:

- Are entries connected or isolated? High isolation means PPR cannot propagate
  relevance regardless of algorithm quality.
- Is the graph cross-category or intra-category only? Category-homogeneous graphs
  cannot fix category access imbalance via PPR.
- How many edges were inferred by NLI vs. explicitly written?

Without these six metrics, the graph inference work (#412) is uninspectable.

Affects: operators monitoring knowledge base health; the background tick's maintenance
path that already computes similar aggregates for other health dimensions.

## Goals

1. Add six graph cohesion metrics to `StatusReport` (fields in
   `crates/unimatrix-server/src/mcp/response/status.rs`).
2. Compute the six metrics via a single SQL pass over `GRAPH_EDGES` joined to
   `entries` (active only — deprecated/quarantined excluded).
3. Add the computation function to `crates/unimatrix-store/src/read.rs` as
   `Store::compute_graph_cohesion_metrics()` following the `compute_status_aggregates`
   pattern.
4. Surface the metrics in the `context_status` response (both Summary and Markdown
   format paths in `mcp/response/status.rs`).
5. Call the function during Phase 5 (coherence dimensions) of `compute_report()` in
   `crates/unimatrix-server/src/services/status.rs`.
6. Unit-test the SQL computation function with: all isolated, all connected, mixed,
   cross-category, and same-category-only edge scenarios.

## Non-Goals

- No change to lambda (coherence) computation — the new metrics are informational only
  and do not feed into the `coherence` scalar. The existing `graph_quality_score`
  dimension (HNSW stale ratio) is not replaced.
- No caching in the background tick `MaintenanceDataSnapshot`. The cohesion metrics
  are cheap SQL aggregates computed only when `context_status` is called, not on
  every 15-minute tick.
- No new schema migration — all required data is already in `GRAPH_EDGES` (schema v13)
  and `entries` (schema v17). No new columns added.
- No changes to `TypedRelationGraph` in-memory structure or `build_typed_relation_graph`.
- No alerting thresholds or automated maintenance triggered by cohesion metrics.
- No per-category or per-relation-type breakdown beyond the six specified metrics.
- `inferred_edge_count` does not distinguish between Supports and Contradicts NLI edges
  — total count of `source='nli'` rows only.

## Background Research

### GRAPH_EDGES Schema (schema v13, `crates/unimatrix-store/src/db.rs`)

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

Indexes: `idx_graph_edges_source_id`, `idx_graph_edges_target_id`,
`idx_graph_edges_relation_type`.

Key fields for the six metrics:
- `relation_type`: one of 'Supersedes', 'Contradicts', 'Supports', 'CoAccess',
  'Prerequisite' (string, not integer).
- `source`: `'entries.supersedes'`, `'co_access'`, `'nli'`, `'bootstrap'`, or `''`.
  Confirmed by NLI detection service: inferred edges use `source='nli'`.
- `bootstrap_only = 1` rows are excluded from `TypedRelationGraph.inner` during rebuild.

### StatusReport Struct (`crates/unimatrix-server/src/mcp/response/status.rs`)

A plain struct with `Default` impl, no derives. Fields added by appending to the
struct and default block. Current graph-adjacent fields:
- `graph_stale_ratio: f64` — HNSW stale node ratio (unrelated to GRAPH_EDGES)
- `graph_quality_score: f64` — coherence dimension derived from HNSW stale ratio
- `graph_compacted: bool` — whether HNSW compaction ran this call

The six new fields are added to the same struct following the existing naming pattern.

### `compute_status_aggregates` Pattern (`crates/unimatrix-store/src/read.rs`)

The established pattern for adding a new SQL aggregate group:
1. Define a `GraphCohesionMetrics` struct in `read.rs`.
2. Add `pub async fn compute_graph_cohesion_metrics(&self) -> Result<GraphCohesionMetrics>`
   using `write_pool_server()` (consistent with all server-layer raw queries).
3. Execute one `sqlx::query` with JOINs and aggregations, map to struct.
4. Call from `StatusService::compute_report()` Phase 5 (after active_entries loaded).

### Phase Placement in `compute_report()`

`compute_report()` in `services/status.rs` runs 8 sequential phases:
1. SQL queries (counters, distributions, aggregates)
2. Contradiction scan (O(N) ONNX, skipped in maintenance tick)
3. Observation stats
4. Co-access stats
5. Coherence dimensions (graph_quality_score, freshness, contradiction density)
6. Lambda + recommendations
7. Maintenance operations (confidence refresh, graph compaction)
8. Effectiveness analysis

The new `compute_graph_cohesion_metrics()` call slots into Phase 5, after the
existing HNSW-based graph quality score is computed.

### `source='nli'` Convention

Confirmed in `crates/unimatrix-server/src/services/nli_detection.rs`: NLI-inferred
edges written to `GRAPH_EDGES` with `source='nli'`, `bootstrap_only=0`. The NLI
detection service also asserts this convention in integration tests.

### SQL for the Six Metrics

All six metrics are computable in two SQL queries (or one complex query):

**Query 1** — edge-level counts (pure GRAPH_EDGES):
```sql
SELECT
  COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)   -- not exactly right, needs UNION
  COUNT(*) FILTER (WHERE relation_type = 'Supports')       -- supports_edge_count
  COUNT(*) FILTER (WHERE source = 'nli')                   -- inferred_edge_count
FROM graph_edges
WHERE bootstrap_only = 0
```

**Query 2** — connectivity and cross-category (JOIN to entries):
```sql
SELECT
  COUNT(DISTINCT e.id)        -- active entry count
  COUNT(DISTINCT connected.id) -- entries with ≥1 edge
  COUNT(*)  -- cross-category edges (WHERE src_e.category != tgt_e.category)
FROM entries e
LEFT JOIN graph_edges ge ON ge.bootstrap_only = 0 AND (ge.source_id = e.id OR ge.target_id = e.id)
LEFT JOIN entries src_e ON ge.source_id = src_e.id AND src_e.status = 0
LEFT JOIN entries tgt_e ON ge.target_id = tgt_e.id AND tgt_e.status = 0
WHERE e.status = 0
```

`mean_entry_degree = total_edge_endpoint_appearances / active_entry_count`
(each edge contributes 2 to the degree sum; divided by active entry count).

Actual SQL structure needs care: `isolated_entry_count` uses a LEFT JOIN + IS NULL
to find entries with no edges.

### Existing Tests Pattern

Unit tests for `compute_status_aggregates` are in `crates/unimatrix-store/src/read.rs`
`#[cfg(test)]` block using `open_test_store()` from `test_helpers`. The same helper
is used for the new tests.

Integration tests use `crates/unimatrix-server/src/infra/` helpers. The six metrics
do not require new integration tests beyond the unit tests if the SQL is exercised
fully there.

### Constraints from Prior Work

- `crates/unimatrix-store/src/read.rs` uses `write_pool_server()` for all server-layer
  raw SQL queries — confirmed by `compute_status_aggregates`. Must follow this pattern.
- `StatusReport` fields must have a `Default` value. f64 fields default to `0.0`,
  u64 to `0`.
- File size limit: `read.rs` is currently large. The new function should be ≤ 40 lines
  and appended after existing graph-related methods.
- `bootstrap_only = 1` rows must be excluded from all six metrics to match the
  semantics of `TypedRelationGraph.inner` (which already excludes them).

## Proposed Approach

### Layer 1: Store query function (`unimatrix-store/src/read.rs`)

Add `GraphCohesionMetrics` struct and `compute_graph_cohesion_metrics()` using two
SQL queries:

1. A UNION-based query to count all distinct entry IDs that appear in GRAPH_EDGES
   (for connectivity and degree), plus filter counts for Supports edges and nli-source
   edges.
2. A JOIN query to `entries` (status=0, i.e., Active) to compute isolated entry count
   and cross-category edge count.

This stays in the store layer, not in the server crate, consistent with
`compute_status_aggregates`.

### Layer 2: `StatusReport` fields (`mcp/response/status.rs`)

Six new fields appended to `StatusReport`:
```rust
pub graph_connectivity_rate: f64,
pub isolated_entry_count: u64,
pub cross_category_edge_count: u64,
pub supports_edge_count: u64,
pub mean_entry_degree: f64,
pub inferred_edge_count: u64,
```

Default values: f64 fields → `0.0`, u64 fields → `0`.

### Layer 3: Service call (`services/status.rs`)

In `compute_report()` Phase 5, after the existing HNSW graph quality score:
```rust
let graph_cohesion = self.store.compute_graph_cohesion_metrics().await
    .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;
report.graph_connectivity_rate = graph_cohesion.connectivity_rate;
// ... six assignments
```

### Layer 4: Format output (`mcp/response/status.rs`)

Summary format: append a graph cohesion line if any data is present:
```
Graph cohesion: 78.3% connected, 12 isolated, 45 cross-category, 23 inferred
```

Markdown format: a new "### Graph Cohesion" section in the Coherence block.

### Rationale for Key Choices

- **SQL in store layer, not service layer**: follows `compute_status_aggregates`
  precedent. Keeps raw SQL out of `StatusService`.
- **Two SQL queries, not one**: the connectivity (JOIN to entries) and edge-count
  (pure GRAPH_EDGES) queries have different JOIN shapes. A single mega-query would
  require complex CTEs that are harder to test and maintain.
- **Not in MaintenanceDataSnapshot**: the cohesion metrics are diagnostic, not
  operational. They don't drive any maintenance action. Skipping them in the tick is
  consistent with how Phase 2, 4, 6, 7 are already skipped.
- **bootstrap_only = 0 filter**: matches TypedRelationGraph semantics.

## Acceptance Criteria

- AC-01: `StatusReport` struct contains all six new fields with correct types
  (`graph_connectivity_rate: f64`, `isolated_entry_count: u64`,
  `cross_category_edge_count: u64`, `supports_edge_count: u64`,
  `mean_entry_degree: f64`, `inferred_edge_count: u64`).
- AC-02: `Store::compute_graph_cohesion_metrics()` returns correct values for
  a store with all entries isolated (no edges) — connectivity_rate=0.0,
  isolated_entry_count=total_active, cross_category_edge_count=0.
- AC-03: `Store::compute_graph_cohesion_metrics()` returns correct values for
  a store where all active entries have at least one edge — connectivity_rate=1.0,
  isolated_entry_count=0.
- AC-04: `cross_category_edge_count` counts only edges where both endpoint entries
  have Active status and different categories. Same-category edges are excluded.
- AC-05: `inferred_edge_count` counts exactly the rows with `source='nli'` and
  `bootstrap_only=0`.
- AC-06: `supports_edge_count` counts exactly the rows with `relation_type='Supports'`
  and `bootstrap_only=0`.
- AC-07: `mean_entry_degree` is `0.0` when the active entry set has no edges.
  For a non-empty connected graph, equals `(2 * edge_count) / active_entry_count`
  (treating each edge as contributing 1 to each endpoint's degree).
- AC-08: Deprecated and quarantined entries are excluded from connectivity and
  cross-category computations (only `status = 0` / Active entries count).
- AC-09: `context_status` Summary format response includes at minimum
  `isolated_entry_count` and `cross_category_edge_count` values.
- AC-10: `context_status` Markdown format response includes a dedicated "Graph
  Cohesion" section with all six metric values labeled.
- AC-11: `compute_graph_cohesion_metrics()` is called in `compute_report()` Phase 5
  and results are stored in the `StatusReport` before formatting.
- AC-12: All six fields default to `0` / `0.0` in `StatusReport::default()`.
- AC-13: Unit tests cover: all-isolated, all-connected, mixed, cross-category-only,
  same-category-only, and nli-source edge scenarios.

## Constraints

- **SQLite via sqlx**: all queries use `sqlx::query` with `write_pool_server()`.
  No raw `rusqlite` connections.
- **bootstrap_only=0 filter mandatory**: matches `TypedRelationGraph.inner` semantics.
  Bootstrap-only edges are not real graph edges and must not inflate metrics.
- **Active-only join for connectivity metrics**: only `entries.status = 0` (Active)
  rows count. Deprecated/quarantined entries are invisible to PPR so should be
  invisible to cohesion metrics.
- **No schema migration**: all data is already present in v17 schema.
- **File size**: `read.rs` is large; keep the new function and struct ≤ 50 lines total.
  If the file approaches 500 lines, split into `read_graph.rs` module.
- **No new crate dependency**: the computation uses only `sqlx` and scalar arithmetic,
  no new external crates.
- **`mean_entry_degree` definition**: in+out degree (undirected view). Each edge
  (source_id, target_id) contributes 1 to both source and target degree. Mean = sum of
  degrees / active entry count = (2 * non-bootstrap edge count) / active entry count.
  Rationale: out-degree only would misrepresent highly-referenced entries (many incoming
  Supports edges) as near-zero. In+out correctly measures how embedded an entry is.
  Edge case: if no active entries, return 0.0.

## Decisions (Scope Review)

1. **`mean_entry_degree`**: in+out degree (2× per edge). Out-degree only would misrepresent
   highly-referenced entries as near-zero. In+out correctly measures graph embeddedness.

2. **Bootstrap-only entries**: treated as isolated. Bootstrap edges were seeded from
   co_access counts, not NLI inference. The metric's purpose is to measure whether #412
   is producing real edges — if bootstrap-only entries count as connected, `isolated_entry_count`
   starts low regardless of inference activity, measuring the wrong thing.

3. **Format placement**: Coherence sub-section. Graph cohesion is a health dimension.
   Standalone section would imply parity with top-level status outputs; Coherence
   sub-section is consistent with contradiction coverage and embedding health treatment.

4. **Computation timing**: per-call SQL. Graph cohesion is diagnostic — used to verify
   #412 after a maintenance window. Per-call gives a live snapshot at query time, which
   is correct behavior when debugging graph inference. Background tick caching would show
   stale last-tick values right after a manually triggered pass.

## Tracking

GH Issue: #413
