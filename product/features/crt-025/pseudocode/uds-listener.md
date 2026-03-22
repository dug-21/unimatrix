# Component 5: UDS Listener
## File: `crates/unimatrix-server/src/uds/listener.rs`

---

## Purpose

Dispatches `HookRequest::RecordEvent` events from the hook path. Currently handles `cycle_start` with force-set attribution and keywords persistence. This component extends that to:

1. Also handle `cycle_phase_end` and `cycle_stop` in the dispatch switch.
2. Mutate `SessionState.current_phase` **synchronously** (before any `spawn_blocking` or `tokio::spawn`) for all three lifecycle event types.
3. Fire-and-forget `insert_cycle_event` to persist every lifecycle event to `CYCLE_EVENTS`.
4. Remove keywords persistence from `handle_cycle_start`.

The synchronous mutation ordering is the critical invariant (SR-01, NFR-02, R-01).

---

## Import Changes

```
// BEFORE:
use crate::infra::validation::{CYCLE_START_EVENT, CYCLE_STOP_EVENT};

// AFTER:
use crate::infra::validation::{
    CYCLE_PHASE_END_EVENT,   // NEW
    CYCLE_START_EVENT,
    CYCLE_STOP_EVENT,
};
```

---

## Modified Dispatch: `RecordEvent` arm in `dispatch_request`

The current RecordEvent arm (at line ~588 in listener.rs) checks for `CYCLE_START_EVENT` and delegates to `handle_cycle_start`. This expands to handle all three lifecycle events before falling through to generic observation persistence.

```
FUNCTION dispatch_request(request, session_id, store, session_registry, ...):
    ...
    match request:
        HookRequest::RecordEvent { event } →

            // col-022 + crt-025: Lifecycle event routing
            IF event.event_type == CYCLE_START_EVENT:
                handle_cycle_event(
                    &event,
                    CycleLifecycle::Start,
                    &session_registry,
                    &store,
                )

            ELSE IF event.event_type == CYCLE_PHASE_END_EVENT:    // NEW
                handle_cycle_event(
                    &event,
                    CycleLifecycle::PhaseEnd,
                    &session_registry,
                    &store,
                )

            ELSE IF event.event_type == CYCLE_STOP_EVENT:         // NEW (was no-op before)
                handle_cycle_event(
                    &event,
                    CycleLifecycle::Stop,
                    &session_registry,
                    &store,
                )

            // #198: Extract feature_cycle from payload (UNCHANGED for all events)
            IF let Some(fc) = event.payload.get("feature_cycle"):
                ... existing feature_cycle attribution logic ...

            // Generic observation persistence (UNCHANGED)
            ...
```

Note: `CycleLifecycle` is a local enum defined in this function or as a file-private enum in listener.rs — not a public type.

---

## Local Enum: `CycleLifecycle`

```
// File-private, used only within listener.rs
enum CycleLifecycle {
    Start,
    PhaseEnd,
    Stop,
}
```

---

## New / Modified Function: `handle_cycle_event`

Replaces `handle_cycle_start`. Handles all three lifecycle events. The existing `handle_cycle_start` attribution + feature_cycle force-set logic is preserved for `Start` events.

