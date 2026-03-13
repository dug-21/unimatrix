# col-022: uds-listener -- Pseudocode

## Purpose

Extend the `RecordEvent` handler in `dispatch_request()` to recognize `cycle_start` events and apply force-set attribution + keywords persistence. Also add `set_feature_force` to `SessionRegistry` and `update_session_keywords` persistence helper. ADR-001 (RecordEvent reuse), ADR-002 (force-set).

## File 1: `crates/unimatrix-server/src/infra/session.rs`

### New Type: `SetFeatureResult`

```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetFeatureResult {
    /// Feature was None, now set.
    Set,
    /// Feature was already set to the same value.
    AlreadyMatches,
    /// Feature was set to a different value, now overwritten.
    Overridden { previous: String },
}
```

### New Method: `SessionRegistry::set_feature_force`

```
impl SessionRegistry:
    /// Unconditionally set the session's feature_cycle (ADR-002).
    ///
    /// Unlike set_feature_if_absent, this overwrites any existing value.
    /// Used exclusively by cycle_start events. All heuristic paths continue
    /// using set_feature_if_absent.
    pub fn set_feature_force(&self, session_id: &str, feature: &str) -> SetFeatureResult:
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())

        match sessions.get_mut(session_id):
            None =>
                // Session not registered. Return Set as no-op indicator.
                // The event is still persisted as observation by the caller.
                // Log at debug -- not an error, session may have closed.
                tracing::debug!(session_id, "set_feature_force: session not in registry")
                return SetFeatureResult::Set  // Treat as "set" for simplicity

            Some(state) =>
                match &state.feature:
                    None =>
                        state.feature = Some(feature.to_string())
                        SetFeatureResult::Set

                    Some(existing) if existing == feature =>
                        SetFeatureResult::AlreadyMatches

                    Some(existing) =>
                        let previous = existing.clone()
                        state.feature = Some(feature.to_string())
                        SetFeatureResult::Overridden { previous }
```

**Design note**: When session is not in registry, returning `SetFeatureResult::Set` is a pragmatic choice. The session may have already closed (timed out, completed). The observation is still persisted. The caller logs based on the result.

## File 2: `crates/unimatrix-server/src/uds/listener.rs`

### New Import

```
use crate::infra::validation::{CYCLE_START_EVENT, CYCLE_STOP_EVENT};
use crate::infra::session::SetFeatureResult;
```

### Modify: `dispatch_request` -- RecordEvent handler

The existing `RecordEvent` handler is at lines 585-662. Insert a new match on `event.event_type` **before** the generic #198 payload extraction (line 598). The cycle_start path replaces `set_feature_if_absent` with `set_feature_force`.

```
HookRequest::RecordEvent { event } =>
    if !uds_has_capability(Capability::SessionWrite):
        return HookResponse::Error { code: -32003, message: "..." }

    tracing::info!(event_type = event.event_type, session_id = event.session_id, "UDS: event recorded")

    // NEW: col-022 -- cycle_start gets force-set attribution + keywords persistence
    if event.event_type == CYCLE_START_EVENT:
        handle_cycle_start(&event, &session_registry, &store)
        // Fall through to observation persistence below

    // cycle_stop: no special handling, falls through to generic observation persistence

    // EXISTING: #198 Part 1 -- extract feature_cycle from payload (unchanged)
    // This still runs for non-cycle events. For cycle_start events,
    // set_feature_if_absent will return false because set_feature_force already ran.
    if let Some(fc) = event.payload.get("feature_cycle").and_then(|v| v.as_str()):
        let fc_clean = sanitize_metadata_field(fc)
        if !fc_clean.is_empty()
            && session_registry.set_feature_if_absent(&event.session_id, &fc_clean):
            // ... existing #198 persistence code ...

    // EXISTING: col-017 topic signal accumulation (unchanged)
    if let Some(ref signal) = event.topic_signal:
        session_registry.record_topic_signal(...)
        // ... existing eager attribution code ...

    // EXISTING: col-012 observation persistence (unchanged)
    let obs = extract_observation_fields(&event)
    spawn_blocking_fire_and_forget(move || insert_observation(...))

    HookResponse::Ack
```

### New Function: `handle_cycle_start`

