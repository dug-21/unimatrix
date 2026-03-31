# Agent Report: crt-035-agent-1-pseudocode

## Task

Produce per-component pseudocode for crt-035: bidirectional co_access graph_edges promotion
tick, v18→v19 migration back-fill, and AC-12 PPR traversal regression test.

## Files Produced

| File | Component | Lines (approx) |
|------|-----------|----------------|
| `product/features/crt-035/pseudocode/OVERVIEW.md` | Cross-cutting | ~120 |
| `product/features/crt-035/pseudocode/tick.md` | co_access_promotion_tick.rs | ~200 |
| `product/features/crt-035/pseudocode/migration.md` | migration.rs + migration_v18_to_v19.rs | ~280 |
| `product/features/crt-035/pseudocode/ac12-test.md` | typed_graph.rs AC-12 test | ~150 |

## Components Covered

1. **Tick (co_access_promotion_tick.rs)** — `promote_one_direction` helper function with
   three-step INSERT/fetch/UPDATE state machine; `run_co_access_promotion_tick` Phase 3
   replacement calling the helper twice per pair; updated log field names
   (`promoted_pairs`, `edges_inserted`, `edges_updated`); all three zero-row/error-return
   paths updated to new field names.

2. **Migration (migration.rs)** — `CURRENT_SCHEMA_VERSION` bump 18→19; `if current_version < 19`
   block with full back-fill SQL (INSERT OR IGNORE with NOT EXISTS guard, SELECT swapping
   source_id/target_id, copying weight/created_by, hardcoding source='co_access' and
   bootstrap_only=0); inline UPDATE counters to 19; trailing INSERT OR REPLACE version stamp
   (unchanged structure).

3. **Migration test (migration_v18_to_v19.rs)** — New file. All 7 MIG-U test cases: constant
   assertion, fresh DB, bootstrap-era back-fill, tick-era back-fill, non-CoAccess edges
   untouched, idempotency, empty table no-op. `create_v18_database` helper (follows
   `create_v17_database` pattern exactly). Three helper functions: `read_schema_version`,
   `count_graph_edges`, `count_all_coaccess_edges`.

4. **AC-12 test (typed_graph.rs)** — `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry`.
   Six steps matching spec AC-12 exactly: SqlxStore::open, two NewEntry inserts,
   direct SQL INSERT of reverse CoAccess edge B→A, TypedGraphState::rebuild(), PPR seeded
   at B, assert A's score > 0.0.

## Wave Plan

Three independent delivery agents can work in parallel:
- W1-A: tick.md → implement tick + tick tests
- W1-B: migration.md → implement migration + new migration test file
- W1-C: ac12-test.md → implement AC-12 test in typed_graph.rs

No inter-dependency exists between the three waves.

## Gate-3b Checks (embedded in pseudocode)

All four non-negotiable gate checks are documented as inline comments in the relevant
pseudocode sections:

- GATE-3B-01 (`"no duplicate"` grep) — documented in tick.md T-BLR-08 section.
- GATE-3B-02 (even `count_co_access_edges` values) — documented in tick.md T-BLR section
  and migration.md MIG-U-06.
- GATE-3B-03 (EXPLAIN QUERY PLAN) — documented in migration.md back-fill SQL section.
- GATE-3B-04 (`wc -l` 500-line limit) — documented in tick.md File Constraint section.
- GATE-3B-05 (SqlxStore grep) — documented in ac12-test.md test algorithm.

## Open Questions

None. All scope open questions from SCOPE.md were resolved in the architecture documents
before this pseudocode was written:
- OQ-1 (weight symmetry): both directions use the same `new_weight`.
- OQ-2 (db.rs fresh path): data-only migration; no db.rs change needed.
- OQ-3 (NOT EXISTS index coverage): deferred to delivery per GATE-3B-03.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found #3891 (ADR-006 crt-034/crt-035 edge
  directionality), #3889 (back-fill reverse edges pattern), #3826 (promotion tick cap per
  tick), #3882 (constants: EDGE_SOURCE_CO_ACCESS, CO_ACCESS_GRAPH_MIN_COUNT), #3890
  (ADR-001 crt-035 eventual consistency). All applied to pseudocode decisions.
- Queried: `context_search` for co_access graph_edges promotion tick patterns — found #3883
  (write_pool_server direct path), #3822 (oscillation idempotency), #3889 (back-fill pattern).
  Confirmed all three followed in tick.md and migration.md.
- Queried: `context_search` for crt-035 architectural decisions — found #3890 (ADR-001
  eventual consistency) and #3891 (ADR-006 follow-up contract). Both applied.

- Deviations from established patterns: none. All patterns from the knowledge base are
  followed. No novel patterns were introduced that would warrant a new Unimatrix entry
  at this stage.
