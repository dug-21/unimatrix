# crt-035 Pseudocode: co_access_promotion_tick.rs

## Purpose

Extract a module-private `promote_one_direction` helper that encapsulates the three-step
INSERT-fetch-UPDATE sequence for a single `(source_id, target_id)` directed edge.
Update `run_co_access_promotion_tick` to call this helper twice per qualifying pair
(forward + reverse) and emit an updated log format with three structured fields.

## File Constraint

`co_access_promotion_tick.rs` must remain under 500 lines after this change.
All tests remain in `co_access_promotion_tick_tests.rs`.

---

## New Function: `promote_one_direction`

### Signature

```rust
async fn promote_one_direction(
    store: &Store,
    source_id: i64,
    target_id: i64,
    new_weight: f64,
) -> (bool, bool) // (inserted, updated)
```

Module-private (`async fn`, no `pub`). Not exported. Not visible outside this file.

### Preconditions

- `source_id != target_id` is not enforced here; the UNIQUE constraint and INSERT OR IGNORE
  handle self-loop pairs silently (EC-05: the second call with the same ids is a no-op).
- `new_weight` is in (0.0, 1.0] given the caller's normalization. No floor is applied here.

### Algorithm

```
FUNCTION promote_one_direction(store, source_id, target_id, new_weight) -> (bool, bool):

  // Step A: INSERT OR IGNORE
  // Inserts (source_id, target_id, 'CoAccess') if the UNIQUE constraint is not violated.
  // On duplicate (UNIQUE conflict), SQLite ignores the INSERT and rows_affected = 0.
  // Fields: relation_type='CoAccess', created_by='tick', source=EDGE_SOURCE_CO_ACCESS ('co_access'),
  //         bootstrap_only=0, created_at=strftime('%s','now')
  insert_result = sqlx::query(
      "INSERT OR IGNORE INTO graph_edges
           (source_id, target_id, relation_type, weight, created_at,
            created_by, source, bootstrap_only)
       VALUES (?1, ?2, 'CoAccess', ?3, strftime('%s','now'), 'tick', ?4, 0)"
  )
  .bind(source_id)   // ?1
  .bind(target_id)   // ?2
  .bind(new_weight)  // ?3
  .bind(EDGE_SOURCE_CO_ACCESS)  // ?4 = "co_access"
  .execute(store.write_pool_server())
  .await

  IF insert_result is Err(e):
    // Infallible contract: log warn! and return failure sentinel.
    tracing::warn!(source_id, target_id, error=%e,
        "co_access promotion tick: INSERT failed for direction")
    RETURN (false, false)

  IF insert_result.rows_affected() > 0:
    // Fresh insert: edge did not previously exist.
    // [R-08 note: this direction counts as +1 toward edges_inserted]
    RETURN (true, false)

  // rows_affected == 0: edge already exists (INSERT was a no-op).
  // Step B: Fetch current stored weight to check for drift.
  // Strict greater-than delta guard: delta == 0.1 does NOT trigger an update (ADR-003).
  fetch_result = sqlx::query_scalar::<_, f64>(
      "SELECT weight FROM graph_edges
       WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'CoAccess'"
  )
  .bind(source_id)
  .bind(target_id)
  .fetch_optional(store.write_pool_server())
  .await

  IF fetch_result is Err(e):
    tracing::warn!(source_id, target_id, error=%e,
        "co_access promotion tick: weight fetch failed for direction; skipping update")
    RETURN (false, false)

  match fetch_result.unwrap():
    None =>
      // Edge disappeared between INSERT no-op and fetch (race with deletion).
      // Harmless: skip; will be re-evaluated on next tick.
      RETURN (false, false)

    Some(existing_weight) =>
      delta = |new_weight - existing_weight|

      IF delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA:  // <= 0.1, strict: equal does NOT update
        // Weight is current; no update needed.
        RETURN (false, false)

      // Step C: UPDATE the weight.
      // [R-02 note: both directions independently apply this delta guard]
      update_result = sqlx::query(
          "UPDATE graph_edges
           SET weight = ?1
           WHERE source_id = ?2 AND target_id = ?3 AND relation_type = 'CoAccess'"
      )
      .bind(new_weight)
      .bind(source_id)
      .bind(target_id)
      .execute(store.write_pool_server())
      .await

      IF update_result is Err(e):
        tracing::warn!(source_id, target_id, new_weight, error=%e,
            "co_access promotion tick: weight UPDATE failed for direction")
        RETURN (false, false)

      RETURN (false, true)

END FUNCTION
```

