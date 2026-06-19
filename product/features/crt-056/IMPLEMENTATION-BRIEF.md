# crt-056 — Implementation Brief: Per-Slug Intelligence Parity

> Per-slug (per-project) MCP servers reach functional parity with the single-project daemon —
> correct service config (Wave 1) AND maintained analytics (Wave 2) — on a concurrency-clean,
> registry-based per-project tick work-unit. GH #787. Capability C5. Advances #4946
> (personal-cloud) and #4677 (self-learning).
>
> **Regenerated 2026-06-19 after human design-review rework** (OQ-5 settled, A1 audit elevated,
> source line attributions corrected, ADR Unimatrix IDs re-chained).

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-056/SCOPE.md |
| Scope Risk Assessment | product/features/crt-056/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-056/architecture/ARCHITECTURE.md |
| ADR-001 (additive constructor) | product/features/crt-056/architecture/ADR-001-additive-constructor.md |
| ADR-002 (config-parity threading) | product/features/crt-056/architecture/ADR-002-config-parity-threading.md |
| ADR-003 (ServiceLayer owns handle set) | product/features/crt-056/architecture/ADR-003-serviceLayer-owns-handle-set.md |
| ADR-004 (BackgroundJob seam) | product/features/crt-056/architecture/ADR-004-backgroundjob-seam.md |
| ADR-005 (serial loop / per-slug counter) | product/features/crt-056/architecture/ADR-005-serial-loop-per-slug-counter.md |
| ADR-006 (parity definition) | product/features/crt-056/architecture/ADR-006-parity-definition.md |
| Specification | product/features/crt-056/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/crt-056/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-056/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/crt-056/ACCEPTANCE-MAP.md |

## Goal

Bring per-slug MCP servers to functional parity with the single-project daemon: each registered
slug serves with the daemon's real resolved config (NLI on per config, correct fusion/PPR/confidence
params, operator category allowlist + domain packs, shared single loaded NLI/embedding model), and
its analytics handles are maintained by a per-slug tick. The tick is a concurrency-clean,
registry-based `BackgroundJob` work-unit over a `PerSlugTickContext` — the seam (not Step B's
scheduler) — so future background math is "register a job," not "re-architect the loop."

## Delivery Decomposition — Two Waves (sequential by dependency)

**Wave 1 — Service-config parity (the substrate).** Thread the daemon's resolved config + the one
shared loaded `nli_handle` into `build_project_server`; make `UnimatrixServer::new` accept a
pre-built `ServiceLayer` additively (test-default path preserved). Per-slug servers then serve at
config parity and hold analytics handles built *with correct config*.

**Wave 2 — Per-slug tick on the `BackgroundJob` seam (depends on Wave 1).**

> **FIRST ACT OF WAVE 2 — a Wave-2-GATING precondition, before any Wave 2 code.** Run the paired
> source audit:
> 1. **A1 per-op source audit** — for each of the 9 tick operations dispatched inside
>    `run_single_tick` (`background.rs`, fn def ~`413-804`; call sites at `463`/`552`/`566`/`629`/
>    `706`/`794`), source-confirm by closure-check that the op takes `&Store` and writes only the
>    passed-in handle — none closes over a global/static handle or store singleton.
> 2. **AC-4 verify-the-funnel source audit** — grep the job `run` path for a discarded resolved
>    handle (`let _`, unused binding) and for any global/shared analytics-handle write path beside
>    `PerSlugTickContext`; confirm the per-slug handle set is the **sole** mutation route.
>
> These run **together as the first act of Wave 2, not as an end-gate check** (RISK R-01.2/R-02.2).
> Rationale: if even one op carries a hidden global-handle write the probes missed, AC-4 can pass for
> the clean ops while the missed op corrupts B's state. The "MODERATE not HARD" verdict and the whole
> per-slug funnel rest on all 9 ops being confirmed store-parameterized *before* code is written.

