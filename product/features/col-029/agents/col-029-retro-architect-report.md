# col-029 Retrospective — Architect Report

Feature: col-029 (Graph Cohesion Metrics in context_status, GH #413)
Date: 2026-03-26

## Knowledge Extracted

### Patterns (1 new, 2 validated)

| Entry | Action |
|-------|--------|
| #3618 — Cross-category edge count: two-alias LEFT JOIN with CASE guard | New — generalizable SQL technique for any cross-category edge query |
| #3600 — create_graph_edges_table pre-v13 warning | Validated — content accurate, no update needed |
| #3603 — StatusReport struct literal locations (3 files, 8 in mod.rs) | Validated — content accurate, mitigation advice actionable |

### Procedures (1 new)

| Entry | Action |
|-------|--------|
| #3623 — ADR correction cascade: six-document revalidation checklist | New — triggered by write_pool → read_pool correction requiring architecture, spec, risk strategy, brief, and acceptance map revalidation |

### ADR Status

| ADR | Entry | Status |
|-----|-------|--------|
| ADR-001: EDGE_SOURCE_NLI constant | #3591 | Validated |
| ADR-002: Two SQL queries | #3592 | Validated (pseudocode had bugs; ADR architecture decision was correct) |
| ADR-003 original (write_pool) | #3593 | Deprecated (superseded by #3595 during delivery) |
| ADR-003 corrected (read_pool) | #3595 | Validated — correction proven correct |
| ADR-004: Cross-category JOIN, no cartesian product | #3594 | Validated; technique generalized as pattern #3618 |

### Lessons (4 new)

| Entry | Source |
|-------|--------|
| #3619 — Pool selection for read-only context_status aggregates: check compute_status_aggregates as direct precedent | ADR-003 rework cascade (gate 2 failure) |
| #3620 — Architecture governs over Specification when they conflict on behavioral contracts | FR-11 error-handling conflict (gate 2 failure) |
| #3621 — SQL pseudocode with JOIN-heavy queries must be traced against test scenarios before Gate 3a | 3 SQL bugs caught in implementation (cross-category double-count, connectivity false positive, mean_degree including deprecated edges) |
| #3622 — Design agents write to main repo paths when researcher artifacts show main-repo working directory | mutation_spread warning / scope-phase file leak |

## Retrospective Findings Summary

- 4 warnings, 6 info hotspots
- 4 rework signals addressed: ADR-003 pool choice, FR-11 conflict, SQL pseudocode bugs, design file leak
- compile_cycles (57): root cause is mcp/response/mod.rs 8-literal StatusReport trap (covered by pattern #3603)
- 0 negative baseline outliers; knowledge_entries_stored (12 vs mean 8.2) positive
- No ADRs flagged for supersession
