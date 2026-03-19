# Agent Report: crt-021-agent-1-pseudocode

## Task
Produce per-component pseudocode for all 6 crt-021 components.

## Status
COMPLETE — all 7 files written.

## Files Produced

| File | Lines | Notes |
|------|-------|-------|
| `pseudocode/OVERVIEW.md` | 106 | Component interactions, data flow, shared types, build sequencing |
| `pseudocode/engine-types.md` | 505 | Full pseudocode for RelationType, RelationEdge, TypedRelationGraph, build_typed_relation_graph, graph_penalty, find_terminal_active, private helpers |
| `pseudocode/store-schema.md` | 165 | GRAPH_EDGES DDL + 3 indexes + schema_version counter update in create_tables_if_needed |
| `pseudocode/store-migration.md` | 216 | v12→v13 block: DDL, Supersedes bootstrap, CoAccess bootstrap (with COALESCE guard), version bump |
| `pseudocode/store-analytics.md` | 243 | AnalyticsWrite::GraphEdge variant + drain arm + GraphEdgeRow struct + query_graph_edges |
| `pseudocode/server-state.md` | 276 | TypedGraphState struct, new/new_handle/rebuild, search path usage, ~20 call-site rename map |
| `pseudocode/background-tick.md` | 244 | GRAPH_EDGES compaction step, updated tick sequence, rebuild_typed_graph helper, sequencing invariant |

## Key Design Decisions Applied

1. **Supersedes edge source strategy** (ARCHITECTURE open question 1): Supersedes edges are
   derived from `entries.supersedes` (Pass 2a in build_typed_relation_graph), NOT from GRAPH_EDGES
   Supersedes rows. GRAPH_EDGES Supersedes rows are skipped in Pass 2b to avoid duplication.
   This ensures cycle detection operates on the authoritative source.

2. **Cycle detection on Supersedes-only sub-graph**: A temporary StableGraph<u64, ()> is built
   containing only Supersedes edges before running `is_cyclic_directed`. This prevents bidirectional
   CoAccess edges from producing false CycleDetected returns — a case not addressed in the
   architecture or spec, but required for correctness (R-02 extension).

3. **CycleDetected error propagation from rebuild**: `TypedGraphState::rebuild` returns Err to
   signal cycle detection to the caller. The caller (background tick) sets `use_fallback=true`
   and retains the existing state. The specific error variant (InternalError or custom) is flagged
   for the implementer to choose based on existing StoreError variants.

4. **bootstrap_only=false for CoAccess migration bootstrap**: Per ADR-001 §3, CoAccess edges
   bootstrapped from `co_access` at count >= 3 carry `bootstrap_only=0` because they are
   authoritative signals. This is consistent with Supersedes edges also carrying `bootstrap_only=0`.

5. **Weight formula confirmed**: R-15 in RISK-TEST-STRATEGY warns against flat `weight=1.0`.
   The migration pseudocode uses the exact `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`
   formula from FR-09. Test scenarios assert the count=5 pair has strictly higher weight than count=3.

## Open Questions for Implementer

1. **CycleDetected error variant**: `TypedGraphState::rebuild` needs to return a distinguishable
   error for CycleDetected vs. a store I/O error. The background tick handles these two cases
   differently (set use_fallback=true vs. retain old state). The implementer must either add a
   `StoreError::InternalError` variant or repurpose an existing one. Recommendation: add
   `StoreError::GraphCycle` variant.

2. **Store trait boundary for query_graph_edges**: The pseudocode flags that the implementer must
   check whether `Store::query_all_entries` is defined on the `Store` trait in unimatrix-core or
   directly on `SqlxStore`. `query_graph_edges` must follow the same pattern. See FLAG in
   `store-analytics.md` line 193.

3. **write_pool access in background.rs for compaction**: The compaction DELETE needs direct
   `write_pool` access. The implementer must check how existing maintenance writes in
   `maintenance_tick` access the write pool (direct field or method) and replicate the pattern.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for typed relationship graph patterns — found entry #2403
  (typed graph upgrade path: StableGraph<u64, ()> → StableGraph<u64, EdgeType> with penalty
  filter, directly applicable). Found entry #1607 (SupersessionGraph pattern, used as base).
- Queried: `/uni-query-patterns` for crt-021 architectural decisions — found entry #2417
  (ADR-001 crt-021: typed edge weight model, all major decisions confirmed and applied).
- Queried: `/uni-query-patterns` for supersession graph penalty scoring — found entry #1606
  (ADR-006 crt-014: named penalty constants), entry #1602 (ADR-002 per-query vs. tick rebuild),
  entry #1605 (ADR-005 cycle detection fallback). All applied.

## Deviations from Established Patterns

1. **Cycle detection uses Supersedes-only subgraph**: The existing `build_supersession_graph`
   runs `is_cyclic_directed` on the full graph (which only has Supersedes edges). With the typed
   graph now containing CoAccess (bidirectional) edges, running `is_cyclic_directed` on the full
   graph would produce false positives. The pseudocode introduces a temporary Supersedes-only
   subgraph for the cycle check. This is a deviation from the original code structure but is
   required for correctness.

2. **build_typed_relation_graph skips GRAPH_EDGES Supersedes rows**: GRAPH_EDGES Supersedes rows
   are present in the database (written at migration) but skipped during in-memory graph construction
   (Pass 2b filters them out). Supersedes edges are sourced exclusively from `entries.supersedes`
   (Pass 2a). This preserves R-12 safety but means GRAPH_EDGES Supersedes rows serve only as
   audit/attribution records. Documenting this deviation explicitly so it is not removed by an
   implementer who sees "skip Supersedes in Pass 2b" as dead code.
