# Pseudocode: uds-auth-audit

## File: `crates/unimatrix-server/src/uds/listener.rs` (modifications)

### Add AuditLog parameter to handle_connection

```
async fn handle_connection(
    stream: tokio::net::UnixStream,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    server_uid: u32,
    session_registry: Arc<SessionRegistry>,
    services: ServiceLayer,                   // existing param
    audit_log: Arc<AuditLog>,                 // NEW parameter
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // ... existing stream conversion ...
```

### Modify authentication failure path

Currently:
```
Err(e) => {
    tracing::warn!(error = %e, "UDS authentication failed, closing connection");
    return Ok(());
}
```

After:
```
Err(e) => {
    tracing::warn!(error = %e, "UDS authentication failed, closing connection");

    // NEW: Audit event for auth failure (F-23)
    audit_log.emit_audit(AuditEvent {
        event_id: 0,          // allocated by audit log
        timestamp: 0,         // filled by audit log
        session_id: String::new(),
        agent_id: "unknown".to_string(),
        operation: "uds_auth_failure".to_string(),
        target_ids: vec![],
        outcome: Outcome::Failure,
        detail: format!("Authentication failed: {e}"),
    });

    return Ok(());
}
```

### Pass AuditLog in start_uds_listener

The `start_uds_listener` function spawns `handle_connection` for each accepted stream.
It needs to receive `Arc<AuditLog>` and pass it through.

```
pub async fn start_uds_listener(
    // ... existing params ...
    audit_log: Arc<AuditLog>,    // NEW
) {
    // ... existing accept loop ...

    tokio::spawn(async move {
        if let Err(e) = handle_connection(
            stream,
            store,
            embed_service,
            vector_store,
            entry_store,
            adapt_service,
            server_uid,
            session_registry,
            services,
            audit_log,       // NEW: pass to handler
        ).await {
            tracing::warn!("handle_connection error: {e}");
        }
    });
}
```

## File: `crates/unimatrix-server/src/server.rs` (modifications)

### Pass AuditLog to UDS listener start

```
// In UnimatrixBackend startup or main.rs where start_uds_listener is called:
start_uds_listener(
    // ... existing args ...
    Arc::clone(&audit_log),   // NEW
)
```

Note: AuditLog is already constructed during server startup. The `audit` field on
`SecurityGateway` holds `Arc<AuditLog>`. It is also accessible from `StoreService.audit`.
The Arc<AuditLog> should be passed directly from where it is created.

## File: `crates/unimatrix-server/src/infra/audit.rs` (no changes needed)

The existing `AuditLog::log_event()` and `emit_audit()` methods work unchanged.
`emit_audit` is fire-and-forget (returns `Result` which is `let _ =`'d by callers).

## Open Questions

1. Need to verify the exact location where `start_uds_listener` is called and what
   variables are in scope. The Arc<AuditLog> is created in the server startup path.
   The main question is whether it needs to be threaded through from main.rs or if
   it can be extracted from the ServiceLayer/SecurityGateway. Since gateway fields are
   pub(crate), we could do `services.store_ops.gateway.audit.clone()` but that is
   reaching through layers. Better to pass explicitly.
