# co_access_promotion_tick — Pseudocode

## Component: `services/co_access_promotion_tick.rs` (new module)

### Purpose

Recurring background tick step that promotes qualifying `co_access` pairs into
`GRAPH_EDGES` as `CoAccess`-typed edges and refreshes the weight of already-promoted
edges when the normalized weight has drifted. This closes the gap where PPR's
`TypedRelationGraph` is rebuilt each tick from a `GRAPH_EDGES` table that had been frozen
at bootstrap since schema v13.

Infallible: signature is `async fn ... -> ()`. All errors logged at `warn!`, tick continues.
Pure SQL path: no rayon pool, no ML inference, no COUNTERS marker.

---

## Files

| File | Action |
|------|--------|
| `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` | **Create** |
| `crates/unimatrix-server/src/services/mod.rs` | **Modify** — add `pub(crate) mod co_access_promotion_tick;` |

---

## Module-Level Constants

```
/// Minimum absolute weight difference required to trigger an UPDATE on an
/// already-promoted CoAccess edge.
///
/// ADR-003 (#3825): f64, NOT f32. sqlx fetches SQLite REAL columns as f64.
/// Comparing a fetched weight (f64) against 0.1f32 cast to f64 produces
/// 0.100000001490116..., which would incorrectly treat a delta of exactly 0.1
/// as exceeding the threshold. Using f64 avoids this precision noise.
///
/// Not operator-configurable: this is a calibrated noise floor, not a domain
/// policy parameter. Operators control the tick budget via
/// InferenceConfig::max_co_access_promotion_per_tick.
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;
```

---

## Imports

```
use unimatrix_core::Store;
use unimatrix_store::{CO_ACCESS_GRAPH_MIN_COUNT, EDGE_SOURCE_CO_ACCESS};

use crate::infra::config::InferenceConfig;
```

No rayon imports. No NLI handle imports. No analytics drain imports.

---

## Row Type (module-private)

```
/// One row from the batch candidate SELECT.
///
/// `max_count` is Option<i64> because the scalar subquery returns NULL when
/// the co_access table has no rows passing the WHERE count >= threshold filter.
/// A NULL max_count signals an empty qualifying set → early return before
/// the per-pair loop (R-02: no division-by-zero risk).
#[derive(sqlx::FromRow)]
struct CoAccessBatchRow {
    entry_id_a: i64,
    entry_id_b: i64,
    count: i64,
    max_count: Option<i64>,
}
```

---

## Primary Function

### `run_co_access_promotion_tick`

```
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
) -> ()
```

**Contract**: Infallible. Never panics. All database errors logged at warn! and skipped.
No return value. Always emits a tracing::info! at the end.

**Pseudocode**:

