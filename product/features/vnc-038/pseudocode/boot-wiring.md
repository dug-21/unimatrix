# Component 7 — Boot Wiring (Rust)

**File:** `crates/unimatrix-server/src/main.rs`
**ADR:** ADR-003 (#5082), ADR-004 (#5083), ADR-006 (#5087) · **AC:** AC-09, AC-10 · **Risk:** R-10, R-13

## Purpose

Build the unified resolver from `project_slugs` ONLY. Empty `[[projects]]` ⇒ nothing servable + loud "register a project to begin" (no `DefaultResolver`, no auto-serve). Delete the boot-bound observe store resolution. Leave the local STDIO (`:1158`) / UDS (`:859`) boot paths completely untouched (ADR-006 hard boundary).

## A. Resolver build (MODIFY main.rs:1004-1043 — collapse the swap)

```
// BEFORE: if project_slugs.is_empty() { DefaultResolver::with_adapter(store, server, ...) }
//         else { MultiProjectRouter::from_servers(default_store=store, default_server=server, slug_servers, ...) }
// AFTER (ADR-004): no DefaultResolver branch; empty => nothing servable.

max_body = config.http.max_request_body_bytes
allowed_origins = config.http.allowed_origins.clone()

if project_slugs.is_empty():
    // AC-09 / R-10: loud, actionable, NOTHING servable. Do NOT build a resolver, do NOT auto-serve.
    tracing::error!("no projects registered — nothing is servable. \
                     Run `unimatrix register <slug>` then restart to begin.")
    // The HTTP listener either does not start the served stack, or starts a resolver with an
    // empty slug map so EVERY /v1/... request returns UnknownProject (404) loudly.
    // CHOICE (recommended): build an empty-slug-map MultiProjectRouter so /health still works and
    // every served request fails loud uniformly through the SAME funnel (no special no-projects code path).
    resolver = Arc::new(MultiProjectRouter::from_servers(vec![], max_body, allowed_origins))  // empty map
else:
    base_dir = paths.data_dir.parent().unwrap_or(&paths.data_dir)
    slug_servers = Vec::new()
    for slug in &project_slugs:
        input = http_provision::build_project_server(base_dir, slug, &embed_handle,
                    permissive, server_instructions.clone()).await?     // per-slug isolated subsystems (FR-C3)
        slug_servers.push(input)
    router = MultiProjectRouter::from_servers(slug_servers, max_body, allowed_origins)
                .map_err(ServerError::Config)?
    tracing::info!(slug_count = project_slugs.len(), "project routing active ([[projects]] declared)")
    resolver = Arc::new(router)
```

> The `store` / `server` (the former default single-project server) are NO LONGER threaded into the resolver as a default entry. Audit their remaining uses: they may still seed per-slug provisioning context or be removed if they only fed the deleted default. Flag any orphaned `store`/`server` binding (R-07 call-site audit) rather than leaving dead bindings.

## B. Observe boot binding (DELETE main.rs:1045-1052 — see Component 6)

```
// DELETE: let served_store = resolver.resolve_store(&ProjectKey::Default)?;
// REPLACE the ObserveContext construction to hold Arc::clone(&resolver) instead of served_store.
observe_ctx = ObserveContext {
    resolver: Arc::clone(&resolver),
    embed_service: Arc::clone(&embed_handle),
    vector_store: Arc::clone(&async_vector_store),
    adapt_service: Arc::clone(&adapt_service),
    server_version: env!("CARGO_PKG_VERSION").to_string(),
    session_registry: Arc::clone(&session_registry),
    pending_entries_analysis: Arc::clone(&pending_entries_analysis),
    services: services.clone(),
}
// PathRouter::new(resolver, observe_ctx): slug_router and observe handler share the SAME resolver.
```

## C. Local STDIO / UDS boot paths (DO NOT TOUCH — ADR-006 / C-13 / R-13)

```
// main.rs:859  (UDS bind)   — opens ~/.unimatrix/{hash}/unimatrix.db DIRECTLY, threads Arc<Store> to handler.
// main.rs:1158 (STDIO bind) — same direct path-hash open.
// HARD BOUNDARY: these paths do NOT call parse_project_key, do NOT construct the resolver,
// do NOT reference ProjectKey::Default, do NOT use a bundle. The Component 5/7 edits MUST NOT reach them.
// Component 11 (local-binding-guard) asserts this structurally.
```

## Data Flow

- IN: `project_slugs: Vec<ProjectSlug>` from `load_config_and_build_allowlist` (boot read of `[[projects]]`).
- OUT: `Arc<dyn StoreResolver>` (slug-keyed; empty map if no projects), shared by `SlugRouter` and the observe handler.
- Local boot: a separate, unchanged direct path-hash `Arc<Store>` → local handlers (never the resolver).

## Error Handling

- `build_project_server` failure (missing per-slug store — `register` is the sole creator) → loud `ServerError`, no auto-create (C5/OQ-PR-5, unchanged).
- Duplicate slug in config → `from_servers` returns Err → `ServerError::Config` (defensive; config-validate already rejects).
- Empty `[[projects]]` → not an error: a loud-but-running server whose served funnel rejects every request (AC-09/R-10).

## Key Test Scenarios (hints)

1. AC-09 / R-10: from empty `[[projects]]`, no servable store exists; every `/v1/...` request (MCP and observe) → 404 with actionable message; assert no adopt/derive/path-hash-migration code path runs.
2. Boot read loop (R-06 sc.2): write a `[[projects]]` stanza, restart, assert the slug is in `project_slugs` and resolves.
3. N=2 (AC-04): two stanzas → two per-slug servers built → both routable.
4. R-13 cross-check: assert the resolver-build / observe-binding edits do NOT touch `main.rs:859`/`:1158`; local boot still opens its path-hash store directly (Component 11 guard).
5. No orphaned default: assert no surviving `resolve_store(&ProjectKey::Default)` and no `DefaultResolver` construction at boot.
