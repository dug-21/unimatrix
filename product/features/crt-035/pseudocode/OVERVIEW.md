# crt-035 Pseudocode Overview: Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## Problem Statement

crt-034 (ADR-006) wrote CoAccess edges one-directionally: `(entry_id_a → entry_id_b)` with
`a < b`. PPR traverses `Direction::Outgoing` only, so seeding the higher-ID entry found no
outgoing CoAccess path back to the lower-ID peer. This halved effective PPR coverage for the
CoAccess signal.

crt-035 fixes this via:
1. **Tick change** — `run_co_access_promotion_tick` writes both `(a→b)` and `(b→a)` going forward.
2. **Migration** — v18→v19 back-fills `(b→a)` for all pre-existing forward-only rows where
   `relation_type = 'CoAccess' AND source = 'co_access'`.

No PPR code, no TypedGraphState code, no cycle detection code, and no CO_ACCESS table schema
changes are required. The fix is purely in the data written to GRAPH_EDGES.

---

## Components Involved

| Component | File | What Changes |
|-----------|------|-------------|
| Tick | `unimatrix-server/src/services/co_access_promotion_tick.rs` | Extract `promote_one_direction` helper; call it twice per pair; update log fields |
| Migration | `unimatrix-store/src/migration.rs` | Bump `CURRENT_SCHEMA_VERSION` 18→19; add `if current_version < 19` block |
| Migration test | `unimatrix-store/tests/migration_v18_to_v19.rs` | New integration test file (7 MIG-U cases) |
| AC-12 regression test | `unimatrix-server/src/services/typed_graph.rs` | Add `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` to existing test block |

Blast-radius files (tick tests, NOT covered in these pseudocode files — handled by tester agent):
- `unimatrix-server/src/services/co_access_promotion_tick_tests.rs` (8 T-BLR + 3 T-NEW tests)

---

## Data Flow

```
CO_ACCESS table (unchanged schema, CHECK entry_id_a < entry_id_b)
    |
    | qualifying pairs: count >= CO_ACCESS_GRAPH_MIN_COUNT (3)
    | ORDER BY count DESC, LIMIT max_co_access_promotion_per_tick
    v
run_co_access_promotion_tick  [co_access_promotion_tick.rs]
    |
    | for each pair (a, b) with new_weight = count / max_count:
    |   promote_one_direction(store, a, b, new_weight) → (inserted, updated)
    |   promote_one_direction(store, b, a, new_weight) → (inserted, updated)
    |
    | accumulate: inserted_count, updated_count, qualified_count
    |
    | tracing::info!(promoted_pairs=qualified_count, edges_inserted=inserted_count,
    |               edges_updated=updated_count, "co_access promotion tick complete")
    v
GRAPH_EDGES (new rows or updated rows for both (a→b) and (b→a))

GRAPH_EDGES v18 (forward-only CoAccess rows: source='co_access')
    |
    | v18→v19 migration back-fill  [migration.rs]
    | INSERT OR IGNORE reverse edge for each forward-only row
    | (NOT EXISTS guard skips already-bidirectional pairs)
    v
GRAPH_EDGES v19 (all CoAccess rows are bidirectional)
    |
    | TypedGraphState::rebuild() reads all bootstrap_only=0 edges  [unchanged]
    v
TypedRelationGraph (petgraph StableGraph)
    |
    | personalized_pagerank(graph, seed_scores, alpha, iterations)  [unchanged]
    | traverses Direction::Outgoing
    v
PPR scores — both (a→b) and (b→a) traversal paths now active
```

---

## Shared Types (No New Types Introduced)

All types are unchanged from crt-034. The function `promote_one_direction` is new but
module-private (not a public type boundary).

| Type | Crate | Usage in crt-035 |
|------|-------|-----------------|
| `Store` (trait alias `unimatrix_core::Store`) | unimatrix-core | Tick and AC-12 test |
| `SqlxStore` | unimatrix-store | Migration tests, AC-12 test |
| `InferenceConfig` | unimatrix-server (infra::config) | Tick (unchanged) |
| `CoAccessBatchRow` (module-private, `#[derive(sqlx::FromRow)]`) | unimatrix-server | Tick (unchanged) |
| `TypedGraphState` | unimatrix-server | AC-12 test |
| `TypedRelationGraph` | unimatrix-engine | AC-12 test |
| `personalized_pagerank` | unimatrix-engine::graph | AC-12 test |

---

## Sequencing Constraints (Wave Plan)

crt-035 has three independent implementation units. None depends on another at the
code level; all three can be written concurrently by separate agents.

**Wave 1 (parallel, no inter-dependency):**
- W1-A: `co_access_promotion_tick.rs` — extract `promote_one_direction`, double the calls,
  update log fields. Tick tests must also be updated in this wave (T-BLR-01 through T-BLR-08,
  T-NEW-01 through T-NEW-03).
- W1-B: `migration.rs` + `tests/migration_v18_to_v19.rs` — bump schema version, add
  back-fill block, create new integration test file.
- W1-C: `typed_graph.rs` AC-12 test — add PPR regression test to existing test block.

**Gate 3b checks (non-negotiable, run after W1 completes):**
- GATE-3B-01: grep `co_access_promotion_tick_tests.rs` for `"no duplicate"` — must be zero matches.
- GATE-3B-02: grep for `count_co_access_edges` assertions — all values must be even (0, 2, 4, 6, 10...).
- GATE-3B-03: run `EXPLAIN QUERY PLAN` on back-fill SQL and confirm NOT EXISTS uses the UNIQUE
  B-tree index (`sqlite_autoindex_graph_edges_1`), not a full scan. Document output as a
  comment in `migration_v18_to_v19.rs`.
- GATE-3B-04: `wc -l co_access_promotion_tick.rs` — must be <= 500.
- GATE-3B-05: grep `typed_graph.rs` for `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry`
  — confirm the test opens a `SqlxStore`, not a bare `TypedRelationGraph::new()`.

---

## Key Invariants (Must Hold After crt-035)

1. Every logical co-access pair `(a, b)` with `a < b` produces exactly **two** rows in
   `GRAPH_EDGES`: `(source_id=a, target_id=b)` and `(source_id=b, target_id=a)`, both with
   `relation_type='CoAccess'` and `source='co_access'`.
2. Both rows carry the **same weight** (derived from the same `co_access.count / max_count`).
3. Both rows carry the **same `created_by`** (either `'bootstrap'` or `'tick'`).
4. `CO_ACCESS` table is **unchanged** — the canonical `CHECK (entry_id_a < entry_id_b)`
   ordering is preserved; the reverse edge lives only in `GRAPH_EDGES`.
5. `count_co_access_edges` (the test helper counting all CoAccess rows in GRAPH_EDGES) returns
   **2N** for N logical pairs — any odd return value indicates a bug.

---

## Open Questions at Pseudocode Time

None. All scope open questions from SCOPE.md are resolved in the architecture:
- OQ-1 (weight symmetry): both directions use the same `new_weight`; independent updates converge to equal values.
- OQ-2 (db.rs fresh path): data-only migration; no change to `create_tables_if_needed()`.
- OQ-3 (NOT EXISTS index coverage): delivery agent runs EXPLAIN QUERY PLAN per GATE-3B-03.
