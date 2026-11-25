# C6 Observe Gate — `http/router/handlers.rs`

ADR-004 (#5826), FR-14a, AC-18, HD 2. Depends on C2 (identity capabilities from C4 table), C3 (runs first).

## Purpose
Reject observe/session telemetry for credentials lacking `SessionWrite` with 403 before any dispatcher, store-state or registry work.

## Route Inventory
`PathRouter::call` has one non-MCP `/v1/` route: `POST /v1/{slug}/observe` → `route_observe`. All session telemetry (SessionRegister/Close, RecordEvent(s), ContextSearch injection) travels inside observe frames. No other session route exists today; if a later route is added it must call `require_session_write` first (comment at the helper).

## `route_observe` change
Move identity extraction to the top and add the check BEFORE Step 0 (`parse_project_key` / `resolve_store`) and before body read:
```
// Step -1 (vnc-050 ADR-004): SessionWrite gate, pre-dispatch.
let identity = match request.extensions().get::<ResolvedIdentity>() {
    Some(id) => id.clone(),
    None => { tracing::error!(...existing...); return Ok(json_error_response(500, "internal error: identity not resolved")) }
};
if let Err(resp) = require_session_write(&identity) { return Ok(resp); }
// Step 0 ... existing (parse_project_key, resolve_store, registry_for, pending_for, services_for)
// Step 1: existing identity extraction removed (reuse `identity`)
```

```
/// 403 when the auth-layer identity lacks SessionWrite (read tokens).
pub(crate) fn require_session_write(id: &ResolvedIdentity) -> Result<(), Response<BoxBody<Bytes, Infallible>>> {
    if id.capabilities.contains(&Capability::SessionWrite) { Ok(()) }
    else { Err(json_error_response(StatusCode::FORBIDDEN, "token lacks SessionWrite")) }   // {"error":"token lacks SessionWrite"}
}
```
Ordering: C3 already ran (slug 403 wins over SessionWrite 403 for a wrong slug). Legacy and read-write identities contain SessionWrite (C4 table) → unchanged behavior (FR-39). The `http_access` audit row (C3) is still written for a 403 here.

## Error Handling
403 body fixed; no token echo; nothing dispatched, no store resolved.

## Key Test Scenarios (test seams: OVERVIEW §Test-Only Seams)
After extraction tick, drain = `UsageService::flush`.
- Read token on `/v1/A/observe` with each frame type (SessionRegister, RecordEvent, RecordEvents, SessionClose, ContextSearch) → 403 exact body; `DISPATCH_SPY` has no entry for the test slug; observations, sessions, signal_queue, cycle_events, cycle_tags, co_access, injection_log, query_log, entries unchanged after extraction tick + drain (R-17.1).
- Read-write and legacy → 200 and rows written (companion).
- Read token bound to A on `/v1/B/observe` → 403 `token not permitted for project` (C3 first).
