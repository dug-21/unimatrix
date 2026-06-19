# crt-056 Pseudocode — OVERVIEW

> Per-slug intelligence parity. Wave 1 = service-config parity; Wave 2 = per-slug tick on the
> `BackgroundJob` seam. GH #787. Source-of-truth: ARCHITECTURE.md (ADR-001..006),
> SPECIFICATION.md (FR-1..18, AC-1..7), RISK-TEST-STRATEGY.md (R-01..12), IMPLEMENTATION-BRIEF.md.
>
> Every interface name/signature below is traced to the architecture's Integration Surface (§6)
> or to live source. No name is invented. Source line spans verified on the working tree.

## Components (per-file)

| File | Component | Wave | ADR |
|------|-----------|------|-----|
| `unimatrix-server-new.md` | `UnimatrixServer::new` additive `Option<ServiceLayer>` | 1 | ADR-001 |
| `build-project-server.md` | `build_project_server` config-parity threading | 1 | ADR-002, ADR-006 |
| `daemon-http-boot.md` | Daemon HTTP boot — thread Arcs, collect `PerSlugTickContext`s, retire global-handle wiring | 1+2 | ADR-002, ADR-003, ADR-005 |
| `per-slug-tick-context.md` | `PerSlugTickContext` borrow bundle | 2 | ADR-003 |
| `background-job-seam.md` | `BackgroundJob` trait + registry + `Cadence`/`ResourceClass`/`SharedTickResources`; 9 jobs | 2 | ADR-004 |
| `per-slug-tick-loop.md` | Serial loop, per-slug counter, serialized rayon | 2 | ADR-005 |
| `multi-slug-harness.md` | Layer-2 N=2 harness, rayon `panic_handler` | 2 | ADR-005, NFR-7 |

## Sequencing constraints (build order)

1. `unimatrix-server-new.md` (ADR-001) — the `Some(ServiceLayer)` arm must exist before any caller can pass one.
2. `build-project-server.md` (ADR-002) — depends on #1; builds the `Some(...)` value per slug.
3. `daemon-http-boot.md` (Wave 1 half) — depends on #2; passes resolved Arcs at the call site.
   Wave 1 completes here. **Wave-2-gating audit (A1 per-op + verify-the-funnel) runs FIRST, before any Wave 2 code** (R-01.2/R-02.2) — see `background-job-seam.md §Wave-2 gating audit`.
4. `per-slug-tick-context.md` (ADR-003) — the borrow bundle the jobs mutate.
5. `background-job-seam.md` (ADR-004) — trait + registry + shared types + 9 wrapped ops; depends on #4.
6. `per-slug-tick-loop.md` (ADR-005) — runs the registry over the contexts; depends on #4, #5.
7. `daemon-http-boot.md` (Wave 2 half) — collects contexts, builds `SharedTickResources`, drives the loop, retires `spawn_background_tick` global-handle wiring (`main.rs:957-991`); depends on #4, #5, #6.
8. `multi-slug-harness.md` — extends the existing Layer-2 harness for AC-3/4/5 at N=2.

## Data flow

### Wave 1 (config parity), per slug at boot
```
main.rs daemon HTTP boot — per-slug loop (main.rs:1084, call site 1085-1092)
  for slug in &project_slugs:
    build_project_server(base_dir, slug, &embed_handle, permissive, instructions,
                         + 8 resolved-config Arcs)              [ADR-002]
      ├─ open per-slug store + vector_index (existing 148-170)
      ├─ ServiceLayer::new(... config-driven values, Arc::clone(shared nli_handle) ...)
      │     └─ constructs the 5 analytics handles WITH correct config
      └─ UnimatrixServer::new(..., instructions, Some(service_layer))   [ADR-001]
            └─ serving path reads ServiceLayer's handles at query time
  daemon's OWN path: UnimatrixServer::new(..., Some(services))  (it already builds `services`
                     at main.rs:880-898 and currently DISCARDS it — same Some(...) parity path)
```

