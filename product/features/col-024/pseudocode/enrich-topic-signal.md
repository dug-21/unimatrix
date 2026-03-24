# Component: enrich_topic_signal Helper + Four Write Site Applications
# File: crates/unimatrix-server/src/uds/listener.rs

## Purpose

Introduce a private free function `enrich_topic_signal` that centralises the fallback
logic for all four observation write sites in the UDS listener (ADR-004). When
`extract_topic_signal` returns `None` (or `event.topic_signal` is `None`), the function
reads `session_registry.get_state(session_id)?.feature` and uses that as the
`topic_signal`. This ensures that observations written after `context_cycle(start)` carry
attribution even when the event's input text contains no recognisable feature ID pattern.

The function is module-private (`fn`, not `pub`). It must not be exported.

## New/Modified Functions

### `enrich_topic_signal` (new private free function)

Placed in `listener.rs` among other module-private helpers, after the existing helper
functions but before or after the `ObservationRow` struct — positioning is at the
implementation agent's discretion as long as it is before the first call site.

```
/// Enrich an observation topic_signal using the session registry fallback.
///
/// Returns `extracted` unchanged when it is `Some(_)`.
/// The explicit hook-side signal always wins over the registry value (AC-08, FR-14).
///
/// When `extracted` is `None`, reads session_registry.get_state(session_id)
/// and returns state.feature.clone() if the session has a registered feature.
/// Returns None if the session is not registered or has no feature set (FR-13).
///
/// When `extracted` is `Some(x)` and the session registry has a different feature,
/// emits tracing::debug! with both values for attribution forensics (AC-08).
/// The extracted signal is still returned unchanged; the debug log is informational only.
///
/// This is a synchronous Mutex read (~microseconds); no await, no spawn_blocking.
/// The registry lock is held for the duration of the read only.
///
/// Precondition: session_registry is valid (not poisoned). Uses unwrap_or_else to
/// recover from a poisoned Mutex (returns None in degraded state rather than panicking).
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String> {

    // Read the registry feature for this session, if any.
    // get_state acquires a Mutex lock briefly. Uses unwrap_or_else to handle
    // a poisoned lock without panicking (FM-04).
    let registry_feature: Option<String> = session_registry
        .get_state(session_id)
        .and_then(|state| state.feature);

    // Case 1: explicit extracted signal present.
    if let Some(ref explicit) = extracted {
        // AC-08: explicit signal wins unconditionally.
        // Diagnostic: log if the explicit signal differs from the registry feature.
        if let Some(ref reg_feat) = registry_feature {
            if explicit != reg_feat {
                tracing::debug!(
                    session_id = session_id,
                    extracted_signal = %explicit,
                    registry_feature = %reg_feat,
                    "enrich_topic_signal: explicit signal differs from registry feature; \
                     explicit wins (AC-08)"
                );
            }
        }
        return extracted;  // return Some(explicit) unchanged
    }

    // Case 2: no explicit signal — use registry feature as fallback.
    // Returns None if session not registered or feature not set (FR-13, I-03).
    registry_feature
}
```

## Application at Four Write Sites

### Site 1: RecordEvent (~line 684)

Current code (relevant excerpt):
```
let obs = extract_observation_fields(&event);
spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) { ... }
});
```

Modified pseudocode:
```
// After extract_observation_fields, override topic_signal with enriched value.
// This avoids mutating the immutable ImplantEvent (ADR-004).
let mut obs = extract_observation_fields(&event);
obs.topic_signal = enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);
let store_for_obs = Arc::clone(store);
spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "observation write failed");
    }
});
```

Key: `obs` must be declared `mut` to allow the field override after construction.
The `session_registry` reference is available in the handler scope before the
`spawn_blocking_fire_and_forget` closure.

### Site 2: Rework candidate (~line 592)

Current code (relevant excerpt):
```
let obs = extract_observation_fields(&event);
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) { ... }
});
```

Modified pseudocode:
```
let mut obs = extract_observation_fields(&event);
obs.topic_signal = enrich_topic_signal(obs.topic_signal, &event.session_id, session_registry);
let store_for_obs = Arc::clone(store);
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "rework observation write failed");
    }
});
```

Same pattern as Site 1. The session_registry reference is available before the closure.

### Site 3: RecordEvents batch (~line 784-785)

Current code:
```
let obs_batch: Vec<ObservationRow> =
    events.iter().map(extract_observation_fields).collect();
spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observations_batch(&store_for_obs, &obs_batch) { ... }
});
```

Modified pseudocode:
```
// Per-event enrichment inside the map closure (FR-11, ADR-004).
// session_registry is captured by reference; each event's session_id is used.
let obs_batch: Vec<ObservationRow> = events
    .iter()
    .map(|event| {
        let mut obs = extract_observation_fields(event);
        obs.topic_signal = enrich_topic_signal(
            obs.topic_signal,
            &event.session_id,
            session_registry,
        );
        obs
    })
    .collect();
let store_for_obs = Arc::clone(store);
spawn_blocking_fire_and_forget(move || {
    if let Err(e) = insert_observations_batch(&store_for_obs, &obs_batch) {
        tracing::error!(error = %e, "batch observation write failed");
    }
});
```

