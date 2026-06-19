# Component: Daemon HTTP boot — thread Arcs, collect contexts, retire global-handle wiring

> Wave 1 + Wave 2. ADR-002, ADR-003, ADR-005. Resolves the FR-1 call-site threading and the FR-11/
> FR-12 context collection; retires the legacy `spawn_background_tick` global-handle path (FR-14).
> Source: per-slug loop `main.rs:1084` (call site 1085-1092); daemon `ServiceLayer` 880-898;
> handle extraction 957-961; `spawn_background_tick` call 968-991.

## Purpose

The convergence point. In `main.rs` it (1) threads the daemon's resolved config `Arc`s into each
`build_project_server` call (Wave 1); (2) for daemon AND each slug, builds a `PerSlugTickContext` from
the server's own `ServiceLayer` + `tick_metadata`; (3) builds `SharedTickResources` once; (4) drives
the serial loop (`spawn_per_slug_tick`); and (5) **removes** the global-handle extraction (957-961)
and `spawn_background_tick(...)` (968-991) from the multi-project path — the #4974 funnel guard
(no surviving global-handle write path beside the per-slug seam).

## Wave 1 — thread resolved config into the per-slug loop

The 8 resolved values already exist in scope at the daemon boot fn (the daemon's own `ServiceLayer`
consumes them at 880-898): `ml_inference_pool`, `nli_handle`, `config.inference.nli_top_k`,
`config.inference.nli_enabled`, `inference_config`, `confidence_params`, `categories`,
`observation_registry`. Thread `Arc::clone`s of the SAME values into the per-slug call (C-3: clone,
never rebuild — guarantees one model in memory, AC-2).

```text
# main.rs multi-project branch (was 1083-1094):
let mut slug_servers: Vec<ProjectServerInput> = Vec::new()
for slug in &project_slugs:
    let input = build_project_server(
        base_dir, slug, &embed_handle, permissive, server_instructions.clone(),
        # crt-056 Wave 1 appended — Arc::clone the daemon's resolved values (from 880-898 scope):
        &Arc::clone(&ml_inference_pool),
        &Arc::clone(&nli_handle),                 # the ONE loaded model — shared, never rebuilt (AC-2)
        config.inference.nli_top_k,
        config.inference.nli_enabled,
        &Arc::clone(&inference_config),
        &Arc::clone(&confidence_params),
        &Arc::clone(&categories),
        &Arc::clone(&observation_registry),
    ).await?
    slug_servers.push(input)
# (If build_project_server also needs boosted_categories per build-project-server.md's flag, thread it
#  from the same resolved source — FLAGGED there.)
```

> The daemon's OWN server (built at 919-933 via `UnimatrixServer::new`) switches to passing
> `Some(services)` — it already builds `services` at 880-898 and currently discards it for the
> in-constructor default. This is the ADR-001 same-`Some(...)`-path compliance (C-6); it is a one-line
> change at the daemon's `UnimatrixServer::new` call (append `, Some(services)`).

## Wave 2 — collect contexts, build shared resources, drive the loop, retire global wiring

