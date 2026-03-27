# service-call-site — Pseudocode

Component: `StatusService::compute_report()` Phase 5 call site
File: `crates/unimatrix-server/src/services/status.rs`

---

## Purpose

Insert the `compute_graph_cohesion_metrics()` call into Phase 5 of `compute_report()`,
immediately after `report.graph_stale_ratio` is assigned (~line 669). Handle errors
non-fatally: log a `tracing::warn!` and leave all six cohesion fields at their default
zero values. The report is returned regardless of whether this call succeeds.

This follows the exact same non-fatal pattern used for Phase 4 co-access stats at
line ~642: `Err(e) => tracing::warn!("co-access stats failed: {e}")`.

---

## Insertion Point in compute_report()

```
// Existing Phase 5 code (unchanged, shown for orientation):

report.graph_quality_score =
    coherence::graph_quality_score(graph_stale_count, graph_point_count);
report.graph_stale_ratio = graph_stale_ratio;

// <<<< INSERT HERE: after graph_stale_ratio assignment, before embed_dim computation >>>>

let embed_dim = if report.embedding_check_performed {
    // ... existing code continues
```

---

## New Code Block

```
// Graph cohesion metrics (col-029)
// Uses read_pool() — see ADR-003. WAL snapshot staleness is acceptable for
// this diagnostic aggregate. On error, all six fields remain at default 0.
match self.store.compute_graph_cohesion_metrics().await {
    Ok(gcm) => {
        report.graph_connectivity_rate   = gcm.connectivity_rate;
        report.isolated_entry_count      = gcm.isolated_entry_count;
        report.cross_category_edge_count = gcm.cross_category_edge_count;
        report.supports_edge_count       = gcm.supports_edge_count;
        report.mean_entry_degree         = gcm.mean_entry_degree;
        report.inferred_edge_count       = gcm.inferred_edge_count;
    }
    Err(e) => tracing::warn!("graph cohesion metrics failed: {e}"),
}
```

---

## Call Site Constraints

- `self.store` is the `Arc<Store>` on `StatusService`. `compute_graph_cohesion_metrics`
  takes `&self` — no ownership change.
- The `.await` must be present — this is an async fn.
- The `match` must be exhaustive. The `Ok` arm assigns all six fields. The `Err` arm
  only logs. No `?` or `return` in the error arm.
- The block is placed inside `compute_report()`, not inside `load_maintenance_snapshot()`
  or `maintenance_tick()`. NFR-01 and AC-15 require exactly one call site.

---

## Non-Fatal Error Rationale

The ALIGNMENT-REPORT.md WARN 1 resolution explicitly chooses the non-fatal pattern
over SPECIFICATION FR-11's original fatal propagation. Three of four documents
(ARCHITECTURE.md, RISK-TEST-STRATEGY.md, SCOPE.md) describe non-fatal handling.
The implementation follows the architecture's pattern, consistent with the Phase 4
co-access error handling precedent.

If `compute_graph_cohesion_metrics()` returns `Err`, the response is still returned
with all six cohesion fields showing `0` / `0.0`. The operator sees a partial report
rather than a failed `context_status` call at the moment they most need it (e.g.,
during active NLI inference when the store is under write pressure).

---

## State Machine

`compute_report()` is not a state machine but it has a sequential phase structure.
The new block slots into Phase 5 as follows:

```
Phase 5 sequence:
  1. confidence_freshness_score   (existing)
  2. stale_confidence_count       (existing)
  3. graph_quality_score          (existing)
  4. graph_stale_ratio            (existing)
  5. [NEW] compute_graph_cohesion_metrics() -> assign six fields or warn
  6. embed_dim computation        (existing, unchanged)
  7. embedding_consistency_score  (existing)
  8. contradiction_density_score  (existing)
  -- Phase 5b: lambda + recommendations (existing, unchanged)
```

The new step 5 does not affect any subsequent computation. The six cohesion fields
are display-only and do not feed into lambda, graph_quality_score, or any other
coherence dimension.

---

## Error Handling

| Condition | Behavior |
|-----------|----------|
| `compute_graph_cohesion_metrics()` returns `Ok(gcm)` | All six fields assigned from `gcm`. |
| `compute_graph_cohesion_metrics()` returns `Err(e)` | `tracing::warn!("graph cohesion metrics failed: {e}")`. All six fields remain at default `0` / `0.0`. Report continues to be built and returned. |
| `report` fields unset before this block | All six fields were set to default `0` / `0.0` when `StatusReport::default()` was called at the top of `compute_report()`. No additional initialization needed. |

---

## Key Test Scenarios

### Happy path — fields populated from Ok result

```
Setup: A real or mock Store with graph_edges rows present.

Call: compute_report() -> format_status_report()

Assert:
  - report.graph_connectivity_rate  matches GraphCohesionMetrics.connectivity_rate
  - report.isolated_entry_count     matches GraphCohesionMetrics.isolated_entry_count
  - (all six fields match gcm values)
  - format output includes non-zero values in the Graph Cohesion section
```

### Error path — fields remain at default zero

```
Setup: A mock Store whose compute_graph_cohesion_metrics() returns Err.
       (Can be simulated with a closed store or injected error in integration test.)

Call: compute_report()

Assert:
  - No panic, no early return.
  - report.graph_connectivity_rate  == 0.0
  - report.isolated_entry_count     == 0
  - (all six fields remain at default)
  - report is still returned (not an error result).
  - tracing::warn! message was emitted (verify via tracing subscriber capture).
```

### Single call site (AC-15, R-06)

```
Static check: grep compute_graph_cohesion_metrics across all crates/

Expected: exactly one match, in services/status.rs inside compute_report().
          No matches in load_maintenance_snapshot() or maintenance_tick().
```

### ADR-003 comment present (R-11)

```
Static check: the code block contains a comment referencing read_pool() and ADR-003.
              Verifiable by code review at delivery gate.
```

---

## Knowledge Stewardship

- Queried: /uni-query-patterns — found #726 (SQL Aggregation Struct), #1588 (Active-only query). No new deviation from established patterns.
- Deviations from established patterns: none. The non-fatal match/warn pattern is the established Phase 4 co-access precedent already in services/status.rs.
