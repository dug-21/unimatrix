# crt-056 Specification — Per-Slug Intelligence Parity

> Source: `product/features/crt-056/SCOPE.md` (scoped 2026-06-19, GH #787).
> Risk basis: `product/features/crt-056/SCOPE-RISK-ASSESSMENT.md`.
> Downstream consumers: architect, pseudocode, tester, risk strategist.

## Objective

Bring per-slug (per-project) MCP servers to **functional parity** with the single-project
daemon — correct service config **and** maintained analytics — so a registered slug is a
first-class Unimatrix rather than a degraded one. vnc-034 delivered multi-project routing and
per-slug stores but left the serving path stubbed (test-config mode) and the analytics
un-maintained; crt-056 closes both defects on a **concurrency-clean, registry-based per-project
tick work-unit** that establishes the seam for future background math without building Step B's
scheduler.

## Scope Waves

The feature is two sequential waves; Wave 2 depends on Wave 1 (Wave 2 maintains the handles
Wave 1 builds).

- **Wave 1 — Service-config parity (the substrate).** Thread real config into the per-slug
  `ServiceLayer` so per-slug servers serve at config parity and hold analytics handles built
  with correct config.
- **Wave 2 — Per-slug tick on the `BackgroundJob` seam.** A `PerSlugTickContext` per registered
  slug, iterated serially, maintaining each slug's own handle set with no cross-context shared
  mutable state.

---

## Functional Requirements

### Wave 1 — Service-config parity

- **FR-1 (config threading).** `build_project_server` (`http_provision.rs`) MUST receive and
  thread the daemon's resolved config inputs into the per-slug `ServiceLayer`: `config`,
  `ml_inference_pool`, `nli_handle`, `inference_config`, `confidence_params`, `categories`
  (operator category allowlist + domain packs), and `observation_registry`. These are already in
  scope in `main.rs` at the per-slug call site (`main.rs:1085-1091`, inside the
  `for slug in &project_slugs` loop at `main.rs:1084`); FR-1 threads them into
  `build_project_server` (today `http_provision.rs:125-131` takes only
  `base_dir, slug, embed_handle, permissive, instructions`) rather than synthesizing new ones.
  *Testable:* per-slug `ServiceLayer` construction reads each input from the resolved daemon
  config, not from a test default.

- **FR-2 (NLI per config).** A per-slug `ServiceLayer` MUST have NLI **enabled when the daemon's
  config enables it** (and disabled when the config disables it) — never hardcoded off.
  *Testable:* per-slug NLI-enabled flag equals the daemon's resolved NLI-enabled flag.

- **FR-3 (correct scoring params).** A per-slug `ServiceLayer` MUST use the daemon's resolved
  `InferenceConfig` (fusion/PPR weights) and `ConfidenceParams`, not defaults.
  *Testable:* per-slug `InferenceConfig` and `ConfidenceParams` equal the daemon's resolved
  values, field by field.

- **FR-4 (operator categories + domain packs).** A per-slug `ServiceLayer` MUST use the
  operator's resolved `CategoryAllowlist` and domain packs, not an empty allowlist /
  built-in-only packs.
  *Testable:* per-slug category allowlist and domain pack set equal the daemon's resolved set.

- **FR-5 (rayon pool sized per config).** The per-slug serving path MUST observe a rayon ML
  inference pool sized per the daemon's config (the shared pool), not the hardcoded size-1 test
  pool.
  *Testable:* per-slug servers reference the daemon's pool; effective pool size equals config.

- **FR-6 (shared loaded model).** All per-slug `ServiceLayer`s MUST reference the **single**
  loaded NLI model and the single loaded embedding model via shared read-only `Arc`s — never a
  fresh unloaded `NliServiceHandle` and never N model copies.
  *Testable:* exactly one NLI model and one embedding model are loaded in process; per-slug
  handles are the same `Arc` instances as the daemon's.

- **FR-7 (pre-built `ServiceLayer` constructor).** `UnimatrixServer::new` MUST accept a
  pre-built `ServiceLayer` so the per-slug path supplies the config-driven layer. The change MUST
  be **additive**: the existing test-default construction path is preserved (see FR-8). The
  single-project daemon MUST traverse the **same** parity construction path as per-slug servers —
  no cloud-only branch (vnc-034 ADR-003 isolation seam).
  *Testable:* per-slug and single-project daemon both build their `ServiceLayer` through the
  config-driven path; only unit tests use the test-default path.

- **FR-8 (test-default path preserved).** The pre-existing `UnimatrixServer::new` test-default
  construction (NLI off, pool size 1, default `InferenceConfig`/`ConfidenceParams`, empty
  allowlist) MUST remain available for unit tests. Existing unit tests MUST compile and pass
  unchanged in behavior.
  *Testable:* existing `UnimatrixServer::new` unit-test call sites still construct a valid server
  with test defaults.

- **FR-9 (parity scope = global config only).** The threaded config MUST be the daemon's
  **single resolved global** config. crt-056 MUST NOT introduce per-slug config overrides
  (per-project categories/domain overlays) — that is #785 / capability C6.
  *Testable:* no per-slug config override path exists; all per-slug servers resolve to the same
  global config values.

- **FR-10 (closed parity checklist — `adapt_service` in; `session_capabilities` OUT).** The
  specification MUST define a closed parity checklist (per OQ-5, resolved by the human and
  ADR-006). The following parity decisions are recorded here and MUST be honored:
  - `adapt_service` adaptive state is **per-slug** (independent state per project; this matches
    the current per-`ServiceLayer` ownership and the in-memory-hot-path principle — each slug
    adapts to its own store). It is NOT shared across slugs. Parity here means **same config,
    independent state**: `AdaptConfig::default()` is the resolved value today
    (`build_project_server` constructs `AdaptationService::new(AdaptConfig::default())`).
  - `session_capabilities` is **OUT of crt-056 parity scope.** It is a per-session/per-client
    negotiated surface (the agent capability allowlist sourced from `[agents]
    session_capabilities` in config and enforced via `AgentRegistry`), not a config-driven
    analytics or retrieval-quality field. It does not feed, and is not fed by, the per-slug
    analytics handles or the threaded service config; neither crt-056 defect (test-config
    serving, dead analytics) touches it. Including it would expand scope without advancing the
    C5 "first-class Unimatrix" claim (retrieval quality + maintained analytics). If the human
    later rules it in, it becomes an additive 9th AC-1 checklist item — not a re-architecture.
  *Testable:* per-slug `adapt_service` state is independent per slug (no cross-slug bleed);
  the AC-1 parity checklist is the 8 config-driven fields (ADR-006) and does NOT assert
  `session_capabilities`.

### Wave 2 — Per-slug tick on the `BackgroundJob` seam

- **FR-11 (`PerSlugTickContext`).** A `PerSlugTickContext` MUST exist, one per registered
  resident slug, bundling that slug's store handle and its `ServiceLayer` analytics **handle
  set** (`ConfidenceState`, `EffectivenessState`, `TypedGraphState`, `PhaseFreqTable`,
  `ContradictionScanCache`, `TickMetadata`) plus a **per-slug** `tick_counter`.
  *Testable:* a registered slug yields exactly one `PerSlugTickContext`; its handle set is the
  same instance the serving path reads (FR-15).

- **FR-12 (serial registry-based loop).** The tick loop MUST iterate the resident registered
  slugs **serially**, one `PerSlugTickContext` at a time. The set of ticked slugs MUST be derived
  from the registry, not hardcoded into the loop.
  *Testable:* ticking N registered slugs visits each `PerSlugTickContext` exactly once per loop
  pass; adding/removing a registered slug changes the iterated set with no loop-body edit.

- **FR-13 (per-slug analytics maintenance).** For each `PerSlugTickContext`, the tick MUST run
  the existing tick operations against **that slug's own store** and rebuild **that slug's own
  handle set** — maintenance/effectiveness, co-access promotion, `TypedGraphState::rebuild`,
  `PhaseFreqTable::rebuild`, contradiction scan, extraction, NLI inference, graph enrichment.
  No new scoring math is added (SCOPE Non-Goal).
  *Testable:* after a tick, a slug's confidence / co-access graph / phase table / contradiction
  cache reflect that slug's store contents (AC-3).

- **FR-14 (no cross-context shared mutable state).** The per-slug tick MUST mutate **only** its
  own `PerSlugTickContext` handle set. It MUST NOT write any global/shared analytics handle and
  MUST NOT write another slug's handles. The naive "iterate over N stores writing shared global
  handles" approach is explicitly prohibited — there MUST be no parallel global-handle write path
  beside the per-slug seam.
  *Testable:* ticking slug A leaves slug B's handle set byte-for-byte unchanged (AC-4); a code
  audit finds the per-slug handle set is the sole mutation route.

- **FR-15 (serve/tick handle identity — in-memory hot path).** The handle set the tick rebuilds
  MUST be the **same instance** the per-slug serving path reads at query time. There MUST be no
  second handle set for serving. No DB reads occur on the query hot path (principle 7).
  *Testable:* a search on slug A reflects A's post-tick maintained state, not stale state (AC-5).

- **FR-16 (`BackgroundJob` work-unit seam).** The per-slug tick MUST be expressed as a
  `BackgroundJob` work-unit whose `run` operates over a `PerSlugTickContext`. Today's tick
  operations MUST be **registered** as the first jobs, each declaring its **cadence** and
  **resource class**. Adding a hypothetical new background job MUST be "implement the interface +
  register," not a loop rewrite.
  *Testable:* a new no-op job can be added by implementing the interface and registering it,
  without editing the loop body (demonstrated structurally; AC-7).

- **FR-17 (serialized rayon access).** Per-slug ticks MUST serialize access to the shared rayon
  ML inference pool across slugs. Under the serial loop this is automatic; the loop MUST NOT hold
  rayon across all N slugs in a single closure (one slug's rayon work completes before the next
  slug's begins).
  *Testable:* no two slugs' tick closures hold rayon concurrently; rayon is entered/exited within
  a single slug's tick.

- **FR-18 (per-slug tick counter).** Each `PerSlugTickContext` MUST own its own `tick_counter`
  driving interval gates (`tick % 4 == 0`, contradiction-scan cadence). The loop-global counter
  MUST NOT gate per-slug interval operations (OQ-3 resolved to per-slug counters — required by the
  no-cross-context-shared-mutable-state contract). No per-slug job may read loop-global counter
  state.
  *Testable:* interval gates fire independently per slug based on that slug's counter; no shared
  mutable counter is read inside a job.

---

## Non-Functional Requirements

- **NFR-1 (serial, not concurrent).** The OSS tick is a serial loop over resident registered
  slugs. Correctness over throughput at modest N. Concurrency (bounded worker pool, concurrent
  rayon) is OUT (Step B). The serial loop is the accepted execution model; the work-unit shape
  must not *preclude* concurrency but must not *implement* it.

- **NFR-2 (one shared model).** Embedding and NLI models are loaded **once** and shared
  read-only across all slugs (`Arc`). Never N copies; never a per-slug unloaded handle.
  Shared `Arc`s passed to ticks (models, `ConfidenceParams`) MUST be truly immutable — any
  interior-mutable cached state on a "read-only" resource is a cross-slug hazard and is
  disallowed (assumption A2 must be verified by the architect).

- **NFR-3 (serialized rayon — monopolisation envelope).** The shared rayon ML pool serves both
  the MCP hot path and per-slug ticks. The worst-case single-slug tick duration is the
  rayon-monopolisation envelope for MCP latency and MUST be documented. The serial loop MUST NOT
  hold rayon across all N slugs in one closure (evidence #2535).

- **NFR-4 (in-memory hot path, principle 7).** The tick writes the same per-slug handles the
  serving path reads; the query hot path performs **no DB reads** for analytics state.

- **NFR-5 (single isolation seam — vnc-034 ADR-003).** Per-slug parity MUST NOT introduce a
  cloud-only code path the local single-project install never exercises. The single-project
  daemon and per-slug servers traverse the same parity construction and tick path. There is one
  isolation seam, not a cloud-only branch.

- **NFR-6 (additive change).** The constructor refactor and tick changes MUST be additive: no
  existing single-project behavior regresses; the test-default constructor path is preserved.

- **NFR-7 (cumulative test infrastructure).** Tests MUST extend the existing multi-project /
  Layer-2 harness rather than create isolated scaffolding. The corruption guard (AC-4) requires a
  **real two-slug tick** against a running multi-project server, not a unit stub. The multi-slug
  tick harness MUST install the rayon `panic_handler` so a tick-closure panic does not SIGABRT the
  test process (evidence #2543).

- **NFR-8 (idempotent per-store operations).** The 9 tick operations are per-store-parameterized
  and idempotent; per-slug invocation relies on each operation reaching no global store singleton
  (assumption A1). The architect MUST re-verify per operation that none closes over a global
  handle.

- **NFR-9 (Step B not precluded; cadence assumption stated).** The serial loop is correct for
  modest N. If real deployments register large N, the serial tick may fall behind before Step B
  exists — this is accepted. The design MUST NOT preclude Step B (the scheduler is an additive
  follow-up requiring zero work-unit changes); the modest-N cadence assumption is stated, not
  silently relied upon (assumption A4).

---

## Acceptance Criteria

All AC-IDs originate in SCOPE.md. AC-3, AC-4, AC-5 are **behavioral two-slug tests against a
running multi-project server**. AC-6 preserves the test-default constructor. AC-4 is load-bearing
and doubles as the concurrency-readiness proof (SR-07).

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| **AC-1** (config parity) | A per-slug server reports NLI enabled (when config enables it), the daemon's fusion/PPR/confidence params, the operator category allowlist + domain packs, and a rayon pool sized per config. | **Field-by-field equality assertion** against the daemon's **resolved** config (not a representative subset), over the **8-field ADR-006 checklist**: `nli_enabled`, `nli_top_k`, shared loaded `nli_handle`, `InferenceConfig`, `ConfidenceParams`, `CategoryAllowlist`, domain pack set (`observation_registry`), effective rayon pool size. **`session_capabilities` is OUT (FR-10) and is NOT asserted.** Covers FR-2..FR-5, FR-9, FR-10. (SR-05) |
| **AC-2** (shared model) | All per-slug servers reference the one loaded NLI/embedding model. | Assert exactly one NLI model and one embedding model loaded in process; assert per-slug model handles are the same `Arc` instances as the daemon's (no unloaded per-slug handle, no N copies). Covers FR-6, NFR-2. |
| **AC-3** (analytics maintained) | Store to slug A → run a tick → A's confidence/co-access/phase/contradiction caches reflect the write. | **Behavioral test, running multi-project server:** write entries to slug A, run one tick, assert A's `ConfidenceState`/co-access graph/`PhaseFreqTable`/`ContradictionScanCache` changed to reflect the write (not "the handle exists"). Covers FR-13. |
| **AC-4** (isolation — corruption guard) | After ticking A then B, A's `TypedGraphState`/`PhaseFreqTable`/`EffectivenessState`/`ConfidenceState` are unchanged by B's tick. | **Behavioral two-slug test at N=2, running multi-project server:** populate A and B differently, tick A then B, assert A's four handle states are unchanged by B's tick (A's write absent-effect on B and vice versa). N=1 is insufficient — it cannot distinguish a real per-slug funnel from a global-handle bypass. The per-slug handle set MUST be the sole mutation route. **Doubles as the concurrency-readiness proof (AC-7).** Covers FR-14, FR-15. (SR-01, SR-07) |
| **AC-5** (serving reads maintained state) | A search on slug A reflects A's maintained analytics (phase blending, confidence) and is unaffected by B's. | **Behavioral two-slug test, running multi-project server:** after ticking A and B, a search on slug A reflects A's post-tick maintained state (phase blending + confidence applied), and a search on B is unaffected by A's state. Proves serve/tick handle identity, not just handle existence. Covers FR-15, FR-16/FR-10 (adapt). (SR-08) |
| **AC-6** (test path preserved) | The existing `UnimatrixServer::new` test-default construction still works for unit tests. | Existing unit tests using the test-default constructor compile and pass unchanged; the refactor is additive (the test-default path is reachable and produces NLI-off / pool-1 / default-params behavior). Covers FR-7, FR-8, NFR-6. (SR-03) |
| **AC-7** (concurrency-clean, registry-based work-unit) | The per-slug tick is registered `BackgroundJob`s touching only their own `PerSlugTickContext` + shared read-only resources + the rayon pool — no cross-context shared mutable state; adding a job is "implement the interface + register," not a loop rewrite. | (a) **Structural:** today's operations are registered jobs each declaring cadence + resource class; a new no-op job is added by implementing the interface + registering, with no loop-body edit. (b) **Audit:** no cross-context shared mutable state — no global-handle write path, per-slug counters only, serialized rayon. (c) **Behavioral:** AC-4 stands as the concurrency-readiness proof (a work-unit that provably does not touch B's state when ticking A serially is, by construction, safe to run concurrently). Covers FR-11, FR-12, FR-16, FR-17, FR-18. (SR-04, SR-07, SR-09) |

---

## Domain Models / Ubiquitous Language

- **slug / project** — a registered project identity routing to its own per-slug store and
  per-slug `UnimatrixServer`. "slug" and "project" are used interchangeably; a slug is a
  first-class Unimatrix after crt-056, not a degraded one. (One client : one project per vnc-034.)

- **daemon (single-project / global daemon)** — the global `UnimatrixServer` built from the
  resolved config (`main.rs:880-898`). Its **resolved config** is the parity reference for AC-1.

- **`ServiceLayer`** — the per-server object that holds the analytics handle set and serves
  queries. There is one per server (daemon and each slug). It is the convergence point: it is
  built with config (Wave 1) and its handles are maintained by the tick (Wave 2). **The per-slug
  `UnimatrixServer`/`ServiceLayer` IS the per-project work-unit.**

- **handle set (analytics handle set)** — the bundle of analytics state held by a `ServiceLayer`:
  `ConfidenceState`, `EffectivenessState`, `TypedGraphState`, `PhaseFreqTable`,
  `ContradictionScanCache`, `TickMetadata`. In the pre-crt-056 code these are **global
  singletons** (`Arc<RwLock<_>>` not keyed by slug), extracted once from the daemon's single
  `ServiceLayer` at `main.rs:957-961` and threaded into `spawn_background_tick`
  (`main.rs:968-991`) — the root corruption hazard. crt-056 makes
  them **per-slug handle sets**, one per `PerSlugTickContext`, owned by that slug's
  `ServiceLayer`.

- **`PerSlugTickContext`** — the per-project work-unit context: one slug's store handle + its
  `ServiceLayer` handle set + its per-slug `tick_counter`. The tick mutates **only** this. It
  carries **no cross-context shared mutable state**; shared resources (models, rayon pool,
  `ConfidenceParams`) are read-only and supplied at loop level.

- **work-unit** — the unit a `BackgroundJob` operates on: one `PerSlugTickContext` plus shared
  read-only resources plus the rayon pool. The concurrency-clean contract is that a work-unit
  touches nothing outside this set, which is exactly what makes the serial loop correct *and* what
  would make it safe to run concurrently later (Step B).

- **`BackgroundJob`** — the registered work-unit interface: `run(per_slug_tick_context)` + a
  declared **cadence** + a declared **resource class**. Today's 9 tick operations are the first
  registered jobs. The interface is the **seam**, deliberately thin — not Step B's scheduler.

- **shared read-only resources** — the single loaded embedding model, single loaded NLI model
  (handle), `ConfidenceParams`, supplied as immutable `Arc`s shared across all slugs.

- **rayon ML inference pool** — the single shared thread pool for ML inference. Shared between the
  MCP hot path and per-slug ticks; access **serialized** across slugs (the one contention point).

- **config parity** — a per-slug server's resolved service config equals the daemon's **resolved
  global** config, field by field across the 8-field ADR-006 checklist (`nli_enabled`,
  `nli_top_k`, shared `nli_handle`, `InferenceConfig`, `ConfidenceParams`, `CategoryAllowlist`,
  domain packs, rayon pool size). `session_capabilities` is **OUT** (FR-10). Parity is to the
  **global** config only; per-slug overrides are #785.

- **Step B (scale machinery)** — the deferred follow-up: bounded worker pool, LRU
  residency/eviction, per-project cadence, concurrent rayon. Out of scope; lives in a future
  scheduler layer requiring zero work-unit changes.

---

## User / Agent Workflows

- **W-1 (operator runs a multi-project server).** Operator configures the daemon (NLI on,
  category allowlist, domain packs, scoring params). A client registers slug A; slug A's server is
  provisioned at config parity (Wave 1) — its retrieval quality matches the daemon's from the
  first query.

- **W-2 (self-learning per slug).** An agent writes to slug A. The background tick visits A's
  `PerSlugTickContext`, rebuilds A's handle set from A's store (Wave 2). A subsequent search on
  slug A reflects A's maintained analytics (confidence, co-access, phase blending). Slug B is
  unaffected.

- **W-3 (adding future background math — Step B-era).** A future feature implements the
  `BackgroundJob` interface for new math (e.g., GNN, edge-enhancement, hygiene) and registers it.
  The job inherits the concurrency-clean shape for free; no loop rewrite. (Demonstrated
  structurally in crt-056 via AC-7; the scheduler itself is Step B.)

---

## Constraints

- **C-1 (serial, not concurrent).** OSS uses a serial loop over resident registered slugs; do not
  build concurrency. (NFR-1; SR-02)
- **C-2 (work-unit boundary — build/don't build).** The work-unit operates ONLY on its
  `PerSlugTickContext` + shared read-only resources + rayon — no cross-context shared mutable
  state. Build the seam; do NOT build the queue / pool / residency / cadence (Step B). Any
  scheduler machinery is out of scope and should be rejected in review. (FR-16; SR-04)
- **C-3 (one shared model).** Embedding + NLI loaded once, shared read-only; rayon serialized
  across slugs; never per-slug model copies. (NFR-2, NFR-3; SR-02)
- **C-4 (preserve test-default constructor).** Constructor refactor is additive; test-default
  path (NLI off, pool 1, defaults) remains for unit tests. (FR-7, FR-8; SR-03)
- **C-5 (in-memory hot path).** Tick writes the same per-slug handles the serving path reads; no
  DB reads at query time. (NFR-4; SR-08)
- **C-6 (one isolation seam — vnc-034 ADR-003).** No cloud-only code path; single-project daemon
  and per-slug servers share the parity + tick path. (NFR-5; SR-03)
- **C-7 (global config only).** Parity is to the single resolved global config; no per-slug
  overrides (that is #785 / C6). (FR-9; SR-06)
- **C-8 (no new analytics math).** crt-056 maintains existing analytics per-slug; it adds no new
  scoring math. (FR-13)
- **C-9 (cumulative test infra + N=2).** Extend the multi-project / Layer-2 harness; AC-4 needs a
  real two-slug tick, never a unit stub; install the rayon `panic_handler`. (NFR-7; SR-07, SR-10)
- **C-10 (parity definition is closed).** AC-1 asserts field-by-field equality with the resolved
  config — not a representative subset; the OQ-5 parity checklist (FR-10) is closed in this spec.
  (SR-05)

---

## Dependencies

- **Crates / components:** `unimatrix-server` (`main.rs`, `server.rs`, `http_provision.rs`,
  `background.rs`), `unimatrix-core` (analytics state types: `ConfidenceState`,
  `EffectivenessState`, `TypedGraphState`, `PhaseFreqTable`, `ContradictionScanCache`,
  `TickMetadata`; `InferenceConfig`, `ConfidenceParams`, `CategoryAllowlist`), `unimatrix-store`
  (per-slug stores), `unimatrix-embed` (embedding model), NLI model/handle, `rayon`.
- **Existing components reused:** the per-store-parameterized tick operations, invoked from
  `run_single_tick` (`background.rs`, dispatched via the `run_single_tick` call at
  `background.rs:363`; the ops themselves live inside `run_single_tick` ~`background.rs:441-803`):
  `maintenance_tick` (~463), co-access promotion (~552), `TypedGraphState::rebuild` (~566),
  `PhaseFreqTable::rebuild` (~629), contradiction scan (~703, interval-gated),
  `extraction_tick` (~744), NLI graph inference (~780), graph enrichment (~794). The per-slug
  store registry/routing from vnc-034; the multi-project / Layer-2 test harness.
- **No blocking dependency** (per-slug stores/routing, C3, are done).
- **Upstream of:** #785 (C6 per-slug custom config) and C0★ (full-fidelity parity).
- **Advances goals:** #4946 (`personal-cloud`, capability C5) and #4677 (`self-learning`).
- **Tracking:** GH #787.

---

## NOT in Scope (explicit exclusions)

- **Step B scale machinery** — bounded worker pool, LRU residency/eviction, lazy
  rebuild-on-cold-access, per-project tick cadence, concurrent rayon across slugs. The design must
  not *preclude* Step B but must not *build* it.
- **Per-slug CUSTOM config** — per-project category/domain overlays or any per-slug config
  override. That is #785 / capability C6. crt-056 brings parity using the **global** config only.
- **New functional analytics / new scoring math.** crt-056 maintains existing analytics per-slug;
  no new math.
- **The standing release gate** (N5 nfr-maintenance feature) and **#767** (model bake).
- **Any cloud-only code path** the local single-project install does not exercise (vnc-034
  ADR-003).

---

## Open Questions

Resolved in this specification (recorded for traceability):

- **OQ-1 (handle ownership/lifecycle).** Resolved: the per-slug `ServiceLayer` owns the single
  handle set; the tick references and mutates exactly that set; serving reads exactly that set.
  One handle set per slug, shared between serve and tick. (FR-11, FR-15; SR-01/SR-08)
- **OQ-3 (`tick_counter` gating).** Resolved: **per-slug counters** (not synchronized
  loop-global gating) — required by the no-cross-context-shared-mutable-state contract. (FR-18)
- **OQ-5 (parity definition).** Resolved (human + ADR-006): `adapt_service` is per-slug
  (independent state, same config); `session_capabilities` is **OUT of crt-056 parity scope**.
  AC-1 is a closed field-by-field checklist over the 8 config-driven fields. (FR-10, AC-1)

Open for the architect (design decisions, not blockers):

- **OQ-2 (`BackgroundJob` interface altitude).** How thin is `run(project_ctx)` + cadence +
  resource-class? Just enough to express today's ops as jobs — no Step B scheduler features. The
  architect picks the minimal interface; the spec constrains it to the seam (FR-16).
- **OQ-4 (constructor refactor shape).** Required `ServiceLayer` param vs `Option<ServiceLayer>`
  (None ⇒ test default). The architect chooses the additive form that least disturbs existing
  call sites/tests (FR-7, FR-8); both forms satisfy AC-6.

For the human:

- **HQ-1 (modest-N cadence acceptance).** The serial tick is accepted as correct "for modest N"
  (assumption A4). Confirm there is no near-term OSS deployment expectation of large N that would
  make the serial tick fall behind before Step B exists. If large N is expected soon, Step B
  prioritization should be revisited (it remains out of crt-056 regardless).

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned general delivery/wave patterns and prior
  crt/vnc decisions; none override the SCOPE/risk inputs. Notable adjacent: #3756 (two-wave
  delivery by dependency order) aligns with the Wave 1 → Wave 2 sequencing; #3753 (search.rs
  pipeline reads PhaseFreqTableHandle/TypedGraphStateHandle) corroborates the serve/tick handle
  identity requirement (FR-15). No generalizable pattern stored (spec is feature-specific,
  read-only tier).
