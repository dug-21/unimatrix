# Scope Risk Assessment: col-029

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `inferred_edge_count` relies on `source='nli'` literal — if #412 (automated graph inference) uses a different source string the metric will undercount or break | High | Med | Architect must coordinate source tag convention with #412 before design is finalised |
| SR-02 | Six-metric SQL pass joins GRAPH_EDGES to ENTRIES (active-only filter); on large corpora this is a potentially expensive cross-join at maintenance-tick frequency | Med | Low | Architect should evaluate whether a single aggregating CTE covers all six metrics in one pass, and verify query plan |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Scope says "computed in maintenance path, not on every status call" but `run_maintenance` is called from `maintenance_tick` which uses `MaintenanceDataSnapshot`, not a full `StatusReport` — caching in the maintenance report struct requires plumbing the computed values back to `compute_report` for display | High | High | Spec writer must define exactly where the graph cohesion cache lives and how `compute_report` reads it (same pattern as `ContradictionScanCacheHandle`) |
| SR-04 | `cross_category_edge_count` requires category lookup for both source and target entries; deprecated/quarantined entries must be excluded, which is not trivially satisfied by querying GRAPH_EDGES alone | Med | Med | Spec must state the join strategy: JOIN ENTRIES twice on active status, or a pre-computed per-entry category map |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `StatusReport` struct has many fields with explicit `Default` impl — adding six fields risks missing them in the default block, causing compile error or silent zero-reporting | Med | High | Architect should follow the existing pattern; spec writer must include a self-check that all six fields appear in `StatusReport::default()` |
| SR-06 | `maintenance_tick` builds a thin `StatusReport` stub (`graph_stale_ratio` only) — new graph cohesion fields cached in `StatusReport` will not be populated via the tick path unless they are also plumbed into `MaintenanceDataSnapshot` or a separate cache handle | High | High | Architect must decide: (a) extend `MaintenanceDataSnapshot` with graph cohesion, or (b) introduce a `GraphCohesionCacheHandle` following the contradiction-cache pattern (entry #274, #3559) |

## Assumptions

- **Issue assumption** (§Implementation): "All six metrics are derivable from a single SQL pass over GRAPH_EDGES joined to ENTRIES." This is true for `isolated_entry_count`, `graph_connectivity_rate`, and `mean_entry_degree`, but `cross_category_edge_count` requires joining ENTRIES twice (source side and target side). A true single-pass query may require a CTE or subquery — not a simple JOIN.
- **Issue assumption** (§Metrics): `inferred_edge_count` is defined as `edges with source='nli'`. This coupling to a string literal is only safe if #412 (automated NLI inference pass) also uses `source='nli'`. The value of that constant is currently confirmed in `nli_detection.rs` but is not a named constant shared across crates.

## Design Recommendations

- **SR-03/SR-06**: The architect must choose a caching strategy before implementation. The safest path mirrors `ContradictionScanCacheHandle` (ADR entry #274): a separate `Arc<RwLock<Option<GraphCohesionSnapshot>>>` written by the background tick and read by `compute_report`. This avoids touching the `MaintenanceDataSnapshot` struct.
- **SR-01**: Define a named constant `EDGE_SOURCE_NLI: &str = "nli"` shared between `nli_detection.rs` and any new cohesion query to prevent silent divergence when #412 ships.
- **SR-04**: The SQL for `cross_category_edge_count` should be designed by the architect and reviewed by the spec writer before implementation; a naive double-join may produce a cartesian product on large graphs.
