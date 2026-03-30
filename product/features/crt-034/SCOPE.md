# crt-034: Recurring co_access → GRAPH_EDGES Promotion Tick

## Problem Statement

The v12→v13 migration bootstrapped all `co_access` pairs with `count >= 3` into
`GRAPH_EDGES` as `CoAccess`-typed edges. Since that one-time run, every new co-access
pair written to the `co_access` table is permanently invisible to Personalized PageRank
(PPR). PPR reads the in-memory `TypedRelationGraph`, which is rebuilt from `GRAPH_EDGES`
every tick. With `CoAccess` edges frozen at bootstrap time, the PPR graph does not
reflect real access patterns that have accumulated since schema v13.

This gap undermines the self-reinforcing flywheel: queries build co-access signal, but
that signal never reaches PPR, so graph-driven retrieval stays anchored to the initial
bootstrap snapshot forever.

The fix must ship before GH #409 (intelligence-driven retention). If #409 prunes stale
`co_access` rows before this promotion runs, signal crossing the threshold is lost permanently.

## Goals

1. Add a recurring background tick step that promotes qualifying `co_access` pairs
   (count >= threshold) into `GRAPH_EDGES` as `CoAccess` edges.
2. Update the `weight` of existing `CoAccess` edges when the normalized weight has
   drifted by more than a configurable delta (default 0.1).
3. Cap promotions per tick with a new `max_co_access_promotion_per_tick` config field,
   mirroring the `max_graph_inference_per_tick` throttle pattern.
4. Add the new `max_co_access_promotion_per_tick` field to `InferenceConfig` with serde
   default, range validation, and project-level config merge.
5. Ensure correct tick ordering: promotion runs AFTER `maintenance_tick` (which calls
   `cleanup_stale_co_access`) and BEFORE `TypedGraphState::rebuild()`.
6. Expose a named constant `EDGE_SOURCE_CO_ACCESS = "co_access"` in `unimatrix-store`
   parallel to `EDGE_SOURCE_NLI`.

## Non-Goals

- No changes to the v12→v13 migration. The bootstrap SQL is not touched.
- No removal of the `AnalyticsWrite::GraphEdge` analytics queue path. The new promotion
  tick uses the direct `write_pool_server()` path (as NLI does) because it needs conditional
  UPDATE semantics and the analytics drain only supports `INSERT OR IGNORE`.
- No changes to `co_access` write paths or the co-access staleness cleanup logic.
- No new schema migration. `GRAPH_EDGES` table schema is unchanged; no new columns needed.
- No changes to PPR, `TypedGraphState`, or downstream search scoring.
- No changes to `w_coac` or the fusion scoring formula (crt-032 already zeroed `w_coac`).
- No promotion of `bootstrap_only = 1` edges; those are already handled by crt-023.
- No GC of `CoAccess` edges whose `co_access` pairs have since dropped below threshold.
  That belongs to #409.

## Background Research

### Bootstrap Pattern (migration.rs lines 412–446)

The v13 bootstrap SQL is the canonical template:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
SELECT
    entry_id_a AS source_id,
    entry_id_b AS target_id,
    'CoAccess'  AS relation_type,
    COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0) AS weight,
    strftime('%s','now') AS created_at,
    'bootstrap' AS created_by,
    'co_access' AS source,
    0           AS bootstrap_only
