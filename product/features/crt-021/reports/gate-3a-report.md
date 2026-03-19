# Gate 3a Report: crt-021

> Gate: 3a (Design Review)
> Date: 2026-03-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All component boundaries, interfaces, and ADR decisions reflected correctly |
| Specification coverage | PASS | All 21 FRs covered; no scope additions detected |
| Risk coverage | PASS | All 15 risks (R-01–R-15) mapped to test scenarios |
| Interface consistency | PASS | Shared types consistent across pseudocode files and Overview |
| Known design issue 1: Cycle detection on Supersedes-only subgraph | PASS | Temporary StableGraph<u64, ()> used; not full TypedRelationGraph |
| Known design issue 2: Supersedes edge direction | PASS | source_id = entry.supersedes (old), target_id = entry.id (new) |
| Known design issue 3: bootstrap_only=1 structural exclusion | PASS | Excluded in Pass 2b of build_typed_relation_graph via CONTINUE, not at traversal time |
| Known design issue 4: TypedGraphState holds pre-built graph | PASS | typed_graph: TypedRelationGraph field present; no Vec<GraphEdgeRow> |
| Known design issue 5: edges_of_type sole filter boundary | PASS | Invariant documented; no direct .edges_directed() in penalty/traversal functions |
| Known design issue 6: R-06 CoAccess empty-table guard | PASS | COALESCE/NULLIF guard present; migration selects zero rows on empty table |
| Knowledge stewardship compliance | PASS | Both agent reports have stewardship sections with Queried/Stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

- `engine-types.md`: `TypedRelationGraph` wraps `StableGraph<u64, RelationEdge>` with `HashMap<u64, NodeIndex>` — matches Architecture §1 exactly. `edges_of_type` method signature matches Architecture §1 filter-boundary spec.
- `store-schema.md`: DDL matches Architecture §2a exactly including `metadata TEXT DEFAULT NULL` and `UNIQUE(source_id, target_id, relation_type)`.
- `store-migration.md`: v12→v13 block structure matches Architecture §2b: DDL first, Supersedes bootstrap, CoAccess bootstrap with COALESCE formula, no Contradicts.
- `store-analytics.md`: `AnalyticsWrite::GraphEdge` variant fields match Architecture §2c; shedding policy doc comment is present.
- `server-state.md`: `TypedGraphState` struct holds `typed_graph: TypedRelationGraph` (not raw rows) — implements VARIANCE 2 resolution from IMPLEMENTATION-BRIEF correctly. Search path clone-then-release pattern matches Architecture §3b.
- `background-tick.md`: Tick sequence matches Architecture §4: maintenance → GRAPH_EDGES compaction → VECTOR_MAP compaction → TypedGraphState rebuild → contradiction scan. Compaction uses direct `write_pool` per architecture requirement.
- ADR-001 decisions (StableGraph, string encoding, single graph, edges_of_type filter pattern) are all reflected faithfully.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence — all functional requirements traced**:

| FR | Covered By |
|----|------------|
| FR-01 | engine-types.md RelationType enum, as_str/from_str |
| FR-02 | engine-types.md RelationEdge struct |
| FR-03 | engine-types.md bootstrap_only field on RelationEdge; build_typed_relation_graph Pass 2b exclusion |
| FR-04 | engine-types.md Prerequisite variant defined; no write path shown |
| FR-05 | store-schema.md DDL block, exact match |
| FR-06 | store-schema.md UNIQUE constraint present |
| FR-07 | store-migration.md v12→v13 block with all 5 steps |
| FR-08 | store-migration.md Step 2 Supersedes bootstrap with source_id=entry.supersedes, target_id=entry.id |
| FR-09 | store-migration.md Step 3 CoAccess with COALESCE formula; CO_ACCESS_BOOTSTRAP_MIN_COUNT=3 |
| FR-10 | store-migration.md Step 4 comment — no Contradicts SQL emitted |
| FR-11 | store-analytics.md GraphEdge variant with all 7 fields |
| FR-12 | store-analytics.md drain arm with INSERT OR IGNORE and weight.is_finite() guard |
| FR-13 | store-analytics.md variant_name() arm "GraphEdge" |
| FR-14 | store-analytics.md variant in analytics queue with shed-safe doc comment |
| FR-15 | server-state.md rename table + ~20 call-site map |
| FR-16 | server-state.md TypedGraphState struct with typed_graph, all_entries, use_fallback |
| FR-17 | server-state.md TypedGraphState::rebuild with store.query_all_entries + store.query_graph_edges |
| FR-18 | server-state.md TypedGraphState::new() sets use_fallback=true, empty graph |
| FR-19 | engine-types.md TypedRelationGraph struct definition |
| FR-20 | engine-types.md edges_of_type method as sole filter boundary |
| FR-21 | engine-types.md graph_penalty and find_terminal_active using Supersedes only via edges_of_type |
| FR-22 | server-state.md INVARIANT note: build_typed_relation_graph never called on search hot path |
| FR-23 | engine-types.md build_typed_relation_graph Pass 2b: bootstrap_only=true rows excluded before adding to inner graph |
| FR-24 | background-tick.md tick sequence with steps 1–5 strictly sequential |
| FR-25 | background-tick.md Step 2 DELETE SQL via write_pool |
| FR-26 | store-migration.md Test 7 (AC-21) documents DELETE+INSERT promotion pattern |
| FR-27 | IMPLEMENTATION-BRIEF.md confirms entry #2417 ADR already stored; AC-16 is a manual gate |

