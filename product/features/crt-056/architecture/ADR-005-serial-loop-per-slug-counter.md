## ADR-005: Serial per-slug tick loop, per-slug `tick_counter`, serialized rayon

### Context
The per-slug tick must run today's operations over each registered slug. Three forces shape the
loop:

- **SR-02 (rayon monopolisation, evidence #2535):** the shared rayon ML pool serves both the MCP
  hot path and the background ticks. A loop that holds rayon across all N slugs in one closure
  monopolises the pool for the full N-slug duration, degrading MCP latency.
- **SR-09 (loop-global `tick_counter`, OQ-3):** today `current_tick` comes from the single global
  `Arc<Mutex<TickMetadata>>` (`background.rs:352-358`, `server.rs:340`). If reused globally,
  interval gates (`tick % 4 == 0`, contradiction-scan cadence) fire for **all** slugs synchronously
  → periodic latency spike, and a job reading loop-global counter state breaks the
  no-cross-context-shared-mutable-state contract (AC-7).
- **A4 / Non-Goals:** serial is correct "for modest N"; Step B (concurrency) is OUT but must not be
  precluded.

### Decision
**1. Serial loop over resident registered slugs.** Iterate the `Vec<PerSlugTickContext>` one slug at
a time, running the job registry (ADR-004) for each. Serialization across slugs comes **free** and
is the SR-02 mitigation: only one slug's jobs touch rayon at a time, so the pool is never held
across all N. The loop MUST NOT wrap all slugs in a single rayon closure — rayon is entered and
exited per job per slug (preserving today's per-op spawn pattern).

```rust
for ctx in &contexts {                              // serial — rayon serialized free (SR-02)
    let current_tick = ctx.next_tick();             // PER-SLUG counter (below)
    for job in &registry {
        if job.cadence().fires(current_tick) {
            if let Err(e) = job.run(ctx, &shared).await {
                tracing::error!(slug = %ctx.slug, job = job.name(),
                                "per-slug tick job failed: {e}; continuing");
            }   // per-slug, per-job isolation: one failure never aborts another (background.rs:393)
        }
    }
}
```

**2. Per-slug `tick_counter` (resolves OQ-3 → per-slug, the preferred arm for SR-09).** Each
`PerSlugTickContext` carries its **own** `Arc<Mutex<TickMetadata>>` (ADR-003). The counter falls out
for free: it is already a field of the per-server `TickMetadata` (`server.rs:340,370`), and each
slug already has its own server/`ServiceLayer`. `next_tick()` reads+increments that slug's own
counter (the existing `background.rs:352-358` logic, scoped to `ctx.tick_metadata`).

- **Why per-slug over synchronized gating:** per-slug counters mean no cross-context shared mutable
  state — interval ops do **not** fire for all slugs on the same cycle (no synchronized latency
  spike), and no job reads a loop-global counter (preserving AC-7). Synchronized gating would
  require documenting a latency-spike envelope and asserting the counter is read-only-shared; the
  per-slug counter avoids both. The **contradiction-scan interval** (the heaviest interval op) is
  thereby naturally staggered across slugs by their independent counters rather than firing in
  lockstep.

**3. Single isolation seam.** The single-project daemon path also runs this loop — with a
one-element `Vec<PerSlugTickContext>` built from its own server's `ServiceLayer` handles. The
multi-project path runs the same loop over N elements. There is no separate global tick code path
(vnc-034 ADR-003 single-seam constraint; ADR-001/ADR-003 consistency). The legacy
`spawn_background_tick` global-handle signature is retired in favor of this loop.

**4. Do NOT preclude Step B; do NOT build it.** Concurrency lives entirely in a future scheduler
layer that would replace the `for ctx` serial iteration with a bounded-pool dispatch + rayon
semaphore. Because `job.run(ctx, shared)` already touches only `ctx` + read-only `shared`, that swap
needs ZERO work-unit changes (SCOPE L54-58). crt-056 builds the serial loop only.

### Consequences
- **Easier / risk retired:** serialized rayon (SR-02) and per-slug counters (SR-09) both fall out of
  the structure — no extra machinery. The per-slug counter makes the work-unit contract clean (AC-7)
  and staggers heavy interval ops.
- **Harder / accepted (A4):** worst-case full-loop duration ≈ N × single-slug-tick. At large N the
  loop falls behind the tick interval before Step B exists. **Accepted** per Non-Goals; the single-
  slug worst-case tick duration is documented as the monopolisation envelope (SR-02), and the OSS N
  assumption is flagged for human confirmation (ARCHITECTURE.md §8, Open Question 2).
- **Per-slug isolation:** a job error on slug A is logged and the loop proceeds to slug B (mirrors
  `background.rs:393-395`); the outer panic→restart wrapper (`background.rs:286-301`) and the rayon
  `panic_handler` (SR-10) are retained — the multi-slug test harness MUST install the
  `panic_handler` (extend the Layer-2 harness, no new scaffolding).
- **Boundary:** this ADR builds the loop + per-slug counter only; the queue/pool/residency/cadence-
  signals (Step B) are out (SR-04, ADR-004).

Related: ADR-003 (per-slug handle set + `TickMetadata` in the context), ADR-004 (the registry the
loop runs + `Cadence` the counter feeds).