After the gating audit: introduce `PerSlugTickContext` (a thin **borrow** of the slug's
`ServiceLayer` handle set + per-slug `TickMetadata`); a serial loop ticks each registered slug,
running a registered `BackgroundJob` registry; shared models + rayon pool passed read-only at loop
level with serialized rayon; per-slug `tick_counter`. Wave 2 maintains the handles Wave 1 builds.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `UnimatrixServer::new` (additive `Option<ServiceLayer>`) | pseudocode/unimatrix-server-new.md | test-plan/unimatrix-server-new.md |
| `build_project_server` (config-parity threading) | pseudocode/build-project-server.md | test-plan/build-project-server.md |
| Daemon HTTP boot (thread Arcs, collect contexts) | pseudocode/daemon-http-boot.md | test-plan/daemon-http-boot.md |
| `PerSlugTickContext` (borrow bundle) | pseudocode/per-slug-tick-context.md | test-plan/per-slug-tick-context.md |
| `BackgroundJob` trait + registry + `Cadence`/`ResourceClass`/`SharedTickResources` | pseudocode/background-job-seam.md | test-plan/background-job-seam.md |
| Per-slug tick loop (serial, per-slug counter, serialized rayon) | pseudocode/per-slug-tick-loop.md | test-plan/per-slug-tick-loop.md |
| Multi-slug test harness (Layer-2, rayon panic_handler) | pseudocode/multi-slug-harness.md | test-plan/multi-slug-harness.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |
| Wave-2 gating audit record (A1 per-op + funnel) | test-plan/wave2-gating-audit.md | Wave 2 start, Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. Components above are the
expected set from the architecture; actual file paths are confirmed during delivery.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Constructor refactor shape (OQ-4) | Additive: append final `services: Option<ServiceLayer>`; `Some(s)` ⇒ use it, `None` ⇒ existing test-default body. Test callers append `, None`. | SR-03, FR-7/FR-8 | architecture/ADR-001-additive-constructor.md |
| Config threading mechanism (FR-1) | Thread 8 daemon-resolved inputs into `build_project_server` params-at-end; `Arc::clone` the one loaded `nli_handle` — never rebuild. Global config only (no per-slug overrides; #785/C6). | SR-05, SR-06, A2/A3 | architecture/ADR-002-config-parity-threading.md |
| Handle ownership/lifecycle (OQ-1) | Per-slug `ServiceLayer` is the sole owner of that slug's handle set; `PerSlugTickContext` **borrows** it via `*_handle()` accessors (`Arc::clone` of the same `Arc<RwLock<_>>`). No parallel handle registry; no global-handle write path. | SR-01, SR-07, SR-08 | architecture/ADR-003-serviceLayer-owns-handle-set.md |
| BackgroundJob interface altitude (OQ-2) | Minimal trait: `name`/`cadence`/`resource_class`/`run(ctx, shared)`; static `Vec<Box<dyn BackgroundJob>>` registry; today's 9 ops registered as first jobs in order. Build the seam only — NO queue/pool/residency/cadence-signal. | SR-04, SR-07 | architecture/ADR-004-backgroundjob-seam.md |
| Tick loop + counter (OQ-3) | Serial loop over resident registered slugs; per-slug `tick_counter` (own `Arc<Mutex<TickMetadata>>` per context); rayon serialized free via serial loop, never held across all N in one closure. | SR-02, SR-09, A4 | architecture/ADR-005-serial-loop-per-slug-counter.md |
| Parity definition (OQ-5 — SETTLED) | Closed **8-field** checklist asserted field-by-field vs daemon's resolved config. `adapt_service`: per-slug independent state (same config). `session_capabilities`: **OUT of crt-056 parity scope** (human design-review decision — per-session/per-client handshake surface, no analytics dependence). AC-1 does NOT assert `session_capabilities`. | SR-05 | architecture/ADR-006-parity-definition.md |

ADRs are also stored in Unimatrix (post-design-review `context_correct` chain): ADR-001 #5136,
ADR-002 #5165, ADR-003 #5166, ADR-004 #5167, ADR-005 #5168, ADR-006 #5164.

## Files to Create / Modify

All paths under `crates/unimatrix-server/src/` unless noted.

| File | Wave | Change |
|------|------|--------|
| `server.rs` | 1 | `UnimatrixServer::new`: append `services: Option<ServiceLayer>`; move existing test-default body into the `None` arm (`server.rs:281-386`, defaults `306-333`). |
| `http_provision.rs` | 1 | `build_project_server`: append 8 config-parity params; build the config-driven `ServiceLayer`; pass `Some(service_layer)` to constructor (`125-204`; today's signature `125-131`). Replace per-slug `AdaptConfig::default()`/`CategoryAllowlist::new()` defaults at `180-181` with threaded operator values (adapt stays per-slug independent state). |
| `main.rs` | 1 + 2 | Daemon HTTP boot (`1077-1107`): in the per-slug loop (`main.rs:1084`, call site `main.rs:1085-1092`) pass the in-scope config `Arc`s (`Arc::clone` from `880-898`) to `build_project_server`; daemon's own path switches to `Some(services)` (it already builds them, currently discards). Retire the global-handle wiring: the five singleton handles extracted at `main.rs:957-961` and threaded into the `spawn_background_tick` call (`main.rs:968-991`) are removed from the multi-project path; build a `Vec<PerSlugTickContext>` and drive the new serial loop. |
| `background.rs` (new section) | 2 | Per-slug serial tick loop; `BackgroundJob` trait + `Cadence`/`ResourceClass`/`SharedTickResources`; `build_job_registry()`; wrap the existing 9 ops (live inside `run_single_tick`, fn def ~`413-804`; call sites `463`/`552`/`566`/`629`/`706`/`794`) as jobs preserving cadence and the ordering invariant (`547-550`: co-access promotion at `552` runs AFTER orphaned-edge compaction `~496-540`, BEFORE `TypedGraphState::rebuild` at `566`). Retain per-slug failure isolation (`393-395`), panic→restart wrapper (`286-301`), rayon `panic_handler`. |
| `services/mod.rs` | — | No structural change; `ServiceLayer` becomes the per-slug work-unit state owner. Existing `*_handle()` accessors (`274-316`) are the borrow surface. |
| New: `PerSlugTickContext` (location TBD in `background.rs` or new module) | 2 | The borrow bundle struct (see Data Structures). |
| Test harness (Layer-2 / multi-project) | 2 | Extend cumulatively; install rayon `panic_handler`; support a real two-slug tick (AC-4). No new isolated scaffolding. |

## Data Structures

```rust
// New — ADR-003. A thin BORROW bundle, NOT a new owner.
// Handles are Arc::clones of the slug's ServiceLayer accessors — the SAME Arc<RwLock<_>>.
pub struct PerSlugTickContext {
    pub slug: ProjectSlug,
    pub store: Arc<Store>,
    pub vector_index: Arc<VectorIndex>,
    pub confidence: ConfidenceStateHandle,        // = Arc<RwLock<ConfidenceState>>
    pub effectiveness: EffectivenessStateHandle,
    pub typed_graph: TypedGraphStateHandle,
    pub contradiction: ContradictionScanCacheHandle,
    pub phase_freq: PhaseFreqTableHandle,
    pub tick_metadata: Arc<Mutex<TickMetadata>>,  // per-slug counter (ADR-005)
}

// New — ADR-004. All read-only Arc; no cross-context mutable state crosses job.run.
pub struct SharedTickResources {
    pub embed_service: Arc<EmbedServiceHandle>,
    pub nli_handle: Arc<NliServiceHandle>,        // the ONE loaded model
    pub inference_config: Arc<InferenceConfig>,
    pub confidence_params: Arc<ConfidenceParams>,
    pub rayon_pool: Arc<RayonPool>,
    pub audit: Arc<AuditLog>,
    pub category_allowlist: Arc<CategoryAllowlist>,
    pub retention_config: Arc<RetentionConfig>,
}

pub enum Cadence { EveryTick, EveryN(u32) }   // EveryN(n).fires(t) = t % n == 0
pub enum ResourceClass { Io, Rayon }          // DECLARATION ONLY — nothing reads it in crt-056
```

Handle type aliases (existing, `services/mod.rs:47-60`): each is one `Arc<RwLock<_>>` per analytics
state. `TickMetadata` (existing, `server.rs:340,370`) holds the `tick_counter`; one per slug.

## Function Signatures

```rust
// ADR-001 — append final param; None arm preserves byte-for-byte test defaults.
UnimatrixServer::new(
    /* existing 10 params */, instructions: Option<String>,
    services: Option<ServiceLayer>,   // NEW, last
) -> Self

// ADR-002 — params-at-end; caller Arc::clones the daemon's resolved values (main.rs:880-898).
// Today's signature takes only (base_dir, slug, embed_handle, permissive, instructions) at
// http_provision.rs:125-131.
build_project_server(
    base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool, instructions: Option<String>,
    // crt-056 Wave 1 appended:
    rayon_pool: &Arc<RayonPool>, nli_handle: &Arc<NliServiceHandle>,
    nli_top_k: usize, nli_enabled: bool,
    inference_config: &Arc<InferenceConfig>, confidence_params: &Arc<ConfidenceParams>,
    categories: &Arc<CategoryAllowlist>, observation_registry: &Arc<DomainPackRegistry>,
) -> Result<ProjectServerInput, ServerError>

// ADR-004 — the seam. run touches ONLY ctx's handle set + shared (read-only) + rayon.
#[async_trait]
pub trait BackgroundJob: Send + Sync {
    fn name(&self) -> &str;
    fn cadence(&self) -> Cadence;
    fn resource_class(&self) -> ResourceClass;
    async fn run(&self, ctx: &PerSlugTickContext, shared: &SharedTickResources)
        -> Result<(), String>;
}
fn build_job_registry() -> Vec<Box<dyn BackgroundJob>>;

// Existing accessors (services/mod.rs:274-316) — the borrow surface for PerSlugTickContext.
ServiceLayer::confidence_state_handle() -> ConfidenceStateHandle
ServiceLayer::effectiveness_state_handle() -> EffectivenessStateHandle
ServiceLayer::typed_graph_handle() -> TypedGraphStateHandle
ServiceLayer::contradiction_cache_handle() -> ContradictionScanCacheHandle
ServiceLayer::phase_freq_table_handle() -> PhaseFreqTableHandle
```

Registered first jobs (preserve today's cadence + the ordering invariant `background.rs:547-550`):
`MaintenanceJob` (EveryTick, Io), `CoAccessPromotionJob` (EveryTick, Io — call site `552`, runs
AFTER orphaned-edge compaction `~496-540`, BEFORE TypedGraphRebuild at `566`), `TypedGraphRebuildJob`
(EveryTick, Rayon), `PhaseFreqRebuildJob` (EveryTick, Io), `ContradictionScanJob` (EveryN(n), Rayon —
existing interval, call site `706`), `NliInferenceJob` / `GraphEnrichmentJob` (Rayon, existing
cadence; enrichment call site `794`). Registry order IS the ordering invariant.

## Constraints

- **C-1 Serial, not concurrent.** Serial loop over resident registered slugs at OSS N. Correctness over throughput.
- **C-2 Work-unit boundary (build/don't build).** Job touches ONLY its `PerSlugTickContext` + read-only `SharedTickResources` + rayon. Build the seam; do NOT build queue/pool/residency/eviction/cadence-signals (Step B). Reviewer rejects scheduler machinery.
- **C-3 One shared model.** Embedding + NLI loaded once, shared read-only `Arc`; `Arc::clone`, never rebuild; never `NliServiceHandle::new()` on the per-slug path; rayon serialized across slugs, never held across all N in one closure.
- **C-4 Preserve test-default constructor.** `None` arm holds the exact prior body; existing unit tests compile/pass unchanged.
- **C-5 In-memory hot path (principle 7).** Tick writes the same handles the serving path reads; no DB reads at query time.
- **C-6 One isolation seam (vnc-034 ADR-003).** Daemon and per-slug servers traverse the same parity + tick path; `None` is unit-test-only; no cloud-only branch.
- **C-7 Global config only.** Parity is to the single resolved global config; no per-slug overrides (#785/C6).
- **C-8 No new analytics math.** Maintain existing analytics per-slug; add no scoring math.
- **C-9 Cumulative test infra + N=2.** Extend the Layer-2 / multi-project harness; AC-4 needs a real two-slug tick (never a unit stub); install rayon `panic_handler` (evidence #2543).
- **C-10 Parity definition is closed (8 fields).** AC-1 asserts field-by-field equality vs resolved config over the 8 ADR-006 fields, not a representative subset; `session_capabilities` is OUT and NOT asserted.

## Dependencies

- **Crates:** `unimatrix-server` (`main.rs`, `server.rs`, `http_provision.rs`, `background.rs`, `services/mod.rs`), `unimatrix-core` (analytics state types, `InferenceConfig`, `ConfidenceParams`, `CategoryAllowlist`, `DomainPackRegistry`), `unimatrix-store`, `unimatrix-embed`, NLI model/handle, `rayon`, `async_trait`.
- **Reused:** the 9 per-store-parameterized tick ops invoked from `run_single_tick` (`background.rs`, fn def ~`413-804`; loop body that invokes it at `363-391`); vnc-034 per-slug store registry/routing; the Layer-2 / multi-project test harness.
- **No blocking dependency** (per-slug stores/routing, C3, done). **Upstream of** #785 (C6 per-slug custom config) and C0★ (full-fidelity parity).

## NOT in Scope

- **Step B scale machinery** — bounded worker pool, LRU residency/eviction, lazy rebuild-on-cold-access, per-project tick cadence, concurrent rayon across slugs. Must not be precluded, must not be built.
- **Per-slug CUSTOM config** — per-project category/domain overlays or any per-slug config override. That is #785 / C6. crt-056 uses the global config only.
- **New functional analytics / new scoring math.**
- **`session_capabilities` in parity** — SETTLED OUT of crt-056 parity scope (ADR-006, human design-review decision). It is a per-session/per-client negotiated handshake surface, not a config-driven analytics or retrieval-quality field; it does not feed or depend on the per-slug analytics handles or threaded config. AC-1 does NOT assert it. If later required it is an additive, separately-scoped AC — not a 9th checklist item retro-fitted here.
- **The standing release gate** (N5 nfr-maintenance) and **#767** (model bake).
- **Any cloud-only code path** the local single-project install does not exercise (vnc-034 ADR-003).

## Alignment Status

Vision guardian: **6/6 PASS, 0 VARIANCE, 0 FAIL** (ALIGNMENT-REPORT.md, 2026-06-19). Directly advances
#4946 (C5 parity — literal completion of the goal's per-slug isolation criterion incl. analytics) and
#4677 (per-slug self-learning, no new math). Honors principles 5 (graceful degradation — absent
maintenance = clean-default handles, never another slug's state), 6 / single isolation seam, and 7
(in-memory hot path). Step B forward-seam handled the correct way per prior pattern #3742 (deferred in
all three docs, zero test scenarios for the scheduler) — no WARN. AC-4 correctly elevated to a
cross-tenant data-isolation proof (a cross-slug handle write would leak slug A's analytics into slug
B's serving results) without over-elevating defensiveness to a goal.

## Load-Bearing Items (carry forward into delivery)

- **Wave-2 gating audit (first act of Wave 2, before any Wave 2 code).** The A1 per-op source audit
  (each of the 9 tick ops takes `&Store` and reaches no global store singleton) paired with the AC-4
  verify-the-funnel source audit (no discarded resolved handle `let _`, no surviving global-handle
  write path beside `PerSlugTickContext`) is a **Wave-2-gating precondition**, NOT an end-gate check
  (RISK R-01.2/R-02.2). The whole per-slug funnel and the "MODERATE not HARD" verdict depend on all 9
  ops being confirmed store-parameterized before code is written.
- **AC-4 at N=2 is the critical, non-substitutable proof.** It is the cross-slug corruption guard, the
  cross-tenant data-isolation proof, AND the concurrency-readiness proof for AC-7. It MUST run at N=2
  against a running multi-project server with two real, differently-populated slugs. N=1 cannot
  distinguish a real per-slug funnel from a global-handle bypass (#4974 ceremonial-seam precedent).
  No N=1 test may stand in for it.
- **A2 interior-immutability audit is a delivery item (closed as accepted, Step-B precondition).** The
  serial loop HIDES interior-mutable read-path cache state on shared `Arc`s (`nli_handle`,
  `inference_config`, `confidence_params`, `ml_inference_pool`, `embed_handle`); it surfaces only under
  Step B. AC-2 (one model in memory, same `Arc` instances) covers it structurally; delivery MUST also
  run a type-level audit for interior mutability (`RwLock`/`Mutex`/`Cell`/`AtomicX`/unsynchronized
  cache) on the inference read path. Any mutability found is documented as a Step-B concurrency
  blocker, not silently accepted (R-04).

## Human-Confirmed Decisions (closed — not open questions)

These were resolved in the design-review rework; recorded for traceability:

1. **HQ-1 — large-N cadence envelope (A4, OQ-3): ACCEPTED.** The serial tick is accepted as correct
   "for modest N" at OSS scale; no near-term large-N OSS deployment is expected before Step B. The
   worst-case single-slug-tick monopolisation envelope is still documented (NFR-3, #2535); Step B
   priority is revisited only if large N is expected. Aligned with #4946 ("extend, never re-architect").
2. **A2 — interior-immutability: ACCEPTED as a Step-B precondition.** AC-2 covers it structurally now;
   the type-level audit is a delivery item (see Load-Bearing Items). Not a scope/vision deviation.
3. **OQ-5 — parity definition: SETTLED.** `adapt_service` is per-slug (independent state, same config);
   `session_capabilities` is **OUT** of crt-056 parity scope. AC-1 is the closed 8-field ADR-006
   checklist and does NOT assert `session_capabilities` (ADR-006).

## GitHub Issue

#787 — https://github.com/dug-21/unimatrix/issues/787