Note: the `map(extract_observation_fields)` function-pointer form must change to a
closure form since we need to add the enrichment step. The `session_registry` is not
`Send` so it must not be moved into the `spawn_blocking_fire_and_forget` closure —
the enrichment happens BEFORE the closure, in the handler's synchronous scope.

### Site 4: ContextSearch (~line 842)

Current code (relevant excerpt):
```
let topic_signal = unimatrix_observe::extract_topic_signal(&query);
// ...
let obs = ObservationRow {
    session_id: sid.clone(),
    ts_millis: ...,
    hook: ...,
    tool: None,
    input: Some(truncated_input),
    response_size: None,
    response_snippet: None,
    topic_signal: topic_signal.clone(),
};
```

Modified pseudocode:
```
let topic_signal = unimatrix_observe::extract_topic_signal(&query);
// enrich_topic_signal is called here, where session_id is `sid` (the &str from Some(ref sid)).
// FR-12: if extract_topic_signal returned None, registry fallback applies.
let enriched_signal = enrich_topic_signal(topic_signal, sid, session_registry);

if let Some(ref signal) = enriched_signal {
    session_registry.record_topic_signal(sid, signal.clone(), unix_now_secs());
}

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
```

Note: `record_topic_signal` previously only fired when `topic_signal` from
`extract_topic_signal` was `Some`. After enrichment, the signal may become `Some` from
the registry. The `record_topic_signal` call should use `enriched_signal` so that
registry-enriched signals are also accumulated. This is an improvement in signal fidelity;
implementation agent should verify this is the intended behavior (it matches FR-12).

## State Machines

None. `enrich_topic_signal` is a stateless function. The session registry is read-only
from this function's perspective.

## Initialization Sequence

None. The function is a free function with no initialization. The session registry is
already initialised by the server and passed into the handler.

## Data Flow

```
Input:
  extracted: Option<String>         -- from extract_topic_signal or event.topic_signal
  session_id: &str                  -- from the ImplantEvent
  session_registry: &SessionRegistry -- read-only; mutex locked briefly

Processing:
  session_registry.get_state(session_id) -- Option<SessionState>, cloned
  state.feature                          -- Option<String>

Output:
  Some(extracted)       -- when extracted was Some, regardless of registry
  Some(registry_feature) -- when extracted was None and registry has a feature
  None                  -- when extracted was None and registry has no feature or no entry

Side effects:
  tracing::debug! when extracted is Some(x) and registry has a different feature (AC-08)
```

## Error Handling

| Failure Point | Behavior |
|---------------|----------|
| `session_registry` Mutex poisoned | `get_state` uses `unwrap_or_else(|e| e.into_inner())` internally (see session.rs line 206); no panic from `enrich_topic_signal` perspective. Returns None in worst case (FM-04). |
| Session not registered | `get_state` returns None; `and_then` produces None; function returns None (FR-13, I-03). Silent no-op. |
| `state.feature` is None | `and_then(|state| state.feature)` produces None; function returns None (FR-13). |

No `?` operator is used. No error propagation. Failures degrade silently to `None`.

## Key Test Scenarios

| Test Name | Covers | Setup |
|-----------|--------|-------|
| `enrich_explicit_signal_unchanged` | AC-08, R-04 | extracted = Some("bugfix-342"), registry feature = "col-024"; assert returns Some("bugfix-342") |
| `enrich_explicit_signal_debug_log_on_mismatch` | AC-08, R-04 scenario 3 | same as above; assert tracing::debug! fires with both values |
| `enrich_explicit_signal_no_log_when_match` | AC-08 | extracted = Some("col-024"), registry feature = "col-024"; assert no debug log fires |
| `enrich_fallback_from_registry` | AC-05, AC-06, AC-07, R-02 | extracted = None, registry feature = "col-024"; assert returns Some("col-024") |
| `enrich_no_registry_entry` | FR-13, I-03 | extracted = None, session not registered; assert returns None |
| `enrich_registry_no_feature` | FR-13 | extracted = None, session registered but feature = None; assert returns None |
| `enrich_record_event_site_applies` | AC-05, R-02 | Full RecordEvent handler test: session with feature, event with no topic_signal; assert stored observation has topic_signal from registry |
| `enrich_record_events_batch_all_events` | AC-07, R-02 | Batch of 3 events, session with feature, no explicit topic_signal; assert all 3 stored observations have topic_signal |
| `enrich_context_search_site_applies` | AC-06, R-02 | ContextSearch with non-feature-ID query, session with feature; assert stored observation has topic_signal |
| `enrich_rework_candidate_site_applies` | AC-07, R-02 | Rework event with no topic_signal, session with feature; assert stored observation has topic_signal |

## Constraints

- `fn enrich_topic_signal` must be `fn` (not `pub fn`). Scope-private (R-12, Constraint 6).
- No `await`, no `spawn_blocking`, no I/O inside this function. Sync Mutex read only (NFR-04).
- Called only from the four named write sites in `listener.rs`. No call sites outside this file.
- `obs` must be `mut` at each call site so `obs.topic_signal` can be overwritten after
  `extract_observation_fields`. The immutable `ImplantEvent` is never mutated (ADR-004).
- `session_registry` is borrowed at each call site BEFORE the `spawn_blocking_fire_and_forget`
  closure captures `obs`. The enrichment happens in the synchronous handler context, not
  inside the closure. This avoids borrow conflicts and `Send` issues.