FROM co_access
WHERE count >= 3
```

`INSERT OR IGNORE` provides idempotency via `UNIQUE(source_id, target_id, relation_type)`.
The window function `MAX(count) OVER ()` normalizes weight across the promoted batch.

The tick promotion differs from the bootstrap in two ways:
1. It must also UPDATE existing edges whose weight has drifted (the bootstrap ran once so
   there was nothing to update).
2. It must be capped per tick (the bootstrap ran unbounded in a migration transaction).

### Tick Ordering in background.rs (run_single_tick)

The relevant existing sequence (confirmed by reading background.rs lines 447–784):

```
1. maintenance_tick()                   -- calls cleanup_stale_co_access() inside StatusService
2. GRAPH_EDGES orphaned-edge compaction -- DELETE WHERE endpoint not in entries
3. TypedGraphState::rebuild()           -- reads GRAPH_EDGES, builds in-memory graph
4. PhaseFreqTable::rebuild()
5. Contradiction scan (every N ticks)
6. extraction_tick()
7. maybe_run_bootstrap_promotion()      -- crt-023: one-shot NLI bootstrap, idempotent
8. run_graph_inference_tick()           -- crt-029: recurring NLI Supports edge inference
```

The new promotion step must be inserted between step 1 and step 3 — specifically after
`maintenance_tick` (so stale co_access pairs are already cleaned) and after the
orphaned-edge compaction (step 2), but before `TypedGraphState::rebuild()` (step 3) so the
freshly promoted edges are immediately visible to PPR in the same tick cycle.

Proposed insertion point: after step 2 (orphaned-edge compaction), before step 3 (rebuild).

### GRAPH_EDGES Schema

Table: `graph_edges`
- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `source_id INTEGER NOT NULL`
- `target_id INTEGER NOT NULL`
- `relation_type TEXT NOT NULL`  — e.g. `'CoAccess'`, `'Supports'`, `'Supersedes'`, `'Contradicts'`
- `weight REAL NOT NULL DEFAULT 1.0`
- `created_at INTEGER NOT NULL`
- `created_by TEXT NOT NULL DEFAULT ''`
- `source TEXT NOT NULL DEFAULT ''`  — e.g. `'co_access'`, `'nli'`, `'entries.supersedes'`
- `bootstrap_only INTEGER NOT NULL DEFAULT 0`
- `metadata TEXT DEFAULT NULL`
- `UNIQUE(source_id, target_id, relation_type)`

Indexes: `idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type`.

No existing UPDATE path for graph edge weights. NLI promotion uses
`promote_bootstrap_edge()` (DELETE + INSERT OR IGNORE) but that is for bootstrap→NLI
upgrades. The co_access weight update is simpler: `UPDATE graph_edges SET weight = ?`
where the delta exceeds threshold.

### Throttle Pattern (max_graph_inference_per_tick)

`InferenceConfig.max_graph_inference_per_tick` (default 100, range [1, 1000]) is defined
in `config.rs` with `#[serde(default = "default_max_graph_inference_per_tick")]`. Validation
in `validate()` enforces the range. Config merge follows the project-level-overrides-global
pattern used by all `InferenceConfig` fields.

The new `max_co_access_promotion_per_tick` field follows the exact same pattern.

### Constants