### Return Value Semantics

| Return | Meaning |
|--------|---------|
| `(true, false)` | New edge inserted this tick |
| `(false, true)` | Existing edge's weight updated |
| `(false, false)` | No-op (weight current) or any error (logged at warn!) |

### Error Handling

All three SQL steps use the infallible pattern: `match result { Err(e) => { warn!(...); return (false, false) } }`.
No error propagates out of this function. Each direction is fully independent: a failure
on the reverse call does NOT affect the forward call result (ADR-001 eventual consistency).

---

## Updated Function: `run_co_access_promotion_tick`

### Signature (unchanged)

```rust
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
)
```

Return type is `()`. The function remains infallible.

### What Changes

1. The per-pair write logic (Steps A/B/C in the current loop body) is replaced by two calls
   to `promote_one_direction`.
2. `inserted_count` and `updated_count` accumulate results from both calls per pair.
3. The final `tracing::info!` fields change from `inserted`/`updated`/`qualifying` to
   `promoted_pairs`/`edges_inserted`/`edges_updated` (D2, FR-05).

### Module-Level Doc Comment Update

Replace the existing module doc comment line:
```
//! - One-directional edges v1 (ADR-006): `source_id = entry_id_a`, `target_id = entry_id_b`.
```
with:
```
//! - Bidirectional edges (crt-035, ADR-006 follow-up): both (a→b) and (b→a) written per pair.
//!   `promote_one_direction` helper called twice; forward direction: (entry_id_a, entry_id_b);
//!   reverse direction: (entry_id_b, entry_id_a).
```

### Algorithm (Phase 3 replacement only; Phases 1/2/4 structure unchanged)

```
// Phase 1: unchanged — batch fetch qualifying pairs
// Phase 2: unchanged — extract max_count, guard degenerate

// Phase 3: Per-pair bidirectional write (REPLACES existing Phase 3 loop body)
let qualified_count = rows.len()   // kept for promoted_pairs field
let inserted_count: usize = 0
let updated_count: usize = 0

FOR row IN &rows:
  new_weight: f64 = row.count as f64 / max_count as f64
  // new_weight in (0.0, 1.0] guaranteed by count >= 3 filter and max_count > 0 guard.

  // Forward direction: (entry_id_a → entry_id_b)
  (fwd_inserted, fwd_updated) =
      promote_one_direction(store, row.entry_id_a, row.entry_id_b, new_weight).await

  // Reverse direction: (entry_id_b → entry_id_a)
  // Independent call — failure here does not abort or re-run the forward call (ADR-001).
  (rev_inserted, rev_updated) =
      promote_one_direction(store, row.entry_id_b, row.entry_id_a, new_weight).await

  // Accumulate across both directions.
  // inserted_count can be 0, 1, or 2 per pair.
  // updated_count can be 0, 1, or 2 per pair.
  IF fwd_inserted: inserted_count += 1
  IF rev_inserted: inserted_count += 1
  IF fwd_updated:  updated_count += 1
  IF rev_updated:  updated_count += 1

END FOR

// Phase 4: Summary log (FR-05, D2)
// Fields renamed: inserted→edges_inserted, updated→edges_updated, qualifying→promoted_pairs.
// [R-08 note: edges_inserted will be even (0, 2, 4...) on a fresh graph with no prior edges]
tracing::info!(
    promoted_pairs = qualified_count,
    edges_inserted = inserted_count,
    edges_updated  = updated_count,
    "co_access promotion tick complete"
)
```

### Zero-Row and Error-Return Paths (unchanged structure, updated log fields)

On batch fetch error:
```rust
tracing::warn!(error = %e, "co_access promotion tick: batch fetch failed");
tracing::info!(
    promoted_pairs = 0,
    edges_inserted = 0,
    edges_updated  = 0,
    "co_access promotion tick complete (fetch error)"
);
return;
```

On qualifying_count == 0:
```rust
// SR-05 early-tick warn path: unchanged
tracing::info!(
    promoted_pairs = 0,
    edges_inserted = 0,
    edges_updated  = 0,
    "co_access promotion tick complete"
);
return;
```

