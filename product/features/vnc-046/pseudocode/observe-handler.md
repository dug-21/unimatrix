# Component: observe-handler (`route_observe`) + `dispatch_request` param cleanup

**Source:** `crates/unimatrix-server/src/http/router/handlers.rs:40` (`route_observe`);
`crates/unimatrix-server/src/uds/listener.rs:773` (`dispatch_request`)
**ADR:** ADR-001 · **FR:** FR-5, FR-6, FR-11 · **AC:** AC-09 · **Risks:** R-02, R-14

## Purpose

Resolve registry/pending/services per request from the already-parsed `ProjectKey`, pass them to
`dispatch_request`, and map any post-`resolve_store` `*_for` error to **500, never 404** (R-14).
Delete the vestigial `_vector_store`/`_adapt_service` params from `dispatch_request` (AC-09).

## `route_observe` — insert resolution after `resolve_store` (`handlers.rs:66-80`)

The key is already parsed at Step 0 (`handlers.rs:51`) and the store already resolved at
`handlers.rs:66`. Reuse the **same** `key` — do not re-parse.

```
// ... existing Step 0 (parse key) and store = observe_ctx.resolver.resolve_store(&key)? ...

// NEW: resolve the per-slug observe state from the SAME funnel + SAME key.
// A post-store Err here is a boot-wiring contradiction (ADR-003 forecloses it) → 500, NOT 404.
registry = MATCH observe_ctx.resolver.registry_for(&key):
    Ok(r)  => r
    Err(_) => RETURN Ok(internal_error_response())        // 500 (R-14)
pending  = MATCH observe_ctx.resolver.pending_for(&key):
    Ok(p)  => p
    Err(_) => RETURN Ok(internal_error_response())        // 500
services = MATCH observe_ctx.resolver.services_for(&key):
    Ok(s)  => s
    Err(_) => RETURN Ok(internal_error_response())        // 500

// ... existing Steps 1-5 unchanged (identity, size check, Accept, body collect, deserialize,
//     prefix_session_id) ...

// Step 6: dispatch with the RESOLVED per-slug handles (was observe_ctx.session_registry/... globals).
response = dispatch_request(
    hook_request,
    &store,                    // store
    &observe_ctx.embed_service,
    &store,                    // entry_store (boot pairing preserved per-request)
    &observe_ctx.server_version,
    &registry,                 // &Arc<SessionRegistry> derefs to &SessionRegistry
    &pending,                  // &Arc<Mutex<PendingEntriesAnalysis>>
    &services,                 // &ServiceLayer
    &identity.capabilities,
).await
```

Key points:
- `dispatch_request` takes `session_registry: &SessionRegistry` (`listener.rs:781`); `&registry`
  (a `&Arc<SessionRegistry>`) coerces via `Deref`. `pending`/`services` are passed by ref as before.
- The two removed args (`&observe_ctx.vector_store`, `&observe_ctx.adapt_service`) are **gone** from
  this call (they no longer exist on `ObserveContext`, and the params are removed — below).
- **R-14 discipline:** never map a `*_for` `Err` to `json_error_response(NOT_FOUND, ...)`. Use
  `internal_error_response()` (500, already defined at `handlers.rs:198`). The genuine unregistered-slug
  404 stays at `resolve_store` (Step 0/`handlers.rs:66-73`), unchanged.

## `dispatch_request` — remove 2 vestigial params (`listener.rs:773`)

```
pub(crate) async fn dispatch_request(
    request, store,
    embed_service,
    // REMOVE: _vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store,
    // REMOVE: _adapt_service: &Arc<AdaptationService>,
    server_version, session_registry, pending_entries_analysis, services, capabilities,
) -> HookResponse
```
Both params are `_`-prefixed and unused in the body — pure signature deletion, no body change.

### Blast radius (BIG — mechanical, must be complete)
`dispatch_request` has ~100 call sites (grep `dispatch_request(`):
- **Production:** `handlers.rs:150` (updated above) and the real UDS path `listener.rs:727`.
- **Tests:** ~90 sites in `uds/listener.rs` (3566…9732), `uds/listener/tests/transcript.rs:50`,
  `uds/listener/tests/stamp_read.rs:142`, and the `mock_dispatch_request` helper indirection in
  `http/router/tests.rs`.

Every site must drop the two positional args (the `_vector_store` and `_adapt_service` handles they
currently pass). This is a wide but trivial edit; leaving one site untouched is a compile error
(safe). The `listener.rs:727` UDS call currently passes `async_vector_store`/`adapt_service` — drop
those two args; the UDS path keeps its own store/registry/pending/services unchanged (NFR-4, local
paths behavior-invariant).

**If the blast radius is deferred:** AC-09's hard requirement is deleting the two `ObserveContext`
*fields* (done in `observe-context.md`). Removing the `dispatch_request` params is the natural
completion but touches ~100 sites; it cannot be half-done (route_observe has no source for the
removed args once the fields are gone), so plan the full sweep in one pass. **No placeholder
handles** — do not fabricate a dummy `AsyncVectorStore`/`AdaptationService` to keep the params.

## Data Flow

- **In:** `ObserveContext { resolver, embed_service, server_version }`, HTTP request.
- **Out:** `HookResponse` → `observe_response_to_http` (unchanged Step 7).

## Error Handling

| Condition | Response |
|---|---|
| invalid slug (parse) | 400 (existing) |
| unregistered slug (`resolve_store` Err) | 404 (existing, unchanged) |
| `*_for` Err after store resolved | **500** `internal_error_response()` (R-14) |
| missing identity, body too large, bad JSON | existing 500/413/400 |

## Key Test Scenarios (hints)

- INV-T1 assembled-wiring: POST `transcript_delta` to `/v1/{A}/observe` → `cycle_review` via A's
  `McpAdapter` folds it (R-02 — drives `route_observe`, no hand-passed registry).
- R-14 unit: store resolvable but `*_for` forced to `Err` → `route_observe` returns **500**, not 404,
  no panic.
- Unregistered slug still 404s at `resolve_store` (regression).
- Compile: no call site of `dispatch_request` passes the removed args.
