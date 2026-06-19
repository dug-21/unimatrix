# Component: `BackgroundJob` trait + registry + shared types + the 9 jobs

> Wave 2. ADR-004 (#5167). Resolves OQ-2, FR-16. Covers AC-7. Risks R-01 (ceremonial seam),
> R-09 (Step B leakage), R-02 (per-op global-handle audit).
> Source: ops live inside `run_single_tick` (`background.rs:441-804`); call sites 463/513-544/552/
> 564-566/628-629/682+703-706/743-744/780/794; ordering invariant 546-551.

## Purpose

Define the **minimal** work-unit seam — just enough to express today's 9 tick operations as
registered jobs over a `PerSlugTickContext` + read-only `SharedTickResources`. Build the trait + a
static `Vec<Box<dyn BackgroundJob>>` registry. Build **NO** queue, pool, residency, eviction, or
cadence-signal machinery (C-2, R-09). The seam carries the value the day it ships (anti-ceremonial,
#4974): jobs touch ONLY their `ctx` + read-only `shared` + rayon — the sole mutation route.

## ⚠ Wave-2 gating audit — FIRST ACT OF WAVE 2, BEFORE ANY WAVE 2 CODE

Two paired source audits (R-01.2 + R-02.2) gate Wave 2; record results in
`test-plan/wave2-gating-audit.md`. NOT an end-gate check.
1. **A1 per-op closure audit.** For each of the 9 ops dispatched in `run_single_tick`, source-confirm
   the op takes `&Store` and writes only the passed-in handle — none closes over a global/static
   handle or a store singleton. Call sites: maintenance (463→`maintenance_tick`), orphaned-edge
   compaction (513-544, uses `store.write_pool_server()`), co-access promotion (552→
   `run_co_access_promotion_tick(store,...)`), TypedGraph rebuild (564-566→`TypedGraphState::rebuild(&store_clone)`),
   PhaseFreq rebuild (628-629→`PhaseFreqTable::rebuild(&store_clone,...)`), contradiction scan
   (682 gate + 687 `store.query_by_status` + 703-706 rayon `scan_contradictions`), extraction
   (743-744→`extraction_tick(store,...)`), graph inference (780→`run_graph_inference_tick(store,...)`),
   graph enrichment (794→`run_graph_enrichment_tick(store,...)`).
2. **AC-4 verify-the-funnel audit.** Grep the job `run` path for a discarded resolved handle
   (`let _`, unused binding) and for any global/shared analytics-handle write path beside
   `PerSlugTickContext`. Confirm the per-slug handle set is the **sole** mutation route; confirm
   `BackgroundJob::run` has **no trait-default** that could reintroduce a `{}` no-op bypass.

Rationale: if even one op carries a hidden global-handle write, AC-4 passes for the clean ops while
the missed op corrupts B's state. The "MODERATE not HARD" verdict rests on all 9 ops being confirmed
store-parameterized before code is written.

## Integration surface (exact) — shared types

```text
enum Cadence:                       # ADR-004 — STATIC predicate, no cron/signal machinery
    EveryTick
    EveryN(u32)
    fn fires(&self, t: u64) -> bool:
        match self:
            EveryTick   => true
            EveryN(n)   => n != 0 && t % (n as u64) == 0     # guard n==0 (avoid div-by-zero)

enum ResourceClass:                 # ADR-004 — DECLARATION ONLY; nothing in crt-056 reads it
    Io
    Rayon

struct SharedTickResources:         # ADR-004 — all read-only Arc; no cross-context mutable state
    embed_service:      Arc<EmbedServiceHandle>
    nli_handle:         Arc<NliServiceHandle>     # the ONE loaded model (G-1: A2 immutability is a delivery audit)
    inference_config:   Arc<InferenceConfig>
    confidence_params:  Arc<ConfidenceParams>
    rayon_pool:         Arc<RayonPool>
    audit:              Arc<AuditLog>
    category_allowlist: Arc<CategoryAllowlist>
    retention_config:   Arc<RetentionConfig>

#[async_trait]
trait BackgroundJob: Send + Sync:   # ADR-004 — the seam; NO trait-default on run (anti-bypass, #4974)
    fn name(&self) -> &str
    fn cadence(&self) -> Cadence
    fn resource_class(&self) -> ResourceClass
    async fn run(&self, ctx: &PerSlugTickContext, shared: &SharedTickResources) -> Result<(), String>
```

## The 9 jobs (delegate to existing ops — NO logic copied, C-8)

Each job's `run` calls the EXISTING op fn / inlines the EXISTING block, passing `ctx.<field>` for
handles and store, `shared.<field>` for read-only resources. Registry order == the ordering invariant.

| # | Job | Cadence | ResourceClass | Delegates to (existing) | Mutates (ctx handle) |
|---|-----|---------|---------------|-------------------------|----------------------|
| 1 | `MaintenanceJob` | EveryTick | Io | `maintenance_tick(status_svc, ..., effectiveness, ...)` (463) | effectiveness |
| 2 | `OrphanedEdgeCompactionJob` | EveryTick | Io | the DELETE block (513-544) over `ctx.store.write_pool_server()` | (store-only; no handle) |
| 3 | `CoAccessPromotionJob` | EveryTick | Io | `run_co_access_promotion_tick(ctx.store, shared.inference_config, t)` (552) | (store edges) |
| 4 | `TypedGraphRebuildJob` | EveryTick | Rayon | `TypedGraphState::rebuild(&ctx.store)` swap (564-599) | typed_graph |
| 5 | `PhaseFreqRebuildJob` | EveryTick | Io | `PhaseFreqTable::rebuild(&ctx.store, lookback, min_pairs)` swap (621-664) | phase_freq |
| 6 | `ContradictionScanJob` | EveryN(CONTRADICTION_SCAN_INTERVAL_TICKS) | Rayon | gate+fetch+`scan_contradictions` via `shared.rayon_pool.spawn` (682-739) | contradiction |
| 7 | `ExtractionJob` | EveryTick | Rayon | `extraction_tick(ctx.store, ctx.vector_index, shared.embed_service, ...)` (743-744) | tick_metadata stats |
| 8 | `GraphInferenceJob` | EveryTick | Rayon | `run_graph_inference_tick(ctx.store, shared.nli_handle, ctx.vector_index, shared.rayon_pool, shared.inference_config)` (780) | (store edges) |
| 9 | `GraphEnrichmentJob` | EveryTick | Io | `run_graph_enrichment_tick(ctx.store, shared.inference_config, t)` (794) | (store edges) |

> Cadence note: today only the contradiction scan is interval-gated (`current_tick.is_multiple_of(
> CONTRADICTION_SCAN_INTERVAL_TICKS)`, 682). All others run every tick (S8 inside enrichment is
> internally gated by enrichment itself — leave that internal gate inside the op, do NOT lift it to a
> job `Cadence`; keep behavior byte-identical, C-8/NFR-07). So `MaintenanceJob`..`GraphEnrichmentJob`
> are `EveryTick` except `ContradictionScanJob` = `EveryN(...)`. This preserves today's gating exactly.

### `build_job_registry()`

```text
fn build_job_registry() -> Vec<Box<dyn BackgroundJob>>:
    # REGISTRY ORDER IS THE ORDERING INVARIANT (background.rs:546-551):
    #   compaction(2) → co-access promotion(3) → TypedGraph rebuild(4) → ... per the live tick.
    # Order here MUST equal the run_single_tick step order. Jobs are NOT reordered.
    vec![
        Box::new(MaintenanceJob),
        Box::new(OrphanedEdgeCompactionJob),
        Box::new(CoAccessPromotionJob),     # AFTER compaction, BEFORE TypedGraphRebuild — invariant
        Box::new(TypedGraphRebuildJob),
        Box::new(PhaseFreqRebuildJob),
        Box::new(ContradictionScanJob),
        Box::new(ExtractionJob),
        Box::new(GraphInferenceJob),
        Box::new(GraphEnrichmentJob),
    ]
```

### Per-job `run` shape (example: TypedGraphRebuildJob)

```text
impl BackgroundJob for TypedGraphRebuildJob:
    fn name(&self) -> &str { "typed_graph_rebuild" }
    fn cadence(&self) -> Cadence { Cadence::EveryTick }
    fn resource_class(&self) -> ResourceClass { ResourceClass::Rayon }
    async fn run(&self, ctx, _shared) -> Result<(), String>:
        # EXACT existing logic (background.rs:562-599): timeout-wrapped tokio::spawn rebuild;
        # on Ok(new_state) swap under ctx.typed_graph write lock; on cycle set use_fallback=true;
        # on err/timeout retain existing state. Return Ok(()) — errors are logged-and-continue (retain).
        rebuild_typed_graph_with_retain(&ctx.store, &ctx.typed_graph).await
        return Ok(())
```

> Jobs whose existing op returns `()` and logs internally (co-access promotion, graph inference,
> graph enrichment) return `Ok(())` after delegating. Jobs with retain-on-error semantics
> (TypedGraph, PhaseFreq, contradiction) keep that semantics inside `run` and return `Ok(())` (the
> existing tick never aborts on these; the loop's error log is for unexpected job-level errors).
> MaintenanceJob / ExtractionJob keep their `TICK_TIMEOUT` + `tick_metadata` stat writes (via
> `ctx.tick_metadata`). Do not change which errors are swallowed vs surfaced — byte-identical behavior.

## Explicitly NOT built (C-2, R-09 — reviewer rejects)
- No bounded worker pool / worker threads (serial loop only — `per-slug-tick-loop.md`).
- No LRU residency / lazy-rebuild-on-cold-access / eviction.
- No cadence *signals* — `Cadence` is a static `EveryTick`/`EveryN`, never a scheduler input.
- No concurrent rayon across slugs (serialized via the serial loop).
- `ResourceClass` is a self-tag only — nothing reads it in crt-056 (forward hook for Step B's semaphore).

## Data flow
- Inputs to `run`: `&PerSlugTickContext` (mutable handles for THIS slug) + `&SharedTickResources`
  (read-only) + the rayon pool (via `shared.rayon_pool`).
- Outputs: `Result<(), String>` — `Ok` after delegating; `Err` only for an unexpected job-level failure
  the loop logs (per-slug isolation).
- Transformation: each job rebuilds/updates one analytics handle from the slug's store (no new math, C-8).

## Error handling
- `run -> Result<(), String>`. The loop logs `Err` and continues to the next job/slug (R-01/ADR-005;
  mirrors `background.rs:393-395`). Retain-on-error for rebuild jobs stays internal (existing semantics).
- No panics introduced; rayon work goes through `shared.rayon_pool.spawn` exactly as today (the harness
  installs the rayon `panic_handler`, `multi-slug-harness.md`).

## Key test scenarios (hints for tester)
- **AC-7a / R-01.3 (registry-not-loop-hardcode).** Add a no-op `BackgroundJob`, register it, run a pass:
  it executes with ZERO loop-body edits. Unregister a job: it stops running.
- **R-01.2 / R-02.2 (gating audits).** The two source audits above pass and are recorded BEFORE code.
- **R-09.1 (scope-boundary audit).** No queue/pool/residency/cadence-signal/concurrent-rayon present;
  `ResourceClass` unread; `Cadence` is only `EveryTick`/`EveryN`.
- **Cadence.fires unit.** `EveryTick.fires(any)==true`; `EveryN(4).fires(0)==true`, `.fires(4)==true`,
  `.fires(3)==false`; `EveryN(0).fires(_)==false` (guard).
- **No trait-default bypass.** `BackgroundJob::run` has no default impl (compile-enforced).
- **Behavior parity.** A single-slug tick through the registry produces the same handle mutations as
  today's `run_single_tick` (regression guard, C-8/NFR-07).
</content>
