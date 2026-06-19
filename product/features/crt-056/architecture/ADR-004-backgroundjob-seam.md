## ADR-004: `BackgroundJob` work-unit seam — build the SHAPE, not a scheduler (Step B stays out)

### Context
Goal 3 (SCOPE L38-46) wants the per-slug tick expressed as a **concurrency-clean, registry-based
work-unit**: future background math (GNN, edge-enhancement, hygiene) becomes "register a job," not
"re-architect the loop," and inherits the concurrency-clean shape for free.

But there are two High traps:
- **SR-04 (Step B leakage):** bounded worker pool, LRU residency/eviction, cadence *signals*,
  concurrent rayon are explicitly OUT. A `BackgroundJob` seam invites "just a small scheduler."
  Shipping half a scheduler inflates the feature and risks shipping the wrong half.
- **SR-07 (ceremonial seam, direct precedent #4974):** a seam built ahead of Step B can
  resolve-then-discard or sit beside a parallel path, passing every N=1 test while the
  concurrency-clean contract is unproven.

The enabling fact (A1, re-verified): all 9 tick operations live inside `run_single_tick`
(fn def `background.rs:413-804`, invoked from the loop body at `background.rs:363-391`); the
individual op call sites are dispersed within it — `maintenance_tick` at `463`, co-access promotion
at `552`, `TypedGraphState::rebuild` at `566`, `PhaseFreqTable::rebuild` at `629`, the contradiction
scan at `706`, graph enrichment at `794`. Each already takes `&Store` explicitly, is idempotent, and
reaches no global store singleton. They are callable per-slug as-is. OQ-2 asks how thin the interface
should be. (Note: `background.rs:363-794` is the *loop-body / run_single_tick* span, NOT the op list
itself — the ops are at the specific call sites above.)

### Decision
Define the **minimal** work-unit interface — just enough to express today's tick operations as jobs
and run them over a `PerSlugTickContext`. Build the trait + a static registry. Build **NO** queue,
pool, residency, eviction, or cadence-signal machinery.

```rust
pub enum Cadence { EveryTick, EveryN(u32) }     // EveryN(n).fires(t) = t % n == 0
pub enum ResourceClass { Io, Rayon }            // DECLARATION ONLY — no scheduler reads it in crt-056

pub struct SharedTickResources {                // all read-only Arc
    pub embed_service: Arc<EmbedServiceHandle>,
    pub nli_handle: Arc<NliServiceHandle>,      // the one loaded model
    pub inference_config: Arc<InferenceConfig>,
    pub confidence_params: Arc<ConfidenceParams>,
    pub rayon_pool: Arc<RayonPool>,
    pub audit: Arc<AuditLog>,
    pub category_allowlist: Arc<CategoryAllowlist>,
    pub retention_config: Arc<RetentionConfig>,
}

#[async_trait]
pub trait BackgroundJob: Send + Sync {
    fn name(&self) -> &str;
    fn cadence(&self) -> Cadence;
    fn resource_class(&self) -> ResourceClass;
    /// Touches ONLY `ctx`'s handle set + `shared` (read-only) + the rayon pool.
    /// NO cross-context shared mutable state. (AC-7)
    async fn run(&self, ctx: &PerSlugTickContext, shared: &SharedTickResources)
        -> Result<(), String>;
}
```

**Registration:** today's tick operations become jobs, registered in a plain `Vec<Box<dyn
BackgroundJob>>` (a `fn build_job_registry() -> Vec<Box<dyn BackgroundJob>>`). Examples and their
cadence (preserving today's gating):
- `MaintenanceJob` (EveryTick, Io) — wraps `maintenance_tick` (`background.rs:463-475`).
- `CoAccessPromotionJob` (EveryTick, Io) — `run_co_access_promotion_tick`.
- `TypedGraphRebuildJob` (EveryTick, Rayon) — `TypedGraphState::rebuild`.
- `PhaseFreqRebuildJob` (EveryTick, Io) — `PhaseFreqTable::rebuild`.
- `ContradictionScanJob` (EveryN(n), Rayon) — preserves the existing contradiction-scan interval
  (the same `tick % n` gate, now per-slug via ADR-005).
- `NliInferenceJob` / `GraphEnrichmentJob` (Rayon) — preserve existing cadence.

The **ORDERING INVARIANT** documented at `background.rs:547-550` (co-access promotion —
`run_co_access_promotion_tick` at line `552` — runs AFTER the GRAPH_EDGES orphaned-edge compaction
block at `~496-540`, and BEFORE `TypedGraphState::rebuild` at line `566`) is preserved by **registry
order** — the loop runs
jobs in registration order. This is documented as a registry invariant; jobs are not reordered.

**Explicitly NOT built (SR-04 — reviewer rejects any of these):**
- no bounded worker pool / worker threads (serial loop only, ADR-005),
- no LRU residency / lazy-rebuild-on-cold-access / eviction,
- no cadence *signals* (`Cadence` is a static `EveryN`, not a scheduler input),
- no concurrent rayon across slugs (serialized, ADR-005),
- `ResourceClass` is a **self-tag only** — nothing in crt-056 reads it; it is a forward hook so
  Step B's semaphore can group jobs without changing the work-unit.

**Anti-ceremony (SR-07):** the seam is the **sole** route the tick takes — there is no parallel
direct-call path that bypasses the registry. AC-7 requires that adding a hypothetical new job is
"implement `BackgroundJob` + register," not a loop rewrite; AC-4 (run at N=2) proves the registered
jobs actually honor the no-cross-context-state contract rather than just compiling.

### Consequences
- **Easier:** future background math is additive — implement the trait + register. The seam carries
  the concurrency-clean contract, so Step B's scheduler is a contained ~1-2 week follow-up needing
  ZERO work-unit changes (SCOPE L54-58).
- **Harder / discipline:** the interface is deliberately under-powered — no priorities, deadlines,
  backpressure, or dynamic cadence. Resisting these is the point (SR-04). Reviewer rejects scheduler
  machinery.
- **`ResourceClass` is dead weight in crt-056** by design — it exists only so Step B doesn't have to
  touch the trait. This is an accepted, documented forward hook, not speculative scope.
- **Risk:** `async_trait` boxing adds a small per-job allocation; negligible at tick cadence.

Related: ADR-003 (`PerSlugTickContext` the job mutates), ADR-005 (the serial loop that runs the
registry + per-slug counter feeding `Cadence`).