- `CO_ACCESS_BOOTSTRAP_MIN_COUNT: i64 = 3` — defined in `migration.rs` (file-private).
  This constant is used only in the migration. The tick must either re-export it or define
  its own equivalent constant in the promotion module. Exposing it from `unimatrix-store`
  as `pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` avoids duplication and is the
  preferred approach (issue #455 suggests `CO_ACCESS_GRAPH_MIN_COUNT` as the public name).

- `EDGE_SOURCE_NLI: &str = "nli"` — defined in `unimatrix-store/src/read.rs` at line 1630,
  publicly re-exported. A parallel `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` constant
  should be added alongside it.

### Write Path Selection

NLI tick writes use `write_pool_server()` directly (not the analytics queue) because the
analytics `GraphEdge` variant only supports `INSERT OR IGNORE` — no UPDATE semantics.

The co_access promotion tick must also use `write_pool_server()` directly for the same
reason: UPDATE of existing edge weights cannot go through the analytics drain.

Shedding policy in the analytics drain notes: "W1-2 NLI confirmed edge writes MUST NOT use
this variant — use direct write_pool path instead." The co_access promotion shares this
constraint (it needs conditional UPDATE and is not shed-safe for the same correctness
reasons).

### Weight Normalization

The bootstrap uses `count / MAX(count) OVER ()` normalized across all promoted rows in one
SQL pass. The tick promotion processes one batch at a time. For the tick:
- Fetch qualifying pairs: `SELECT entry_id_a, entry_id_b, count FROM co_access WHERE count >= threshold ORDER BY count DESC LIMIT cap`
- Compute `max_count` from the fetched batch (or via a separate `SELECT MAX(count) FROM co_access WHERE count >= threshold`)
- Normalized weight per pair: `count as f32 / max_count as f32`
- For INSERT: use normalized weight
- For UPDATE: compare to existing weight; update if `|new_weight - existing_weight| > delta` (default 0.1)

The `MAX(count)` should be computed over ALL qualifying pairs (not just the capped batch)
to keep weights comparable across ticks. This requires either a separate SQL query or a
subquery.

### ADR Context

- ADR-001 crt-021 (entry #2417): Typed Edge Weight Model — CoAccess edges use count-based
  normalized weight at write time.
- ADR-005 crt-023 (entry #2704): Bootstrap idempotency via COUNTERS table — not applicable
  here (this tick is recurring, not one-shot).
- ADR-001 crt-032 (entry #3785): w_coac zeroed; PPR is the sole co-access signal carrier
  via `ppr_blend_weight`. This makes crt-034 critical — frozen CoAccess edges in GRAPH_EDGES
  mean PPR's co-access signal is permanently stale.
- entry #3739 (crt-030): PPR operates on pre-built TypedRelationGraph (dense CoAccess graph
  from bootstrap). crt-034 keeps that graph current.

## Proposed Approach

Add a new async function `run_co_access_promotion_tick(store, config)` in a new module
`services/co_access_promotion_tick.rs` (parallel to `nli_detection_tick.rs`), following
the same structural pattern.

The function:
1. Fetches `MAX(count)` over all pairs with `count >= threshold` (one read query).
2. Fetches the capped batch: `SELECT entry_id_a, entry_id_b, count FROM co_access WHERE count >= threshold ORDER BY count DESC LIMIT cap` (one read query).
3. For each pair in the batch:
   a. Compute normalized weight: `pair.count as f32 / max_count as f32`.
   b. Attempt `INSERT OR IGNORE INTO graph_edges ... 'CoAccess' ...` (direct `write_pool_server()`).
   c. If INSERT was a no-op (rows_affected == 0), fetch current weight and UPDATE if delta > threshold.
4. After the loop, emit a `tracing::info!` with counts of inserted and updated edges.

The function is unconditional — it does not require NLI. It is called on every tick, after
orphaned-edge compaction and before `TypedGraphState::rebuild()`.

Config addition: `InferenceConfig.max_co_access_promotion_per_tick: usize`, default 200,
range [1, 10000]. A larger default than NLI (200 vs 100) is appropriate because co_access
reads are cheap SQL lookups with no ML inference cost.

## Acceptance Criteria

- AC-01: A `co_access` pair that crosses `count >= threshold` after bootstrap is promoted
  into `GRAPH_EDGES` as a `CoAccess` edge on the next background tick.
- AC-02: A `co_access` pair already present in `GRAPH_EDGES` as `CoAccess` with a weight
  delta > 0.1 from the new normalized weight is updated within one tick.
- AC-03: A `co_access` pair already present in `GRAPH_EDGES` as `CoAccess` with a weight
  delta <= 0.1 is NOT updated (no unnecessary writes).
- AC-04: Promotions per tick are capped at `max_co_access_promotion_per_tick`; only the
  pairs with the highest `count` are selected when the cap is reached.
- AC-05: The promotion step runs AFTER `cleanup_stale_co_access` and AFTER orphaned-edge
  compaction, and BEFORE `TypedGraphState::rebuild()`.
- AC-06: `InferenceConfig.max_co_access_promotion_per_tick` has a serde default of 200,
  validates in range [1, 10000], and participates in project-level config merge.
- AC-07: The constant `CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` is exposed as a public
  constant from `unimatrix-store` (matching the migration's bootstrap threshold).
- AC-08: The constant `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` is exported from
  `unimatrix-store` alongside `EDGE_SOURCE_NLI`.
- AC-09: When `co_access` table is empty or no pairs meet the threshold, the promotion
  tick completes as a no-op with no errors or warnings.
- AC-10: When `max_co_access_promotion_per_tick = 0` is rejected by config validation
  with a clear error message.
- AC-11: `run_co_access_promotion_tick` is infallible — write errors are logged at `warn!`
  and the tick proceeds; no tick-level error propagation.
- AC-12: Inserted edges have `bootstrap_only = 0`, `source = 'co_access'`,
  `created_by = 'tick'`, `relation_type = 'CoAccess'`.
- AC-13: Weight normalization uses `MAX(count)` across ALL qualifying pairs
  (not just the capped batch), ensuring weights are globally comparable across ticks.

## Constraints

- **No new schema migration.** GRAPH_EDGES schema is complete. Adding a migration would
  bump schema version unnecessarily; this is a pure behavior change.
- **Direct write_pool path.** UPDATE semantics are required for weight refresh; the
  `AnalyticsWrite::GraphEdge` analytics drain only supports `INSERT OR IGNORE` and cannot
  be used without adding a new variant (out of scope).
- **File size limit (500 lines).** The new `co_access_promotion_tick.rs` module must stay
  under 500 lines per workspace conventions. The function should be straightforward enough
  to fit comfortably.
- **Infallible tick contract.** All background tick functions are infallible (`async fn ...
  -> ()`). Errors are logged, not propagated.
- **No rayon pool.** Unlike NLI inference, co_access promotion is pure SQL — no CPU-bound
  ML work, no rayon pool needed.
- **Blocking dependency on #409.** This feature must ship before GH #409 prunes `co_access`
  rows. If #409 ships first, signal is lost before promotion runs.
- **SQLite write contention.** The promotion uses `write_pool_server()`. On a busy tick,
  write pool contention may cause individual INSERTs/UPDATEs to time out; per infallible
  contract, log and continue.

## Design Decisions (Human-Approved)

1. **Weight delta (0.1) — named constant.** This is an internal churn-suppression parameter,
   not operator-tunable domain behavior. Config fields are reserved for things operators
   legitimately tune across domains. Use `CO_ACCESS_WEIGHT_UPDATE_DELTA: f32 = 0.1`.

2. **Promotion cap default — 200.** NLI uses 100 due to ML inference cost; co_access
   promotion is pure SQL so 200 is appropriate. The candidate query **must** use
   `ORDER BY count DESC LIMIT N` so highest-signal pairs are promoted first when the cap
   is reached. Arbitrary drain order is unacceptable.

3. **Two-query (global normalization) — confirmed.** The bootstrap used
   `MAX(count) OVER ()` — global normalization. Promoted edges must use the same scale as
   bootstrapped edges; otherwise PPR weights are inconsistent. One extra SQL round-trip is
   trivial (co_access table is ~0.34 MB).

4. **Stale edge GC — deferred to #409.** Scattering retention logic across two features
   would make #409 harder to reason about. The orphaned-edge compaction already handles
   the structural case (edges pointing to deleted entries). GC of CoAccess edges for
   sub-threshold pairs belongs in #409.

5. **No COUNTERS marker — confirmed.** The marker pattern is for one-shot operations.
   This tick is explicitly recurring. Idempotency is structural: `INSERT OR IGNORE` for
   new edges, `UPDATE WHERE delta > threshold` for existing ones.

## Known Limitation

**CoAccess edge directionality.** The bootstrap writes edges as
`source_id = entry_id_a (min), target_id = entry_id_b (max)` — one direction only.
PPR traverses `Direction::Outgoing`, so seeding the min-id entry reaches the max-id entry
but not the reverse. For a symmetric signal, this means half the traversal paths are
missing. **v1 must match the bootstrap behavior (one direction only) for consistency.**
Writing both directions is the correct fix but is a follow-up issue — call it out as a
known limitation in the architecture.

## Tracking

https://github.com/dug-21/unimatrix/issues/456
