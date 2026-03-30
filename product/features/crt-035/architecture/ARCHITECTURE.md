# crt-035 Architecture: Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## System Overview

crt-035 fulfills the follow-up contract stated in ADR-006 (Unimatrix #3830). crt-034
introduced a recurring promotion tick that writes `co_access` pairs into `GRAPH_EDGES` as
`CoAccess`-typed edges, but wrote only one direction: `(entry_id_a, entry_id_b)` with
`entry_id_a < entry_id_b`. PPR traverses `Direction::Outgoing` only, so seeding the
higher-ID entry in any pair found no path back to the lower-ID peer. Half of all CoAccess
traversal paths were missing.

This feature closes that gap via two complementary changes:

1. **Tick change** — `run_co_access_promotion_tick` writes both `(a→b)` and `(b→a)` for
   each qualifying pair going forward.
2. **Migration** — a v18→v19 schema migration back-fills the missing reverse edge for every
   existing `CoAccess`-typed row in `GRAPH_EDGES` where `source = 'co_access'` (covers both
   `created_by = 'bootstrap'` edges from the v13 migration and `created_by = 'tick'` edges
   from the crt-034 tick).

No changes to PPR traversal logic, `TypedGraphState`, `CO_ACCESS` table, or cycle detection
are required.

## Component Breakdown

### Component 1: `co_access_promotion_tick.rs` (unimatrix-server)

**File:** `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`

**Responsibility:** Recurring background tick that promotes qualifying co_access pairs into
`GRAPH_EDGES`.

**Change:** Introduce a module-private async helper `promote_one_direction` that encapsulates
the three-step INSERT-fetch-UPDATE sequence for a single `(source_id, target_id)` pair.
The main loop calls this helper twice per row: once for `(entry_id_a, entry_id_b)` and once
for `(entry_id_b, entry_id_a)`.

The helper signature:

```rust
async fn promote_one_direction(
    store: &Store,
    source_id: i64,
    target_id: i64,
    new_weight: f64,
) -> (bool, bool) // (inserted, updated)
```

The main loop accumulates `inserted_count` and `updated_count` across both directions and
both are reported in the final `tracing::info!`. The `promoted_pairs` field is also emitted
(the count of `co_access` rows processed) to keep the business metric distinct from the
edge-write counts.

**Updated log format (D2):**

```
promoted_pairs: N, edges_inserted: M, edges_updated: K
```

Where `M` can be up to `2 * N` on a fresh graph and `K` can be up to `2 * N` when weights
drift on all pairs.

**Atomicity decision for SR-01:** Per-pair updates are NOT wrapped in a single transaction.
The infallible-tick constraint (SCOPE.md §Constraints) requires per-operation error handling:
a failure writing one direction must log at `warn!` and proceed without aborting the pair.
Both directions use the same `new_weight` (derived from the same `co_access` row), so if a
partial failure leaves one direction stale, the next tick will converge it: the INSERT will
be a no-op and the UPDATE path will detect the delta and correct it. This is eventual
consistency, not atomicity. SR-07 (oscillation risk) is accepted at low severity because
both directions converge to the same weight on the next tick.

### Component 2: `migration.rs` (unimatrix-store)

**File:** `crates/unimatrix-store/src/migration.rs`

**Responsibility:** One-time back-fill migration v18→v19.

**Change:**
- Bump `CURRENT_SCHEMA_VERSION` from 18 to 19.
- Add `if current_version < 19 { ... }` block in `run_main_migrations` with the back-fill
  SQL and a `schema_version` counter update to 19.
- No DDL changes. No new tables. No `ALTER TABLE`. Pure data migration.

The v18→v19 block runs inside the main transaction, consistent with how v17→v18 and earlier
pure-data migrations are structured. On rollback (any SQL error), the transaction reverts
the back-fill and the database stays at v18.

**Back-fill SQL (D4 — NOT EXISTS guard):**

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

`INSERT OR IGNORE` provides idempotency via `UNIQUE(source_id, target_id, relation_type)`.
The `NOT EXISTS` guard (D4) makes the intent explicit and makes re-runs efficient by
skipping pairs that already have their reverse edge.

**Index coverage for NOT EXISTS (SR-04):** The `UNIQUE(source_id, target_id, relation_type)`
constraint in SQLite is backed by a B-tree index on the `(source_id, target_id,
relation_type)` triple. The NOT EXISTS self-join lookups `rev.source_id = g.target_id AND
rev.target_id = g.source_id AND rev.relation_type = 'CoAccess'` match the leading columns
of that constraint index. No additional index is required; the UNIQUE constraint covers the
join. SR-04 is resolved.

**`db.rs` fresh-DB path:** `create_tables_if_needed()` creates the `graph_edges` table DDL
with no data. Since the back-fill is data-only, a fresh DB has zero `CoAccess` rows and the
back-fill is a no-op. No changes to `db.rs` are required (SCOPE.md open question 2 resolved:
no change needed).

### Component 3: `migration_v18_to_v19.rs` test (unimatrix-store)

**File:** `crates/unimatrix-store/tests/migration_v18_to_v19.rs`

**Responsibility:** Integration test for the v18→v19 migration.

Tests required (following pattern of `migration_v17_to_v18.rs`):
- `CURRENT_SCHEMA_VERSION == 19`
- Fresh DB creates schema v19
- v18→v19 back-fills reverse edges for bootstrap-era forward edges (`created_by='bootstrap'`)
- v18→v19 back-fills reverse edges for tick-era forward edges (`created_by='tick'`)
- Back-fill is idempotent (second open produces no duplicate rows)
- Non-CoAccess edges (`Supersedes`, `Contradicts`, `Supports`) are unaffected
- Empty `graph_edges` at migration time: back-fill is a no-op, no error

### Component 4: `co_access_promotion_tick_tests.rs` (unimatrix-server)

**File:** `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`

**Responsibility:** Unit tests for the tick. All 19 existing tests must be updated where they
assert unidirectional behavior. See SR-05 blast radius analysis below.

### Component 5: `typed_graph.rs` test block (unimatrix-server)

**File:** `crates/unimatrix-server/src/services/typed_graph.rs` (`#[cfg(test)] mod tests`)

**Responsibility:** AC-12 PPR regression test (D3). Add one `#[tokio::test]` to the existing
SQLite-backed test block. Uses a real `SqlxStore` (tempfile-backed), inserts a reverse CoAccess
edge `(B→A)` directly into `GRAPH_EDGES`, calls `TypedGraphState::rebuild()`, runs PPR seeded
at B, and asserts entry A has a non-zero score. This tests the full GRAPH_EDGES → rebuild →
PPR pipeline, not just the PPR algorithm in isolation.

## Component Interactions

```
CO_ACCESS table
    |
    | qualifying pairs (count >= 3)
    v
run_co_access_promotion_tick
    |-- promote_one_direction(a, b, weight)  --> GRAPH_EDGES (a→b CoAccess)
    |-- promote_one_direction(b, a, weight)  --> GRAPH_EDGES (b→a CoAccess)

GRAPH_EDGES (v18, forward-only CoAccess rows)
    |
    | v18→v19 migration back-fill
    v
GRAPH_EDGES (v19, bidirectional CoAccess rows)
    |
    | TypedRelationGraph::build_typed_relation_graph
    v
TypedGraphState (petgraph StableGraph)
    |
    | personalized_pagerank (Direction::Outgoing)
    v
PPR scores (both a→b and b→a traversal paths now active)
```

## Technology Decisions

See ADR-001 (this feature) for the atomicity decision. Technology stack is unchanged from
crt-034: SQLite via sqlx, tokio async, tracing for observability.

## Integration Points

### Tick → GRAPH_EDGES

The tick uses `store.write_pool_server()` directly (ADR-001 from crt-034, entry #3821).
`AnalyticsWrite::GraphEdge` is not used — it cannot express conditional UPDATE semantics.
This constraint is unchanged; `promote_one_direction` inherits it.

### Migration → GRAPH_EDGES

The migration uses the dedicated non-pooled `SqliteConnection` passed to
`migrate_if_needed`. All SQL in v18→v19 runs inside the main transaction (`txn`), consistent
with all prior data migrations.

### TypedGraphState ← GRAPH_EDGES

`build_typed_relation_graph` reads all non-bootstrap-only edges from `GRAPH_EDGES`. Reverse
CoAccess edges written by the tick or back-fill (`bootstrap_only = 0`) are included. No
code changes required in the graph layer.

### PPR ← TypedGraphState

`personalized_pagerank` traverses `Direction::Outgoing`. With both `(a→b)` and `(b→a)` now
present, seeding either endpoint reaches the other. No code changes required in PPR.

### Cycle Detection ← TypedGraphState

Cycle detection uses a Supersedes-only temp graph (Pattern #2429, ADR-006 #3830). CoAccess
edges — including the new reverse edges — are excluded from the cycle detection subgraph.
`test_cycle_detection_on_supersedes_subgraph_only` in `graph_tests.rs` already asserts
bidirectional CoAccess edges do not trigger false-positive cycle detection. No code changes
required.

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `run_co_access_promotion_tick` | `async fn(&Store, &InferenceConfig, u32)` | `co_access_promotion_tick.rs:74` |
| `promote_one_direction` (new) | `async fn(&Store, i64, i64, f64) -> (bool, bool)` | `co_access_promotion_tick.rs` (new, module-private) |
| `CURRENT_SCHEMA_VERSION` | `pub const u64 = 19` | `migration.rs:19` (bumped from 18) |
| `run_main_migrations` | `async fn(&mut Transaction<Sqlite>, u64, &Path) -> Result<()>` | `migration.rs:116` (unchanged signature) |
| `GRAPH_EDGES` UNIQUE constraint | `UNIQUE(source_id, target_id, relation_type)` | `db.rs:817`, `migration.rs:347` |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `const f64 = 0.1` | `co_access_promotion_tick.rs:33` (unchanged) |
| `tracing::info!` fields | `promoted_pairs: usize, edges_inserted: usize, edges_updated: usize` | `co_access_promotion_tick.rs` (updated from `inserted`/`updated`) |
| `personalized_pagerank` | `fn(&TypedRelationGraph, &HashMap<u64,f64>, f64, usize) -> HashMap<u64,f64>` | `graph_ppr.rs` (unchanged) |

## SR-05: Test Blast Radius — Complete Enumeration

All tests in `co_access_promotion_tick_tests.rs` that assert edge count or direction must
be updated. The complete list:

| Test | Current Assertion | Required Change |
|---|---|---|
| `test_basic_promotion_new_qualifying_pair` (line 101) | `fetch_co_access_edge(2,1).is_none()` — no reverse edge | Assert `fetch_co_access_edge(2,1).is_some()` and same weight |
| `test_inserted_edge_is_one_directional` (line 151) | `count == 1`, reverse must not exist | Replace: assert `count == 2`, both directions exist |
| `test_double_tick_idempotent` (line 282) | `count == 1` after first tick | Assert `count == 2` after first tick; `count == 2` after second tick |
| `test_cap_selects_highest_count_pairs` (line 172) | `count_co_access_edges == 3` | Assert `count == 6` (3 pairs × 2 directions) |
| `test_write_failure_mid_batch_warn_and_continue` (line 432) | Implicit via forward fetch only | Also verify reverse edges for (1,3) and (1,4) exist |
| `test_cap_equals_qualifying_count` (line 578) | `count == 5` | Assert `count == 10` (5 pairs × 2 directions) |
| `test_cap_one_selects_highest_count` (line 596) | `count == 1` | Assert `count == 2` (1 pair × 2 directions) |
| `test_tied_counts_secondary_sort_stable` (line 565) | `count == 3` | Assert `count == 6` (3 pairs × 2 directions) |
| `test_existing_edge_stale_weight_updated` (line 213) | `count == 1` (no duplicate) | Assert `count == 2` (both directions); reverse has same weight |
| `test_single_qualifying_pair_weight_one` (line 542) | Tests forward edge only | Also assert reverse edge `(2,1)` has `weight == 1.0` |

Tests NOT requiring changes (do not assert edge count or direction):
- `test_inserted_edge_metadata_all_four_fields` — checks field values on forward edge only (still valid; reverse test is separate)
- `test_existing_edge_current_weight_no_update` — checks forward weight unchanged; also add assertion that reverse is inserted
- `test_weight_delta_exactly_at_boundary_no_update` — delta boundary logic unchanged per direction
- `test_empty_co_access_table_noop_late_tick` — zero rows, count stays 0
- `test_all_below_threshold_noop_late_tick` — zero qualifying, count stays 0
- `test_early_tick_warn_when_qualifying_count_zero` — logging only
- `test_late_tick_no_warn_empty_table` — logging only
- `test_fully_promoted_table_no_warn` — logging only; pre-seeded forward edges, but the tick now also inserts reverses; `count` not asserted
- `test_write_failure_info_log_always_fires` — logging only
- `test_global_max_normalization_subquery_shape` — weight correctness, not direction count
- `test_global_max_outside_capped_batch` — weight correctness, not direction count
- `test_sub_threshold_pair_not_gc` — GC behavior, unaffected
- `test_self_loop_pair_no_panic` — panic guard, unaffected

New Group I (bidirectional assertions):
- `test_both_directions_written_same_weight` — seed pair `(5,10)`, run tick, assert both `(5→10)` and `(10→5)` exist with identical weight
- `test_both_directions_updated_on_weight_drift` — pre-seed both directions at `weight=0.5`, seed co_access with `count=10` (only pair → `weight=1.0`, delta=0.5 > 0.1), assert both directions updated to `1.0`
- `test_reverse_direction_fields` — verify `created_by='tick'`, `source='co_access'`, `bootstrap_only=0` on reverse edge

## SR-06: AC-12 Test Path

The AC-12 test must verify the full pipeline: a reverse CoAccess edge written to GRAPH_EDGES
is read by `TypedGraphState::rebuild()` and produces PPR scores when seeded at the high-ID
entry. The test is placed in the existing `#[cfg(test)] mod tests` block in
`crates/unimatrix-server/src/services/typed_graph.rs`, which already contains SQLite-backed
tokio tests using `SqlxStore` + `tempfile::TempDir` (see `test_rebuild_excludes_quarantined_entries`
for the established pattern).

Test name: `test_reverse_coaccess_high_id_to_low_id_ppr_regression`

Steps:
1. Open a real `SqlxStore` (tempfile-backed).
2. Insert two entries (IDs A < B).
3. Insert a `CoAccess` edge `(B → A)` directly into `GRAPH_EDGES` (simulating what the tick/back-fill writes).
4. Call `TypedGraphState::rebuild()` on the store.
5. Call `personalized_pagerank` seeded at B.
6. Assert entry A has a non-zero PPR score.

This tests the production path — not just the PPR algorithm in isolation — confirming the
GRAPH_EDGES → TypedGraphState::rebuild() → PPR integration works end-to-end.

## Open Questions

None. The two scope open questions from SCOPE.md are resolved:

- **OQ-1 (weight symmetry on update):** Each direction is updated independently to the same
  `new_weight`. Since `new_weight` is derived from the same `co_access` row for both
  directions, independent updates converge both to the same value. Confirmed correct semantic.

- **OQ-2 (`db.rs` fresh path):** `create_tables_if_needed()` creates `graph_edges` DDL with
  no rows. The back-fill is data-only and selects zero rows on a fresh DB. No change to
  `db.rs` needed.
