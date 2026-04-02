## ADR-004: GraphCohesionMetrics Extension Scope

### Context

The crt-041 eval gate (§Goals 7) requires verifying two metrics after delivery:
- `cross_category_edge_count` — non-bootstrap edges connecting entries with different category values
- `isolated_entry_count` — active entries with zero non-bootstrap edges

The SCOPE.md §Background Research (GraphCohesionMetrics section) describes these as
"not yet a field" and "also not yet a field," suggesting they may need to be added.

After examining the codebase, both fields already exist. In `read.rs`:

```rust
pub struct GraphCohesionMetrics {
    pub connectivity_rate: f64,
    pub isolated_entry_count: u64,        // added col-029
    pub cross_category_edge_count: u64,   // added col-029
    pub supports_edge_count: u64,
    pub mean_entry_degree: f64,
    pub inferred_edge_count: u64,
}
```

The `compute_graph_cohesion_metrics()` function implements both via two SQL queries
(col-029 ADR-002). Definitions from the implementation:

**cross_category_edge_count:** Non-bootstrap edges (bootstrap_only=0) joining two
entries where e1.category != e2.category. Both endpoints must be active (status=0).
SQL: a JOIN of graph_edges to entries on both endpoints with a WHERE on category inequality.

**isolated_entry_count:** Derived in Rust as `active_entry_count - connected_entry_count`.
`connected_entry_count` is the count of active entries that appear in at least one
non-bootstrap edge (source_id or target_id). Computed via a UNION subquery that
collects both endpoint sides.

Both fields are already tested with integration tests in `read.rs::tests`.

Two scenarios were considered for scope:

**Option A** — Declare crt-041 in-scope for adding new `GraphCohesionMetrics` fields
(e.g., `s1_edge_count`, `s2_edge_count`, `s8_edge_count` per-source breakdowns).
These would help operators understand graph composition but are additive to what
already exists and are not required by the eval gate.

**Option B** — Use the existing `isolated_entry_count` and `cross_category_edge_count`
fields as-is. No changes to `GraphCohesionMetrics`. The eval gate is satisfied by
the current metrics: after S1/S2/S8 populate edges, `cross_category_edge_count`
increases and `isolated_entry_count` decreases. Verify via `context_status` output.

**Option B** is chosen. Adding per-source edge count breakdowns is a useful enhancement
but is not required to pass the eval gate and is outside the stated feature scope.
SCOPE.md §Non-Goals explicitly states "This feature does NOT change how
`inferred_edge_count` is computed" — the precedent is minimal metrics changes.

**SR-05 resolution:** The SCOPE-RISK-ASSESSMENT flagged SR-05 as requiring the exact
SQL definition for both metrics. Those definitions are already in the codebase
(`read.rs` lines ~1048–1134). The risk is resolved — no ambiguity remains.

### Decision

No changes to `GraphCohesionMetrics` or `compute_graph_cohesion_metrics()`.

Both eval gate fields (`cross_category_edge_count` and `isolated_entry_count`) already
exist in `GraphCohesionMetrics` and are computed by the existing SQL queries in
`compute_graph_cohesion_metrics()`.

The eval gate for crt-041 is: after one complete tick cycle following delivery,
`context_status` output shows:
- `cross_category_edge_count` > pre-delivery value
- `isolated_entry_count` < pre-delivery value

No code changes are needed in `unimatrix-store` for this eval gate.

The delivery agent must note the pre-delivery baseline values by calling
`compute_graph_cohesion_metrics()` before running the first S1/S2/S8 tick,
then confirming the expected direction of change after the tick completes.

### Consequences

Easier: No store-layer changes needed for the eval gate. Zero risk of
introducing regressions in `GraphCohesionMetrics` computation.

Harder: Operators cannot currently see a per-source breakdown (how many edges
came from S1 vs S2 vs S8) in `context_status`. If this becomes a support or
debugging need, a follow-up feature can add per-source counts to the metrics.
That work is deferred.
