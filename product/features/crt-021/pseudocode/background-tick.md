# background-tick — Pseudocode

**File**: `crates/unimatrix-server/src/background.rs`
**Changes**:
- Update use declaration: `supersession` module → `typed_graph` module
- Update all `SupersessionState`/`SupersessionStateHandle` references
- Add GRAPH_EDGES orphaned-edge compaction step between maintenance_tick and VECTOR_MAP compaction
- Update tick sequence to call `TypedGraphState::rebuild` (not `SupersessionState::rebuild`)

---

## Purpose

Insert GRAPH_EDGES orphaned-edge compaction into the maintenance tick at the correct
sequence position (before VECTOR_MAP compaction, before TypedGraphState rebuild). Update
all renamed symbol references. The tick sequence after crt-021 must be strictly sequential:
maintenance → GRAPH_EDGES compaction → VECTOR_MAP compaction → TypedGraphState rebuild →
contradiction scan. No concurrent execution of steps 2, 3, 4 (C-07, FR-24).

---

## Updated use declarations

```
-- OLD (line 43):
use crate::services::supersession::{SupersessionState, SupersessionStateHandle};

-- NEW:
use crate::services::typed_graph::{TypedGraphState, TypedGraphStateHandle};
```

---

## Background tick function signature update

The `spawn_background_tick` function (or equivalent) accepts a `TypedGraphStateHandle`
parameter. Update all parameter types and internal references.

```
-- OLD parameter type: supersession_handle: SupersessionStateHandle
-- NEW parameter type: typed_graph_handle: TypedGraphStateHandle
```

---

## Updated Tick Sequence

The existing tick sequence is extended at step 2 (GRAPH_EDGES compaction). All steps
execute sequentially — each `await` completes before the next begins.

```
FUNCTION maintenance_and_rebuild_tick(
    store:                &Store,
    typed_graph_handle:   &TypedGraphStateHandle,   -- renamed from supersession_handle
    vector_index:         &VectorIndex,
    -- ... other existing parameters unchanged ...
):

    -- Step 1: Existing maintenance tick (unchanged)
    -- co-access cleanup, confidence refresh, observation retention, session GC, etc.
    maintenance_tick(store, /* ... */).await?

    -- Step 2: GRAPH_EDGES orphaned-edge compaction (NEW — crt-021)
    --
    -- Deletes edges where either endpoint no longer exists in the entries table.
    -- Uses direct write_pool — this is a bounded maintenance write, not an analytics event.
    -- The indexes on source_id and target_id make the NOT IN subquery efficient (SR-03).
    --
    -- NOTE: This is intentionally unbounded in crt-021 (NF-09). A per-tick LIMIT 500
    -- batch is left as a post-ship optimization. The architect accepted this for crt-021.
    compaction_result = store.write_pool_execute(
        "DELETE FROM graph_edges
         WHERE source_id NOT IN (SELECT id FROM entries)
            OR target_id NOT IN (SELECT id FROM entries)"
    ).await

    MATCH compaction_result:
        Ok(result):
            LET rows_deleted = result.rows_affected()
            IF rows_deleted > 0:
                tracing::info!(
                    rows_deleted = rows_deleted,
                    "background tick: GRAPH_EDGES orphaned-edge compaction complete"
                )
        Err(e):
            -- Log error but do not abort the tick; rebuild proceeds on pre-compaction state.
            -- Orphaned edges persist for one more tick cycle — not a correctness issue
            -- (build_typed_relation_graph skips edges whose endpoints are missing from node_index).
            tracing::error!(
                error = %e,
                "background tick: GRAPH_EDGES compaction failed; proceeding with rebuild on pre-compaction state"
            )

    -- Step 3: VECTOR_MAP compaction (existing — unchanged)
    -- Must run after GRAPH_EDGES compaction and before TypedGraphState rebuild.
    compact_vector_map(store, vector_index).await?

    -- Step 4: TypedGraphState rebuild (UPGRADED from SupersessionState::rebuild)
    --
    -- Queries all entries + all GRAPH_EDGES rows, builds TypedRelationGraph in memory,
    -- swaps into the handle under write lock.
    -- Graph construction (the expensive step) runs before the write lock is acquired.
    -- The write lock is held only for the final pointer swap.
    rebuild_typed_graph(store, typed_graph_handle).await

    -- Step 5: Contradiction scan (existing — unchanged)
    contradiction_scan(store, /* ... */).await
```

---

## rebuild_typed_graph helper (new or inlined)

This logic may be inlined into the tick function or extracted as a private helper.
The pseudocode presents it as a helper for clarity.