### Wave 2 (analytics maintained), per tick cycle
```
per-slug tick loop  — serial over Vec<PerSlugTickContext>      [ADR-005]
  for ctx in &contexts:                     # serial ⇒ rayon serialized free
    current_tick = ctx.next_tick()          # PER-SLUG counter (ctx.tick_metadata)
    for job in &registry:                   # registry order = ORDERING INVARIANT
      if job.cadence().fires(current_tick):
        job.run(ctx, &shared)               # mutates ONLY ctx's handle set + reads shared
          └─ existing tick op over ctx.store, writes ctx's ServiceLayer handles
    # serving path on this slug now reads the freshly-maintained handles (AC-5)
```

`shared: &SharedTickResources` = read-only `Arc`s (embed, the one loaded `nli_handle`,
`inference_config`, `confidence_params`, rayon_pool, audit, category_allowlist, retention_config).
No cross-context mutable state crosses `job.run`.

## Cross-component boundary: handle identity (the load-bearing integration point)

One handle set per slug, **built** at boot (ServiceLayer), **borrowed** by the tick
(PerSlugTickContext via `*_handle()` = `Arc::clone` of the same `Arc<RwLock<_>>`), **read** by
serving (same accessors). Divergence at any of the three points = green-but-broken (R-03).
Structural guard: `PerSlugTickContext` handles MUST be `Arc::clone`s of the slug's `ServiceLayer`
accessors, NEVER freshly constructed (pattern #4097: a copied inner-`T` makes post-construction
writes invisible). `Arc::ptr_eq` between tick-context handle and serving accessor is the test.

## Shared types (defined here; used across component files)

Handle type aliases are EXISTING (`services/mod.rs:47-60`), one `Arc<RwLock<_>>` per analytics state:
`ConfidenceStateHandle`, `EffectivenessStateHandle`, `TypedGraphStateHandle`,
`ContradictionScanCacheHandle`, `PhaseFreqTableHandle`. `TickMetadata` is EXISTING
(`server.rs:340,370`; counter logic `background.rs:352-358`).

NEW types introduced by crt-056 (location: `background.rs` new section, or a small new module
referenced from it):

```text
// ADR-003 — a thin BORROW bundle, NOT a new owner. Handles are Arc::clones of the slug's
// ServiceLayer accessors — the SAME Arc<RwLock<_>>.
struct PerSlugTickContext:
    slug:          ProjectSlug
    store:         Arc<Store>
    vector_index:  Arc<VectorIndex>
    confidence:    ConfidenceStateHandle           // = Arc<RwLock<ConfidenceState>>
    effectiveness: EffectivenessStateHandle
    typed_graph:   TypedGraphStateHandle
    contradiction: ContradictionScanCacheHandle
    phase_freq:    PhaseFreqTableHandle
    tick_metadata: Arc<Mutex<TickMetadata>>        // per-slug counter (ADR-005)

// ADR-004 — all read-only Arc; no cross-context mutable state crosses job.run.
struct SharedTickResources:
    embed_service:      Arc<EmbedServiceHandle>
    nli_handle:         Arc<NliServiceHandle>       // the ONE loaded model
    inference_config:   Arc<InferenceConfig>
    confidence_params:  Arc<ConfidenceParams>
    rayon_pool:         Arc<RayonPool>
    audit:              Arc<AuditLog>
    category_allowlist: Arc<CategoryAllowlist>
    retention_config:   Arc<RetentionConfig>

// ADR-004 — cadence is a STATIC predicate; no cron/signal machinery (Step B is OUT).
enum Cadence:
    EveryTick
    EveryN(u32)            // EveryN(n).fires(t) == (t % n == 0)
    fires(t: u64) -> bool

// ADR-004 — DECLARATION ONLY. Nothing in crt-056 reads it; forward hook for Step B's semaphore.
enum ResourceClass:
    Io
    Rayon

// ADR-004 — the seam. run touches ONLY ctx's handle set + shared (read-only) + rayon.
#[async_trait]
trait BackgroundJob: Send + Sync:
    fn name(&self) -> &str
    fn cadence(&self) -> Cadence
    fn resource_class(&self) -> ResourceClass
    async fn run(&self, ctx: &PerSlugTickContext, shared: &SharedTickResources) -> Result<(), String>

fn build_job_registry() -> Vec<Box<dyn BackgroundJob>>   // today's 9 ops, in ordering-invariant order
```