```
fn handle_cycle_start(
    event: &ImplantEvent,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
):
    // Step 1: Extract feature_cycle from payload
    let feature_cycle = match event.payload.get("feature_cycle").and_then(|v| v.as_str()):
        Some(fc) => sanitize_metadata_field(fc)
        None =>
            tracing::warn!(session_id = %event.session_id, "cycle_start missing feature_cycle in payload")
            return

    if feature_cycle.is_empty():
        tracing::warn!(session_id = %event.session_id, "cycle_start feature_cycle is empty after sanitize")
        return

    // Step 2: Force-set attribution (ADR-002)
    let result = session_registry.set_feature_force(&event.session_id, &feature_cycle)

    match &result:
        SetFeatureResult::Set =>
            tracing::info!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                "col-022: feature_cycle set via explicit cycle_start"
            )
        SetFeatureResult::AlreadyMatches =>
            tracing::info!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                "col-022: feature_cycle already matches (no-op)"
            )
        SetFeatureResult::Overridden { previous } =>
            tracing::warn!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                previous = %previous,
                "col-022: feature_cycle overridden by explicit cycle_start"
            )

    // Step 3: Persist feature_cycle to SQLite (fire-and-forget)
    // Only persist if the value changed (Set or Overridden)
    if matches!(result, SetFeatureResult::Set | SetFeatureResult::Overridden { .. }):
        let store_fc = Arc::clone(store)
        let sid = event.session_id.clone()
        let fc = feature_cycle.clone()
        spawn_blocking_fire_and_forget(move ||
            if let Err(e) = update_session_feature_cycle(&store_fc, &sid, &fc):
                tracing::warn!(error = %e, "col-022: feature_cycle persist failed")
        )

    // Step 4: Extract and persist keywords (fire-and-forget, independent of attribution)
    if let Some(keywords_json) = event.payload.get("keywords").and_then(|v| v.as_str()):
        if !keywords_json.is_empty():
            let store_kw = Arc::clone(store)
            let sid = event.session_id.clone()
            let kw = keywords_json.to_string()
            spawn_blocking_fire_and_forget(move ||
                if let Err(e) = update_session_keywords(&store_kw, &sid, &kw):
                    tracing::warn!(error = %e, "col-022: keywords persist failed")
            )
```

### New Function: `update_session_keywords`

```
/// Persist keywords JSON string to the session record (col-022).
///
/// Uses the existing `store.update_session` read-modify-write pattern.
fn update_session_keywords(
    store: &Store,
    session_id: &str,
    keywords_json: &str,
) -> Result<(), unimatrix_store::StoreError>:
    store.update_session(session_id, |record|
        record.keywords = Some(keywords_json.to_string())
    )
```

**Design note**: `update_session_keywords` is a separate function from `update_session_feature_cycle` because:
- They run as independent fire-and-forget tasks
- Keywords persistence failure should not affect feature_cycle persistence
- They update different fields via the same `update_session` read-modify-write

**Concurrency note**: Both `update_session_feature_cycle` and `update_session_keywords` use `store.update_session` which acquires `BEGIN IMMEDIATE`. SQLite serializes these. If they race, the second one reads the first's committed state and applies its own update. No data loss.

## Error Handling

- `set_feature_force` on unregistered session: returns `SetFeatureResult::Set`, logged at debug. Event still persisted as observation.
- `update_session_feature_cycle` failure: logged at warn, fire-and-forget. In-memory state is correct; SQLite may lag.
- `update_session_keywords` failure: logged at warn, independent of feature_cycle. Keywords may be NULL for this session.
- Missing `feature_cycle` in cycle_start payload: logged at warn, `handle_cycle_start` returns early. The generic #198 handler below still runs (defense-in-depth).
- Malformed `keywords` JSON: the hook serializes keywords via `serde_json::to_string`, so the listener receives valid JSON. If somehow malformed, `update_session_keywords` stores whatever string was passed -- the consumer (future injection pipeline) must handle parse errors.

## Key Test Scenarios

### set_feature_force (unit tests in session.rs)

1. **Set when absent**: register session with no feature, call `set_feature_force("col-022")`. Returns `SetFeatureResult::Set`. Verify `state.feature == Some("col-022")`.
2. **AlreadyMatches**: register session, set feature to "col-022", call `set_feature_force("col-022")`. Returns `AlreadyMatches`.
3. **Overridden**: register session, set feature to "col-017" (via eager attribution), call `set_feature_force("col-022")`. Returns `Overridden { previous: "col-017" }`. Verify `state.feature == Some("col-022")`.
4. **Unregistered session**: call `set_feature_force` on unknown session_id. Returns `Set` (graceful).

### handle_cycle_start (integration tests in listener.rs)

5. **Full cycle_start flow**: dispatch `RecordEvent { event_type: "cycle_start", payload: { feature_cycle: "col-022", keywords: "[\"kw1\"]" } }`. Verify: `set_feature_force` called, `update_session_feature_cycle` persisted, `update_session_keywords` persisted, observation row inserted.
6. **cycle_start overrides eager attribution**: register session, trigger eager attribution to set feature "col-017", then dispatch cycle_start with "col-022". Verify feature is now "col-022" in both registry and SQLite.
7. **cycle_start with same feature**: session already has "col-022", dispatch cycle_start with "col-022". Verify `AlreadyMatches`, no persistence writes triggered.
8. **cycle_start without keywords**: payload has `feature_cycle` but no `keywords`. Verify feature set, keywords not written (stays NULL).
9. **cycle_stop event**: dispatch `RecordEvent { event_type: "cycle_stop", ... }`. Verify: no `handle_cycle_start` called, observation row inserted, session feature unchanged.
10. **cycle_start missing feature_cycle**: payload is `{}`. Verify: warn logged, falls through to generic handler.
11. **Keywords persistence failure**: mock/force `update_session_keywords` to fail. Verify: warn logged, feature_cycle attribution still succeeds.
12. **Concurrent cycle_start events**: two cycle_start events for same session with different topics. Verify: last writer wins in registry, SQLite state matches.
13. **After cycle_start, set_feature_if_absent is no-op**: dispatch cycle_start, then dispatch a regular RecordEvent with different `feature_cycle` in payload. Verify: #198 `set_feature_if_absent` returns false, feature unchanged.
14. **Observation row for cycle_start**: verify the observation table has a row with `hook = "cycle_start"` and correct `topic_signal`.
