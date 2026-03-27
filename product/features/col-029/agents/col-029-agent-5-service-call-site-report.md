# Agent Report: col-029-agent-5-service-call-site

Feature: col-029 — graph cohesion metrics (GH #413)
Agent ID: col-029-agent-5-service-call-site
Component: `StatusService::compute_report()` Phase 5 call site

---

## Task Summary

Insert the `compute_graph_cohesion_metrics()` call into Phase 5 of `compute_report()`
in `crates/unimatrix-server/src/services/status.rs`, after the
`report.graph_stale_ratio = graph_stale_ratio;` assignment. Non-fatal error handling
per alignment report WARN 1 resolution (architecture pattern, not SPECIFICATION FR-11).

---

## Files Modified

- `crates/unimatrix-server/src/services/status.rs` — inserted 15 lines after line 676

---

## What Was Done

Inserted the following block at line 678 (after `report.graph_stale_ratio = graph_stale_ratio;`):

```rust
// Graph cohesion metrics (col-029)
// ADR-003: read_pool() — WAL snapshot semantics intentional, bounded staleness accepted.
// On error, all six fields remain at default 0 / 0.0.
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

The `StatusReport::default()` stub fields (`graph_connectivity_rate: 0.0`, etc.) were
already present from the previous wave agent (agent-4-status-report-fields). No duplication.

---

## Static Checks

### AC-15: Single call site

```
/workspaces/unimatrix-col-029/crates/unimatrix-store/src/read.rs:1012  (definition)
/workspaces/unimatrix-col-029/crates/unimatrix-store/src/read.rs:1807  (unit test)
... (7 unit test calls in read.rs #[cfg(test)] block)
/workspaces/unimatrix-col-029/crates/unimatrix-server/src/mcp/response/status.rs:72  (doc comment)
/workspaces/unimatrix-col-029/crates/unimatrix-server/src/services/status.rs:681  (THE call site)
```

Exactly one production call site in `services/status.rs` inside `compute_report()`.
Not present in `load_maintenance_snapshot()` or `maintenance_tick()`. AC-15 PASS.

### R-11: ADR-003 comment present

Comment `// ADR-003: read_pool() — WAL snapshot semantics intentional, bounded staleness accepted.`
is present at the call site. R-11 PASS.

### AC-11: All six fields assigned in Ok arm — PASS.

---

## Tests

All unimatrix-server tests: **2248 passed; 0 failed** (2163 unit + 46 + 16 + 16 + 7 across test suites).

---

## Commit

`d3475c9 impl(service-call-site): wire compute_graph_cohesion_metrics into Phase 5 of compute_report (#413)`

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` via context_search for "col-029 architectural decisions" — found ADR-001 through ADR-004 for col-029 (entries #3591, #3592, #3594, #3595). ADR-003 confirmed read_pool() is correct; earlier draft stated write_pool_server() (now corrected). Applied ADR-003 comment at call site as required by R-11.
- Stored: nothing novel to store — the non-fatal match/warn pattern is the established Phase 4 co-access precedent already documented. The ADR-003 pool-choice correction is already stored as entry #3595. No new gotchas discovered.