```
FUNCTION handle_cycle_event(
    event:            &ImplantEvent,
    lifecycle:        CycleLifecycle,
    session_registry: &SessionRegistry,
    store:            &Arc<Store>,
):

    // === SYNCHRONOUS SECTION (must complete before any spawn) ===

    // Step 1: Extract and sanitize feature_cycle from payload
    feature_cycle = match event.payload.get("feature_cycle").and_then(as_str):
        None →
            warn!("cycle event missing feature_cycle in payload")
            // Still proceed to DB write if possible (ADR, FR-04.4: orphaned events valid)
            // Use empty string as fallback for session state ops, skip them
            proceed with feature_cycle = "" (but skip session_registry calls)
        Some(fc) → sanitize_metadata_field(fc)

    IF feature_cycle is not empty:

        // Step 2: Force-set feature attribution (preserved from handle_cycle_start, Start only)
        IF lifecycle == CycleLifecycle::Start:
            result = session_registry.set_feature_force(&event.session_id, &feature_cycle)
            log info/warn based on result (same as existing handle_cycle_start logic)

        // Step 3: SYNCHRONOUS current_phase mutation (crt-025 NEW, CRITICAL ORDER)
        //
        // This MUST happen before the DB spawn below. Any context_store call that arrives
        // after this point in the same session will observe the updated phase (SR-01, NFR-02).
        next_phase_val = event.payload.get("next_phase").and_then(as_str)
            .map(|s| s.to_string())

        match lifecycle:
            CycleLifecycle::Start →
                IF next_phase_val is Some(np):
                    session_registry.set_current_phase(&event.session_id, Some(np))
                // else: no change to current_phase
            CycleLifecycle::PhaseEnd →
                IF next_phase_val is Some(np):
                    session_registry.set_current_phase(&event.session_id, Some(np))
                // else: no change to current_phase (leave as-is)
            CycleLifecycle::Stop →
                session_registry.set_current_phase(&event.session_id, None)

    // === END OF SYNCHRONOUS SECTION ===

    // Step 4: Persist feature_cycle to SQLite for Start events (UNCHANGED, fire-and-forget)
    IF lifecycle == CycleLifecycle::Start AND feature_cycle is not empty:
        IF result was Set or Overridden:
            tokio::spawn(async move { update_session_feature_cycle(...).await })

    // Step 5: Fire-and-forget CYCLE_EVENTS INSERT (crt-025 NEW)
    //
    // Runs after synchronous mutation. Latency budget: 40ms (C-10, NFR-01).
    // seq is computed inside the spawned task via SELECT COALESCE(MAX(seq), -1) + 1.
    // This is advisory (ADR-002): ordering at query time uses (timestamp ASC, seq ASC).
    phase_val      = event.payload.get("phase").and_then(as_str).map(to_owned)
    outcome_val    = event.payload.get("outcome").and_then(as_str).map(to_owned)
    next_phase_for_db = event.payload.get("next_phase").and_then(as_str).map(to_owned)
    event_type_str = event.event_type.clone()
    cycle_id       = feature_cycle.clone() or validated.topic
    timestamp      = now_unix_secs()
    store_clone    = Arc::clone(store)

    IF cycle_id is not empty:
        let _ = tokio::spawn(async move {
            let seq = compute_next_seq(&store_clone, &cycle_id).await   // advisory
            IF let Err(e) = store_clone.insert_cycle_event(
                &cycle_id,
                seq,
                &event_type_str,
                phase_val.as_deref(),
                outcome_val.as_deref(),
                next_phase_for_db.as_deref(),
                timestamp as i64,
            ).await:
                warn!("insert_cycle_event failed: {e}")
                // Fire-and-forget: error logged, not propagated
        })

    // Step 6: Remove keywords persistence (REMOVED from handle_cycle_start)
    // BEFORE: if let Some(keywords_val) = event.payload.get("keywords") { ... }
    // AFTER: nothing — keywords are no longer read from payload here
```

---

## Helper: `compute_next_seq`

```
// Inside the spawned async closure (or extracted as private async fn)
FUNCTION compute_next_seq(store: &Arc<Store>, cycle_id: &str) -> i64:
    // Uses store's write_pool or read_pool (read is acceptable for seq computation)
    result = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?1"
    )
    .bind(cycle_id)
    .fetch_one(store.read_pool())
    .await
    .unwrap_or(0)   // On error: default to 0 (advisory seq; worst case: seq duplication)
    return result as i64
```

Note: `seq` is advisory per ADR-002. The `(timestamp, seq)` ordering at query time tolerates duplicates. On `fetch_one` error (DB unavailable), using `0` is safe — the AUTOINCREMENT `id` ensures row identity.

---

## Ordering Guarantee

```
Timeline within a single session event handler:

T0: HookRequest::RecordEvent { event_type: "cycle_phase_end", next_phase: "implementation" }
T1: session_registry.set_current_phase(sid, Some("implementation"))   ← SYNC (T1 < T2)
T2: tokio::spawn(insert_cycle_event(...))                              ← ASYNC fire-and-forget
T3: context_store call arrives → reads current_phase = Some("implementation") ← correct
```

Any `context_store` processed after T1 will observe `current_phase = Some("implementation")`. The DB write at T2 may lag but does not affect phase tagging.

---

## Error Handling

| Situation | Behavior |
|-----------|----------|
| `feature_cycle` missing from payload | Warn + skip session_registry ops + skip DB write (or write with empty cycle_id filtered out) |
| `insert_cycle_event` returns Err | Warn log, not propagated; tool call is unaffected |
| `set_current_phase` called on unknown session | Silent no-op (see component 4) |
| Mutex lock poisoned on `set_current_phase` | Recovered via `unwrap_or_else(|e| e.into_inner())` |

---

## Key Test Scenarios

1. `cycle_start` with `next_phase="scope"` → `get_state().current_phase == Some("scope")` immediately after handler returns
2. `cycle_start` without `next_phase` → `current_phase` unchanged
3. `cycle_phase_end` with `next_phase="implementation"` → `current_phase = Some("implementation")` synchronously
4. `cycle_phase_end` without `next_phase` → `current_phase` unchanged (previous value retained)
5. `cycle_stop` → `current_phase = None`
6. After `cycle_phase_end`, immediate `context_store` in same session → phase captured as expected (R-01 coverage)
7. `insert_cycle_event` failure → handler does not propagate error
8. Three sequential lifecycle events → `CYCLE_EVENTS` rows have seq = 0, 1, 2
9. `cycle_stop` → keywords NOT extracted from payload (removed)