```
FUNCTION run_co_access_promotion_tick(store, config, current_tick):

  -- Phase 1: Batch fetch qualifying pairs with embedded global MAX normalization.
  --
  -- Single SQL round-trip (ADR-001). The scalar subquery computes MAX(count)
  -- over ALL qualifying pairs (not just the capped batch), ensuring weight
  -- normalization is globally consistent regardless of cap.
  --
  -- LIMIT binds config.max_co_access_promotion_per_tick (usize → i64 cast).
  -- ORDER BY count DESC guarantees highest-signal pairs are selected first (R-11).

  rows = sqlx::query_as::<_, CoAccessBatchRow>(
      "SELECT
           entry_id_a,
           entry_id_b,
           count,
           (SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count
       FROM co_access
       WHERE count >= ?1
       ORDER BY count DESC
       LIMIT ?2"
  )
  .bind(CO_ACCESS_GRAPH_MIN_COUNT)                         -- ?1: i64 = 3
  .bind(config.max_co_access_promotion_per_tick as i64)   -- ?2: i64 cap
  .fetch_all(store.write_pool_server())
  .await

  -- Phase 1 fetch error handling:
  IF rows is Err(e):
    warn!("co_access promotion tick: batch fetch failed: {e}")
    info!(inserted=0, updated=0, "co_access promotion tick complete (fetch error)")
    RETURN

  rows = rows (unwrapped Vec<CoAccessBatchRow>)
  qualifying_count = rows.len()

  -- SR-05 early-tick detectability (ADR-005, FR-08):
  -- Emit warn! only when BOTH conditions are true:
  --   1. qualifying_count == 0 (no pairs meet the threshold)
  --   2. current_tick < PROMOTION_EARLY_RUN_WARN_TICKS (within early-run window)
  -- Outside this window, zero qualifying rows produces only the info! log (no warn!).
  -- False-positive risk on server restart is documented in KNOWN_LIMITATIONS.
  IF qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS:
    warn!(
      current_tick = current_tick,
      warn_window = PROMOTION_EARLY_RUN_WARN_TICKS,
      "co_access promotion tick: zero qualifying pairs in early-tick window — \
       verify GH #409 has not pruned co_access before crt-034 deployed (SR-05)"
    )

  -- Empty result: clean no-op.
  IF qualifying_count == 0:
    info!(inserted=0, updated=0, "co_access promotion tick complete")
    RETURN

  -- Phase 2: global max_count.
  -- All rows share the same max_count (scalar subquery, same ?1 predicate).
  -- Safe to read from first row. Unwrap is safe: rows is non-empty and max_count
  -- is Some because the WHERE count >= ?1 predicate guarantees at least one
  -- qualifying row exists (the same predicate in the subquery will find it).
  max_count = rows[0].max_count.unwrap_or(1)   -- unwrap_or(1) is a belt-and-suspenders
                                                 -- guard; None here is structurally
                                                 -- impossible given non-empty rows

  IF max_count <= 0:
    -- Degenerate: all counts are 0 or negative. Not possible given count >= 3 filter,
    -- but guard against data corruption without panicking.
    warn!("co_access promotion tick: max_count <= 0 despite non-empty rows; skipping")
    info!(inserted=0, updated=0, "co_access promotion tick complete (degenerate max)")
    RETURN

  -- Phase 3: Per-pair two-step write.
  -- One INSERT OR IGNORE per pair; on no-op (rows_affected==0), check weight delta,
  -- and UPDATE only if delta exceeds CO_ACCESS_WEIGHT_UPDATE_DELTA.
  -- Errors on individual pairs are logged and skipped (infallible contract).
  -- No transaction: pairs are independent; partial completion is acceptable.

  inserted_count = 0
  updated_count = 0

  FOR EACH row IN rows:
    new_weight: f64 = row.count as f64 / max_count as f64
    -- new_weight is in (0.0, 1.0] given 0 < row.count <= max_count.

    -- Step A: INSERT OR IGNORE.
    -- Inserts the edge if (source_id, target_id, 'CoAccess') is not already in graph_edges.
    -- On UNIQUE constraint conflict, SQLite silently ignores the INSERT and rows_affected = 0.

    insert_result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by,
              source, bootstrap_only)
         VALUES (?1, ?2, 'CoAccess', ?3, strftime('%s','now'), 'tick', ?4, 0)"
    )
    .bind(row.entry_id_a)    -- ?1: source_id (ADR-006: one direction only, min-id first)
    .bind(row.entry_id_b)    -- ?2: target_id
    .bind(new_weight)        -- ?3: REAL, normalized [0.0, 1.0]
    .bind(EDGE_SOURCE_CO_ACCESS)  -- ?4: "co_access"
    .execute(store.write_pool_server())
    .await

    IF insert_result is Err(e):
      warn!(
        entry_id_a = row.entry_id_a,
        entry_id_b = row.entry_id_b,
        error = %e,
        "co_access promotion tick: INSERT failed; skipping pair"
      )
      CONTINUE to next row

    rows_affected = insert_result.rows_affected()

    IF rows_affected > 0:
      -- New edge inserted. Pair was not previously in graph_edges.
      inserted_count += 1
      CONTINUE to next row

    -- rows_affected == 0: edge already exists (INSERT was a no-op).
    -- Step B: Check whether the weight needs refreshing.

    -- Fetch the current stored weight.
    fetch_result = sqlx::query_scalar::<_, f64>(
        "SELECT weight FROM graph_edges
         WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'CoAccess'"
    )
    .bind(row.entry_id_a)
    .bind(row.entry_id_b)
    .fetch_optional(store.write_pool_server())
    .await

    IF fetch_result is Err(e):
      warn!(
        entry_id_a = row.entry_id_a,
        entry_id_b = row.entry_id_b,
        error = %e,
        "co_access promotion tick: weight fetch failed; skipping update"
      )
      CONTINUE to next row

    existing_weight = fetch_result.flatten()

    IF existing_weight is None:
      -- Edge disappeared between INSERT no-op and this fetch (race with deletion).
      -- Harmless: skip, will be re-evaluated on next tick.
      CONTINUE to next row

    existing_weight = existing_weight.unwrap()

    -- Delta guard: suppress churn for small weight changes (R-09, ADR-003).
    -- Strict greater-than: delta exactly equal to CO_ACCESS_WEIGHT_UPDATE_DELTA is NOT updated.
    -- E-05 boundary: |0.6 - 0.5| = 0.1 is exactly the threshold → no update.
    delta = (new_weight - existing_weight).abs()

    IF delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA:
      -- Weight drift is within acceptable noise floor. No write.
      CONTINUE to next row

    -- Step C: UPDATE the weight.
    update_result = sqlx::query(
        "UPDATE graph_edges
         SET weight = ?1
         WHERE source_id = ?2 AND target_id = ?3 AND relation_type = 'CoAccess'"
    )
    .bind(new_weight)        -- ?1: new normalized weight (f64)
    .bind(row.entry_id_a)    -- ?2
    .bind(row.entry_id_b)    -- ?3
    .execute(store.write_pool_server())
    .await

    IF update_result is Err(e):
      warn!(
        entry_id_a = row.entry_id_a,
        entry_id_b = row.entry_id_b,
        new_weight = new_weight,
        error = %e,
        "co_access promotion tick: weight UPDATE failed"
      )
      CONTINUE to next row

    updated_count += 1

  -- Phase 4: Summary log (FR-10, R-01).
  -- Always emits, even if all individual writes failed (info! fires regardless of batch result).
  info!(
    inserted = inserted_count,
    updated = updated_count,
    qualifying = qualifying_count,
    "co_access promotion tick complete"
  )

END FUNCTION
```

