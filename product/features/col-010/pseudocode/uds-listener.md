# Pseudocode: uds-listener

Component: UDS Listener Integration (P0)
Files: `crates/unimatrix-server/src/uds_listener.rs`

---

## Purpose

Add persistent writes to the UDS event dispatcher for SessionRegister, SessionClose, and ContextSearch events. All writes are fire-and-forget via `spawn_blocking` to avoid blocking the async event loop. Adds `session_id` input sanitization.

---

## 1. session_id Sanitization Helper

```
fn sanitize_session_id(session_id: &str) -> Result<(), HookError>:
    // Enforce: [a-zA-Z0-9-_], max 128 chars
    if session_id.len() > 128:
        return Err(HookError::InvalidSessionId("too long"))
    for ch in session_id.chars():
        if not (ch.is_alphanumeric() || ch == '-' || ch == '_'):
            return Err(HookError::InvalidSessionId("invalid character"))
    Ok(())
```

Called at SessionRegister dispatch, before any SESSIONS write. Returns error response if validation fails.

---

## 2. SessionRegister Handler Changes

Location: `dispatch_request` → `HookRequest::SessionRegister` arm

```
// EXISTING: look up or create in-memory SessionRegistry
// ...existing registry code...

// NEW (col-010): sanitize session_id
if let Err(e) = sanitize_session_id(&session_id):
    return Ok(HookResponse::error(e.to_string()))

// NEW (col-010): persist SessionRecord to SESSIONS
let record = SessionRecord {
    session_id: session_id.clone(),
    feature_cycle: feature.clone(),
    agent_role: agent_role.clone(),
    started_at: unix_now_secs(),
    ended_at: None,
    status: SessionLifecycleStatus::Active,
    compaction_count: 0,
    outcome: None,
    total_injections: 0,
}
let store_clone = Arc::clone(&store)
spawn_blocking_fire_and_forget(move || {
    if let Err(e) = store_clone.insert_session(&record):
        tracing::warn!(session_id = %record.session_id, error = %e, "SESSIONS insert failed")
})
```

Error policy: log warn but never fail SessionRegister. The in-memory registry is the authoritative runtime store; SESSIONS is a durability layer.

---

## 3. agent_role / feature_cycle Sanitization (SR-SEC-02 resolution)

Apply the same sanitization rule to `agent_role` and `feature_cycle` fields before writing to `SessionRecord`. These values appear in auto-outcome entry content, so they must not contain control characters or injection vectors.

```
fn sanitize_metadata_field(value: &str) -> String:
    // Truncate to 128 chars, strip non-printable ASCII
    value.chars()
         .filter(|c| c.is_ascii() && !c.is_ascii_control())
         .take(128)
         .collect()
```

Apply `sanitize_metadata_field` to `feature_cycle` and `agent_role` in the SessionRecord before writing. The sanitized versions are used both for SESSIONS storage and auto-outcome content generation.

---

## 4. SessionClose Handler Changes

Location: `process_session_close` in `uds_listener.rs`

```
// EXISTING: drain_and_signal_session, process signals
// ...existing code...

// NEW (col-010): resolve status and outcome
let (final_status, outcome_str) = match signal_output.final_outcome:
    SessionOutcome::Success  => (SessionLifecycleStatus::Completed, "success")
    SessionOutcome::Rework   => (SessionLifecycleStatus::Completed, "rework")
    SessionOutcome::Abandoned => (SessionLifecycleStatus::Abandoned, "abandoned")

let injection_count: u32 = session_registry
    .get_injection_count(&session_id)
    .unwrap_or(0) as u32

// NEW (col-010): update SessionRecord
let session_id_clone = session_id.clone()
let store_clone = Arc::clone(&store)
let feature_cycle_clone = feature_cycle.clone()
let agent_role_clone = agent_role.clone()
let status_clone = final_status.clone()
let outcome_str_copy = outcome_str.to_string()
let compaction_count = session_registry.get_compaction_count(&session_id).unwrap_or(0)

spawn_blocking_fire_and_forget(move || {
    let result = store_clone.update_session(&session_id_clone, |r| {
        r.status = status_clone
        r.ended_at = Some(unix_now_secs())
        r.outcome = Some(outcome_str_copy.clone())
        r.total_injections = injection_count
        r.compaction_count = compaction_count
    })
    if let Err(e) = result:
        tracing::warn!(session_id = %session_id_clone, error = %e, "SESSIONS update failed")
})

// NEW (col-010): write auto-outcome entry if applicable
if final_status != SessionLifecycleStatus::Abandoned && injection_count > 0:
    write_auto_outcome_entry(store, session_id, outcome_str, injection_count, feature_cycle, agent_role)
    // see auto-outcomes component
```

