# SPECIFICATION: crt-035 — Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## Objective

crt-034 (ADR-006, Unimatrix #3830) wrote CoAccess edges as one-directional
(`source_id = entry_id_a (min)`, `target_id = entry_id_b (max)`) as a v1
intentional decision. The structural consequence is that PPR seeding the
higher-ID entry finds no path back to the lower-ID entry via CoAccess, halving
the effective coverage of the co-access signal. crt-035 makes the promotion tick
write both `(a→b)` and `(b→a)` going forward, and adds a v18→v19 migration that
back-fills the reverse edge for all existing forward-only CoAccess edges.

---

## Functional Requirements

**FR-01** — The `run_co_access_promotion_tick` function must write two rows per
qualifying pair: `(entry_id_a, entry_id_b, 'CoAccess')` and
`(entry_id_b, entry_id_a, 'CoAccess')`.
*Verification:* after one tick on a fresh pair, `count_co_access_edges = 2`.

**FR-02** — Both edges written in FR-01 must carry the same normalized weight
(`count / max_count`) derived from the same `co_access` row.
*Verification:* fetch both directions; assert weights equal within 1e-9.

**FR-03** — Both the forward and reverse edge are subject to the weight-update
logic: if the existing weight in `GRAPH_EDGES` differs from `new_weight` by more
than `CO_ACCESS_WEIGHT_UPDATE_DELTA` (0.1, strict greater-than), the edge is
updated. If delta is exactly 0.1, no update occurs.
*Verification:* pre-seed both directions at weight 0.5, run tick with
`new_weight = 1.0`; both edges must be updated to 1.0.

**FR-04** — INSERT for each direction uses `INSERT OR IGNORE`. When a direction
already exists (e.g., after back-fill), the insert is silently skipped; no
duplicate row is created and no error is raised.
*Verification:* pre-seed forward edge, run tick, assert edge count = 2
(not 3 or more).

**FR-05** — The `tracing::info!` summary at the end of the tick must use the
format: `promoted_pairs: N, edges_inserted: M, edges_updated: K`. `N` is the
count of qualifying pairs processed; `M` counts all individual edge inserts
(up to 2N on a fully-fresh graph); `K` counts all individual edge updates. The
fields must remain as structured key-value fields on the info! macro, not embedded
in the message string.
*Verification:* tracing_test captures confirm the three fields are present with
correct values.

**FR-06** — The promotion logic for each direction must be factored into a
module-private helper (e.g., `promote_one_direction`) that accepts `(store,
source_id, target_id, new_weight, bootstrap_only_flag)` and returns
`(inserted: bool, updated: bool)`. The main loop calls this helper twice per
pair. This is a structural requirement to keep `co_access_promotion_tick.rs`
under 500 lines.
*Verification:* `wc -l` on `co_access_promotion_tick.rs` must be <= 500 after
the change.

**FR-07** — The v18→v19 migration must insert the reverse edge for every row in
`GRAPH_EDGES` where `relation_type = 'CoAccess'` AND `source = 'co_access'`
AND the reverse direction does not already exist. The exact SQL is defined in
Constraint C-07. Both `created_by = 'bootstrap'` and `created_by = 'tick'` rows
are covered by the `source = 'co_access'` filter.
*Verification:* build a v18 DB with bootstrap-only forward edges; open; assert
that each forward edge has gained its reverse.

**FR-08** — `CURRENT_SCHEMA_VERSION` in `migration.rs` must be incremented from
18 to 19.
*Verification:* assert `unimatrix_store::migration::CURRENT_SCHEMA_VERSION == 19`
in the new integration test.

**FR-09** — A new integration test file `tests/migration_v18_to_v19.rs` must be
added to `crates/unimatrix-store/`, following the pattern of
`tests/migration_v17_to_v18.rs`. It must cover the test cases enumerated in
AC-10.

**FR-10** — Unimatrix entry #3830 (ADR-006) must be updated to confirm that the
follow-up contract is fulfilled: bidirectional writes are now the default, the
back-fill is bounded by `source = 'co_access'` on `GRAPH_EDGES`, and the v1
forward-edge layout was intentional.

**FR-11** — The tick's infallible contract must hold for both directions
independently: a failure writing the forward direction must not prevent the
reverse direction from being attempted, and vice versa. Both errors are logged at
`warn!` and the tick continues to the next pair.
*Verification:* existing write-failure tests (Groups F) must continue to pass
with bidirectional semantics.

**FR-12** — The weight update for the reverse edge uses the same `new_weight` as
the forward edge. When the forward edge has weight W1 and the reverse edge has
weight W2 (e.g., from a partial prior write), both are independently updated to
`new_weight` if their respective deltas exceed the threshold. This achieves
convergence on the next tick after any asymmetric state.
*Verification:* pre-seed forward at 0.5 and reverse at 0.2, run tick with
`new_weight = 1.0`; both must be updated to 1.0.

---

## Non-Functional Requirements

**NFR-01 — Infallible tick contract.** `run_co_access_promotion_tick` has return
type `async fn ... -> ()`. All errors on individual operations are logged at
`warn!`; no error propagates out of the function. This applies to all four SQL
operations per pair (insert-forward, insert-reverse, weight-fetch-forward,
weight-fetch-reverse, update-forward, update-reverse).

**NFR-02 — Idempotency.** The migration back-fill is idempotent via the
`UNIQUE(source_id, target_id, relation_type)` constraint plus the `NOT EXISTS`
guard. Re-opening the database after the v18→v19 migration must produce no
duplicates and no errors. The `INSERT OR IGNORE` path in the tick is similarly
idempotent.

**NFR-03 — 500-line file limit.** `co_access_promotion_tick.rs` must remain
under 500 lines after the change. The `promote_one_direction` helper (FR-06)
is the primary mechanism. Tests remain in the separate `_tests.rs` file.

**NFR-04 — Performance: 2x edge inserts per pair.** The tick processes up to
`max_co_access_promotion_per_tick` pairs; each pair now triggers up to 4 SQL
calls (2 INSERT + up to 2 UPDATE) vs. the prior 2. This is still pure SQL on
a single connection pool. No performance budget change is required — the
operation is I/O bound and the insert count doubles from N to at most 2N.

**NFR-05 — Migration performance.** The `NOT EXISTS` self-join in the back-fill
SQL (Constraint C-07) traverses `GRAPH_EDGES` once per qualifying row. The
existing indexes `idx_graph_edges_source_id`, `idx_graph_edges_target_id`, and
`idx_graph_edges_relation_type` must be confirmed to cover the NOT EXISTS
sub-select join. If a composite index on `(source_id, target_id, relation_type)`
does not exist, the one-time cost is acceptable but the architect should verify
this is not blocking on production-sized graphs.

**NFR-06 — No rayon pool.** Pure SQL; no CPU-bound work. No thread pool changes.

**NFR-07 — Weight symmetry eventual consistency.** Forward and reverse edge
weights may transiently differ if a partial tick write occurs. The next tick
converges both to the same `new_weight`. This is acceptable and consistent with
the infallible tick contract (NFR-01). Per pattern #3822, oscillating pairs
near threshold are handled per-direction independently; no atomic-pair transaction
is required.

**NFR-08 — No changes to `CO_ACCESS` table.** The `CHECK (entry_id_a < entry_id_b)`
constraint and the `co_access_key()` function in `schema.rs` are unchanged.
The reverse edge is written only to `GRAPH_EDGES`, never to `CO_ACCESS`.

**NFR-09 — Cycle detection unaffected.** Cycle detection uses a Supersedes-only
temp graph. CoAccess edges are excluded entirely (confirmed by Pattern #2429 and
ADR-006 #3830). Bidirectional CoAccess edges introduce no false-positive cycles.
No changes to cycle detection code are required.

---

## Acceptance Criteria

**AC-01** — After crt-035 ships, `run_co_access_promotion_tick` writes BOTH
`(entry_id_a, entry_id_b, 'CoAccess')` AND `(entry_id_b, entry_id_a, 'CoAccess')`
for each qualifying pair in a single tick run.
*Verification method:* new unit test in Group I — seed one pair, run one tick,
assert `count_co_access_edges = 2`, assert both `fetch_co_access_edge(a, b)` and
`fetch_co_access_edge(b, a)` return Some.

**AC-02** — Both forward and reverse edges written by the tick carry the same
normalized weight (`count / max_count`) from the same `co_access` row.
*Verification method:* Group I test — assert `edge_ab.weight == edge_ba.weight`
within 1e-9.

**AC-03** — Both forward and reverse edges are subject to weight update logic: if
the existing edge weight differs from `new_weight` by more than
`CO_ACCESS_WEIGHT_UPDATE_DELTA`, the edge is updated. Delta exactly equal to 0.1
is NOT updated.
*Verification method:* Group I test — pre-seed both directions at 0.5, run with
`new_weight = 1.0` (delta = 0.5); both must update. Pre-seed at 0.6, compute
`new_weight = 0.5` (delta = 0.1 exactly); neither updates.

**AC-04** — When one direction already exists and the other does not, `INSERT OR IGNORE`
silently skips the existing direction and inserts only the missing direction. No
duplicate rows are created.
*Verification method:* pre-seed forward edge; run tick; assert edge count = 2;
assert no duplicate (SELECT COUNT must be exactly 1 per direction).

**AC-05** — The `tracing::info!` summary at the end of the tick includes the
fields `promoted_pairs`, `edges_inserted`, and `edges_updated`. For a fully-fresh
single pair tick, `promoted_pairs = 1`, `edges_inserted = 2`, `edges_updated = 0`.
*Verification method:* `tracing_test::traced_test` — assert log fields match.

**AC-06** — The v18→v19 migration back-fills the reverse edge for every row in
`GRAPH_EDGES` where `relation_type = 'CoAccess'` AND `source = 'co_access'` AND
no reverse already exists. Covers both `created_by = 'bootstrap'` and
`created_by = 'tick'` edges.
*Verification method:* MIG-U-03 and MIG-U-04 in `tests/migration_v18_to_v19.rs`.

**AC-07** — The back-fill uses `INSERT OR IGNORE` and is idempotent. Re-running
the migration on an already-migrated database produces no duplicates and no errors.
*Verification method:* MIG-U-06 (idempotency test) in `tests/migration_v18_to_v19.rs`.

**AC-08** — The v18→v19 migration does NOT modify `Supersedes`, `Contradicts`, or
`Supports` edges. Only rows with `relation_type = 'CoAccess'` AND
`source = 'co_access'` are affected.
*Verification method:* MIG-U-05 in `tests/migration_v18_to_v19.rs` — pre-seed
non-CoAccess edges; after migration, assert they are unmodified in count and content.

**AC-09** — `CURRENT_SCHEMA_VERSION` is 19 in `migration.rs` after this change.
*Verification method:* MIG-U-01: `assert_eq!(CURRENT_SCHEMA_VERSION, 19)`.

**AC-10** — `tests/migration_v18_to_v19.rs` covers these cases:
- MIG-U-01: `CURRENT_SCHEMA_VERSION == 19`.
- MIG-U-02: Fresh DB creates schema v19 (no migration needed).
- MIG-U-03: v18→v19 inserts reverse edges for bootstrap-era (`created_by = 'bootstrap'`) forward CoAccess edges.
- MIG-U-04: v18→v19 inserts reverse edges for tick-era (`created_by = 'tick'`) forward CoAccess edges.
- MIG-U-05: Non-CoAccess edges are unmodified after migration.
- MIG-U-06: Idempotency — second open does not duplicate reverse edges.
- MIG-U-07: Empty `graph_edges` at migration time — back-fill is a no-op; no error.
*Verification method:* all seven tests must pass.

**AC-11** — Every test in `co_access_promotion_tick_tests.rs` that carries a
one-directional or edge-count assertion is updated to reflect bidirectional
semantics. The full blast radius is enumerated in the Test Blast Radius section
below. No test may contain a stale assertion that passes only because the reverse
direction is absent.

**AC-12** — PPR seeding the higher-ID entry in a co_access pair surfaces the
lower-ID entry via the reverse `CoAccess` edge. The test must use a real
SQLite-backed `TypedGraphState` (via `SqlxStore`), not an in-memory synthetic
fixture. The test:
1. Opens a real `SqlxStore` (tempfile-backed).
2. Inserts two entries (ID A < ID B).
3. Inserts a `CoAccess` edge `(B → A)` directly into `GRAPH_EDGES` (simulating
   what the tick writes as the reverse edge).
4. Calls `TypedGraphState::rebuild()` on the store.
5. Calls `personalized_pagerank` seeded at B.
6. Asserts that entry A has a non-zero PPR score.
*Placement:* extend existing `#[cfg(test)] mod tests` block in `typed_graph.rs`
(cumulative infrastructure rule, D3).
*Verification method:* new tokio test `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry`.

**AC-13** — Cycle detection behavior is unchanged. No false-positive cycles are
introduced by bidirectional CoAccess edges. The cycle detection temp graph uses
Supersedes edges only; CoAccess is excluded.
*Verification method:* all existing cycle detection tests pass; no new failure
in `test_build_typed_relation_graph_cycle_detection`.

**AC-14** — Unimatrix entry #3830 (ADR-006) is updated to confirm: (a) the
follow-up contract is fulfilled by crt-035, (b) bidirectional writes are now the
default tick behavior, (c) the v1 forward-edge layout was intentional and the
back-fill scope is bounded by `source = 'co_access'`.
*Verification method:* `context_correct` call on entry #3830 at delivery time.

---

## Test Blast Radius

### Tests in `co_access_promotion_tick_tests.rs` that require modification

The following 5 tests carry assertions that are directly broken by the
bidirectional change. Each is described with its current (before) assertion and
the required (after) assertion.

---

**T-BLR-01: `test_basic_promotion_new_qualifying_pair` (Group A, line 101)**

*Before:* Asserts `fetch_co_access_edge(&store, 2, 1).await.is_none()` with
comment "no reverse edge" (lines 122–125).

*After:* Remove the is_none assertion for the reverse direction. Add:
- `assert_eq!(count_co_access_edges(&store).await, 2, "both directions must be inserted")`.
- `assert!(fetch_co_access_edge(&store, 2, 1).await.is_some(), "reverse edge must exist")`.
- Assert that the reverse edge has the same weight and metadata fields as the forward edge.

*Why broken:* The existing assertion explicitly checks that the reverse edge is
absent. After crt-035 the reverse edge must exist.

---

**T-BLR-02: `test_inserted_edge_is_one_directional` (Group A, line 151)**

*Before:* Asserts `count_co_access_edges == 1`, `fetch_co_access_edge(5, 10).is_some()`,
and `fetch_co_access_edge(10, 5).is_none()` with comment "reverse edge must not be created".

*After:* This test must be replaced or inverted entirely. The replacement test
`test_inserted_edge_is_bidirectional` must assert:
- `count_co_access_edges == 2`.
- `fetch_co_access_edge(5, 10).is_some()` (forward still present).
- `fetch_co_access_edge(10, 5).is_some()` (reverse now required).
- Both edges have equal weight.

*Why broken:* The test name, comment, and all three assertions directly encode
the old one-directional contract. Every assertion will fail with crt-035.

---

**T-BLR-03: `test_double_tick_idempotent` (Group D, line 282)**

*Before:* Asserts `count_co_access_edges(&store).await == 1` after two ticks
(line 296, comment "exactly 1 row after 2 ticks").

*After:* Assert `count_co_access_edges(&store).await == 2` after the first tick
and still `== 2` after the second tick. The weight comparison after the second
tick must fetch both edges and assert neither changed.

*Why broken:* With bidirectional promotion the count after one tick is 2, not 1.
The count assertion on line 296 will fail.

---

**T-BLR-04: `test_cap_selects_highest_count_pairs` (Group B, line 172)**

*Before:* Asserts `count_co_access_edges(&store).await == 3` (cap=3, line 183,
comment "cap must be respected").

*After:* Assert `count_co_access_edges(&store).await == 6` (3 pairs × 2
directions). The cap still limits pairs processed, not edge rows written. The
individual pair presence assertions (`fetch_co_access_edge(10, 20).is_some()`,
`fetch_co_access_edge(9, 19).is_some()`, `fetch_co_access_edge(8, 18).is_some()`)
remain correct. Also assert the reverse of each selected pair is present.

*Why broken:* The `count == 3` assertion does not account for the reverse
direction. With cap=3 and bidirectional writes, the count is 6.

---

**T-BLR-05: `test_tied_counts_secondary_sort_stable` (Group H / E-02, line 565)**

*Before:* Asserts `count_co_access_edges(&store).await == 3` (cap=3, line 574,
comment "cap=3 respected").

*After:* Assert `count_co_access_edges(&store).await == 6` (3 pairs × 2
directions). The cap-respected semantic is unchanged; only the total row count
doubles.

*Why broken:* Same pattern as T-BLR-04 — the count assertion does not account
for bidirectional writes.

---

**T-BLR-06: `test_cap_equals_qualifying_count` (Group H / E-03, line 578)**

*Before:* Asserts `count_co_access_edges(&store).await == 5` (5 pairs, all
promoted, line 590, comment "all 5 pairs promoted").

*After:* Assert `count_co_access_edges(&store).await == 10` (5 pairs × 2
directions). The "all pairs promoted" semantics is preserved; only the row count
doubles.

*Why broken:* Same pattern as T-BLR-04 and T-BLR-05.

---

**T-BLR-07: `test_cap_one_selects_highest_count` (Group H / E-04, line 597)**

*Before:* Asserts `count_co_access_edges(&store).await == 1` (cap=1 with one
selected pair, line 606).

*After:* Assert `count_co_access_edges(&store).await == 2` (1 pair × 2 directions).
The `fetch_co_access_edge(1, 2).is_some()` assertion stays. Add
`fetch_co_access_edge(2, 1).is_some()`.

*Why broken:* Count is 1 under the current one-directional behavior. With
bidirectional writes it becomes 2.

---

### Tests that do NOT require modification

The following tests have no edge-count or directionality assertions that are
broken by the bidirectional change. They are listed explicitly to bound the blast
radius and prevent over-modification.

- `test_inserted_edge_metadata_all_four_fields` (Group A, line 130) — asserts
  metadata fields on `(1, 2)` edge only; does not assert total count or absence
  of reverse. No change needed. Note: after crt-035 a reverse edge also exists;
  this test's assertions remain valid.
- `test_existing_edge_stale_weight_updated` (Group C, line 213) — asserts
  `count_co_access_edges == 1` via comment "no duplicate". CAUTION: this
  assertion will break. See T-BLR-08.
- `test_existing_edge_current_weight_no_update` (Group C, line 233) — no count
  assertion on total edge rows; only checks weight on specific edges. No change needed.
- `test_weight_delta_exactly_at_boundary_no_update` (Group C / E-05, line 256) —
  no total count assertion. No change needed.
- `test_sub_threshold_pair_not_gc` (Group D, line 311) — asserts
  `fetch_co_access_edge(1, 2).is_some()` after promotion, not a count. No count
  regression.
- `test_empty_co_access_table_noop_late_tick` (Group E, line 341) — count == 0
  still correct (nothing to promote). No change needed.
- `test_all_below_threshold_noop_late_tick` (Group E, line 357) — count == 0
  still correct. No change needed.
- `test_early_tick_warn_when_qualifying_count_zero` (Group E, line 372) — log
  assertion only. No change needed.
- `test_late_tick_no_warn_empty_table` (Group E, line 388) — log assertion only.
  No change needed.
- `test_fully_promoted_table_no_warn` (Group E, line 403) — log assertion only;
  no count assertion on total CoAccess edges. No change needed.
- `test_write_failure_mid_batch_warn_and_continue` (Group F, line 431) — checks
  `fetch_co_access_edge(1, 3).is_some()` and `fetch_co_access_edge(1, 4).is_some()`;
  no total count. No change needed.
- `test_write_failure_info_log_always_fires` (Group F, line 459) — log assertion
  only. No change needed.
- `test_global_max_normalization_subquery_shape` (Group G, line 482) — weight
  assertions only, no count of total CoAccess edges. No change needed.
- `test_global_max_outside_capped_batch` (Group G, line 509) — weight assertions
  only. No change needed.
- `test_single_qualifying_pair_weight_one` (Group H / E-01, line 543) — weight
  assertion only (no count). Second tick still leaves correct weight. No change needed.
- `test_self_loop_pair_no_panic` (Group H / E-06, line 621) — verifies no-panic
  on empty qualifying set. No change needed.

---

**T-BLR-08 (additional): `test_existing_edge_stale_weight_updated` (Group C, line 213)**

*Before:* Asserts `count_co_access_edges(&store).await == 1` (line 224, comment
"no duplicate"). The test pre-seeds only the forward edge `(1, 2)` at weight 0.5.

*After:* After crt-035, the tick writes the reverse edge `(2, 1)` as a new
INSERT. Assert `count_co_access_edges == 2` (no duplicate, both directions).
Assert forward edge weight is updated to 1.0. Assert reverse edge is present at
weight 1.0 (newly inserted, delta from 0 is not relevant — it is a new row).

*Why broken:* The `count == 1` assertion ("no duplicate") was asserting that a
second tick does not create a duplicate. After crt-035, the correct count is 2:
forward (updated) + reverse (newly inserted). The count == 1 assertion fails.

---

### New Tests Required (Group I: Bidirectional Assertions)

These tests must be added to `co_access_promotion_tick_tests.rs` (Group I):

**T-NEW-01: `test_bidirectional_edges_inserted_same_weight`**
Seed one pair `(1, 2)` with count=5. Run one tick. Assert both
`fetch_co_access_edge(1, 2)` and `fetch_co_access_edge(2, 1)` are Some with equal
weight (1.0 each). Assert `count_co_access_edges == 2`.

**T-NEW-02: `test_bidirectional_both_directions_updated_when_drift_exceeds_delta`**
Pre-seed forward edge `(1, 2)` at weight 0.5, reverse edge `(2, 1)` at weight
0.2. Seed `co_access` pair with count=10 (single pair, `new_weight = 1.0`).
Run tick. Assert both edges updated to 1.0 (deltas 0.5 and 0.8 both exceed 0.1).

**T-NEW-03: `test_log_format_promoted_pairs_and_edges_inserted`**
Decorated with `#[tracing_test::traced_test]`. Seed two pairs. Run one tick.
Assert log contains `promoted_pairs: 2`, `edges_inserted: 4`, `edges_updated: 0`.

---

## Domain Models

### Entities

**CoAccess pair** — A row in the `CO_ACCESS` table. Represents a co-retrieval
event between two entries `(entry_id_a, entry_id_b)` where `entry_id_a < entry_id_b`
(enforced by `CHECK`). The pair is canonical; there is no separate
`(entry_id_b, entry_id_a)` row.

**Forward edge** — The `GRAPH_EDGES` row with `source_id = entry_id_a`,
`target_id = entry_id_b`, `relation_type = 'CoAccess'`, `source = 'co_access'`.
This is the direction that has always existed (bootstrap + crt-034 tick).

**Reverse edge** — The `GRAPH_EDGES` row with `source_id = entry_id_b`,
`target_id = entry_id_a`, `relation_type = 'CoAccess'`, `source = 'co_access'`.
This is the new direction introduced by crt-035.

**Bidirectional CoAccess edge pair** — The logical unit consisting of both the
forward edge and the reverse edge for the same `co_access` pair. They share the
same `weight` and `created_by` values.

**Promotion tick** — The recurring background function
`run_co_access_promotion_tick`. Reads qualifying `co_access` rows and promotes
them (or refreshes their weights) into `GRAPH_EDGES`. After crt-035: writes both
forward and reverse directions.

**Back-fill migration** — The v18→v19 one-time SQL migration that inserts the
reverse edge for every existing forward-only CoAccess row in `GRAPH_EDGES` where
`source = 'co_access'`. Bounded to rows already present at the time of the first
migration run.

**Bootstrap-era edge** — A CoAccess `GRAPH_EDGES` row with `created_by = 'bootstrap'`,
written by the v12→v13 migration bootstrap query. Forward-only before crt-035.

**Tick-era edge** — A CoAccess `GRAPH_EDGES` row with `created_by = 'tick'`,
written by the `run_co_access_promotion_tick` function. Forward-only between
crt-034 and crt-035.

**`created_by` provenance** — The `created_by` column on `GRAPH_EDGES` records
the origin of the _relationship_, not the code path that wrote the row. A
back-filled reverse edge copies `created_by` from the forward edge (D1). A
bootstrap reverse edge retains `created_by = 'bootstrap'`; a tick reverse edge
retains `created_by = 'tick'`.

**`source = 'co_access'`** — The filter token in `GRAPH_EDGES` that identifies
all CoAccess edges regardless of `created_by`. This is the authoritative scope
boundary for both the back-fill and ongoing tick operations.

### Relationships

- One `co_access` row → one bidirectional CoAccess edge pair in `GRAPH_EDGES`
  (after crt-035).
- `UNIQUE(source_id, target_id, relation_type)` distinguishes `(a, b, CoAccess)`
  from `(b, a, CoAccess)` as separate rows with no constraint conflict.
- `CO_ACCESS` table: immutable direction constraint (`entry_id_a < entry_id_b`).
  `GRAPH_EDGES` table: no such constraint; both `(a, b)` and `(b, a)` are valid rows.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| qualifying pair | A `co_access` row with `count >= CO_ACCESS_GRAPH_MIN_COUNT` (3) |
| `new_weight` | `count / max_count` for a qualifying pair, in `(0.0, 1.0]` |
| delta guard | The check `(new_weight - existing_weight).abs() > CO_ACCESS_WEIGHT_UPDATE_DELTA` |
| infallible tick | A tick function with return type `()` that logs errors and continues |
| idempotent back-fill | A migration that can be re-run without creating duplicates |
| PPR | Personalized PageRank — traverses `Direction::Outgoing` on positive edges |
| forward edge | The GRAPH_EDGES row `(entry_id_a → entry_id_b)` |
| reverse edge | The GRAPH_EDGES row `(entry_id_b → entry_id_a)` |
| `promote_one_direction` | Module-private helper that performs INSERT + conditional UPDATE for one direction |

---

## User Workflows

### Tick execution (recurring background)

1. Background tick loop calls `run_co_access_promotion_tick`.
2. Tick queries qualifying `co_access` pairs (count >= 3, capped).
3. For each qualifying pair:
   a. Call `promote_one_direction(forward)` → `(inserted_f, updated_f)`.
   b. Call `promote_one_direction(reverse)` → `(inserted_r, updated_r)`.
   c. Accumulate `inserted_count += inserted_f + inserted_r`.
   d. Accumulate `updated_count += updated_f + updated_r`.
4. Emit `tracing::info!` with `promoted_pairs`, `edges_inserted`, `edges_updated`.

### Database open with migration (one-time, per existing DB)

1. `SqlxStore::open` reads `schema_version` from `counters`.
2. If `schema_version < 19`, run back-fill SQL inside `run_main_migrations`.
3. Back-fill inserts reverse edges for all existing forward-only `CoAccess` rows.
4. Bump `schema_version` to 19.
5. All subsequent opens skip the migration (version guard).

### PPR retrieval (search hot path — no code change)

1. Search service reads `TypedGraphState` under read lock.
2. PPR traverses `Direction::Outgoing` CoAccess edges.
3. With bidirectional edges present:
   - Seed entry A → PPR finds B via edge `(A → B)` (unchanged).
   - Seed entry B → PPR finds A via edge `(B → A)` (newly present after crt-035).

---

## Constraints

**C-01** — `UNIQUE(source_id, target_id, relation_type)` is the sole idempotency
mechanism for `GRAPH_EDGES`. This constraint must not be changed. It treats
`(a, b, type)` and `(b, a, type)` as distinct rows, ensuring the reverse edge
can be inserted without conflict.

**C-02** — `co_access` CHECK enforces `entry_id_a < entry_id_b`. The tick reads
pairs in canonical order; the reverse direction is written to `GRAPH_EDGES` only.
No changes to `co_access` schema or `co_access_key()`.

**C-03** — File size limit: `co_access_promotion_tick.rs` must remain under 500
lines. The `promote_one_direction` helper (FR-06) is required to satisfy this constraint.

**C-04** — Both forward and reverse write operations per pair are individually
error-handled. A failure on one direction logs at `warn!` and continues; it does
not prevent the other direction from being attempted.

**C-05** — No rayon pool. No CPU-bound work.

**C-06** — Direct `write_pool` path only. `AnalyticsWrite::GraphEdge` cannot
express the conditional UPDATE semantics and must not be used (ADR-001).

**C-07** — Back-fill SQL must use the `NOT EXISTS` guard (D4). The exact SQL is:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
SELECT
    g.target_id     AS source_id,
    g.source_id     AS target_id,
    'CoAccess'      AS relation_type,
    g.weight        AS weight,
    strftime('%s','now') AS created_at,
    g.created_by    AS created_by,
    'co_access'     AS source,
    0               AS bootstrap_only
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

**C-08** — Schema version is 18 at crt-035 start. The migration block gates on
`current_version < 19`. The `CURRENT_SCHEMA_VERSION` constant is bumped to 19.

**C-09** — The `create_tables_if_needed()` path in `db.rs` creates `graph_edges`
with the existing DDL (unchanged) but inserts no data. A fresh database has
nothing to back-fill; no changes to `db.rs` are required.

**C-10** — The `co_access_promotion_tick.rs` doc comment on line 12 (`One-directional
edges v1 (ADR-006): ...`) must be updated to reflect the bidirectional behavior.

---

## Dependencies

| Dependency | Details |
|------------|---------|
| `unimatrix-store` `migration.rs` | Version bump 18→19, new migration block |
| `unimatrix-store` `tests/migration_v18_to_v19.rs` | New integration test file (pattern: `migration_v17_to_v18.rs`) |
| `unimatrix-server` `co_access_promotion_tick.rs` | Bidirectional writes, `promote_one_direction` helper, updated log format |
| `unimatrix-server` `co_access_promotion_tick_tests.rs` | 8 test updates + 3 new tests (Group I) |
| `unimatrix-server` `typed_graph.rs` | 1 new tokio test for AC-12 PPR behavioral regression |
| Unimatrix entry #3830 | `context_correct` call to update ADR-006 (FR-10, AC-14) |
| `sqlx` `query_as`, `query_scalar`, `query` | Existing dependency; no new crates |
| `tempfile`, `tracing_test` | Existing test dependencies |

---

## NOT in Scope

- Changes to the `CO_ACCESS` table schema or write paths.
- Changes to `co_access_key()` in `schema.rs`.
- Changes to PPR traversal logic in `graph_ppr.rs` or `TypedGraphState`.
- GC of sub-threshold or orphaned CoAccess edges (GH #409).
- Changes to `CO_ACCESS_WEIGHT_UPDATE_DELTA`, `PROMOTION_EARLY_RUN_WARN_TICKS`,
  or `max_co_access_promotion_per_tick` config semantics.
- Changes to cycle detection logic.
- Removal of `AnalyticsWrite::GraphEdge` or the analytics drain path.
- Back-fill of `Supersedes`, `Contradicts`, or `Supports` edges.
- New public constants, config fields, or re-exports beyond crt-034.
- Atomic-pair transactions (forward + reverse writes per pair are independent
  per the infallible tick contract).
- Changes to `db.rs` `create_tables_if_needed` path.
- Index additions to `GRAPH_EDGES` (existing indexes are sufficient for the
  back-fill NOT EXISTS join; this is confirmed by the architect, not spec-level).

---

## Resolved Questions

**OQ-01 — CLOSED**: `test_existing_edge_stale_weight_updated` (T-BLR-08) count changes
from 1 to 2. Confirmed correct: after crt-035 the tick inserts both the updated forward
edge AND a newly inserted reverse edge — total 2 rows. The "no duplicate" comment's intent
becomes "no duplicate per direction," which `INSERT OR IGNORE` + `UNIQUE` constraint
guarantees. Update assertion to `assert_eq!(count, 2)` and revise the comment accordingly.

**OQ-02 — CLOSED**: No weight floor. `weight = 0.0` on a back-filled reverse edge is
acceptable — PPR simply assigns zero traversal weight to that edge, which is the correct
behavior for a pair with zero co-access signal. Applying a floor would silently inject
signal that wasn't earned. Back-fill copies `weight` from forward edge verbatim.

**OQ-03 — DELIVERY GATE (R-01)**: The `UNIQUE(source_id, target_id, relation_type)`
constraint creates a composite B-tree index in SQLite that is expected to cover the NOT
EXISTS sub-join in the back-fill SQL. The delivery agent must run `EXPLAIN QUERY PLAN` on
the back-fill SQL against a representative database before merging. If SQLite does not use
the UNIQUE index for the inner select, a composite covering index must be added as part of
the v18→v19 migration DDL before merge. See R-01 in RISK-TEST-STRATEGY.md.

---

## Knowledge Stewardship

Queried: `mcp__unimatrix__context_briefing` — found relevant entries:
- #3889 (pattern: back-fill reverse edges for symmetric relation type using
  INSERT OR IGNORE with SELECT swapping source/target) — directly applicable,
  confirms the back-fill SQL approach.
- #3827 (crt-034 decision: promotion tick ordering) — confirms tick ordering context.
- #3830 (crt-034 ADR-006: edge directionality) — primary reference for this feature.
- #3822 (pattern: near-threshold oscillation behavior) — addressed in NFR-07.
