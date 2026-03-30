# crt-035 Implementation Brief: Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-035/SCOPE.md |
| Architecture | product/features/crt-035/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-035/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-035/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-035/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| co_access_promotion_tick.rs | pseudocode/co_access_promotion_tick.md | test-plan/co_access_promotion_tick.md |
| migration.rs (v18→v19) | pseudocode/migration_v18_to_v19.md | test-plan/migration_v18_to_v19.md |
| typed_graph.rs (AC-12 test) | pseudocode/typed_graph_ac12.md | test-plan/typed_graph_ac12.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

crt-035 fulfills the follow-up contract in ADR-006 (Unimatrix #3830): make the
co_access promotion tick write both `(a→b)` and `(b→a)` CoAccess edges per qualifying
pair, and back-fill all pre-existing forward-only CoAccess rows in `GRAPH_EDGES` via a
v18→v19 schema migration. This eliminates the structural defect where PPR seeding the
higher-ID entry in any co-access pair found no path back to the lower-ID peer, which
halved the effective coverage of the CoAccess signal in PPR-driven retrieval.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| D1 — back-fill `created_by` value | Copy from forward edge (`'bootstrap'` or `'tick'`); `created_by` tracks relationship origin, not code path. No new sentinel value. | SCOPE.md §Decisions | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |
| D2 — tick log format | `promoted_pairs: N, edges_inserted: M, edges_updated: K` as structured key-value fields on `tracing::info!`. `N` = pairs processed; `M` up to `2*N`; `K` up to `2*N`. | SCOPE.md §Decisions | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |
| D3 — AC-12 test placement | Extend existing `#[cfg(test)] mod tests` in `typed_graph.rs` using `SqlxStore` + `TypedGraphState::rebuild()`. NOT graph_ppr_tests.rs. Spec is authoritative over architecture doc. | SCOPE.md §Decisions, SPECIFICATION.md AC-12 | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |
| D4 — back-fill SQL uses NOT EXISTS guard | `INSERT OR IGNORE` plus `AND NOT EXISTS (SELECT 1 FROM graph_edges rev WHERE rev.source_id = g.target_id AND rev.target_id = g.source_id AND rev.relation_type = 'CoAccess')` for explicit intent and efficient re-runs. | SCOPE.md §Decisions | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |
| SR-01 — atomicity | Eventual consistency: `promote_one_direction` called twice per pair as independent SQL sequences. A partial failure logs `warn!` and the next tick converges the stale direction via delta detection. No per-pair transaction wrapping. | ADR-001 | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |
| OQ-03 (SR-04) — NOT EXISTS index coverage | Deferred to delivery gate R-01: delivery agent must run `EXPLAIN QUERY PLAN` on the back-fill SQL and confirm the `UNIQUE(source_id, target_id, relation_type)` B-tree index covers the NOT EXISTS sub-join. Document result in migration test file. | RISK-TEST-STRATEGY.md R-01 | architecture/ADR-001-bidirectional-tick-eventual-consistency.md |

---

## Files to Create / Modify

### Modify

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` | Extract `promote_one_direction` helper; call it twice per qualifying pair (forward + reverse); update `tracing::info!` to emit `promoted_pairs`/`edges_inserted`/`edges_updated`. Must remain under 500 lines. |
| `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs` | Update 8 tests (T-BLR-01 through T-BLR-08) that assert one-directional edge counts; add 3 new Group I tests (T-NEW-01, T-NEW-02, T-NEW-03). |
| `crates/unimatrix-store/src/migration.rs` | Bump `CURRENT_SCHEMA_VERSION` from 18 to 19; add `if current_version < 19 { ... }` block with back-fill SQL. |
| `crates/unimatrix-server/src/services/typed_graph.rs` | Add `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` test to existing `#[cfg(test)] mod tests` block (AC-12). |

### Create

| File | Purpose |
|------|---------|
| `crates/unimatrix-store/tests/migration_v18_to_v19.rs` | Integration test suite for v18→v19 migration (7 MIG-U test cases per AC-10). |

---

## Data Structures

### GRAPH_EDGES schema (unchanged DDL, new data shape)

```
graph_edges (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL,
    target_id       INTEGER NOT NULL,
    relation_type   TEXT NOT NULL,            -- 'CoAccess' | 'Supports' | 'Supersedes' | ...
    weight          REAL NOT NULL DEFAULT 1.0,
    created_at      INTEGER NOT NULL,
    created_by      TEXT NOT NULL DEFAULT '', -- 'bootstrap' | 'tick'
    source          TEXT NOT NULL DEFAULT '', -- 'co_access' for all CoAccess edges
    bootstrap_only  INTEGER NOT NULL DEFAULT 0,
    metadata        TEXT DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

After crt-035, each logical co-access pair `(entry_id_a, entry_id_b)` has two rows:
- forward: `(source_id=a, target_id=b, relation_type='CoAccess', source='co_access')`
- reverse: `(source_id=b, target_id=a, relation_type='CoAccess', source='co_access')`

Both rows carry the same `weight` and `created_by`. The `UNIQUE` constraint distinguishes them.

### CO_ACCESS table (unchanged)

The `CO_ACCESS` table enforces `CHECK (entry_id_a < entry_id_b)`. The canonical ordering
is preserved. The reverse edge is written only to `GRAPH_EDGES`, never to `CO_ACCESS`.

---

## Function Signatures

### New: `promote_one_direction` (module-private, `co_access_promotion_tick.rs`)

```rust
async fn promote_one_direction(
    store: &Store,
    source_id: i64,
    target_id: i64,
    new_weight: f64,
) -> (bool, bool) // (inserted, updated)
```

Encapsulates: INSERT OR IGNORE → on no-op: fetch existing weight → if delta > 0.1:
UPDATE. Returns `(true, false)` on fresh insert, `(false, true)` on weight update,
`(false, false)` on no-op or error. Errors are logged at `warn!` and the function
returns `(false, false)` without propagating.

### Updated: `run_co_access_promotion_tick` (public, `co_access_promotion_tick.rs`)

Signature unchanged: `pub async fn run_co_access_promotion_tick(store: &Store, config: &InferenceConfig, tick_count: u32)`.

Internal change: per qualifying pair, calls `promote_one_direction` twice:
1. `promote_one_direction(store, row.entry_id_a, row.entry_id_b, new_weight)`
2. `promote_one_direction(store, row.entry_id_b, row.entry_id_a, new_weight)`

Accumulates `inserted_count` and `updated_count` across both calls. Emits:
```rust
tracing::info!(promoted_pairs = qualified_count, edges_inserted = inserted_count, edges_updated = updated_count, "co_access promotion tick complete");
```

### Updated: `CURRENT_SCHEMA_VERSION` (`migration.rs`)

```rust
pub const CURRENT_SCHEMA_VERSION: u64 = 19;
```

### Back-fill SQL (`migration.rs`, inside `if current_version < 19 { ... }`)

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
SELECT
    g.target_id          AS source_id,
    g.source_id          AS target_id,
    'CoAccess'           AS relation_type,
    g.weight             AS weight,
    strftime('%s','now') AS created_at,
    g.created_by         AS created_by,
    'co_access'          AS source,
    0                    AS bootstrap_only
FROM graph_edges g
WHERE g.relation_type = 'CoAccess'
  AND g.source = 'co_access'
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = 'CoAccess'
  )
```

---

## Constraints

1. **500-line file limit** — `co_access_promotion_tick.rs` must remain under 500 lines after the change. The `promote_one_direction` helper is the primary mechanism. Tests stay in `_tests.rs`.
2. **Infallible tick contract** — `run_co_access_promotion_tick` returns `()`. All SQL errors on individual operations are logged at `warn!` and do not propagate. Each direction is independent: a failure on the reverse does not abort the forward or skip the next pair.
3. **INSERT OR IGNORE idempotency** — All inserts use `INSERT OR IGNORE`. The `UNIQUE(source_id, target_id, relation_type)` constraint handles duplicates silently. This constraint must not be changed.
4. **No CO_ACCESS schema changes** — `co_access_key()` in `schema.rs` and the `CHECK (entry_id_a < entry_id_b)` constraint are unchanged.
5. **Migration version gate** — The back-fill runs under `if current_version < 19 { ... }` inside the main migration transaction. The version bump to 19 runs unconditionally at the end of `run_main_migrations` via the final `INSERT OR REPLACE INTO counters`.
6. **No PPR/cycle-detection code changes** — `personalized_pagerank`, `build_typed_relation_graph`, and cycle detection code are unchanged. Cycle detection uses a Supersedes-only subgraph; CoAccess edges are excluded (ADR-006 #3830, Pattern #2429).
7. **Delivery gate R-01** — Before closing, the delivery agent must run `EXPLAIN QUERY PLAN` on the back-fill SQL and confirm the UNIQUE index covers the NOT EXISTS sub-join. Document the result in `migration_v18_to_v19.rs` as a comment.

---

## Dependencies

### Crates (no new dependencies)

| Crate | Usage |
|-------|-------|
| `unimatrix-server` | `co_access_promotion_tick.rs`, `typed_graph.rs` |
| `unimatrix-store` | `migration.rs`, `tests/migration_v18_to_v19.rs` |
| `sqlx` 0.8 (sqlite, runtime-tokio, macros) | All SQL operations — unchanged |
| `tracing` | `warn!`, `info!` in tick — unchanged |
| `tracing-test` | `#[traced_test]` for T-NEW-03 — already used in test suite |
| `tempfile` | `SqlxStore` test fixture for AC-12 — already used in typed_graph.rs tests |

### External services

None. crt-035 is a pure SQLite data migration and tick code change.

### Unimatrix entries to update at delivery

| Entry | Action |
|-------|--------|
| #3830 (ADR-006) | `context_correct` — confirm follow-up contract fulfilled: bidirectional writes now default, back-fill bounded by `source = 'co_access'`, v1 forward-only layout was intentional |
| #3891 (ADR-006 correction chain) | Referenced by Unimatrix — no separate update needed |

---

## NOT in Scope

- No changes to `CO_ACCESS` table schema or `co_access_key()` ordering function.
- No changes to `CO_ACCESS_WEIGHT_UPDATE_DELTA` (0.1), `PROMOTION_EARLY_RUN_WARN_TICKS`, or `max_co_access_promotion_per_tick` config semantics.
- No changes to PPR traversal logic, `TypedGraphState`, or `personalized_pagerank`.
- No GC of sub-threshold or orphaned CoAccess edges (GH #409).
- No changes to weight update delta semantics.
- No removal of `AnalyticsWrite::GraphEdge` or the analytics drain path.
- Back-fill does NOT touch `Supersedes`, `Contradicts`, or `Supports` edges.
- No new public constants, config fields, or re-exports beyond `CURRENT_SCHEMA_VERSION = 19`.
- No changes to cycle detection logic.
- No `db.rs` changes — `create_tables_if_needed()` fresh-DB path is a no-op for the data migration and requires no update.

---

## Alignment Status

**Overall: 4 PASS, 2 WARN, 1 VARIANCE — VARIANCE RESOLVED before delivery.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly addresses W1-1 graph integrity gap; PPR coverage is a core intelligence pipeline correctness issue |
| Milestone Fit | PASS | Cortical phase follow-up contract from crt-034/ADR-006; no future-milestone capabilities |
| Scope Gaps | PASS | All SCOPE.md goals and AC are covered across all three source documents |
| Scope Additions | WARN | ARCHITECTURE.md referenced `crates/unimatrix-engine` (non-existent crate). Resolved: Architecture doc corrected to `typed_graph.rs` in `unimatrix-server`. AC-12 test file path is confirmed. |
| Architecture Consistency | WARN (RESOLVED) | ARCHITECTURE.md Component 5 described an in-memory fixture path contradicting SPECIFICATION.md AC-12. Resolved before delivery: ARCHITECTURE.md was corrected to match the spec (`typed_graph.rs` + `SqlxStore`). SPECIFICATION.md is authoritative (R-07). |
| Risk Completeness | PASS | 10 risks, 3 integration risks, 6 edge cases, 2 security risks in RISK-TEST-STRATEGY.md; all SCOPE-RISK-ASSESSMENT.md items addressed |

**Delivery-gate requirements from alignment resolution:**

- Gate-3b grep: `typed_graph.rs` test `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` must open a `SqlxStore` (not a bare `TypedRelationGraph::new()`).
- Gate-3b grep: `co_access_promotion_tick_tests.rs` must not contain the string `"no duplicate"` (stale T-BLR-08 assertion) or any odd-valued `count_co_access_edges` assertion.
- Gate-3b grep: `count_co_access_edges` assertion values must be even (0, 2, 4, 6, 10 ...). Any odd value indicates a missed blast-radius update.

---

## Test Blast Radius Summary

### Tests requiring modification (T-BLR-01 through T-BLR-08)

| Test | File | Current Count Assertion | Required Count After |
|------|------|------------------------|---------------------|
| T-BLR-01: `test_basic_promotion_new_qualifying_pair` | tick_tests.rs | no reverse (is_none) | reverse is_some; count == 2 |
| T-BLR-02: `test_inserted_edge_is_one_directional` | tick_tests.rs | count == 1, reverse is_none | rename to `_bidirectional`; count == 2, both is_some |
| T-BLR-03: `test_double_tick_idempotent` | tick_tests.rs | count == 1 after ticks | count == 2 after first tick, == 2 after second |
| T-BLR-04: `test_cap_selects_highest_count_pairs` | tick_tests.rs | count == 3 | count == 6 (3 pairs × 2) |
| T-BLR-05: `test_tied_counts_secondary_sort_stable` | tick_tests.rs | count == 3 | count == 6 (3 pairs × 2) |
| T-BLR-06: `test_cap_equals_qualifying_count` | tick_tests.rs | count == 5 | count == 10 (5 pairs × 2) |
| T-BLR-07: `test_cap_one_selects_highest_count` | tick_tests.rs | count == 1 | count == 2 (1 pair × 2) |
| T-BLR-08: `test_existing_edge_stale_weight_updated` | tick_tests.rs | count == 1 ("no duplicate") | count == 2; both weights == 1.0 |

### New tests to add

| Test | File | Coverage |
|------|------|---------|
| T-NEW-01: `test_bidirectional_edges_inserted_same_weight` | tick_tests.rs (Group I) | Both directions written with equal weight (FR-01, FR-02) |
| T-NEW-02: `test_bidirectional_both_directions_updated_when_drift_exceeds_delta` | tick_tests.rs (Group I) | Both directions converge on stale pre-seed (FR-03, FR-12) |
| T-NEW-03: `test_log_format_promoted_pairs_and_edges_inserted` | tick_tests.rs (Group I) | `promoted_pairs`/`edges_inserted`/`edges_updated` fields (FR-05, D2) |
| `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` | typed_graph.rs tests | Full GRAPH_EDGES → rebuild → PPR pipeline with reverse CoAccess edge (AC-12, D3) |
| MIG-U-01 through MIG-U-07 | tests/migration_v18_to_v19.rs | Schema version, back-fill correctness, idempotency, edge cases (AC-09, AC-10) |