```text
# Build the context set: daemon (N=1) or multi-project (N≥2). One mechanism for both (C-6).
let mut contexts: Vec<PerSlugTickContext> = Vec::new()

# (a) Daemon's own context — from the daemon server's ServiceLayer + tick_metadata.
#     `services` (880-898) is the daemon ServiceLayer; `server.tick_metadata` is its counter.
contexts.push(PerSlugTickContext::from_server(
    DAEMON_SLUG /* the daemon's own ProjectKey/slug identity */,
    Arc::clone(&store),
    &services,                          # the daemon's config-driven ServiceLayer (G-2: accessible at boot)
    Arc::clone(&server.tick_metadata),
    Arc::clone(&vector_index),
))

# (b) Per-slug contexts — for the multi-project branch, AFTER build_project_server returns each input.
for input in &slug_servers:
    contexts.push(PerSlugTickContext::from_server(
        input.slug.clone(),
        Arc::clone(&input.store),
        input.server.service_layer(),   # G-2: read the slug's ServiceLayer off the server
        Arc::clone(&input.server.tick_metadata),
        input.server.vector_index(),    # G-3: vector_index off the server (or add to ProjectServerInput)
    ))

# Build SharedTickResources ONCE — read-only Arcs, the same resolved values (C-3).
let shared = SharedTickResources {
    embed_service:      Arc::clone(&embed_handle),
    nli_handle:         Arc::clone(&nli_handle),        # the ONE loaded model
    inference_config:   Arc::clone(&inference_config),
    confidence_params:  Arc::clone(&confidence_params),
    rayon_pool:         Arc::clone(&ml_inference_pool),
    audit:              Arc::clone(&audit),
    category_allowlist: Arc::clone(&categories),
    retention_config:   Arc::clone(&retention_config),
}

# Drive the serial loop (per-slug-tick-loop.md). This is the SOLE tick path now.
let tick_handle = spawn_per_slug_tick(contexts, shared)

# ── RETIRE the global-handle wiring (FR-14, #4974 guard) ──────────────────────────────
# DELETE main.rs:957-961  (confidence_state_handle()/effectiveness_state_handle()/typed_graph_handle()/
#                          contradiction_cache_handle()/phase_freq_table_handle() global extraction)
# DELETE main.rs:968-991  (spawn_background_tick(...) call with the 5 global handles)
# There must be NO surviving global-handle write path beside the per-slug seam. The Wave-2 verify-
# the-funnel audit greps for any residual `*_handle()` extraction feeding a tick. (R-01.2/R-02.2)
```

> Single-project (non-multi) deployments that DON'T declare `[[projects]]` still take path (a) only:
> one daemon context, the same `spawn_per_slug_tick`. No separate code path (C-6). The legacy
> `spawn_background_tick` symbol is removed from the daemon path entirely; if it has no other caller,
> delete the fn (ADR-005 "retired in favor of this loop"). FLAGGED: confirm no other caller before deleting.

## Data flow
- Inputs: resolved config `Arc`s (880-898 scope), `project_slugs`, `embed_handle`, per-slug `ProjectServerInput`s.
- Outputs: `Vec<PerSlugTickContext>`, one `SharedTickResources`, a `JoinHandle` for the tick; the
  per-slug servers wired into `MultiProjectRouter` exactly as today (1095-1101, unchanged).
- Transformation: `Arc::clone` of shared resources into both `build_project_server` (Wave 1) and
  `SharedTickResources` (Wave 2) — same allocations, one model in memory.

## Error handling
- `build_project_server` errors propagate via `?` as today (unregistered slug → `ServerError::Config`).
- Context construction is infallible (borrows). `spawn_per_slug_tick` returns a `JoinHandle`; the loop's
  internal panic→restart + per-slug error isolation are owned by `per-slug-tick-loop.md`.
- Boot must not silently fall back to defaults for any of the 8 threaded fields (required params → a
  missing one is a compile error at the call site, not a runtime degrade — the anti-Defect-1 guard).

## Flagged gaps (also in OVERVIEW)
- **G-2:** `PerSlugTickContext::from_server` needs the slug's `ServiceLayer` + `tick_metadata` readable
  off `input.server`. They are `UnimatrixServer` fields (`server.rs:368,370`). If not exposed, add thin
  accessors `service_layer()` / `tick_metadata()` (no new state). The daemon's own `services` (880-898)
  is in local scope — directly usable.
- **G-3:** `vector_index` for the context. `ProjectServerInput` exposes `{slug,store,server}` (1099-203);
  read `input.server.vector_index` (`server.rs:351`) via accessor, or add `vector_index` to
  `ProjectServerInput`. Additive either way.

## Key test scenarios (hints for tester)
- **AC-1/AC-2 boot wiring.** After boot, each per-slug server's `ServiceLayer` matches the daemon's
  resolved config (8 fields) and shares the same `nli_handle`/`embed_handle` `Arc` (`Arc::ptr_eq`).
- **R-06.2 same-path.** Daemon and per-slug both reach `Some(config-driven)`; no cloud-only branch.
- **#4974 funnel guard (R-01.2).** Source audit: 957-961 + 968-991 removed; no surviving `*_handle()`
  extraction feeding a tick; `spawn_per_slug_tick` is the sole tick spawn.
- **AC-3/AC-4/AC-5** are exercised against this boot (running multi-project server) — see harness file.
- **N=1 daemon path** builds exactly one context and ticks via the same loop (C-6 structural check).
</content>
