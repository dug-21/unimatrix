# vnc-022 Pseudocode Overview: Remote Observation Transport

## Components

| # | Component | File Modified | Purpose |
|---|-----------|--------------|---------|
| 1 | compact-payload-wire | `unimatrix-engine/src/wire.rs` | Add `transcript_excerpt: Option<String>` to CompactPayload |
| 2 | capability-extension | `unimatrix-server/src/http/auth.rs` | Add `SessionWrite` to HTTP ResolvedIdentity capabilities |
| 3 | dispatch-request-refactor | `unimatrix-server/src/uds/listener.rs` | Make `dispatch_request` pub(crate), add `capabilities` param |
| 4 | observe-context | `unimatrix-server/src/http/router.rs` + `main.rs` | ObserveContext struct, PathRouter field, main.rs construction |
| 5 | observe-handler | `unimatrix-server/src/http/router.rs` | Replace 501 stub with real handler + response mapper |

## Wave Ordering (Build Sequence)

**Wave 1** (no dependencies, parallel):
- compact-payload-wire (wire.rs only, engine crate)
- capability-extension (auth.rs only, server crate)
- dispatch-request-refactor (listener.rs only, server crate)

**Wave 2** (depends on Wave 1):
- observe-context (depends on dispatch-request-refactor for the parameter list)

**Wave 3** (depends on Wave 2):
- observe-handler (depends on observe-context for handle access, dispatch-request-refactor for pub(crate) dispatch_request, capability-extension for SessionWrite)

## Data Flow

```
HTTP Client
  |  POST /observe, Bearer token, JSON body
  v
StaticTokenAuth middleware (unchanged)
  |  inserts ResolvedIdentity { capabilities: [Read, Write, Search, SessionWrite] }
  v
PathRouter::call()  (POST /observe arm)
  |  1. extract ResolvedIdentity from extensions
  |  2. Content-Length fast-path size check
  |  3. Limited body collection (stream-level 1MB cap)
  |  4. serde_json::from_slice::<HookRequest>(&body)
  |  5. prefix session_id: format!("http-{}", client_session_id)
  |  6. call dispatch_request(..., &identity.capabilities)
  |  7. observe_response_to_http(hook_response) -> HTTP Response
  v
HTTP Response to client
```

## Shared Types

### ObserveContext (new, router.rs)

```
struct ObserveContext {
    store:                    Arc<Store>,
    embed_service:            Arc<EmbedServiceHandle>,
    vector_store:             Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store:              Arc<Store>,
    adapt_service:            Arc<AdaptationService>,
    server_version:           String,
    session_registry:         Arc<SessionRegistry>,
    pending_entries_analysis:  Arc<Mutex<PendingEntriesAnalysis>>,
    services:                 ServiceLayer,
}
```

All fields derive `Clone` (Arc types + String + ServiceLayer which already implements Clone).

### CompactPayload (modified, wire.rs)

One new field: `transcript_excerpt: Option<String>` with `serde(default, skip_serializing_if = "Option::is_none")`.

### Capability Set Constants

- UDS: `[Read, Search, SessionWrite]` (unchanged, in `uds/mod.rs`)
- HTTP: `[Read, Write, Search, SessionWrite]` (modified, in `auth.rs`)

### Session ID Prefix

- HTTP handler applies `format!("http-{}", client_session_id)` before dispatch
- UDS path: no prefix (unchanged)
- The prefix `http-` uses only allowed characters (alphanumeric + hyphen)

## Integration Surface References

All names below are traced to ARCHITECTURE.md Integration Surface table:

| Name | Source |
|------|--------|
| `dispatch_request` signature | ARCH Integration Surface row 1 |
| `ObserveContext` struct | ARCH Integration Surface row 2 |
| `PathRouter::new` signature | ARCH Integration Surface row 3 |
| `ResolvedIdentity.capabilities` | ARCH Integration Surface row 4 |
| `CompactPayload.transcript_excerpt` | ARCH Integration Surface row 5 |
| `observe_response_to_http` | ARCH Integration Surface row 9 |
| `DEFAULT_MAX_BODY_BYTES` | ARCH Integration Surface row 8 |
| `UDS_CAPABILITIES` | ARCH Integration Surface row 10 |