Note: SessionClose must NOT block on ONNX. All writes are fire-and-forget.

---

## 5. ContextSearch Handler Changes

Location: `handle_context_search` in `uds_listener.rs`, after step 10 (injection tracking)

```
// EXISTING: session_registry.record_injection(...)
// ...existing code...

// NEW (col-010): persist injection log batch
if let Some(ref sid) = session_id:
    if !sid.is_empty() && !filtered_results.is_empty():
        let now = unix_now_secs()
        let records: Vec<InjectionLogRecord> = filtered_results
            .iter()
            .map(|(entry, sim)| InjectionLogRecord {
                log_id: 0,  // will be allocated by insert_injection_log_batch
                session_id: sid.clone(),
                entry_id: entry.id,
                confidence: compute_rerank_score(sim, entry),  // reranked score
                timestamp: now,
            })
            .collect()
        let store_clone = Arc::clone(&store)
        spawn_blocking_fire_and_forget(move || {
            if let Err(e) = store_clone.insert_injection_log_batch(&records):
                tracing::warn!(
                    session_id = %sid_clone, count = %records.len(),
                    error = %e, "INJECTION_LOG batch write failed"
                )
        })
```

Critical (ADR-003): one `spawn_blocking` call for all N entries per ContextSearch response. Not one call per entry.

---

## 6. unix_now_secs Helper

```
fn unix_now_secs() -> u64:
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
```

If this helper already exists in the codebase, reuse it. If not, add it at the module level.

---

## 7. spawn_blocking_fire_and_forget Pattern

```
// Pattern used throughout this component
fn spawn_blocking_fire_and_forget<F>(f: F)
where F: FnOnce() + Send + 'static:
    tokio::task::spawn_blocking(f);
    // returned JoinHandle is dropped; task runs in background
```

---

## Error Handling

| Event | Error Policy |
|-------|-------------|
| SessionRegister: sanitize_session_id fails | Return HookResponse::error; do not write to SESSIONS |
| SessionRegister: insert_session fails | Log warn; continue (in-memory registry is authoritative) |
| SessionClose: update_session fails | Log warn; session_id may not exist if insert was missed |
| ContextSearch: insert_injection_log_batch fails | Log warn; injection tracking continues in-memory |

---

## Key Test Scenarios

1. SessionRegister with valid session_id → `get_session` returns Active record.
2. SessionRegister with invalid session_id (contains `!`) → returns error, no SESSIONS write.
3. SessionRegister with session_id > 128 chars → returns error.
4. SessionClose with Success → `get_session` returns Completed/success/total_injections=N.
5. SessionClose with Abandoned → status=Abandoned; no auto-outcome written.
6. ContextSearch with 3 results → 3 InjectionLogRecord rows in one transaction (single `next_log_id` increment of 3).
7. ContextSearch with empty results → no INJECTION_LOG write.
8. ContextSearch with no session_id → no INJECTION_LOG write.
9. Server restart: drop and reopen store → previously written SessionRecord and InjectionLogRecords survive (AC-14).
