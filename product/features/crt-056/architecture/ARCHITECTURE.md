# crt-056 — Architecture: Per-Slug Intelligence Parity

> Per-slug (per-project) MCP servers reach functional parity with the single-project daemon:
> correct service config (Wave 1) AND maintained analytics (Wave 2), on a concurrency-clean,
> registry-based per-project tick work-unit. GH #787. Capability C5.

All load-bearing claims below cite live source on `arch-research`. Files:
`server.rs`, `http_provision.rs`, `main.rs`, `background.rs`, `services/mod.rs` (all in
`crates/unimatrix-server/src/`).

---

## 1. System Overview

vnc-034 delivered multi-project **routing + per-slug stores** but left the per-slug serving
path **stubbed and un-maintained**. Two defects, one root cause: vnc-034 wired the per-slug
*path* but not the per-slug *intelligence*.

- **Defect 1 (config):** `UnimatrixServer::new` builds the per-slug `ServiceLayer` with
  hardcoded **test defaults** (`server.rs:306-333`): NLI disabled, rayon pool size 1, default
  `InferenceConfig`/`ConfidenceParams`, empty `CategoryAllowlist`, built-in-only domain packs,
  and a **fresh unloaded** `NliServiceHandle::new()`. The daemon, by contrast, builds its
  `ServiceLayer` with full config-driven values (`main.rs:880-898`).
- **Defect 2 (analytics):** the daemon extracts the five global handles from its single global
  `ServiceLayer` (`main.rs:957-961`) and spawns the background tick **once**, bound to the single
  global store, via `spawn_background_tick(...)` (`main.rs:968-991`); the tick never iterates the
  per-slug stores. Each per-slug `ServiceLayer` constructs its own analytics handles but **nothing
  rebuilds them**.

Both defects converge on the per-slug `ServiceLayer`: it *holds* the analytics handles
(`services/mod.rs:237-266` struct fields `confidence`, `effectiveness_state`, `typed_graph_state`,
`contradiction_cache`, `phase_freq_table`, exposed via the `*_handle()` accessors at
`services/mod.rs:274-316`). Goal 1 builds
it with correct config; Goal 2 wires a tick to maintain *those same handles*, which are exactly
what the per-slug serving path *reads* at query time (in-memory hot path, principle 7).

**The architectural pivot (resolves OQ-1, SR-01, SR-08 as one structural decision):** the
per-slug `ServiceLayer` IS the per-project work-unit's state. `BackgroundJob::run(ctx)` = "tick
this server's `ServiceLayer` handles." There is **one handle set per slug**, owned by that
slug's `ServiceLayer`, **referenced** (via the existing `*_handle()` accessors) by both the
serving path and the tick. No parallel registry of handles; no global handle write path beside
the per-slug seam.

### Two waves (sequential by dependency)

- **Wave 1 — service-config parity (the substrate).** Thread the config `Arc`s + the shared
  loaded `nli_handle` into `build_project_server`; make `UnimatrixServer::new` accept a
  pre-built `ServiceLayer` additively (test-default path preserved). Result: per-slug servers
  serve at config parity, and their analytics handles now exist *with correct config*.
