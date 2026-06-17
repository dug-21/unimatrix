# Component 6 — Observe Route + Handler (Rust)

**Files:** `crates/unimatrix-server/src/http/router.rs`, `main.rs`
**ADR:** ADR-003 (#5082) · **AC:** AC-06, AC-07, AC-08 · **Risk:** R-02, R-09, R-12

## Purpose

Move observe from a top-level boot-bound single-store route to a per-slug route on the per-request funnel: `/v1/{slug}/observe` resolved via `resolve_store(parse_project_key(path))` on EACH call. Delete the top-level `/observe` split (`router.rs:188`) and the boot-bound `resolve_store(Default)` (`main.rs:1045-1052`). The resolved per-request handle is the SOLE observe route — no boot-bound or parallel path (the #4974 ceremonial-funnel guard).

## A. `ObserveContext` (MODIFY — hold the resolver, not a pre-resolved store)

```
// BEFORE (main.rs:1062): ObserveContext { store, embed_service, vector_store, entry_store,
//                        adapt_service, server_version, session_registry, pending_entries_analysis, services }
//   where `store` and `entry_store` were the boot-resolved single served_store.
// AFTER (ADR-003): the per-request store is resolved per call, so the context holds the RESOLVER,
//   not a fixed store.

struct ObserveContext {
    resolver: Arc<dyn StoreResolver>,    // NEW — the SAME resolver SlugRouter holds (one funnel)
    embed_service, vector_store, adapt_service, server_version,
    session_registry, pending_entries_analysis, services,    // UNCHANGED
    // REMOVED: store, entry_store (pre-resolved single handles — the #4974 boot-bound bypass)
}
```

> `dispatch_request` (in `observe.rs` / `router.rs`) currently takes `&observe_ctx.store` and `&observe_ctx.entry_store`. After resolving per-request, pass the resolved `Arc<Store>` for BOTH (the served store was used for both `store` and `entry_store` at boot, main.rs:1063/1066 — preserve that pairing with the per-request handle).

## B. `router.rs` — replace the top-level `/observe` arm with per-slug routing

```
// BEFORE (router.rs:182-188): match (method, path):
//    (GET, "/health") => health
//    (POST, "/observe") => observe_ctx handler with boot-bound store
//    ... else -> slug_router (MCP)
// AFTER:
fn call(request):
    method = request.method(); path = request.uri().path()
    match (method, path):
        (GET, "/health") => health_response                         // UNCHANGED, store-independent, stays top-level
        // DELETED: (POST, "/observe") top-level arm.
        // Observe is now a slug route. Dispatch by SUFFIX on the slug path:
        (POST, p) if p starts "/v1/" and p ends "/observe" =>
            route_observe(request)                                  // see C
        _ =>
            slug_router.route_mcp(request)                          // MCP path (Component 5), UNCHANGED edge
```

> Decision: observe is detected by the `/v1/.../observe` shape and dispatched to `route_observe`, which re-uses `parse_project_key` to get the `ProjectKey::Slug`. MCP (`/v1/{slug}/tools/...`) falls through to `route_mcp`. Both enter the ONE funnel; they differ only in handler (R-02 sole-route). Keep `/health` top-level (store-independent). This split logic is small — it helps `router.rs` reach ≤500 lines (Component 12 / AC-12) by removing the large inline observe block (`:188-~280`) into `route_observe`.

## C. `route_observe` (NEW — per-request resolve through the funnel)

```
async fn route_observe(observe_ctx: ObserveContext, request) -> Response:
    // 1. Transport-derived identity via the SAME grammar as MCP.
    key = match parse_project_key(request.uri().path()):
        Ok(k) => k
        Err(InvalidSlug)   => return json_error(400, "invalid project slug")
        Err(UnknownProject)=> return json_error(404, "unknown project")
    // 2. THE single funnel — resolve PER CALL (no boot-bound handle, the #4974 guard).
    store = match observe_ctx.resolver.resolve_store(&key):
        Ok(s) => s
        Err(UnknownProject) => return json_error(404, "unknown project")   // observe to unregistered slug -> loud (R-09)
        Err(InvalidSlug)    => return json_error(400, "invalid project slug")
    // 3. The rest of the existing observe handler body (router.rs:192-~280) UNCHANGED, except it
    //    now uses the per-request `store` instead of observe_ctx.store:
    identity = request.extensions().get::<ResolvedIdentity>() or 500
    size-check (Content-Length fast path) — unchanged
    wants_text = Accept contains "text/plain" — read BEFORE into_parts (unchanged, R-07 of vnc-024)
    collect limited body — unchanged
    hook_request = parse JSON or 400 — unchanged
    prefix_session_id(&mut hook_request) — unchanged
    response = dispatch_request(hook_request, &store /*per-request*/, &embed_service,
                 &vector_store, &store /*entry_store=per-request*/, &adapt_service,
                 &server_version, &session_registry, &pending_entries_analysis, &services)
    return map response (text/json per wants_text) — unchanged
```

## D. Boot binding deletion (main.rs — see also Component 7)

```
// DELETE main.rs:1045-1052:
//   let served_store = resolver.resolve_store(&ProjectKey::Default)?;
// DELETE the ObserveContext { store: served_store, entry_store: served_store, ... } construction.
// REPLACE with:
observe_ctx = ObserveContext { resolver: Arc::clone(&resolver), embed_service, vector_store,
                               adapt_service, server_version, session_registry,
                               pending_entries_analysis, services }
// PathRouter is constructed with this observe_ctx (which holds the resolver) AND the slug_router
// (which holds the SAME resolver) — one funnel, two entry handlers.
```

## Data Flow

- IN: `POST /v1/{slug}/observe` (verbatim from the bundle's `observe_url`).
- `parse_project_key` → `Slug` → `resolver.resolve_store` (per call) → `Arc<Store>` for the slug.
- OUT: hook dispatched against the per-slug store; 200 on success.

## Error Handling

- No-slug / unregistered observe → 404 `UnknownProject`, loud, never a default store (R-09/R-10).
- Identity missing from extensions → 500 (unchanged).
- Oversized body → 413; bad JSON → 400 (unchanged).

## Key Test Scenarios (hints)

1. R-02 sc.1 (structure): grep/AST assertion that `ObserveContext` holds NO pre-resolved `store`/`entry_store` field and NO boot-bound `resolve_store(&ProjectKey::Default)` survives; the handler resolves per call.
2. R-02 sc.2 (counting resolver, N=2): each observe request consults the resolver exactly once with the transport-derived key; no parallel observe dispatch path exists.
3. AC-07 (#766 repro): `init --bundle` Ping to `/v1/{slug}/observe` → 200 (was 404 on `/v1/observe`).
4. AC-08: runtime hook POST to `/v1/{slug}/observe` → 200, resolves to the bundle's store.
5. R-09 observe isolation at N=2: observe to `/v1/{A}/observe` writes A only; B untouched; and vice-versa.
6. Observe to an unregistered slug → 404 `UnknownProject`, never another store (edge).
