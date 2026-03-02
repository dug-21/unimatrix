# Pseudocode: injection-tracking

## Purpose

Modify col-007's ContextSearch handler to record which entries were injected into each session. Integrate SessionRegistry into SessionRegister/SessionClose handlers. Replace CoAccessDedup with SessionRegistry's coaccess methods.

## Changes to uds_listener.rs

### 1. Replace CoAccessDedup with SessionRegistry

The `accept_loop` function currently creates `Arc::new(CoAccessDedup::new())`. Replace with `Arc::new(SessionRegistry::new())`.

```
// BEFORE (in accept_loop):
let coaccess_dedup = Arc::new(CoAccessDedup::new());

// AFTER:
// SessionRegistry is created externally and passed in via start_uds_listener
// Remove CoAccessDedup creation from accept_loop
```

The SessionRegistry is created in main.rs and passed through `start_uds_listener()` -> `accept_loop()` -> `handle_connection()` -> `dispatch_request()`.

### 2. Modify start_uds_listener Signature

```
pub async fn start_uds_listener(
    socket_path,
    store,
    embed_service,
    vector_store,
    entry_store,
    adapt_service,
    session_registry: Arc<SessionRegistry>,  // NEW -- replaces internal CoAccessDedup
    server_uid,
    server_version,
) -> io::Result<(JoinHandle<()>, SocketGuard)>
```

Pass `session_registry` through to `accept_loop`, which passes it to each spawned `handle_connection`, which passes it to `dispatch_request`.

### 3. Modify dispatch_request Signature

```
async fn dispatch_request(
    request,
    store,
    embed_service,
    vector_store,
    entry_store,
    adapt_service,
    server_version,
    session_registry: &SessionRegistry,  // CHANGED from coaccess_dedup
) -> HookResponse
```

### 4. SessionRegister Handler -- Register Session

```
HookRequest::SessionRegister { session_id, cwd, agent_role, feature } =>
    tracing::info!(...)

    // NEW: register session in registry
    session_registry.register_session(&session_id, agent_role.clone(), feature.clone());

    warm_embedding_model(embed_service).await
    HookResponse::Ack
```

### 5. SessionClose Handler -- Clear Session

```
HookRequest::SessionClose { session_id, outcome, duration_secs } =>
    tracing::info!(...)

    // CHANGED: clear from SessionRegistry (was coaccess_dedup.clear_session)
    session_registry.clear_session(&session_id);

    HookResponse::Ack
```

### 6. ContextSearch Handler -- Add Injection Tracking + Session-Aware CoAccess

The `handle_context_search` function signature changes to accept session_registry and session_id.

```
async fn handle_context_search(
    query,
    session_id: Option<String>,  // NEW -- from ContextSearch wire type
    k,
    store,
    embed_service,
    vector_store,
    entry_store,
    adapt_service,
    session_registry: &SessionRegistry,  // CHANGED from coaccess_dedup
) -> HookResponse
```

After building the filtered results list (step 9 in existing code), add injection tracking:

```
// 10. Injection tracking (NEW -- col-008)
if let Some(ref sid) = session_id {
    if !sid.is_empty() && !filtered.is_empty() {
        let injection_entries: Vec<(u64, f64)> = filtered
            .iter()
            .map(|(entry, _sim)| (entry.id, entry.confidence))
            .collect();
        session_registry.record_injection(sid, &injection_entries);
    }
}
```

Update co-access pair recording to use session_id when available:

```
// 11. Co-access pair recording with dedup (MODIFIED -- use session_id)
if filtered.len() >= 2 {
    let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect();
    let session_key = session_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("hook-injection");
    if session_registry.check_and_insert_coaccess(session_key, &entry_ids) {
        // ... existing co-access pair generation and recording ...
    }
}
```

### 7. ContextSearch Dispatch Arm -- Pass session_id

```
HookRequest::ContextSearch { query, session_id, role: _, task: _, feature: _, k, max_tokens: _ } =>
    handle_context_search(
        query,
        session_id,  // NEW -- pass through
        k,
        store,
        embed_service,
        vector_store,
        entry_store,
        adapt_service,
        session_registry,  // CHANGED from coaccess_dedup
    ).await
```

### 8. main.rs Change

```
// Create SessionRegistry
let session_registry = Arc::new(SessionRegistry::new());

// Pass to start_uds_listener (new parameter)
let (uds_handle, socket_guard) = uds_listener::start_uds_listener(
    &paths.socket_path,
    Arc::clone(&store),
    Arc::clone(&embed_handle),
    Arc::clone(&async_vector_store),
    Arc::clone(&async_entry_store),
    Arc::clone(&adapt_service),
    Arc::clone(&session_registry),  // NEW
    server_uid,
    env!("CARGO_PKG_VERSION").to_string(),
).await?;
```

### 9. Remove CoAccessDedup Struct

After all usages are replaced by SessionRegistry, remove the `CoAccessDedup` struct and its `impl` block from uds_listener.rs. The tests for CoAccessDedup behavior migrate to session-registry tests.

## Error Handling

- record_injection on unregistered session: silent no-op (FR-02.10, FR-04.3)
- check_and_insert_coaccess on unregistered session: returns false (no co-access recording)
- Injection tracking is fire-and-forget within the handler -- never fails the response

## Key Test Scenarios

1. SessionRegister handler calls session_registry.register_session()
2. SessionClose handler calls session_registry.clear_session()
3. ContextSearch with session_id records injection history
4. ContextSearch without session_id (None) skips injection tracking
5. ContextSearch with empty session_id skips injection tracking
6. Co-access dedup uses session_id from request when available
7. Co-access dedup falls back to "hook-injection" when no session_id
8. Existing dispatch tests pass with SessionRegistry replacing CoAccessDedup
