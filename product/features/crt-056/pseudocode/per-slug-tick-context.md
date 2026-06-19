# Component: `PerSlugTickContext` — the borrow bundle

> Wave 2. ADR-003 (#5166). Resolves OQ-1, FR-11, FR-15. Covers AC-3/AC-4/AC-5 (the unit of work).
> Risks R-02 (cross-slug corruption), R-03 (serve/tick handle divergence). Patterns #1560, #4097.

## Purpose

A thin **borrow** bundle: one slug's `{ store, vector_index, analytics handle set, per-slug
TickMetadata }`. It is the unit the serial tick loop iterates and the only state a `BackgroundJob`
may mutate. It does NOT own handles — its fields are `Arc::clone`s of the slug's `ServiceLayer`
accessors (the SAME `Arc<RwLock<_>>`), so what the tick writes is exactly what serving reads
(in-memory hot path, principle 7; FR-15).

## Integration surface (exact)

Handle aliases EXISTING (`services/mod.rs:47-60`), each `Arc<RwLock<_>>`:
`ConfidenceStateHandle`, `EffectivenessStateHandle`, `TypedGraphStateHandle`,
`ContradictionScanCacheHandle`, `PhaseFreqTableHandle`.
Accessors EXISTING (`services/mod.rs:274-316`) — the borrow surface:
`confidence_state_handle()`, `effectiveness_state_handle()`, `typed_graph_handle()`,
`contradiction_cache_handle()`, `phase_freq_table_handle()`.
`TickMetadata` EXISTING (`server.rs:340,370`); per-slug counter logic mirrors `background.rs:352-358`.

## New type: `PerSlugTickContext`

```text
struct PerSlugTickContext:
    slug:          ProjectSlug
    store:         Arc<Store>
    vector_index:  Arc<VectorIndex>
    confidence:    ConfidenceStateHandle
    effectiveness: EffectivenessStateHandle
    typed_graph:   TypedGraphStateHandle
    contradiction: ContradictionScanCacheHandle
    phase_freq:    PhaseFreqTableHandle
    tick_metadata: Arc<Mutex<TickMetadata>>
```

### Constructor (built at boot — see `daemon-http-boot.md`)

```text
# INVARIANT (ADR-003, pattern #4097): every handle field MUST be an Arc::clone of the slug's
# ServiceLayer accessor — NEVER a freshly-constructed handle and NEVER a copy of the inner T.
# Constructing a new handle here silently reintroduces SR-08 (serve/tick divergence).
fn from_server(slug, store, sl: &ServiceLayer, tick_metadata, vector_index) -> PerSlugTickContext:
    PerSlugTickContext {
        slug,
        store,
        vector_index,
        confidence:    sl.confidence_state_handle(),     # Arc::clone of the SAME Arc<RwLock<_>>
        effectiveness: sl.effectiveness_state_handle(),
        typed_graph:   sl.typed_graph_handle(),
        contradiction: sl.contradiction_cache_handle(),
        phase_freq:    sl.phase_freq_table_handle(),
        tick_metadata,                                    # Arc::clone of input.server.tick_metadata
    }
```

### Per-slug counter accessor (ADR-005)

```text
# Reads + increments THIS slug's counter only (no loop-global state). Mirrors background.rs:352-358.
fn next_tick(&self) -> u64:
    let mut meta = self.tick_metadata.lock().unwrap_or_else(|e| e.into_inner())   # poison-tolerant, as existing
    let t = meta.tick_counter
    meta.tick_counter = meta.tick_counter.wrapping_add(1)                          # wrapping (existing)
    return t
```

## Data flow
- Input (at construction): the slug's `ServiceLayer` + store + vector_index + its `tick_metadata`.
- Output: a borrow bundle consumed by `BackgroundJob::run`.
- Per tick: `next_tick()` yields the gate value for `Cadence::fires`; job `run` writes through the
  handle fields under their `RwLock`s (the existing ops already take these handles by `&`).

## Error handling
- No fallible construction. `next_tick()` is infallible (poison-tolerant lock, matching existing code).
- Mutation errors are owned by the jobs (`run -> Result<(), String>`) and isolated by the loop, not here.

## Structural guarantees this component must uphold (the crux)
- **Sole mutation route (R-02, SR-01).** There is no global handle to reach — each slug owns its own set
  via its `ServiceLayer`; a job receives state handles ONLY through `ctx`. Verified by the Wave-2 funnel
  audit (`background-job-seam.md`).
- **Handle identity (R-03, SR-08).** `ctx.<handle>` and `sl.<>_handle()` are the same `Arc`
  (`Arc::ptr_eq`). The serving path reads the same accessor, so post-tick state is visible to search.
- **Per-slug counter (R-07, SR-09).** `tick_metadata` is the slug's own `Arc<Mutex<TickMetadata>>`
  (`Arc::clone(&input.server.tick_metadata)`), never a loop-global counter.

## Key test scenarios (hints for tester)
- **R-03.2 handle-identity.** `Arc::ptr_eq(ctx.typed_graph, sl.typed_graph_handle())` (and the other 4)
  is true — borrows, not new instances.
- **Counter independence (R-07.1).** Two contexts at different offsets: `next_tick()` advances each
  independently; an `EveryN(4)` gate fires for one and not the other on the same loop pass.
- **AC-4 absent-effect (composed at loop level).** Writing through ctx_A's handles leaves ctx_B's handle
  states unchanged — proven via the loop, but the structural basis is here (distinct `Arc`s per context).
</content>