```
FUNCTION rebuild_typed_graph(store: &Store, handle: &TypedGraphStateHandle) -> ():

    MATCH TypedGraphState::rebuild(store).await:

        Ok(new_state):
            -- Graph was built successfully outside the lock.
            -- Acquire write lock only for the swap.
            LET mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
            *guard = new_state
            DROP guard
            tracing::debug!("background tick: TypedGraphState rebuilt successfully")

        Err(e) if is_cycle_detected_error(&e):
            -- Cycle in Supersedes sub-graph. Set use_fallback=true; retain existing graph.
            -- Search will apply FALLBACK_PENALTY until the cycle is resolved.
            LET mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
            guard.use_fallback = true
            DROP guard
            tracing::error!(
                "background tick: supersession cycle detected; TypedGraphState not updated; search using FALLBACK_PENALTY"
            )

        Err(e):
            -- Store error (query failed). Retain old state — do not modify handle.
            tracing::error!(
                error = %e,
                "background tick: TypedGraphState rebuild failed; retaining previous state"
            )
```

Note: `is_cycle_detected_error` is a helper that checks if the returned `StoreError`
corresponds to a cycle-detection failure from `build_typed_relation_graph`. The implementer
must use a reliable error variant or pattern-match on the error message, per whatever
`StoreError` variant is used to signal cycle detection (see server-state.md).

---

## write_pool_execute helper

The GRAPH_EDGES compaction step needs direct access to `write_pool`. The existing code
in `background.rs` may already have access to `store.write_pool` or an equivalent
method on the `Store` trait. If not, the `Store` trait needs an execute method for
direct maintenance writes:

```
-- Option A: if Store trait exposes write_pool reference:
sqlx::query("DELETE FROM graph_edges WHERE ...")
    .execute(&store.write_pool)
    .await

-- Option B: if Store trait provides a maintenance_execute method:
store.maintenance_execute("DELETE FROM graph_edges WHERE ...").await
```

The implementer should check how existing maintenance writes (e.g., co-access cleanup
in `maintenance_tick`) access the write pool and use the same pattern.

---

## Sequencing Invariant (C-07, FR-24)

Steps 2, 3, 4 must never execute concurrently. The existing tick architecture in
`background.rs` is sequential (single `tokio::spawn` with `await` chaining). Verify
there are no `tokio::spawn` or `join!` calls that would parallelize these steps.

```
INVARIANT: Steps 2, 3, 4 in the tick sequence are separated by `await` —
  each completes before the next begins. No concurrent dispatch.

CORRECT:
  step2.await
  step3.await
  step4.await

INCORRECT:
  tokio::join!(step2, step3, step4)   -- prohibited
  tokio::spawn(step3)                  -- prohibited mid-sequence
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| GRAPH_EDGES compaction DELETE fails | Log ERROR; proceed with tick; rebuild uses pre-compaction state |
| VECTOR_MAP compaction fails | Existing behavior (unchanged) |
| `TypedGraphState::rebuild` store error | Log ERROR; retain old state; use_fallback unchanged |
| `TypedGraphState::rebuild` cycle detected | Set `use_fallback=true`; log ERROR; search degrades gracefully |
| RwLock write poisoned | `.unwrap_or_else(|e| e.into_inner())` recovers; state overwritten |

---

## Key Test Scenarios

1. **Orphaned edge compaction runs before rebuild** (AC-14, R-04):
   - Insert two entries (id=1, id=2). Insert graph_edges rows for (1,2), (99,2), (1,98)
     where 99 and 98 do not exist in entries.
   - Trigger the maintenance tick sequence.
   - After tick: assert (99,2) and (1,98) rows are absent from graph_edges.
   - Assert (1,2) row is still present.
   - Assert the in-memory TypedRelationGraph was built from the post-compaction state.

2. **Compaction failure does not abort rebuild** (failure mode):
   - Inject a write_pool error on the DELETE.
   - Assert the tick continues to step 4 (TypedGraphState::rebuild is called).
   - Assert use_fallback remains unchanged.

3. **Cycle detected: use_fallback=true, handle not replaced** (R-04, existing cycle behavior):
   - Seed entries with a Supersedes cycle (entry A supersedes B, B supersedes A).
   - Trigger tick.
   - Read handle under read lock.
   - Assert `use_fallback = true`.
   - Assert `typed_graph` is the previous state (not a new empty graph).

4. **Rebuild store error: old state retained**:
   - Inject a query_all_entries failure.
   - Trigger tick.
   - Read handle. Assert state is unchanged from pre-tick.

5. **Tick steps are sequential (no concurrent dispatch)**:
   - Code inspection of background.rs confirms no `join!` or concurrent `spawn` across
     compaction, vector compaction, and rebuild steps.

6. **Rename completeness** (R-14):
   - `cargo build --workspace` with no `SupersessionState` or `SupersessionStateHandle`
     references outside comments in background.rs.