On degenerate max_count:
```rust
tracing::warn!("co_access promotion tick: max_count <= 0 ...");
tracing::info!(
    promoted_pairs = 0,
    edges_inserted = 0,
    edges_updated  = 0,
    "co_access promotion tick complete (degenerate max)"
);
return;
```

---

## State Machine: Per-Direction Write

Each call to `promote_one_direction` follows this finite state sequence:

```
START
  |
  v
[INSERT OR IGNORE]
  |
  +-- rows_affected > 0 --> INSERTED (return true, false)
  |
  +-- rows_affected == 0 --> [FETCH weight]
  |                             |
  |                             +-- Err --> WARN + SKIP (return false, false)
  |                             |
  |                             +-- None --> SKIP (disappeared) (return false, false)
  |                             |
  |                             +-- Some(w) --> [CHECK DELTA]
  |                                               |
  |                                               +-- delta <= 0.1 --> NO-OP (return false, false)
  |                                               |
  |                                               +-- delta > 0.1 --> [UPDATE weight]
  |                                                                       |
  |                                                                       +-- Ok --> UPDATED (return false, true)
  |                                                                       |
  |                                                                       +-- Err --> WARN + SKIP (return false, false)
  +-- Err (INSERT) --> WARN + SKIP (return false, false)
```

---

## Key Test Scenarios for Tick

These are hints for the tester agent. The tick tests live in `co_access_promotion_tick_tests.rs`.

**Blast-radius updates required (T-BLR-01 through T-BLR-08):**
- T-BLR-01: `test_basic_promotion_new_qualifying_pair` — after tick on fresh pair (a=1, b=2):
  `count_co_access_edges == 2`. Both `fetch(1,2)` and `fetch(2,1)` are `Some`.
- T-BLR-02: `test_inserted_edge_is_one_directional` → rename to `test_inserted_edge_is_bidirectional`.
  `count == 2`. Both directions `is_some`. Both weights equal.
- T-BLR-03: `test_double_tick_idempotent` — `count == 2` after first tick; `count == 2` after second.
- T-BLR-04: `test_cap_selects_highest_count_pairs` — 3 pairs processed → `count == 6`.
- T-BLR-05: `test_tied_counts_secondary_sort_stable` — 3 pairs → `count == 6`.
- T-BLR-06: `test_cap_equals_qualifying_count` — 5 pairs → `count == 10`.
- T-BLR-07: `test_cap_one_selects_highest_count` — 1 pair → `count == 2`.
- T-BLR-08: `test_existing_edge_stale_weight_updated` — pre-seed forward only, run tick:
  `count == 2`. Forward weight == 1.0, reverse weight == 1.0.
  **[GATE-3B-01: remove the `"no duplicate"` assertion message — must not appear in the file]**
  **[GATE-3B-02: count == 2 is even — confirm this invariant]**

**New Group I tests (T-NEW-01 through T-NEW-03):**
- T-NEW-01: `test_bidirectional_edges_inserted_same_weight` — seed one pair, run tick, verify
  both `(a→b)` and `(b→a)` exist with `|weight_a - weight_b| < 1e-9`.
- T-NEW-02: `test_bidirectional_both_directions_updated_when_drift_exceeds_delta` — pre-seed
  both directions at weight 0.5; seed co_access with count=10 (only pair → weight=1.0, delta=0.5);
  run tick; assert both updated to 1.0.
- T-NEW-03: `test_log_format_promoted_pairs_and_edges_inserted` — use `#[traced_test]`; run tick
  on one fresh pair; assert log contains `promoted_pairs=1`, `edges_inserted=2`, `edges_updated=0`.

---

## Deviation from Prior Pattern

The prior loop body used `continue` after the INSERT step (step A), short-circuiting the
fetch and update steps for that iteration. With `promote_one_direction`, the same control
flow is encapsulated inside the helper with early returns. The main loop never uses `continue`
for per-direction logic; it only ever adds the two `(bool, bool)` return tuples to the
running totals.

---

## Knowledge Stewardship

- Pattern #3883 (crt-034): tick writes use `write_pool_server()` directly — confirmed, followed.
- Pattern #3822 (crt-034): oscillation risk documented in ADR-001 — eventual consistency accepted.
- ADR-001 (crt-035, #3890): forward and reverse updates are independent SQL calls, not atomic.
- Deviations from established patterns: none. Helper extraction is the only structural addition.