---

## State Machine

This module has no persistent state. It is a pure function over SQLite.

Tick lifecycle state (from caller's perspective):
```
[idle] → fetch qualifying pairs → [processing batch] → emit info! → [idle]
         ^ on fetch error: warn! + info! (0,0) → [idle] (early return)
```

---

## Initialization Sequence

No initialization. Module registers nothing. Function is called directly from `background.rs`
each tick.

**Module registration** (`services/mod.rs`): Insert `pub(crate) mod co_access_promotion_tick;`
in alphabetical order among existing module declarations. Current list (as of background
reads): confidence, contradiction_cache, effectiveness, gateway, index_briefing,
nli_detection, nli_detection_tick, observation, search, status, store_correct, store_ops,
typed_graph, usage. Insert `co_access_promotion_tick` before `confidence` (alphabetically).

---

## Data Flow

```
INPUTS:
  store: &Store
    - read: co_access table (via write_pool_server — read-consistent with write sequence)
    - write: graph_edges table (via write_pool_server — direct write pool, not analytics drain)
  config: &InferenceConfig
    - max_co_access_promotion_per_tick: usize  (used as LIMIT ?2)
  current_tick: u32
    - compared against PROMOTION_EARLY_RUN_WARN_TICKS for SR-05 warn

CONSTANTS CONSUMED:
  CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3   (from unimatrix-store)
  EDGE_SOURCE_CO_ACCESS: &str = "co_access"  (from unimatrix-store)
  CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1  (module-private)
  PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5  (from background.rs, passed via current_tick)

OUTPUTS:
  graph_edges table mutations:
    - INSERT OR IGNORE (new edges)
    - UPDATE SET weight (refreshed edges, delta-guarded)
  Structured logs:
    - warn! on batch fetch failure
    - warn! on per-pair INSERT/UPDATE failure
    - warn! on SR-05 trigger (qualifying_count==0 AND current_tick < 5)
    - info! on tick completion (always)
```

---

## Error Handling Summary

| Error Source | Handling | Propagation |
|-------------|----------|-------------|
| `fetch_all` on batch SELECT fails | `warn!`, emit `info!(0,0)`, early return | None — infallible |
| Per-pair `INSERT` fails | `warn!` with pair IDs, `continue` to next pair | None |
| Per-pair weight `SELECT weight` fails | `warn!` with pair IDs, `continue` | None |
| Per-pair `UPDATE` fails | `warn!` with pair IDs and new_weight, `continue` | None |
| `max_count` is None despite non-empty rows | `warn!`, early return | None |
| `max_count` <= 0 (data corruption) | `warn!`, early return | None |

The summary `info!` log always fires, even when all individual writes fail. This ensures
that monitoring can detect a tick that ran but wrote nothing (FM-01).

---

## Key Test Scenarios

**AC-01** (basic promotion): Insert a co_access row with count=5 (threshold=3). Run the
function. Assert a CoAccess edge appears in graph_edges with source_id=entry_id_a,
target_id=entry_id_b, relation_type='CoAccess', bootstrap_only=0, source='co_access',
created_by='tick'.

**AC-02** (weight refresh on drift > delta): Pre-insert a CoAccess edge with weight=0.1.
Insert a co_access row that computes new_weight=0.9 (delta=0.8 > 0.1). Run. Assert
weight updated to 0.9.

**AC-03** (no UPDATE when drift <= delta): Pre-insert CoAccess edge with weight=0.5.
New computed weight=0.55 (delta=0.05 <= 0.1). Run. Assert weight remains 0.5.

**AC-04** (cap + ORDER BY count DESC): Seed 10 pairs with counts [3,3,3,3,3,10,20,50,80,100].
Set cap=3. Run. Assert exactly 3 edges in graph_edges with the counts corresponding to
[100, 80, 50].

**AC-09** (empty/sub-threshold no-op): Run against empty co_access or all-sub-threshold
rows. Assert no panic, no warn!, info! log shows inserted=0, updated=0.

**AC-11** (write failure → continue): Inject a write failure on the first INSERT. Assert
function returns (), warn! emitted with pair identifiers, remaining pairs are attempted.

**AC-12** (metadata fields): Promote one pair. SELECT the resulting row. Assert:
bootstrap_only=0, source='co_access', created_by='tick', relation_type='CoAccess'.

**AC-13** (global MAX normalization): Seed 5 pairs with counts [1,2,3,4,100]. Set cap=3.
Top-3 pairs selected (counts 100, 4, 3). max_count must be 100 (global, not batch-local
max of the subset). Verify by asserting weight of count=4 pair equals 4.0/100.0 = 0.04.

**AC-14** (double-tick idempotency): Promote a pair. Run the tick a second time with
unchanged co_access. Assert exactly one graph_edges row, weight unchanged.

**AC-15** (no GC of sub-threshold edges): Promote a pair (count=5). Drop count to 2
(below threshold). Run the tick. Assert the graph_edges row is still present (GC is #409).

**R-02** (max_count None guard): Run against empty co_access table. Assert the `Option<i64>`
max_count handling produces early return without panic or division.

**R-06 (SR-05 quadrants)**:
- qualifying_count=0, current_tick=0 → warn! emitted
- qualifying_count=0, current_tick=10 → NO warn!
- qualifying_count>0, current_tick=0 → NO warn! (SR-05 only fires on zero qualifying)
- qualifying_count>0, current_tick=10 → NO warn!

**R-10** (one-directional contract): After promoting pair (entry_id_a=1, entry_id_b=2),
query graph_edges for CoAccess edges involving either entry. Assert exactly one row:
source_id=1, target_id=2. Assert no reverse row (source_id=2, target_id=1) exists.

**E-01** (single qualifying pair): One row with count=5; max_count=5; weight=1.0.
Assert weight stored as 1.0. Run twice: second run sees delta=0.0 <= 0.1, no UPDATE.

**E-05** (delta exactly at boundary): Existing edge weight=0.5, new computed weight=0.6.
Delta=0.1 exactly. Assert: strict greater-than means 0.1 is NOT updated (delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA → no write).

**FM-02** (batch fetch failure): Inject a DB error on the batch SELECT. Assert:
warn! emitted, info!(inserted=0, updated=0) fires, function returns ().

---

## File Size Guidance

The implementation must stay under 500 lines (NFR-06, R-12). Estimated line count:
- Module doc comment + imports: ~20 lines
- Constants: ~15 lines
- `CoAccessBatchRow` struct: ~8 lines
- `run_co_access_promotion_tick` function: ~100-130 lines (including in-line comments)
- Tests (if in same file): ~200-250 lines for mandatory AC scenarios

If tests push the file toward 450+ lines, move them to
`crates/unimatrix-server/src/services/co_access_promotion_tick.rs` test module using
`#[cfg(test)]` inline — standard workspace pattern. If the module itself exceeds 350
non-test lines, flag to the implementation agent before submitting.