Non-functional requirements:
- NF-01 (weight.is_finite()): Covered in drain arm pseudocode and engine-types RelationEdge comment.
- NF-02 (UNIQUE constraint): DDL present in both store-schema.md and store-migration.md.
- NF-03 (cold-start fallback): server-state.md TypedGraphState::new() sets use_fallback=true.
- NF-04 (25+ existing tests pass): engine-types.md explicitly lists 34 existing tests; pass condition confirmed.
- NF-05 (StableGraph only): engine-types.md mentions no additional petgraph features.
- NF-06 (#[non_exhaustive]): store-analytics.md notes the catch-all arm contract is preserved.
- NF-07 (no type aliases): server-state.md explicitly states "No type aliases permitted."
- NF-08 (sqlx-data.json): OVERVIEW.md step 6 and test-plan/OVERVIEW.md CI gate both call this out.
- NF-09 (compaction cost): background-tick.md notes unbounded DELETE is accepted for crt-021; documented.
- NF-10 (string encoding only): Carried consistently in RelationType and all insert paths.

**No scope additions detected.** The `metadata TEXT DEFAULT NULL` column is an approved addition documented in ALIGNMENT-REPORT and IMPLEMENTATION-BRIEF (WARN, approved pre-crt-021).

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

| Risk | Priority | Test Coverage |
|------|----------|--------------|
| R-01 (graph_penalty regression) | Critical | engine-types test plan: 34 existing tests listed with names; must pass unchanged; 3 additional mixed-type scenarios |
| R-02 (edges_of_type bypassed) | Critical | engine-types: `test_graph_penalty_identical_with_mixed_edge_types`, `test_find_terminal_active_ignores_non_supersedes_edges`, `test_edges_of_type_filters_correctly`, `test_cycle_detection_on_supersedes_subgraph_only`, grep gate `test_no_direct_edges_directed_calls_at_penalty_sites` |
| R-03 (bootstrap_only=1 reaches graph_penalty) | Critical | engine-types: 3 structural exclusion tests; server-state: `test_typed_graph_state_rebuild_excludes_bootstrap_only_edges` |
| R-04 (tick sequencing violated) | High | background-tick: `test_background_tick_orphaned_edge_absent_from_rebuilt_graph`, sequential inspect gate |
| R-05 (cold-start regression) | High | server-state: `test_typed_graph_state_new_handle_sets_use_fallback_true`, `test_typed_graph_state_cold_start_graph_is_empty` |
| R-06 (CoAccess NULL on empty table) | Critical | store-migration: `test_v12_to_v13_empty_co_access_succeeds` (marked MANDATORY, highest priority) |
| R-07 (NaN/Inf weight propagation) | High | engine-types + store-analytics: 7 weight guard unit tests + drain integration test |
| R-08 (migration not idempotent) | Med | store-migration: `test_v12_to_v13_idempotent_double_run`; store-analytics: idempotent drain test |
| R-09 (sqlx-data.json stale) | High | CI gate in store-schema.md and OVERVIEW.md |
| R-10 (RelationType silent failure) | Med | engine-types: `test_relation_type_from_str_unknown_returns_none`, `test_build_typed_graph_skips_edge_with_unmapped_node_id` |
| R-11 (compaction cost regression) | High | background-tick: `test_background_tick_compaction_completes_within_budget` with 1000-row table |
| R-12 (Supersedes source divergence) | Med | engine-types: `test_supersedes_edges_from_entries_not_graph_edges_table` |
| R-13 (analytics queue shed during bootstrap) | Low | Code inspection only per RISK-TEST-STRATEGY guidance; migration uses direct SQL (confirmed in pseudocode) |
| R-14 (rename incomplete) | Med | Compile-time enforcement; grep gate in server-state.md |
| R-15 (flat weight=1.0 instead of normalized) | Med | store-migration: `test_v12_to_v13_co_access_threshold_and_weights` with explicit weight=0.6 and weight=1.0 assertions |

All 15 risks are covered. The coverage requirement thresholds from RISK-TEST-STRATEGY (min 4 scenarios for Critical, min 2 for High, min 1 for Medium) are met or exceeded.

### Check 4: Interface Consistency

**Status**: PASS

**Evidence — cross-file type consistency verified**:

**RelationType**: Defined in engine-types.md with 5 variants and as_str/from_str. Referenced in store-migration.md (string literals "Supersedes", "CoAccess"), store-analytics.md (relation_type: String field), background-tick.md (indirectly via build_typed_relation_graph). Consistent throughout.

**RelationEdge**: Defined in engine-types.md with 6 fields including bootstrap_only. OVERVIEW.md shared types block lists these fields identically. Used in TypedRelationGraph.inner. Consistent.

**TypedRelationGraph**: Defined in engine-types.md. OVERVIEW.md lists it. server-state.md TypedGraphState.typed_graph field uses it. background-tick.md rebuild references it. Consistent.

**GraphEdgeRow**: Defined in store-analytics.md (read.rs section) with 8 fields. OVERVIEW.md lists it. engine-types.md build_typed_relation_graph imports it. server-state.md rebuild calls store.query_graph_edges() → Vec<GraphEdgeRow>. Background-tick indirectly. All consistent.

**TypedGraphState / TypedGraphStateHandle**: Defined in server-state.md. background-tick.md parameter type is TypedGraphStateHandle. OVERVIEW.md rename block. Consistent.

**Supersedes edge direction (VARIANCE 1)**: store-migration.md Step 2 comment explicitly states `source_id = entry.supersedes (old/replaced), target_id = entry.id (new/correcting)`. engine-types.md Pass 2a uses `pred_idx → succ_idx` where pred_idx is from `entry.supersedes`. store-migration.md Test 7 asserts `source_id=1, target_id=2` for entry id=2 with supersedes=1. All three are consistent.

**bootstrap_only field**: Present on RelationEdge (engine-types.md), GraphEdgeRow (store-analytics.md), AnalyticsWrite::GraphEdge (store-analytics.md), GRAPH_EDGES DDL (store-schema.md, store-migration.md). Consistently typed as `bool` in structs and `INTEGER NOT NULL DEFAULT 0` in SQL. Consistent.

**GRAPH_EDGES DDL**: Identical across store-schema.md and store-migration.md Step 1 — same 10 columns, same constraints, same indexes.

### Known Design Issue 1: Cycle Detection on Supersedes-Only Subgraph

**Status**: PASS

**Evidence**: engine-types.md Pass 3 pseudocode explicitly builds a `temp_graph: StableGraph<u64, ()>` containing only edges where `edge_ref.weight().relation_type == "Supersedes"` before calling `is_cyclic_directed(&temp_graph)`. The full `TypedRelationGraph.inner` (which includes CoAccess bidirectional pairs) is NOT passed to `is_cyclic_directed`. This directly addresses the false-positive cycle detection risk from CoAccess edges.

The pseudocode agent report confirms this as "Deviation 1" — intentional, documented, and necessary for correctness. The test plan covers this with `test_cycle_detection_on_supersedes_subgraph_only` (engine-types.md).

### Known Design Issue 2: Supersedes Edge Direction

**Status**: PASS

**Evidence**: store-migration.md Step 2 SQL: `supersedes AS source_id, id AS target_id`. Comment: "Edge direction: source_id = entry.supersedes (old/replaced), target_id = entry.id (new/correcting)." Test 7 (`test_supersedes_edge_direction_matches_graph_rs`) asserts for entry id=2 with supersedes=1 that `source_id=1, target_id=2`. engine-types.md Pass 2a adds edge `pred_idx → succ_idx` where `pred_idx = graph.node_index[pred_id]` (= entry.supersedes) and `succ_idx = graph.node_index[entry.id]`. Both are consistent with the architecture SQL direction. VARIANCE 1 is correctly resolved.

### Known Design Issue 3: bootstrap_only=1 Structural Exclusion

**Status**: PASS

**Evidence**: engine-types.md Pass 2b pseudocode:
```
FOR each row in edges:
    IF row.bootstrap_only:
        CONTINUE    -- structural exclusion; never added to inner graph
```

The CONTINUE instruction means bootstrap_only=true rows never reach `graph.inner.add_edge()`. There is no runtime check at `graph_penalty` or `find_terminal_active` — the exclusion is structural during graph construction. OVERVIEW.md also documents this: "bootstrap_only=true → structurally excluded from TypedRelationGraph.inner". This satisfies Architecture AC-12 requirement.

### Known Design Issue 4: TypedGraphState Holds Pre-Built Graph

**Status**: PASS

**Evidence**: server-state.md `TypedGraphState` struct defines `typed_graph: TypedRelationGraph` (not `all_edges: Vec<GraphEdgeRow>`). The INVARIANT note states: "build_typed_relation_graph is NEVER called on the search hot path. Only graph_penalty is called, and only on the pre-built typed_graph clone." This satisfies VARIANCE 2 from IMPLEMENTATION-BRIEF (spec FR-16/FR-22 governs over architecture §3a/3b). server-state.md explicitly references VARIANCE 2.

### Known Design Issue 5: edges_of_type Sole Filter Boundary

**Status**: PASS

**Evidence**: engine-types.md explicitly states the INVARIANT: "graph_penalty, find_terminal_active, dfs_active_reachable, and bfs_chain_depth MUST NOT call .edges_directed() or .neighbors_directed() directly." All four private helper functions in the pseudocode use `graph.edges_of_type(...)` exclusively — confirmed by reading each helper. The test plan includes `test_no_direct_edges_directed_calls_at_penalty_sites` as a grep-based CI gate. Architecture §1 requirement ("Direct calls to .edges_directed() outside of this method are prohibited") is satisfied.

### Known Design Issue 6: R-06 CoAccess Empty-Table Guard

**Status**: PASS

**Evidence**: store-migration.md Step 3 SQL uses:
```sql
COALESCE(
    CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0),
    1.0
) AS weight
```
The comment explains: "On a clean install with zero rows matching count >= 3, the INSERT selects zero rows and executes successfully with no data written." The COALESCE and NULLIF are both present. The test plan mandates `test_v12_to_v13_empty_co_access_succeeds` as the highest-priority migration test. The failure mode table in store-migration.md explicitly lists "Empty co_access table (R-06) → Step 3 INSERT selects zero rows; COALESCE never evaluated; succeeds."

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

**crt-021-agent-1-pseudocode-report.md**: Contains `## Knowledge Stewardship` section. Has `Queried:` entries (3 /uni-query-patterns queries with specific entry numbers cited). Has `Stored:` entry described as "nothing novel to store" — however, the agent does NOT give a reason after "nothing novel" (the deviations section documents the new subgraph pattern, but the stewardship block itself doesn't explain why it wasn't stored). This is a WARN per gate rules.

**crt-021-agent-2-testplan-report.md**: Contains `## Knowledge Stewardship` section. Has `Queried:` entries. Has `Stored:` entry: "entry #2428 'Migration test pattern: window function weight normalization with empty-table guard (R-06 pattern)' via /uni-store-pattern" — full entry with ID, title, and method. Fully compliant.

**WARN**: The pseudocode agent (agent-1) stewardship block does not include a reason after "nothing novel to store." The gate rule states: "Present but no reason after 'nothing novel' = WARN." Applying WARN.

---

## Rework Required

None — gate result is PASS.

---

## Warnings

| Warning | Agent | Detail |
|---------|-------|--------|
| Stewardship: no reason after "nothing novel to store" | crt-021-agent-1-pseudocode | The Knowledge Stewardship block lists Queried entries but the Stored line says nothing novel without giving a reason. The deviations section documents two novel patterns (cycle detection subgraph, Supersedes-only sourcing from entries) that the agent explicitly chose not to store — the rationale should appear in the stewardship block, not only in the deviations section. |

---

## Knowledge Stewardship

Queried: /uni-query-patterns before gate review — searched for "gate 3a validation patterns", "typed graph design review", "cycle detection subgraph validation". No directly applicable gate lesson patterns found (gate lessons are stored as lesson-learned, not pattern).

Stored: nothing novel to store — the validation findings for crt-021 are feature-specific and do not constitute a recurring cross-feature pattern. The CoAccess empty-table guard pattern was already stored by the test-plan agent (entry #2428). The Supersedes-only cycle detection subgraph pattern is novel but highly feature-specific to typed multi-edge graphs; not yet a cross-feature pattern.
