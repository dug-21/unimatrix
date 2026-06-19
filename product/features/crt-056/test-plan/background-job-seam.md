# Test Plan: `BackgroundJob` trait + registry + `Cadence`/`ResourceClass`/`SharedTickResources`

> Component: new (`background.rs`), ADR-004. The **seam** — the SHAPE, not Step B's scheduler.
> `trait BackgroundJob { name; cadence; resource_class; async run(ctx, shared) }`;
> `build_job_registry() -> Vec<Box<dyn BackgroundJob>>`; today's 9 ops registered as first jobs in
> order; `Cadence { EveryTick, EveryN(u32) }`; `ResourceClass { Io, Rayon }` (declaration only);
> `SharedTickResources` (all read-only `Arc`).
> Risks: **R-01** (ceremonial seam), **R-04** (interior-mutable shared `Arc`), **R-09** (Step B
> leakage). ACs: **AC-7** (registry-derived, no-loop-rewrite), **AC-7-stepb** (scope boundary),
> feeds AC-2. FR-16, FR-12.

---

## Unit test expectations

### AC-7a — registry-derived, "implement + register" not loop-rewrite (R-01)
- `test_noop_job_runs_when_registered`
  - **Arrange:** define a no-op `BackgroundJob` (records that `run` was called); register it.
  - **Act:** run one loop pass with **zero edits to the loop body**.
  - **Assert:** the no-op job's `run` executed. (Behavioral wiring in `multi-slug-harness.md`; the
    unit form proves the registry dispatches it.)
- `test_unregistered_job_does_not_run`
  - **Assert:** a job NOT in the registry never executes — proving the iterated set is registry-
    derived, not loop-hardcoded (FR-12, FR-16).

### AC-7 — no trait-default `run` bypass (R-01, #4974 checklist 4)
- **Source audit:** `BackgroundJob::run` has **no trait-default impl**. A `{ }`/`{ Ok(()) }`
  default could reintroduce a silent no-op bypass (the vnc-034 `{ None }` trap). Each job MUST
  implement `run` explicitly. (Recorded in `wave2-gating-audit.md` Part B #4.)

### Cadence semantics
- `test_cadence_every_tick_always_fires`
  - **Assert:** `EveryTick.fires(t)` is true for all `t`.
- `test_cadence_every_n_fires_on_multiples`
  - **Assert:** `EveryN(n).fires(t) == (t % n == 0)` across a boundary sweep (e.g. n=4: fires at
    0,4,8; not at 1,2,3,5). This is the interval-gate primitive (R-07).

### Registry order = ordering invariant (preserve `background.rs:547-550`)
- `test_job_registry_preserves_op_order`
  - **Assert:** `build_job_registry()` yields jobs in the order: maintenance, **co-access promotion
    (AFTER orphaned-edge compaction, BEFORE TypedGraph rebuild)**, TypedGraphRebuild, PhaseFreqRebuild,
    contradiction scan (`EveryN`), extraction, NLI inference, graph enrichment. Registry order IS the
    ordering invariant; a reorder that puts co-access promotion after TypedGraph rebuild is a
    regression.
- Per-job cadence/resource_class declarations match today's behavior:
  - `test_registered_jobs_declare_expected_cadence` — contradiction scan is `EveryN(n)` (existing
    interval); all others `EveryTick`; rayon-class jobs tagged `ResourceClass::Rayon`
    (TypedGraphRebuild, contradiction scan, NLI/enrichment), IO-class jobs `ResourceClass::Io`.

---

## Source audit expectations

### AC-7-stepb — Step B scope-boundary audit (R-09)
**Grep + review.** Confirm NONE of the following is present (any is a scope failure → reject):
- queue / channel-based work dispatch
- bounded worker pool / semaphore for jobs
- LRU residency / eviction / cold-access rebuild
- cadence-signal / cron machinery (only `EveryTick`/`EveryN`)
- concurrent rayon across slugs / `spawn`/`join` fan-out across slugs
- **`ResourceClass` is DECLARATION ONLY** — assert nothing in crt-056 *reads* it (no scheduler
  consumes the tag). A reader of `resource_class()` that gates execution is Step B leakage.

### R-04 — `SharedTickResources` interior-immutability type audit (A2, delivery item)
**Type-level audit (NOT a runtime test).** For each shared `Arc` in `SharedTickResources`
(`embed_service`, `nli_handle`, `inference_config`, `confidence_params`, `rayon_pool`, `audit`,
`category_allowlist`, `retention_config`) inspect the type for interior mutability on the
**inference read path**: `RwLock`/`Mutex`/`Cell`/`RefCell`/`AtomicX`/unsynchronized cache.
- **Assert:** each is genuinely read-only at inference time, OR any interior-mutable state found is
  **documented as a Step-B concurrency blocker** (not silently accepted) and a test asserts no
  per-slug tick *writes* it. Mirrors the snapshot-before-spawn closure-capture trap (#1494/#3354).
- The serial loop HIDES this hazard today; it surfaces only under Step B concurrency — so the audit
  is the only proof, AC-4 (serial) may not catch it. Record the finding in this file's audit section
  and surface it in RISK-COVERAGE-REPORT.md as a gap/blocker note.

### R-04 — cross-slug inference independence (behavioral complement)
- `test_cross_slug_inference_independent` (in `multi-slug-harness.md`)
  - Tick A then B exercising NLI inference; assert B's inference does not alter A's results on
    re-query (catches a shared mutable cache keyed globally).

---

## `run` work-unit boundary (C-2, feeds AC-4)
- **Source audit (paired with AC-wave2-gate):** each job's `run` touches ONLY `ctx`'s handle set +
  read-only `shared` + rayon. No `run` writes a global/static handle or another slug's handles. This
  is the per-op half of the funnel proof; the behavioral proof is AC-4 (`multi-slug-harness.md`).

## Coverage requirement

AC-7 = registry-derived dispatch (no-op job runs / unregistered doesn't) + no trait-default `run` +
correct cadence/order/resource-class declarations. AC-7-stepb = scope-boundary grep audit (no
scheduler machinery; `ResourceClass` unread). R-04 = interior-immutability type audit with any
mutability documented as a Step-B blocker.
