# Component: Per-slug tick loop — serial, per-slug counter, serialized rayon

> Wave 2. ADR-005 (#5168). Resolves OQ-3, FR-12, FR-17, FR-18. Covers AC-7 (serial work-unit),
> AC-4 (the loop is where A-then-B isolation is proven). Risks R-07, R-08, R-09.
> Source basis: per-slug counter `background.rs:352-358`; per-tick error log 393-395;
> panic→restart wrapper 286-301.

## Purpose

Drive the registered `BackgroundJob`s over each registered `PerSlugTickContext` **serially**, one
slug at a time, in a loop derived from the registry/context set (not hardcoded). Serial execution
gives serialized rayon for free (C-1, C-3, R-08) and lets each slug use its OWN `tick_counter`
(R-07). This replaces the legacy `spawn_background_tick` global-handle path (retired in
`daemon-http-boot.md`); there is exactly ONE tick code path for daemon (N=1) and multi-project (N≥2),
i.e. one isolation seam (C-6).

## New functions

### `run_per_slug_tick_pass` — one loop pass over all contexts

```text
async fn run_per_slug_tick_pass(
    contexts: &[PerSlugTickContext],     # registry-derived set; N=1 for daemon, N≥2 for multi-project
    registry: &[Box<dyn BackgroundJob>], # build_job_registry() output
    shared:   &SharedTickResources,      # read-only Arcs, built once at loop level
):
    for ctx in contexts:                                  # SERIAL — rayon serialized free (FR-17, R-08)
        current_tick = ctx.next_tick()                    # PER-SLUG counter (FR-18, R-07) — ctx.tick_metadata
        for job in registry:                              # registry order = ordering invariant (ADR-004)
            if job.cadence().fires(current_tick):
                match job.run(ctx, shared).await:         # mutates ONLY ctx's handles + reads shared
                    Ok(())  => {}
                    Err(e)  => tracing::error!(slug=%ctx.slug, job=job.name(),
                                   "per-slug tick job failed: {e}; continuing")
                    # per-slug, per-job isolation: one failure never aborts another (background.rs:393)
        # serving path on ctx.slug now reads freshly-maintained handles (AC-5)
    # NOTE: a job's rayon work is entered+exited WITHIN job.run for that single slug. The loop MUST NOT
    # wrap all N contexts in one rayon closure (FR-17, NFR-3, R-08). Serialization is structural here.
```

### `spawn_per_slug_tick` — the interval driver (replaces `background_tick_loop` + `spawn_background_tick`)

```text
fn spawn_per_slug_tick(
    contexts: Vec<PerSlugTickContext>,   # built at boot (daemon-http-boot.md)
    shared:   SharedTickResources,       # built once at boot
) -> JoinHandle<()>:
    tokio::spawn(async move:
        # OUTER panic→restart wrapper RETAINED (background.rs:286-301): on inner panic, log + sleep 30s + restart.
        loop:
            inner = tokio::spawn({ contexts, shared cloned-by-Arc } async move:
                registry = build_job_registry()                       # once per inner-loop start
                tick_interval_secs = read_tick_interval()             # existing
                interval = tokio::time::interval(tick_interval_secs)
                interval.tick().await                                 # skip immediate t=0 (existing 346-347)
                loop:
                    interval.tick().await
                    run_per_slug_tick_pass(&contexts, &registry, &shared).await
            )
            match inner.await:
                Ok(())                          => break              # clean exit
                Err(e) if e.is_cancelled()      => break              # graceful shutdown
                Err(e)                          => { error!("per-slug tick panicked; restarting in 30s"); sleep(30s) }
    )
```

> The `read_tick_interval()` / `interval.tick()` / skip-first-tick / 30s-restart machinery is the
> EXISTING `background_tick_loop` / `spawn_background_tick` structure (background.rs:304-347, 286-301),
> re-targeted from a single global store to `Vec<PerSlugTickContext>`. The tick INTERVAL is global
> (one timer), correct: the loop visits every resident slug once per interval (FR-12). Per-slug
> staggering of heavy interval ops comes from the per-slug COUNTER (each slug hits `EveryN` gates on
> its own count), not from per-slug timers — that is the ADR-005 design (no per-project cadence = Step B).

## Single-isolation-seam wiring (C-6)
- Daemon path: `contexts` is a one-element `Vec` built from the daemon server's own `ServiceLayer`
  handles + `tick_metadata` (same `PerSlugTickContext::from_server`).
- Multi-project path: `contexts` is N elements, one per registered slug.
- Both call `spawn_per_slug_tick` → `run_per_slug_tick_pass`. There is no second/global tick path; the
  legacy `spawn_background_tick` global-handle signature is retired (daemon-http-boot.md).

## Data flow
- Inputs: `Vec<PerSlugTickContext>` (registry-derived) + `SharedTickResources` (read-only, built once).
- Per pass: for each ctx, `next_tick()` → for each job whose cadence fires, `run(ctx, shared)`.
- Outputs: side effects on each slug's own handle set (via its `Arc<RwLock<_>>`). No return value.

## Error handling
- **Per-job / per-slug isolation:** `Err` from `job.run` is logged with `slug`+`job` and the loop
  continues (next job, then next slug). One slug's failure never aborts another's tick (Failure Mode;
  ADR-005; background.rs:393).
- **Panic isolation:** rayon-closure panics are caught by the rayon `panic_handler` (harness installs it,
  test; prod has it); the outer `tokio::spawn` panic→restart wrapper (30s cooldown) is retained.
- **Empty registry / N=0 contexts:** `run_per_slug_tick_pass` over an empty slice is a no-op, no panic
  (Edge Case "N=0 registered slugs").

## Honored constraints
- **C-1 serial:** no `spawn`/`join` fan-out across slugs; a plain `for ctx in contexts`.
- **C-3 / FR-17 serialized rayon:** only one slug's job touches rayon at a time; rayon entered/exited
  inside a single `job.run`; loop never wraps all N in one rayon closure.
- **FR-18 per-slug counter:** `current_tick` comes from `ctx.next_tick()` (ctx.tick_metadata); no job
  reads a loop-global counter.
- **C-2 / R-09 no scheduler:** the loop is the whole "scheduler"; no queue/pool/residency/cadence-signal.

## Key test scenarios (hints for tester)
- **AC-4 / R-01.1 / R-02.1 (N=2 funnel proof — load-bearing).** Two real slugs A,B populated
  differently; run a pass (tick A then B); assert B's tick leaves A's `TypedGraphState`/`PhaseFreqTable`/
  `EffectivenessState`/`ConfidenceState` byte-for-byte unchanged, and vice versa. N=1 is NOT acceptable.
- **AC-7a (registry-not-hardcode).** Add/remove a job and a slug; iterated set changes with no loop edit.
- **R-07.1 (independent gate firing).** Two slugs at different counter offsets: `EveryN(4)` fires on one,
  not the other, on the same pass — counters advance per-context.
- **R-08.1 (rayon-not-held-across-slugs).** Assert each slug's rayon work enters/exits within its own
  `job.run`; no two slugs' closures hold rayon concurrently; loop does not wrap all N in one closure.
- **R-09.2 (serial-loop assertion).** No `spawn`/`join` fan-out across slugs.
- **Failure isolation.** Inject a failing job for A; assert B still ticks and A's prior state intact.
- **Edge: N=0 / empty store / slug-added-next-pass** behave per the Edge Cases list (no panic; picked up
  next pass).
</content>
