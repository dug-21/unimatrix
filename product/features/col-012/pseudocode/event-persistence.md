# Pseudocode: event-persistence

## File: crates/unimatrix-server/src/uds/listener.rs

### Change: RecordEvent handler -- persist to observations table

Replace the current no-op handler (line 559-572) with:

```
HookRequest::RecordEvent { event } => {
    // Capability check (existing)
    if !uds_has_capability(Capability::SessionWrite):
        return Error

    // Extract fields from ImplantEvent.payload
    let hook = event.event_type.clone()
    let session_id = event.session_id.clone()
    let ts_millis = (event.timestamp as i64).saturating_mul(1000)

    // Field mapping depends on hook type
    let (tool, input, response_size, response_snippet) = match hook.as_str():
        "PreToolUse" =>
            tool = payload.get("tool_name").as_str()
            input = payload.get("tool_input").map(serde_json::to_string)
            (tool, input, None, None)

        "PostToolUse" =>
            tool = payload.get("tool_name").as_str()
            input = payload.get("tool_input").map(serde_json::to_string)
            response_size = payload.get("response_size").as_i64()
            response_snippet = payload.get("response_snippet").as_str()
            (tool, input, response_size, response_snippet)

        "SubagentStart" =>
            tool = payload.get("agent_type").as_str()
            input = payload.get("prompt_snippet").as_str()  // stored as plain string
            (tool, input, None, None)

        "SubagentStop" =>
            (None, None, None, None)

        _ =>
            // Unknown hook type: store with original event_type, no extracted fields
            (None, None, None, None)

    // Fire-and-forget write (same pattern as injection_log)
    let store = Arc::clone(&store)
    tokio::task::spawn_blocking(move || {
        let conn = store.lock_conn()
        match conn.execute(
            "INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, ts_millis, hook, tool, input, response_size, response_snippet]
        ):
            Ok(_) => tracing::debug!("observation persisted")
            Err(e) => tracing::error!("observation write failed: {e}")
    })

    tracing::info!(event_type = event.event_type, session_id = event.session_id, "UDS: event recorded")
    HookResponse::Ack
}
```

### Change: RecordEvents handler -- batch persist in single transaction

Replace the current no-op handler (line 574-583) with:

```
HookRequest::RecordEvents { events } => {
    // Capability check (existing)
    if !uds_has_capability(Capability::SessionWrite):
        return Error

    let store = Arc::clone(&store)
    let events_owned = events  // move into spawn_blocking

    tokio::task::spawn_blocking(move || {
        let conn = store.lock_conn()
        let tx = conn.execute_batch("BEGIN")
        // same field extraction as RecordEvent, but in a loop
        for event in &events_owned:
            extract fields (same logic as single event)
            conn.execute(INSERT INTO observations ...)
        conn.execute_batch("COMMIT")
        // On error: ROLLBACK (R-07 atomicity)
    })

    tracing::info!(count = events.len(), "UDS: batch events recorded")
    HookResponse::Ack
}
```

### Helper function: extract_observation_fields

To avoid duplicating field extraction logic:

```
fn extract_observation_fields(event: &ImplantEvent) -> (String, i64, String, Option<String>, Option<String>, Option<i64>, Option<String>):
    let session_id = event.session_id.clone()
    let ts_millis = (event.timestamp as i64).saturating_mul(1000)
    let hook = event.event_type.clone()

    match hook.as_str():
        "PreToolUse" => ...
        "PostToolUse" => ...
        "SubagentStart" => ...
        "SubagentStop" => ...
        _ => ...

    return (session_id, ts_millis, hook, tool, input, response_size, response_snippet)
```

## Notes

- FR-02.3: spawn_blocking fire-and-forget -- UDS response returns immediately
- FR-02.5: Missing optional fields stored as NULL
- FR-02.6: Unknown hook types stored with original event_type string
- R-01: Field mapping must match architecture spec exactly
- R-06: saturating_mul prevents i64 overflow (year 3000 is ~32B seconds * 1000 = ~32T, well within i64 max)
- R-07: Batch uses single transaction for atomicity
