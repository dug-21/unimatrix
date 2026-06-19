# crt-056 — Per-Slug Intelligence Parity: maintained analytics + correct service config, on a concurrency-clean per-project tick work-unit

> Brings per-slug (per-project) MCP servers to **functional parity** with the single-project daemon —
> correct service config AND maintained analytics — so a registered slug is a *first-class* Unimatrix,
> not a degraded one. Closes the C5 capability gap (`personal-cloud` #4946) and the self-learning
> per-project surface (#4677). GH issue: **#787**. Scoped in a uni-zero session 2026-06-19.

## Problem Statement

vnc-034 delivered multi-project **routing + per-slug stores** but left the per-slug serving path
**stubbed and un-maintained**. Two confirmed defects (live on `arch-research`):

**1. Per-slug servers run in test-config mode.** `UnimatrixServer::new` builds the per-slug
`ServiceLayer` with **hardcoded test defaults** (`server.rs:306-333`): **NLI disabled**, rayon pool
**size 1**, default `InferenceConfig` (wrong fusion/PPR weights), default `ConfidenceParams`, **empty**
`CategoryAllowlist`, built-in-only domain packs, and a **fresh unloaded** `NliServiceHandle` (does not
share the daemon's loaded model). The global daemon, by contrast, builds its `ServiceLayer` with full
config-driven values (`main.rs:880-898`). So per-slug **retrieval quality is degraded right now** —
NLI off, default weights — independent of the tick.

**2. Per-slug analytics are never maintained.** The background tick is spawned **once**, bound to the
single global hash-dir store (`main.rs:968`); it never iterates the per-slug stores. The per-slug
`ServiceLayer` constructs its own analytics handles (`ConfidenceState`, `EffectivenessState`,
`TypedGraphState`, `PhaseFreqTable`, `ContradictionScanCache`) but **nothing ever rebuilds them** — so
confidence stays at defaults, the co-access graph is empty, phase-conditioned blending is off,
NLI/contradiction never run. The self-improving half of the product is dead per-slug.

Both are the same root: vnc-034 wired the per-slug *path* but not the per-slug *intelligence*. Writes
land correctly (routing works); the engine behind them does not.

## Goals

1. **Per-slug service-config parity** — per-slug servers serve with the daemon's real config (NLI on
   per config, correct fusion/PPR/confidence params, operator category allowlist + domain packs),
   sharing the daemon's single loaded NLI model (not N copies, not an unloaded handle).
2. **Per-slug analytics maintained** — the tick rebuilds *each* registered slug's analytics handles
   from *its own* store, with no cross-slug corruption.
3. **A concurrency-clean, registry-based per-project tick work-unit** — the per-slug tick is a
   `BackgroundJob` work-unit over a per-project context, registered (not hardcoded into the loop), and
   *concurrency-clean by construction*: it touches only its own `PerSlugTickContext` + shared read-only
   resources + the rayon pool, with no cross-context shared mutable state. The precise contract is this
   concurrency-cleanliness — the same per-slug handles, per-slug counter, and serialized rayon that make
   crt-056's serial loop correct are exactly what make the work-unit safe to run concurrently later. So
   future background math (GNN, edge-enhancement, hygiene) is "register a job," not "re-architect," and
   that job inherits the concurrency-clean shape for free. (The north star from #787; Step B below stays
   out.)

## Non-Goals

- **Step B — scale machinery.** Bounded worker pool, LRU residency/eviction, per-project tick cadence,
  and **concurrent** rayon across slugs are explicitly OUT. OSS uses a **serial** loop over the
  resident registered slugs (correct for modest N); the design must not *preclude* Step B (keep the
  work-unit shape) but must not *build* it.
  *Rationale (what the concurrency-clean contract buys):* because the work-unit carries no cross-context
  shared mutable state, the eventual scheduler is a **contained, additive ~1–2 week follow-up requiring
  ZERO work-unit changes**. The non-trivial parts — LRU residency + lazy-rebuild-on-cold-access,
  bounded-pool / rayon semaphore, cadence signals — all live in the new scheduler layer, not the
  work-unit. The posture is "add a scheduler when SaaS needs it," not "re-architect the tick."
- **Per-slug CUSTOM config** (per-project categories/domain overlays) — that is **#785 / capability C6**,
  which depends on this. crt-056 brings per-slug servers to parity using the **global** config; #785
  later lets a slug override it.
- **The standing release gate** (an N5 nfr-maintenance feature) and **#767** (model bake) — separate.
- **No new functional analytics.** crt-056 *maintains* the existing analytics per-slug; it adds no new
  scoring math.

## Background Research (grounded in code — two uni-zero investigation passes, 2026-06-19)

**Tick operations are already per-store-parameterized (reusable).** The loop body dispatches one
`run_single_tick` call (`background.rs:363-391`, fn at 413-440), inside which the 9 tick operations run
at distinct call sites — `maintenance_tick` (carrying effectiveness; `background.rs:463`), co-access
promotion (`552`), `TypedGraphState::rebuild` (`566`), `PhaseFreqTable::rebuild` (`629`), contradiction
scan (`682-739`), extraction (`744`), NLI/graph inference (`780`), graph enrichment (`794`) — each takes
`&Store` explicitly and is idempotent; none reaches a global store singleton. Calling them per-slug is
sound.

**The analytics state handles are SINGLETONS — this is the crux.** `ConfidenceState`,
`EffectivenessState`, `TypedGraphState`, `PhaseFreqTable`, `ContradictionScanCache`, and `TickMetadata`
are each one global `Arc<RwLock<_>>` extracted at `main.rs:957-961` and passed into the
`spawn_background_tick` call (`main.rs:968-991`), **not keyed by slug**. The naive fix — iterate the existing loop over N stores writing the shared handles — is a
**correctness bug**: each slug's rebuild overwrites the previous slug's state (slug A's graph replaces
slug B's on alternating ticks). Per-slug handle sets are mandatory.

**Shared resources are correctly shared; one contention point.** Embedding model, NLI model,
`ConfidenceParams` are passed as read-only `Arc` — fine. The **rayon ML inference pool** is the one
resource with contention risk: per-slug ticks must **serialize** rayon access (automatic under a serial
loop; a real concern only for Step B). `tick_counter` is **loop-global**, so interval gates
(`tick % 4 == 0`) fire for all slugs synchronously — needs per-slug counters or accepted synchronized
gating.

**The two defects converge on the per-slug `ServiceLayer`.** It *holds* the analytics handles. Goal 1
builds it with correct config; Goal 2 wires the tick to maintain *those same handles* — which are
exactly what the per-slug serving path *reads* at query time (principle 7). So **the per-slug
`UnimatrixServer`/`ServiceLayer` IS the per-project work-unit**: `BackgroundJob.run(project)` = "tick
this server's handles." The config-parity threading is available at the `build_project_server` call
site already (`main.rs:1085-1092`): `config`, `ml_inference_pool`, `nli_handle`, `inference_config`,
`confidence_params`, `categories`, `observation_registry` are all in scope, just not threaded.

## Proposed Approach — one feature, two waves

**Wave 1 — Service-config parity (the substrate).**
- Thread the 7 config `Arc`s + the **shared loaded** `nli_handle` into `build_project_server`
  (`http_provision.rs`).
- Change `UnimatrixServer::new` to accept a **pre-built `ServiceLayer`** (per-slug passes the
  config-driven one; the existing test-default path is **preserved** for unit tests).
- Result: per-slug servers serve at config parity (NLI on, correct weights, shared model), and their
  analytics handles now exist *with correct config* — the substrate Wave 2 maintains.

**Wave 2 — Per-slug tick on the `BackgroundJob` seam (depends on Wave 1).**
- Introduce a **`PerSlugTickContext`** (per-slug store + its `ServiceLayer` handle set), one per
  registered slug; the tick loop iterates them **serially**; shared resources (models, rayon pool)
  passed at loop level with **serialized** rayon access; **per-slug** `tick_counter`.
- Express the per-slug tick as a `BackgroundJob` work-unit; register today's operations as the first
  jobs (each declares cadence + resource class). Keep the interface minimal (the seam, not Step B's
  scheduler).

Sequential by dependency: Wave 2 maintains the handles Wave 1 builds.

## Acceptance Criteria (behavioral — run against a *running multi-project server*)

- **AC-1 (config parity):** a per-slug server reports NLI **enabled** (when config enables it), the
  daemon's fusion/PPR/confidence params, the operator category allowlist + domain packs, and a rayon
  pool sized per config — assert equality with the global daemon's resolved config.
- **AC-2 (shared model):** all per-slug servers reference the **one** loaded NLI/embedding model (one
  model in memory, not N; no per-slug unloaded handle).
- **AC-3 (analytics maintained):** store to slug A → run a tick → A's confidence/co-access/phase/
  contradiction caches reflect the write. (Behavioral, not "the handle exists.")
- **AC-4 (isolation, the corruption guard):** after ticking A then B, A's `TypedGraphState`/
  `PhaseFreqTable`/`EffectivenessState`/`ConfidenceState` are **unchanged by B's tick** — no
  cross-slug overwrite. This is the test the naive shared-handle approach fails.
- **AC-5 (serving reads maintained state):** a search on slug A reflects A's maintained analytics
  (phase blending, confidence) and is unaffected by B's.
- **AC-6 (test path preserved):** the existing `UnimatrixServer::new` test-default construction still
  works for unit tests (the constructor refactor is additive).
- **AC-7 (concurrency-clean, registry-based work-unit):** the per-slug tick is expressed as registered
  `BackgroundJob`s that touch **only** their own `PerSlugTickContext` + shared read-only resources + the
  rayon pool — **no cross-context shared mutable state**; adding a hypothetical new background job is
  "implement the interface + register," not a loop rewrite. **AC-4 (the cross-slug corruption guard)
  doubles as the concurrency-readiness proof for this contract:** a work-unit that provably doesn't touch
  slug B's state when ticking slug A serially is, by construction, safe to run on a worker concurrently.

## Constraints

- **Serial, not concurrent** (Step B deferred). Correctness over throughput at OSS N.
- **Build / don't build (the work-unit boundary).** The per-slug tick work-unit operates ONLY on its
  own `PerSlugTickContext` + shared read-only resources + the rayon pool, with NO cross-context shared
  mutable state — i.e., it could run concurrently even though crt-056 runs it serially. Do NOT build the
  queue / pool / residency / cadence (that is Step B).
- **One shared model, never per-slug** — embedding + NLI loaded once, shared read-only; rayon access
  serialized across slugs.
- **Preserve the test-default constructor path** (additive change).
- **In-memory hot path (principle 7)** — the tick writes the same per-slug handles the serving path
  reads; no DB reads at query time.
- **Test infrastructure is cumulative** — extend the multi-project / Layer-2 harness; the corruption
  guard (AC-4) needs a real two-slug tick, not a unit stub.
- **One isolation seam (vnc-034 ADR-003)** — per-slug parity must not introduce a cloud-only code path
  the local single-project install doesn't exercise.

## Open Questions (for the design session)

- **OQ-1 (`PerSlugTickContext` ownership/lifecycle):** who owns the per-slug handle sets — the per-slug
  `UnimatrixServer` (the serving side already holds them) referenced by the tick, or a parallel
  registry? They MUST be the same handles the serving path reads. Resolve the ownership so there's one
  handle set per slug, shared between serve and tick.
- **OQ-2 (`BackgroundJob` interface altitude):** `run(project_ctx)` + cadence + resource-class — how
  thin? Just enough for the existing ops as jobs, no Step B scheduler features.
- **OQ-3 (`tick_counter` gating):** per-slug counters vs accepted synchronized gating across slugs
  (interval ops firing for all slugs on the same tick). Pick one; note the contradiction-scan interval.
- **OQ-4 (constructor refactor shape):** `ServiceLayer` as a required param vs an `Option` (None ⇒ test
  default) — choose the additive form that least disturbs existing call sites/tests.
- **OQ-5 (parity definition):** is `adapt_service` per-slug (independent state, current) or shared? And
  `session_capabilities` per-slug — needed for parity or out of scope?

## Dependencies & Tracking

- **No blocking dependency** (per-slug stores/routing, C3, are done). **Upstream of** #785 (C6 per-slug
  custom config) and the marquee **C0★** (full-fidelity parity).
- GH Issue **#787** — https://github.com/dug-21/unimatrix/issues/787 (kept in sync with this scope). Capability **C5** (`personal-cloud` CAPABILITIES.md).
- Advances goal **#4946** (`personal-cloud`) and **#4677** (`self-learning`).
- Session type: **design** (no IMPLEMENTATION-BRIEF exists; the architecture investigation is done —
  remaining work is design decisions, not unknowns).
