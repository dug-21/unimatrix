# crt-043: Phase Capture Write Path — Pseudocode

## Purpose

Add `phase: Option<String>` to the private `ObservationRow` struct in `listener.rs` and
capture `current_phase` from the session registry before entering `spawn_blocking` at all
four observation write sites. This follows the identical pre-capture timing contract as
`enrich_topic_signal` (col-024 ADR-004, entry #3374).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/uds/listener.rs` | Add `phase` field to `ObservationRow`; add pre-capture at four write sites; add `phase` bind to `insert_observation` and `insert_observations_batch` |

---

## Modified Struct: `ObservationRow`

The `ObservationRow` struct is private to `listener.rs` (distinct from the read-side
`unimatrix_store::ObservationRow` in `observations.rs`).

```
// Before:
struct ObservationRow {
    session_id: String,
    ts_millis: i64,
    hook: String,
    tool: Option<String>,
    input: Option<String>,
    response_size: Option<i64>,
    response_snippet: Option<String>,
    /// Hook-side topic signal for feature attribution (col-017).
    topic_signal: Option<String>,
}

// After (crt-043):
struct ObservationRow {
    session_id: String,
    ts_millis: i64,
    hook: String,
    tool: Option<String>,
    input: Option<String>,
    response_size: Option<i64>,
    response_snippet: Option<String>,
    /// Hook-side topic signal for feature attribution (col-017).
    topic_signal: Option<String>,
    /// Active session phase at observation write time (crt-043).
    /// NULL when no active cycle or current_phase not yet set (FR-C-05).
    phase: Option<String>,
}
```

`extract_observation_fields` does NOT capture phase (it has no session_registry access).
Phase is captured at each call site, after `extract_observation_fields` returns, and
after `enrich_topic_signal` enriches `topic_signal`.

---

## Phase Capture Pattern

The identical pattern is applied at all four write sites:

```
// Capture phase BEFORE spawn_blocking — same timing contract as topic_signal enrichment.
// session_registry.get_state() is an O(1) Mutex read (~microseconds).
// If session is unknown, get_state returns None → phase is None (not an error, FR-C-05).
let phase: Option<String> = session_registry
    .get_state(&session_id)
    .and_then(|s| s.current_phase.clone());

obs.phase = phase;
// Then: spawn_blocking(move || { insert_observation(&store, &obs) })
//        The closure captures `obs` by move, including the already-captured phase.
```

Key invariant: `session_registry.get_state()` is called synchronously before any
`spawn_blocking` or `tokio::spawn` call. The captured `Option<String>` is moved
into the closure. No reference to `session_registry` crosses the spawn boundary.

---

## Four Write Sites

### Write Site 1: RecordEvent Path

Located around line 797 in `dispatch_request`, `HookRequest::RecordEvent` arm.

```
// Existing code:
let store_for_obs = Arc::clone(store);
let mut obs = extract_observation_fields(&event);
obs.topic_signal =
    enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);

// ADD (crt-043): capture phase before spawn_blocking
obs.phase = session_registry
    .get_state(&event.session_id)
    .and_then(|s| s.current_phase.clone());

spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "observation write failed");
    }
});
```

### Write Site 2: post_tool_use_rework_candidate Path

Located around line 703 in `dispatch_request`, `HookRequest::PostToolUseReworkCandidate`
arm (the fire-and-forget rework observation write).

```
// Existing code:
let store_for_obs = Arc::clone(store);
let mut obs = extract_observation_fields(&event);
obs.topic_signal =
    enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);

// ADD (crt-043): capture phase before spawn_blocking
obs.phase = session_registry
    .get_state(&event.session_id)
    .and_then(|s| s.current_phase.clone());

tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "rework observation write failed");
    }
});
```

### Write Site 3: RecordEvents Batch Path

Located around line 902 in `dispatch_request`, `HookRequest::RecordEvents` arm.

The batch path constructs an `obs_batch: Vec<ObservationRow>` via an iterator map.
Phase must be captured per-event within the same map closure where `topic_signal` is enriched.

```
// Existing code:
let store_for_obs = Arc::clone(store);
let obs_batch: Vec<ObservationRow> = events
    .iter()
    .map(|event| {
        let mut obs = extract_observation_fields(event);
        obs.topic_signal =
            enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);
        obs
    })
    .collect();

// After (crt-043) — phase captured inside the same map closure:
let store_for_obs = Arc::clone(store);
let obs_batch: Vec<ObservationRow> = events
    .iter()
    .map(|event| {
        let mut obs = extract_observation_fields(event);
        obs.topic_signal =
            enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);
        // crt-043: capture phase per-event, before batch enters spawn_blocking.
        obs.phase = session_registry
            .get_state(&event.session_id)
            .and_then(|s| s.current_phase.clone());
        obs
    })
    .collect();

spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observations_batch(&store_for_obs, &obs_batch) {
        tracing::error!(error = %e, "batch observation write failed");
    }
});
```

The map closure runs synchronously before the spawn (the `collect()` forces evaluation).
All phase values are captured from the live session registry before the batch enters
`spawn_blocking_fire_and_forget`. This satisfies FR-C-03 and C-08.

### Write Site 4: ContextSearch Path

Located around line 1050 in `dispatch_request`, `HookRequest::ContextSearch` arm.
This write site constructs `ObservationRow` inline (not via `extract_observation_fields`).

```
// Existing code:
let obs = ObservationRow {
    session_id: sid.clone(),
    ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
    hook: sanitize_observation_source(source.as_deref()),
    tool: None,
    input: Some(truncated_input),
    response_size: None,
    response_snippet: None,
    topic_signal: enriched_signal,
};

// After (crt-043): add phase field to struct literal.
// Phase is captured from session_registry before the struct is constructed.
// session_id is the `sid` variable (the `&str` from the outer if-let).
let phase: Option<String> = session_registry
    .get_state(sid)
    .and_then(|s| s.current_phase.clone());

let obs = ObservationRow {
    session_id: sid.clone(),
    ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
    hook: sanitize_observation_source(source.as_deref()),
    tool: None,
    input: Some(truncated_input),
    response_size: None,
    response_snippet: None,
    topic_signal: enriched_signal,
    phase,  // crt-043
};
```

Note: `session_id` in this arm is `Option<String>` at the outer scope, but the phase
capture is inside the `if let Some(ref sid) = session_id { if !query.is_empty() { ... } }`
block where `sid` is the unwrapped `&String`. Use `sid.as_str()` for the `get_state` call
if the API requires `&str`.

---

## Modified Functions: SQL Bind for `phase`

### `insert_observation`

Located around line 2642 in `listener.rs`.

```
// Before:
fn insert_observation(
    store: &Store,
    obs: &ObservationRow,
) -> Result<(), unimatrix_store::StoreError> {
    let pool = store.write_pool_server();
    tokio::runtime::Handle::current()
        .block_on(
            sqlx::query(
                "INSERT INTO observations
                    (session_id, ts_millis, hook, tool, input,
                     response_size, response_snippet, topic_signal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&obs.session_id)
            .bind(obs.ts_millis)
            .bind(&obs.hook)
            .bind(&obs.tool)
            .bind(&obs.input)
            .bind(obs.response_size)
            .bind(&obs.response_snippet)
            .bind(&obs.topic_signal)
            .execute(pool),
        )
        .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
    Ok(())
}

// After (crt-043): add phase at position ?9
fn insert_observation(
    store: &Store,
    obs: &ObservationRow,
) -> Result<(), unimatrix_store::StoreError> {
    let pool = store.write_pool_server();
    tokio::runtime::Handle::current()
        .block_on(
            sqlx::query(
                "INSERT INTO observations
                    (session_id, ts_millis, hook, tool, input,
                     response_size, response_snippet, topic_signal, phase)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(&obs.session_id)
            .bind(obs.ts_millis)
            .bind(&obs.hook)
            .bind(&obs.tool)
            .bind(&obs.input)
            .bind(obs.response_size)
            .bind(&obs.response_snippet)
            .bind(&obs.topic_signal)
            .bind(&obs.phase)   // crt-043: ?9
            .execute(pool),
        )
        .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
    Ok(())
}
```

### `insert_observations_batch`

Located around line 2673 in `listener.rs`. The per-row INSERT inside the loop must also
include `phase` at position ?9.

```
// Before (per-row INSERT inside the loop):
sqlx::query(
    "INSERT INTO observations
        (session_id, ts_millis, hook, tool, input,
         response_size, response_snippet, topic_signal)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
)
.bind(&obs.session_id)
.bind(obs.ts_millis)
.bind(&obs.hook)
.bind(&obs.tool)
.bind(&obs.input)
.bind(obs.response_size)
.bind(&obs.response_snippet)
.bind(&obs.topic_signal)
.execute(&mut *txn)

// After (crt-043): add phase at position ?9
sqlx::query(
    "INSERT INTO observations
        (session_id, ts_millis, hook, tool, input,
         response_size, response_snippet, topic_signal, phase)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
)
.bind(&obs.session_id)
.bind(obs.ts_millis)
.bind(&obs.hook)
.bind(&obs.tool)
.bind(&obs.input)
.bind(obs.response_size)
.bind(&obs.response_snippet)
.bind(&obs.topic_signal)
.bind(&obs.phase)   // crt-043: ?9
.execute(&mut *txn)
```

---

## `ObservationRow` Construction Site Audit

`extract_observation_fields` constructs the struct. After adding the `phase` field, all
construction sites that use struct literal syntax must include `phase`. There are two patterns:

1. Sites using `extract_observation_fields` — the function returns an `ObservationRow`;
   the caller then sets fields individually with `obs.phase = ...`. No struct literal change needed.

2. The ContextSearch write site (Write Site 4) — constructs `ObservationRow` inline with
   struct literal syntax. Must add `phase` field as shown above.

The implementation agent must search for all struct literal constructions of `ObservationRow`
in `listener.rs` and confirm none are missed. Compiler will enforce exhaustive field initialization
if `ObservationRow` has no `#[derive(Default)]` or `..Default::default()` usage.

---

## NULL Semantics (FR-C-05)

`get_state(session_id)` returns `None` when the session is not registered. The `and_then`
chain maps this to `None`, which binds as SQL NULL. This is not an error — it is the expected
cold-start path (Workflow 5 from SPECIFICATION.md).

`current_phase` is `Option<String>` in `SessionState`. If no phase has been set (before any
`context_cycle(type=start, next_phase=...)` call for the session), it is `None`, which also
binds as SQL NULL.

No validation, normalization, or allowlist is applied at write time (FR-C-06, C-05).

---

## Phase Value Passthrough (FR-C-06)

Phase values are stored verbatim from `SessionState.current_phase`. Canonical values:
"scope", "design", "delivery", "review". These are advisory — any string may be stored.

Group 6 queries using phase stratification MUST apply `LOWER()` at query time to normalize
case variants (C-05 advisory note). crt-043 does not enforce this at write time.

---

## Timing Contract Verification

The pre-capture pattern must be verified against the `enrich_topic_signal` reference (entry #3374):

| Step | topic_signal (existing) | phase (crt-043) |
|------|------------------------|-----------------|
| 1. Extract | `extract_observation_fields(event)` | same |
| 2. Enrich | `enrich_topic_signal(obs.topic_signal, session_id, session_registry)` | not applicable |
| 3. Phase capture | — | `session_registry.get_state(session_id).and_then(...)` |
| 4. Set field | `obs.topic_signal = enriched` | `obs.phase = phase` |
| 5. Spawn | `spawn_blocking(move || insert_observation(&store, &obs))` | same closure captures both |

Both captures happen synchronously before any spawn. The `obs` value is moved into the closure
with all fields set. No lazy read from session_registry inside the closure.

---

## Error Handling

| Scenario | Handling |
|----------|---------|
| `get_state(session_id)` returns None | phase = None (NULL in SQL); not an error |
| `current_phase` is None | phase = None (NULL in SQL); not an error |
| `insert_observation` SQL error | `tracing::error!` at the spawn site; no retry |
| `insert_observations_batch` SQL error | `tracing::error!` at the spawn site; no retry |
| v21 column absent (pre-migration DB) | SQL error on INSERT; only possible if migration failed (server startup would have failed) |

---

## Key Test Scenarios

For full test scenarios see `test-plan/phase-capture.md`. Required scenarios (AC-08, AC-09, AC-10, R-03, R-04):

1. **RecordEvent with active phase** (R-03 scenario 1) — set `current_phase = "design"` on session; send RecordEvent; read back the observation row; assert `phase = 'design'`.

2. **Rework-candidate path with active phase** (R-03 scenario 2) — same as scenario 1 via the rework path.

3. **RecordEvents batch with active phase** (R-03 scenario 3) — send RecordEvents batch; read back all rows; assert all have `phase = 'design'`.

4. **ContextSearch with active phase** (R-03 scenario 4) — send ContextSearch; assert the written observation row has `phase = 'design'`.

5. **No active cycle — phase IS NULL** (AC-10b, FR-C-05) — no `context_cycle` call before observation; assert `phase IS NULL` on the written row.

6. **Pre-capture timing** (R-04 scenario 2) — set `current_phase = "design"`, insert observation, then change phase to "delivery"; read back row; assert `phase = 'design'` (value at capture time).

7. **Session registry miss** (edge case) — send observation for unknown session_id; assert `phase IS NULL`, no panic.

8. **Whitespace phase via `set_current_phase("")`** (edge case) — if `set_current_phase` allows empty string, verify `phase = ''` is stored (not NULL); document Group 6 `WHERE phase = 'design'` would miss it. Implementation agent must verify `set_current_phase` rejects or normalizes empty strings in `session.rs`.

9. **All four write sites** (R-03, AC-09) — code review checklist: confirm all four sites have the pre-capture line before `spawn_blocking`, and both `insert_observation` and `insert_observations_batch` bind `&obs.phase` at position ?9.