## Modular-file budget (500-line rule)

`background.rs` is already large; the Wave-2 additions are decomposed so no single file exceeds
500 lines. Recommended split (implementer confirms exact module names):
- `background/job.rs` — `BackgroundJob` trait, `Cadence`, `ResourceClass`, `SharedTickResources`, `PerSlugTickContext`, `build_job_registry()`.
- `background/jobs.rs` — the 9 job structs wrapping existing ops (each `run` delegates to the existing fn/inline block; no logic copied).
- `background/tick_loop.rs` — the serial `run_per_slug_tick_pass` loop + `spawn_per_slug_tick`.
- existing `background.rs` retains `run_single_tick` and the op fns the jobs delegate into; jobs CALL them, they are not deleted (no behavior copy).

## Constraint coverage map

| Constraint | Honored in |
|-----------|-----------|
| C-1 serial not concurrent | `per-slug-tick-loop.md` |
| C-2 work-unit boundary (build seam, not scheduler) | `background-job-seam.md` (NOT-built list), `per-slug-tick-loop.md` |
| C-3 one shared model, Arc::clone never rebuild, rayon serialized | `build-project-server.md`, `daemon-http-boot.md`, `per-slug-tick-loop.md` |
| C-4 preserve test-default constructor byte-for-byte | `unimatrix-server-new.md` |
| C-5 in-memory hot path (tick writes handles serving reads) | `per-slug-tick-context.md`, `daemon-http-boot.md` |
| C-6 one isolation seam, no cloud-only branch | `unimatrix-server-new.md`, `daemon-http-boot.md`, `per-slug-tick-loop.md` |
| C-7 global config only (no per-slug overrides) | `build-project-server.md` |
| C-8 no new analytics math | `background-job-seam.md` (jobs DELEGATE to existing ops) |
| C-9 cumulative test infra + N=2 | `multi-slug-harness.md` |
| C-10 closed 8-field parity (session_capabilities OUT) | `build-project-server.md` |

## Open questions / gaps flagged

- **G-1 (delivery item, not a blocker).** A2 interior-immutability of the shared `Arc`s
  (`nli_handle`, `inference_config`, `confidence_params`, `ml_inference_pool`, `embed_handle`) is a
  type-level audit owed at delivery (R-04). Pseudocode treats them as read-only per ADR-002; if the
  audit finds interior-mutable read-path state it is documented as a Step-B blocker, not built around.
- **G-2 (boot-loop module location).** `PerSlugTickContext` construction reads each slug's
  `ServiceLayer` handles. The architecture says location is "TBD in `background.rs` or new module."
  Pseudocode assumes `UnimatrixServer` exposes the per-slug `ServiceLayer` and `tick_metadata` to the
  boot loop. `services` and `tick_metadata` are `UnimatrixServer` fields (`server.rs:368,370`); the
  boot loop must be able to read them off `input.server` after `build_project_server` returns. If
  `ServiceLayer`/`tick_metadata` are not accessible from `ProjectServerInput.server` at the boot site,
  the implementer adds a thin `UnimatrixServer::per_slug_tick_context()` accessor (no new state). FLAGGED.
- **G-3 (vector_index from server).** `PerSlugTickContext.vector_index` must come from the slug's
  server/ServiceLayer. `build_project_server` builds `vector_index` (line 159-170) but
  `ProjectServerInput` exposes only `{slug, store, server}` (line 199-203). The implementer either
  reads it off `input.server.vector_index` (a `UnimatrixServer` field, `server.rs:351`) or adds it to
  `ProjectServerInput`. FLAGGED — additive, no behavior change either way.
</content>
</invoke>