- **Wave 2 — per-slug tick on the `BackgroundJob` seam.** Introduce `PerSlugTickContext` (the
  slug's store + a borrow of its `ServiceLayer` handle set + per-slug `TickMetadata`); a serial
  loop ticks each registered slug; shared models + rayon pool passed at loop level with
  serialized rayon; per-slug `tick_counter`. Express the per-slug tick as a `BackgroundJob`
  work-unit and register today's operations as the first jobs — the **seam**, not Step B's
  scheduler.

---

## 2. Component Breakdown

| Component | Responsibility | Touched by |
|-----------|---------------|------------|
| `UnimatrixServer::new` (`server.rs:281-386`) | Construct a server. **Wave 1:** additively accept a pre-built `ServiceLayer`; keep the test-default body as the `None` path. | ADR-001, ADR-002 |
| `build_project_server` (`http_provision.rs:125-204`) | Build one per-slug server. **Wave 1:** receive + thread the 7 config `Arc`s + shared `nli_handle`; build the config-driven `ServiceLayer`; pass it to the new constructor. | ADR-001, ADR-002 |
| Daemon HTTP boot (`main.rs:1077-1107`) | Loop over `project_slugs` (`main.rs:1084`), call `build_project_server` (`main.rs:1085-1092`), collect `ProjectServerInput`. **Wave 1:** pass the in-scope config `Arc`s. **Wave 2:** retain each slug's `PerSlugTickContext` and hand the set to the per-slug tick. | ADR-002, ADR-004 |
| `ServiceLayer` (struct `services/mod.rs:237-266`; accessors `274-316`) | **Owns** the five analytics handles + exposes `*_handle()` accessors. Unchanged structurally; it becomes the per-slug work-unit's state owner. | ADR-003 (rationale only) |
| `PerSlugTickContext` (new) | Bundle one slug's `{ slug, store, handle set, TickMetadata }`. The unit of work the tick iterates. | ADR-003 |
| `BackgroundJob` trait + registry (new, minimal) | The seam: `run(&PerSlugTickContext, &SharedTickResources)` + `cadence()` + `resource_class()`. Register today's tick operations as the first jobs. | ADR-004 |
| Per-slug tick loop (new, `background.rs`) | Serial loop over the registered `PerSlugTickContext`s; for each, run the registered jobs; serialized rayon; per-slug counter. | ADR-005 |
| `run_single_tick` / 9 tick ops (fn def `background.rs:413-804`; called from the loop body at `background.rs:363-391`; individual op call sites at `463`/`552`/`566`/`629`/`706`/`794`) | Already per-`&Store`-parameterized + idempotent; reach no global store singleton (re-verified, A1). Reused per-slug as-is. | ADR-004 (rationale) |

---

## 3. Component Interactions / Data Flow

### Wave 1 (config parity), per slug at boot
```
main.rs daemon HTTP (loop 1084; call 1085-1092)
  └─ build_project_server(base_dir, slug, embed, permissive, instructions,
                          + config Arcs, + shared nli_handle)        [ADR-002]
       ├─ open per-slug store + vector_index (existing 148-184)
       ├─ ServiceLayer::new(...config-driven values..., shared nli_handle)   [ADR-001]
       │     └─ constructs the 5 analytics handles WITH correct config
       └─ UnimatrixServer::new(..., Some(service_layer))            [ADR-001]
            └─ serving path reads ServiceLayer's handles at query time
```

### Wave 2 (analytics maintained), per tick cycle
```
per-slug tick loop  (serial over registered PerSlugTickContext[])   [ADR-005]
  for ctx in contexts:                       # serial ⇒ rayon serialized free (SR-02)
    current_tick = ctx.tick_metadata.next()  # PER-SLUG counter      [ADR-005, SR-09]
    for job in registry:                      # registered jobs       [ADR-004]
      if job.cadence().fires(current_tick):
        job.run(ctx, &shared)                 # mutates ONLY ctx's handle set (SR-01)
          └─ existing tick op over ctx.store, writes ctx's ServiceLayer handles
    # serving path on this slug now reads the freshly-maintained handles (SR-08, AC-5)
```

`shared` (`SharedTickResources`) = read-only `Arc`s: embedding handle, the one loaded
`nli_handle`, `inference_config`, `confidence_params`, the rayon pool. No cross-context mutable
state crosses `job.run`.

### Error / panic boundaries
- Per-slug tick failure is isolated: a slug's job error is logged and the loop continues to the
  next slug (mirrors the existing `run_single_tick` per-tick error log, `background.rs:393-395`).
  One slug's failure never aborts another's tick.
- The existing outer-handle panic→restart wrapper (`background.rs:286-301`) and rayon
  `panic_handler` are retained; the multi-slug test harness MUST install the rayon
  `panic_handler` (SR-10) — extend the Layer-2 harness, no new scaffolding.

---

## 4. Technology Decisions (ADR index)

| ADR | Title | Unimatrix ID | Resolves |
|-----|-------|--------------|----------|
| ADR-001 | Additive `UnimatrixServer::new` — pre-built `ServiceLayer` via `Option` | #5136 | OQ-4, SR-03 |
| ADR-002 | Config-parity threading into `build_project_server` (params-at-end) | #5165 | SR-05, SR-06, A2/A3 |
| ADR-003 | `ServiceLayer` owns the sole per-slug handle set; `PerSlugTickContext` borrows it | #5166 | OQ-1, SR-01, SR-07, SR-08 |
| ADR-004 | `BackgroundJob` work-unit seam — the SHAPE, not a scheduler | #5167 | OQ-2, SR-04, SR-07 |
| ADR-005 | Serial per-slug tick loop, per-slug `tick_counter`, serialized rayon | #5168 | OQ-3, SR-02, SR-09, A4 |
| ADR-006 | `adapt_service` per-slug; `session_capabilities` OUT of parity scope (settled) | #5164 | OQ-5, SR-05 |

> IDs reflect the post-design-review `context_correct` chain (originals #5137–#5141 were corrected to
> #5165–#5168 and #5164; ADR-001 #5136 unchanged).

Edges (high-bar, traversal-necessary): ADR-003 (#5166) `Prerequisite` ADR-002 (#5165) — the
config-driven `ServiceLayer` must exist before its handles can be borrowed; ADR-005 (#5168)
`Prerequisite` ADR-004 (#5167) — the serial loop runs the registry and feeds the counter to
`Cadence`. Intra-feature `Supports` spine left for retro (lessons/outcomes not yet logged).

---

## 5. Integration Points

- **Upstream (none blocking):** per-slug stores + routing (C3, vnc-034) are done. The 7
  config-parity inputs are already in scope at `main.rs` daemon boot (A3, verified at
  `main.rs:880-898` where the daemon's own `ServiceLayer` consumes them).
- **Shared, read-only (A2 — re-verify truly immutable):** `embed_handle`, `nli_handle` (the one
  loaded model), `inference_config`, `confidence_params`, the `ml_inference_pool`. These cross
  into every slug's serving `ServiceLayer` and into the tick as read-only `Arc`s.
- **Downstream:** #785 / C6 (per-slug CUSTOM config) and C0★ (full-fidelity parity) build on
  this. crt-056 uses the **global** resolved config only (SR-06).
- **One isolation seam (vnc-034 ADR-003):** the single-project daemon and per-slug servers must
  traverse the **same** parity path; no cloud-only branch (ADR-001 enforces this — see ADR-001
  Consequences).

---

## 6. Integration Surface

Exact names/signatures downstream agents implement against. Existing signatures are quoted from
source; new ones are crt-056's proposed contract.

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `UnimatrixServer::new` (existing) | `(entry_store, vector_store, embed_service, registry, audit, categories, store, vector_index, adapt_service, instructions: Option<String>) -> Self` | `server.rs:281-292` |
| `UnimatrixServer::new` (Wave 1, ADR-001) | append final param `services: Option<ServiceLayer>` — `Some(s)` ⇒ use `s`; `None` ⇒ build the existing test-default `ServiceLayer` (current body) | new, ADR-001 |
| `ServiceLayer::new` (existing) | `(store, vector_index, vector_store, entry_store, embed_service, adapt_service, audit, usage_dedup, boosted_categories, rayon_pool, nli_handle, nli_top_k: usize, nli_enabled: bool, inference_config, observation_registry, confidence_params, category_allowlist) -> Self` | `main.rs:880-898` |
| `build_project_server` (existing) | `async (base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>, permissive: bool, instructions: Option<String>) -> Result<ProjectServerInput, ServerError>` | `http_provision.rs:125-131` |
| `build_project_server` (Wave 1, ADR-002) | append (params-at-end, entry #2552/#2553): `rayon_pool: &Arc<RayonPool>, nli_handle: &Arc<NliServiceHandle>, nli_top_k: usize, nli_enabled: bool, inference_config: &Arc<InferenceConfig>, confidence_params: &Arc<ConfidenceParams>, categories: &Arc<CategoryAllowlist>, observation_registry: &Arc<DomainPackRegistry>` | new, ADR-002 |
| `ServiceLayer::*_handle()` (existing accessors) | `confidence_state_handle() -> ConfidenceStateHandle`; `effectiveness_state_handle() -> EffectivenessStateHandle`; `typed_graph_handle() -> TypedGraphStateHandle`; `contradiction_cache_handle() -> ContradictionScanCacheHandle`; `phase_freq_table_handle() -> PhaseFreqTableHandle` | `services/mod.rs:274-316` |
| Handle type aliases | `ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>` etc. (one `Arc<RwLock<_>>` per analytics state) | `services/mod.rs:47-60` |
| `PerSlugTickContext` (new, ADR-003) | `{ slug: ProjectSlug, store: Arc<Store>, vector_index: Arc<VectorIndex>, confidence: ConfidenceStateHandle, effectiveness: EffectivenessStateHandle, typed_graph: TypedGraphStateHandle, contradiction: ContradictionScanCacheHandle, phase_freq: PhaseFreqTableHandle, tick_metadata: Arc<Mutex<TickMetadata>> }` — handles are **clones of the slug's `ServiceLayer` accessors**, not new instances | new, ADR-003 |
| `BackgroundJob` (new, ADR-004) | `trait BackgroundJob { fn name(&self) -> &str; fn cadence(&self) -> Cadence; fn resource_class(&self) -> ResourceClass; async fn run(&self, ctx: &PerSlugTickContext, shared: &SharedTickResources) -> Result<(), String>; }` | new, ADR-004 |
| `Cadence` (new, ADR-004) | `enum Cadence { EveryTick, EveryN(u32) }` — `EveryN(n).fires(t) = t % n == 0`. No cron/cadence-signal machinery (Step B). | new, ADR-004 |
| `ResourceClass` (new, ADR-004) | `enum ResourceClass { Io, Rayon }` — a **declaration only** in crt-056 (a job tags itself); NO scheduler reads it. Forward hook for Step B's semaphore. | new, ADR-004 |
| `SharedTickResources` (new, ADR-004) | `{ embed_service: Arc<EmbedServiceHandle>, nli_handle: Arc<NliServiceHandle>, inference_config: Arc<InferenceConfig>, confidence_params: Arc<ConfidenceParams>, rayon_pool: Arc<RayonPool>, audit: Arc<AuditLog>, ... }` — all read-only `Arc` | new, ADR-004 |
| `TickMetadata` (existing) | holds `tick_counter`; today one global `Arc<Mutex<TickMetadata>>` per server. Wave 2 uses one per slug ⇒ per-slug counter falls out for free (ADR-005). | `server.rs:340,370`; `background.rs:352-358` |

---

## 7. How the Acceptance Criteria are satisfied (traceability)

- **AC-1/AC-2 (config parity, shared model):** ADR-002 threads the daemon's resolved config +
  the one loaded `nli_handle` into the per-slug `ServiceLayer`. Parity is a **closed checklist**
  (ADR-006): the 8 threaded fields, asserted field-by-field against the daemon's resolved config
  (SR-05). `session_capabilities` is **not** part of the AC-1 parity checklist — it is settled OUT
  of crt-056 scope (ADR-006).
- **AC-3/AC-5 (maintained + serving reads it):** ADR-003 — tick mutates the *same* handles the
  serving path reads. The AC-5 behavioral proof (search reflects post-tick state) is real, not
  "handle exists" (SR-08).
- **AC-4 (isolation, corruption guard):** ADR-003 makes the per-slug handle set the **sole**
  route the tick mutates — structural, not convention (SR-01). The test runs at **N=2** with two
  real slugs (SR-07); N=1 cannot distinguish funnel from bypass.
- **AC-6 (test path preserved):** ADR-001 — `None` ⇒ existing test-default body, byte-for-byte
  behavior (SR-03).
- **AC-7 (concurrency-clean work-unit):** ADR-003 + ADR-004 + ADR-005 — `job.run` touches only
  its `PerSlugTickContext` + read-only `SharedTickResources` + rayon; no cross-context mutable
  state. **AC-4 doubles as the concurrency-readiness proof** (SCOPE L135-137).

---

## 8. Open Questions for the Human

> **OQ-5 — SETTLED (not open).** `session_capabilities` is **OUT** of crt-056 parity scope; it is
> NOT a recommendation but a decision the human made in design review. It is a per-session/per-client
> negotiated surface (MCP initialize handshake), not a config-driven analytics or retrieval-quality
> field, and does not feed the per-slug analytics handles or the threaded config. AC-1's parity
> checklist does **not** include it. `adapt_service` per-slug independence stays settled (ADR-006).
> See ADR-006 for the full rationale.

1. **A2 re-verification (cross-cutting, flagged for spec/test).** ADR-002 assumes the shared
   `Arc`s (`nli_handle`, `inference_config`, `confidence_params`, `ml_inference_pool`,
   `embed_handle`) are truly immutable / safe to share read-only across N slugs concurrently
   later. The `nli_handle` carries a loaded-model state machine; the architect asserts it is
   read-only **at inference time**, but a test asserting "one model in memory, no per-slug
   mutation" (AC-2) is the proof. Confirm no interior-mutable cached state in these handles
   (would convert a shared resource into an SR-01-class hazard).
2. **Serial-loop cadence envelope (A4, SR-02).** The serial loop is correct for "modest N." The
   worst-case full-loop duration ≈ N × single-slug-tick; at large N the loop falls behind the
   tick interval before Step B exists. This is **accepted** per Non-Goals, but the human should
   confirm the OSS N envelope assumption (documented, not built around).
