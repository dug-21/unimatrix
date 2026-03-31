# crt-035: Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## Problem Statement

crt-034 (PR #457) implemented the recurring promotion tick that promotes `co_access` pairs
from `CO_ACCESS` into `GRAPH_EDGES`. Per ADR-006 (Unimatrix #3830), v1 intentionally wrote
edges in one direction only (`source_id = entry_id_a (min)`, `target_id = entry_id_b (max)`),
matching the bootstrap shape to avoid PPR asymmetry.

The documented consequence is structural: PPR traverses `Direction::Outgoing` exclusively,
so seeding the higher-ID entry in any co_access pair finds no path back to the lower-ID
entry via CoAccess edges. Half of all CoAccess traversal paths are missing. For any
symmetric co-access signal (two entries co-retrieved together should reinforce each other
symmetrically), this means PPR graph walks starting from the max-ID node never surface
co-accessed peers. This halves the effective coverage of the CoAccess signal in PPR-driven
retrieval.

The back-fill requirement is equally critical: all co_access edges written before crt-035
ships — both from the v13 bootstrap migration and from the crt-034 recurring tick — are
forward-only. Without a one-time back-fill, the graph remains asymmetric for all pre-existing
pairs indefinitely.

Issue #459 requires:
1. Forward-going change: the promotion tick writes BOTH `(a→b)` AND `(b→a)` for each
   qualifying pair going forward.
2. One-time back-fill: a schema migration that inserts the reverse edge for all existing
   bootstrap-era and tick-era forward-only CoAccess edges.
3. ADR-006 (Unimatrix #3830) must be referenced and updated to confirm the follow-up
   contract is fulfilled and forward-edge layout was intentional.

## Goals

1. Modify `run_co_access_promotion_tick` to write both `(entry_id_a, entry_id_b)` and
   `(entry_id_b, entry_id_a)` as distinct `CoAccess`-typed edges in `GRAPH_EDGES` for each
   qualifying pair going forward.
2. Add a one-time schema migration (v18→v19) that back-fills the missing reverse edge for
   every existing forward-only CoAccess edge whose `source = 'co_access'` in `GRAPH_EDGES`
   (covers both bootstrap-era and crt-034 tick-era edges).
3. Confirm via a Unimatrix knowledge update to entry #3830 that ADR-006's follow-up contract
   is fulfilled: forward-edge layout was intentional, back-fill scope is bounded by
   `source = 'co_access'` on `GRAPH_EDGES`, and cycle detection is unaffected.

## Non-Goals

- No changes to the `CO_ACCESS` table schema or co-access write paths.
- No changes to `co_access_key()` in `schema.rs` (which orders pairs as `(min, max)` —
  this ordering is used by the tick to derive `entry_id_a < entry_id_b`, and remains correct).
- No changes to PPR traversal logic, `TypedGraphState`, or downstream search scoring.
- No GC of sub-threshold or orphaned CoAccess edges — that belongs to GH #409.
- No changes to weight update semantics (delta guard, normalization) — those are unchanged.
- No changes to the `CO_ACCESS_WEIGHT_UPDATE_DELTA`, `PROMOTION_EARLY_RUN_WARN_TICKS`, or
  `max_co_access_promotion_per_tick` config semantics.
- No changes to cycle detection logic — ADR-006 (#3830) already documents that CoAccess
  edges are excluded from the Supersedes-only cycle detection subgraph; no code changes needed.
- No removal of `AnalyticsWrite::GraphEdge` or the analytics drain path.
- The back-fill does NOT include `Supersedes`, `Contradicts`, or `Supports` edges — only
  `relation_type = 'CoAccess'` edges with `source = 'co_access'` are affected.
- No new public constants, config fields, or re-exports beyond what crt-034 already ships.

## Background Research

### ADR-006 — v1 Contract and Follow-up Requirements (Unimatrix #3830)

ADR-006 documents three explicit follow-up requirements for crt-035:
1. Write `(entry_id_b, entry_id_a, 'CoAccess')` — distinct from `(entry_id_a, entry_id_b,
   'CoAccess')` under `UNIQUE(source_id, target_id, relation_type)`. No UNIQUE conflict.
2. Back-fill ALL bootstrap-era pairs (`source = 'co_access'`, `created_by = 'bootstrap'` in
   `GRAPH_EDGES`) that have only one direction. Bootstrap pairs are identifiable by these
   fields.
3. Reference ADR-006 to confirm forward-edge layout was intentional.

ADR-006 also explicitly notes: **cycle detection is not broken by bidirectional CoAccess
edges.** Unimatrix cycle detection uses a Supersedes-only temp graph; CoAccess edges are
excluded entirely (confirmed by Pattern #2429). No changes to cycle detection are required.

### GRAPH_EDGES Schema

Table: `graph_edges`
- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `source_id INTEGER NOT NULL`
- `target_id INTEGER NOT NULL`
- `relation_type TEXT NOT NULL` — e.g. `'CoAccess'`, `'Supports'`, `'Supersedes'`
- `weight REAL NOT NULL DEFAULT 1.0`
- `created_at INTEGER NOT NULL`
- `created_by TEXT NOT NULL DEFAULT ''` — `'bootstrap'` for v13 migration, `'tick'` for crt-034 tick
- `source TEXT NOT NULL DEFAULT ''` — `'co_access'` for all CoAccess edges
- `bootstrap_only INTEGER NOT NULL DEFAULT 0`
- `metadata TEXT DEFAULT NULL`
- `UNIQUE(source_id, target_id, relation_type)`

The `UNIQUE(source_id, target_id, relation_type)` constraint treats `(a, b, type)` and
`(b, a, type)` as distinct rows — reverse edge insertion is safe and will not collide with
the existing forward edge for the same pair.

### Existing Forward-Only Promotion Tick (crt-034)

`run_co_access_promotion_tick` in
`crates/unimatrix-server/src/services/co_access_promotion_tick.rs` (line 172–183) writes:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
VALUES (?1, ?2, 'CoAccess', ?3, strftime('%s','now'), 'tick', ?4, 0)
```

with `?1 = row.entry_id_a`, `?2 = row.entry_id_b`. The `CO_ACCESS` table enforces
`CHECK (entry_id_a < entry_id_b)`, so `entry_id_a < entry_id_b` is guaranteed by the
schema for every pair.

The weight update path (Step B / Step C) fetches and updates the existing edge via
`(source_id = entry_id_a, target_id = entry_id_b, relation_type = 'CoAccess')`. The
bidirectional change requires the same UPDATE logic to also cover the reverse edge
`(source_id = entry_id_b, target_id = entry_id_a, relation_type = 'CoAccess')`.

Existing tests in `co_access_promotion_tick_tests.rs` include `test_inserted_edge_is_one_directional`
(Group A, line 151) which explicitly asserts `reverse edge must not be created` — this
test must be inverted/removed and replaced with a bidirectional assertion.

### Bootstrap SQL in migration.rs (v12→v13)

The v13 bootstrap (migration.rs lines 412–446) wrote edges as:

```sql
INSERT OR IGNORE INTO graph_edges ...
SELECT
    entry_id_a AS source_id,
    entry_id_b AS target_id,
    'CoAccess'  AS relation_type,
    ...
    'bootstrap' AS created_by,
    'co_access' AS source,
FROM co_access WHERE count >= 3
```

This wrote forward-only edges. The `created_by = 'bootstrap'` and `source = 'co_access'`
fields uniquely identify these rows for the back-fill. The back-fill query does NOT need to
filter by `created_by` — filtering by `source = 'co_access'` alone covers both bootstrap
and tick-era forward edges (both use `source = 'co_access'`). Filtering also by `NOT EXISTS`
the reverse direction catches any pair where only one direction is present.

### Migration Framework

The migration framework in `migration.rs` uses a main transaction
(`run_main_migrations`) with `if current_version < N` guards. The current version is 18
(set by crt-033). crt-035 requires v18→v19.

Pattern (from v17→v18, v16→v17, etc.):
1. Add `if current_version < 19 { ... }` block in `run_main_migrations`.
2. Back-fill SQL uses `INSERT OR IGNORE` (idempotent via UNIQUE constraint).
3. Bump `CURRENT_SCHEMA_VERSION` from 18 to 19.
4. The final `INSERT OR REPLACE INTO counters` at the end of `run_main_migrations` updates
   to `CURRENT_SCHEMA_VERSION` — it runs unconditionally and covers all migration paths.
5. A new integration test file `tests/migration_v18_to_v19.rs` following the established
   pattern (build a v18-shaped DB, open, assert back-fill results, check idempotency).

The back-fill SQL is pure `INSERT OR IGNORE` — no new DDL, no table creation, no ALTER
TABLE. The migration version bump is the only required change to `migration.rs` constants.

### Back-fill SQL Design

The back-fill must insert the reverse of every existing `source = 'co_access'` CoAccess
edge that does not already have its reverse present:

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
```

`INSERT OR IGNORE` handles idempotency: if a reverse edge already exists (e.g., on a
second migration run or if crt-035 tick has already inserted it), the insert is silently
skipped. The `weight` is copied from the forward edge so the PPR graph sees consistent
weights in both directions. `created_by` is preserved from the original edge, allowing
future audits to distinguish bootstrap-back-filled from tick-back-filled reverse edges.

### Tick Changes: Bidirectional Insert

The promotion tick loop must write two rows per pair instead of one. The INSERT OR IGNORE
path already handles the case where a row exists (via the weight-update branch). The
structural change is:

For each qualifying pair `(entry_id_a, entry_id_b)`:
- Step A-forward: `INSERT OR IGNORE ... (entry_id_a, entry_id_b, 'CoAccess', ...)`
- Step A-reverse: `INSERT OR IGNORE ... (entry_id_b, entry_id_a, 'CoAccess', ...)`
- Steps B/C for forward: weight fetch + update if delta > threshold
- Steps B/C for reverse: weight fetch + update if delta > threshold

The weight for both directions is the same normalized value (`count / max_count`).

### PPR Traversal Direction

PPR traverses `Direction::Outgoing` (ADR-003, crt-030). With bidirectional edges:
- Seeding `entry_id_a` reaches `entry_id_b` via `(a→b)` CoAccess edge (was already working).
- Seeding `entry_id_b` reaches `entry_id_a` via `(b→a)` CoAccess edge (newly added).

This is the correct semantic: co-access is symmetric, so both seeds should surface each
other. No PPR traversal code changes are needed.

### Existing Test Coverage

`co_access_promotion_tick_tests.rs` currently has 19 tests across Groups A–H. Relevant
changes needed:
- `test_inserted_edge_is_one_directional` (Group A, asserts no reverse edge) must be
  replaced by a bidirectional assertion test.
- `test_basic_promotion_new_qualifying_pair` checks `count_co_access_edges == 1` implicitly
  via `fetch_co_access_edge(1,2)` — this must now assert 2 edges exist (forward + reverse).
- `test_double_tick_idempotent` must assert 2 edges after first tick (not 1), and 2 edges
  after second tick.
- New Group I: Bidirectional tests verifying both directions written, same weight, both
  update when drift exceeds delta.

### Weight Symmetry Considerations

Both forward and reverse edges must carry the same weight. The weight formula
(`count / max_count`) is the same for both directions since it comes from the same
`co_access` row. When updating an existing edge (Steps B/C), both the forward and reverse
edges should be updated to the same new weight to maintain consistency.

### Tick Insertion Count

The `inserted_count` and `updated_count` summaries in the final `tracing::info!` must
count both forward and reverse writes. This means up to `2 * qualifying_count` inserts on
a fully-fresh graph, and similarly for updates. The log semantics should clarify "edge
writes" (not "pair promotions") to avoid confusion.

## Proposed Approach

### Part 1: Forward-going tick change

Modify `run_co_access_promotion_tick` in
`crates/unimatrix-server/src/services/co_access_promotion_tick.rs`:

For each pair in the qualifying batch, perform the two-step INSERT-then-check-UPDATE
procedure TWICE — once for `(entry_id_a, entry_id_b)` and once for `(entry_id_b,
entry_id_a)`. Both directions use the same computed `new_weight`.

Refactor the per-direction logic into a helper (module-private) to avoid duplication
and keep the file under the 500-line limit. A helper
`async fn promote_one_direction(store, source, target, new_weight) -> (bool, bool)` that
returns `(inserted, updated)` keeps the main loop readable.

### Part 2: One-time back-fill migration (v18→v19)

In `migration.rs`:
1. Bump `CURRENT_SCHEMA_VERSION` from 18 to 19.
2. Add `if current_version < 19 { ... }` block with the back-fill INSERT OR IGNORE SQL.
3. The version counter update at the end of `run_main_migrations` handles the bump.

Add integration test file `tests/migration_v18_to_v19.rs` following the pattern of
`tests/migration_v17_to_v18.rs`, testing:
- `CURRENT_SCHEMA_VERSION == 19`
- Fresh DB creates schema v19
- v18→v19 back-fill inserts reverse edges for bootstrap-era forward edges
- v18→v19 back-fill inserts reverse edges for tick-era forward edges
- Back-fill is idempotent (second open does not duplicate rows)
- Pre-existing non-CoAccess edges are unaffected
- Empty `graph_edges` at migration time: back-fill is a no-op, no error

### Part 3: ADR-006 update

Update Unimatrix entry #3830 to note that crt-035 fulfills the ADR-006 follow-up
contract: bidirectional writes are now the default, back-fill is complete, and the
forward-edge-only v1 layout was intentional for the reason documented (PPR consistency).

## Acceptance Criteria

- AC-01: After crt-035 ships, `run_co_access_promotion_tick` writes BOTH
  `(entry_id_a, entry_id_b, 'CoAccess')` AND `(entry_id_b, entry_id_a, 'CoAccess')` for
  each qualifying `co_access` pair in a single tick run.
- AC-02: Both forward and reverse edges written by the tick carry the same normalized
  weight (`count / max_count`) from the same `co_access` row.
- AC-03: Both forward and reverse edges are subject to the weight update logic: if the
  existing weight in `GRAPH_EDGES` differs from the new normalized weight by more than
  `CO_ACCESS_WEIGHT_UPDATE_DELTA` (0.1), the edge is updated.
- AC-04: Inserting a bidirectional pair where one direction already exists (e.g., after
  the back-fill) does not create duplicates — `INSERT OR IGNORE` is used; the
  `UNIQUE(source_id, target_id, relation_type)` constraint silently skips the duplicate.
- AC-05: The `tracing::info!` summary at end of tick reflects total edge writes (inserts
  + updates) across both directions.
- AC-06: The v18→v19 migration back-fills the reverse edge for every row in `GRAPH_EDGES`
  where `relation_type = 'CoAccess'` AND `source = 'co_access'` (covers both
  `created_by = 'bootstrap'` and `created_by = 'tick'` edges).
- AC-07: The back-fill uses `INSERT OR IGNORE` and is idempotent — re-running the
  migration on an already-migrated database produces no duplicates and no errors.
- AC-08: The v18→v19 migration does NOT modify `Supersedes`, `Contradicts`, or `Supports`
  edges; only `CoAccess` + `source = 'co_access'` rows are affected.
- AC-09: `CURRENT_SCHEMA_VERSION` is incremented from 18 to 19 in `migration.rs`.
- AC-10: A new integration test file `tests/migration_v18_to_v19.rs` verifies: (a) schema
  version bumped, (b) forward-only bootstrap edges gain their reverse, (c) forward-only
  tick edges gain their reverse, (d) idempotency, (e) non-CoAccess edges untouched.
- AC-11: The `test_inserted_edge_is_one_directional` test (and any other test that asserts
  no reverse edge is created) is updated to assert bidirectional behavior.
- AC-12: PPR seeding the higher-ID entry in a co_access pair now surfaces the lower-ID
  entry via the reverse `CoAccess` edge (behavioral regression test via
  `TypedGraphState` or graph traversal test).
- AC-13: The cycle detection behavior is unchanged — no false-positive cycles are
  introduced by bidirectional CoAccess edges.
- AC-14: Unimatrix entry #3830 (ADR-006) is updated to confirm the follow-up contract is
  fulfilled and the forward-edge v1 layout was intentional.

## Constraints

- **UNIQUE constraint is the idempotency mechanism.** `UNIQUE(source_id, target_id,
  relation_type)` treats `(a, b, type)` and `(b, a, type)` as distinct rows. All inserts
  use `INSERT OR IGNORE`. This constraint must not be changed.
- **`co_access` CHECK enforces `entry_id_a < entry_id_b`.** The tick always reads pairs
  in canonical order (`entry_id_a < entry_id_b`). The reverse direction edge
  `(entry_id_b, entry_id_a)` is written to `graph_edges`, not to `co_access`. No schema
  changes needed.
- **File size limit (500 lines).** `co_access_promotion_tick.rs` must stay under 500 lines.
  The bidirectional loop requires a refactor (helper function or restructuring) to avoid
  doubling the inline code. Tests remain in the separate `_tests.rs` file.
- **Infallible tick contract.** Both forward and reverse write operations must be
  individually error-handled: a failure writing one direction logs at `warn!` and proceeds
  to the next operation without propagating.
- **No rayon pool.** Pure SQL; no CPU-bound work.
- **Direct write_pool path.** UPDATE semantics remain required; `AnalyticsWrite::GraphEdge`
  remains unused for this tick.
- **Schema version is 18 at crt-035 start.** The migration must gate on
  `current_version < 19` and bump to 19. Any `create_tables_if_needed()` fresh-DB path
  in `db.rs` must be verified to not require changes (the `graph_edges` table DDL is
  unchanged; the back-fill is a data migration, not a schema change).
- **Current test count: 19 unit tests in `co_access_promotion_tick_tests.rs`** — all
  must pass after the bidirectional change. At minimum one test must be updated
  (the explicit one-directional assertion). The migration integration test suite has 16
  tests across all migration files; a new file adds to this count.

## Decisions (approved by human, Phase 1)

**D1 — back-fill `created_by`**: Copy from the forward edge (`'bootstrap'` or `'tick'`).
`created_by` tracks the origin of the relationship, not the code path that wrote the row.
A bootstrap reverse edge is a bootstrap relationship. Using `'back-fill'` would create
asymmetric provenance for edges representing the same pair. No new sentinel value.

**D2 — tick log format**: Report both metrics:
`promoted_pairs: N, edges_inserted: M, edges_updated: K`
`max_co_access_promotion_per_tick` is pair-based (operator-visible unit); M can be up to 2N.
Keeps the business metric and the debuggable edge count distinct.

**D3 — AC-12 test placement**: Extend existing `TypedGraphState` tests (cumulative
infrastructure rule). Add a scenario: insert a `CoAccess` edge `(B→A)`, rebuild, run PPR
seeded at `entry_id_b`, assert `entry_id_a` appears with non-zero score.

**D4 — back-fill SQL must include NOT EXISTS guard**: `INSERT OR IGNORE` alone is idempotent
via the UNIQUE constraint but scans all CoAccess edges on every re-run. The WHERE clause
must include a `NOT EXISTS` guard to filter out pairs that already have their reverse edge,
making re-runs efficient and the intent explicit:

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

## Open Questions

1. **Weight symmetry on update path**: when the forward edge has weight W1 and the reverse
   edge has weight W2 (from a previous tick where W2 != W1 due to a race or partial write),
   should the update bring both to the same `new_weight`? The proposed approach (update each
   direction independently to `new_weight`) achieves this — confirm this is the correct
   semantic.

2. **`db.rs` `create_tables_if_needed` path**: the fresh-DB path calls `create_tables_if_needed()`
   which creates `graph_edges` with the correct DDL but inserts no data. Since the back-fill
   is data-only, a fresh DB has nothing to back-fill. Confirm that `create_tables_if_needed`
   does NOT need to be updated for this change (expected: no change needed, but verify).

## Tracking

https://github.com/dug-21/unimatrix/issues/460
