## ADR-003: The per-slug `ServiceLayer` owns the SOLE handle set; `PerSlugTickContext` borrows it

### Context
This is the crux ADR — it resolves OQ-1 and collapses three High risks (SR-01, SR-07, SR-08) into
one structural decision.

Today the five analytics state handles — `ConfidenceState`, `EffectivenessState`,
`TypedGraphState`, `ContradictionScanCache`, `PhaseFreqTable` — are each one global
`Arc<RwLock<_>>`. The daemon extracts them from its single global `ServiceLayer` via the existing
accessors (`main.rs:957-961`: `confidence_state_handle()`, `effectiveness_state_handle()`,
`typed_graph_handle()`, `contradiction_cache_handle()`, `phase_freq_table_handle()` —
`services/mod.rs:274-316`) and passes those exact handles to the `spawn_background_tick(...)` call
(`main.rs:968-991`). So **for the single global server, serve and tick already share one handle set
by construction**: the `ServiceLayer` owns it, the search/status services read it, the tick writes
it (pattern #1560 — "single `Arc<RwLock<T>>` through `ServiceLayer`, sole writer is the tick").

The naive multi-project fix — iterate the existing loop over N stores while writing the **shared
global** handles — is a correctness bug (SR-01): slug A's `TypedGraphState::rebuild` overwrites slug
B's on the next iteration. Per-slug handle SETS are mandatory, and must be **structural, not
convention** — there must be no way for the tick to write a global handle.

OQ-1 asks: who owns the per-slug handle set — the per-slug `UnimatrixServer`/`ServiceLayer` (the
serving side already holds them) referenced by the tick, or a parallel registry? They MUST be the
same handles the serving path reads (SR-08: different instances ⇒ AC-5 passes structurally but
serving reads stale state).

### Decision
**The per-slug `ServiceLayer` is the sole owner of that slug's handle set. There is no parallel
handle registry.** The tick does not own or create handles; it **borrows** them via the same
`*_handle()` accessors the serving path already uses.

Wave 1 (ADR-002) already builds one config-driven `ServiceLayer` per slug, each constructing its
own five handles. Wave 2 introduces `PerSlugTickContext` as a **thin borrow bundle**, NOT a new
owner:

```rust
pub struct PerSlugTickContext {
    pub slug: ProjectSlug,
    pub store: Arc<Store>,
    pub vector_index: Arc<VectorIndex>,
    // Handles are CLONES OF THE SLUG'S ServiceLayer ACCESSORS — same Arc<RwLock<_>>,
    // not new instances. This is the structural guarantee (SR-01/SR-08).
    pub confidence: ConfidenceStateHandle,
    pub effectiveness: EffectivenessStateHandle,
    pub typed_graph: TypedGraphStateHandle,
    pub contradiction: ContradictionScanCacheHandle,
    pub phase_freq: PhaseFreqTableHandle,
    pub tick_metadata: Arc<Mutex<TickMetadata>>,  // per-slug (ADR-005)
}

// Built at boot, immediately after build_project_server returns each server:
let sl = &input.server.services;   // the per-slug ServiceLayer
let ctx = PerSlugTickContext {
    slug: input.slug.clone(),
    store: Arc::clone(&input.store),
    vector_index: /* from server */,
    confidence:    sl.confidence_state_handle(),     // Arc::clone of the SAME handle
    effectiveness: sl.effectiveness_state_handle(),
    typed_graph:   sl.typed_graph_handle(),
    contradiction: sl.contradiction_cache_handle(),
    phase_freq:    sl.phase_freq_table_handle(),
    tick_metadata: Arc::clone(&input.server.tick_metadata),
};
```

Because the `*_handle()` accessors return `Arc::clone`s of the `ServiceLayer`-owned handles, the
`PerSlugTickContext` and the serving path hold the **identical** `Arc<RwLock<_>>`. The tick writes
through it; the next search on that slug reads through it (AC-5, principle 7).

**Structural enforcement (SR-01, "not convention"):**
- A `BackgroundJob` (ADR-004) `run(ctx, shared)` receives state handles **only** through `ctx`. It
  has no path to a global handle — there is no global handle to reach (each slug owns its own).
- The old global `spawn_background_tick` global-handle parameters are **removed** from the
  multi-project path (the daemon's single-store path keeps using its own server's handles via the
  same `PerSlugTickContext` mechanism — there is exactly one isolation seam, ADR-001/ADR-005). No
  parallel global-handle write path survives beside the per-slug seam (the #4974 / SR-07
  ceremonial-funnel guard: grep for any retained global-handle write).

**Proof at N=2, never N=1 (SR-07):** AC-4 ticks slug A then slug B with two real slugs and asserts
A's four caches are unchanged by B's tick. N=1 cannot distinguish "funnel" from "bypass." This same
test **doubles as the concurrency-readiness proof** (SCOPE L135-137): a work-unit that provably
doesn't touch B's state when ticking A serially is, by construction, safe to run concurrently.

### Consequences
- **Easier / collapses risk:** one decision retires SR-01, SR-07, SR-08. Serve/maintain/read are
  provably the same handles. The concurrency-clean contract (AC-7) is satisfied by construction —
  no cross-context shared mutable state, because each context carries its own handle set and
  `shared` is read-only.
- **Harder / cost:** the multi-project tick path must be rebuilt around `PerSlugTickContext` rather
  than reusing `spawn_background_tick`'s global-handle signature. The handles must be wired at boot
  in the same loop that builds the servers (`main.rs:1084`, calling `build_project_server` at
  `main.rs:1085-1092`).
- **Verify-the-funnel obligation:** the implementer MUST confirm no `let _ = ...handle` discard and
  no surviving global-handle write path (the #4974 trap). AC-4 is the load-bearing gate.
- **Invariant for downstream:** `PerSlugTickContext` handles are *borrows of the ServiceLayer's*,
  never freshly constructed. Constructing new handles in the context would silently reintroduce
  SR-08.

Related: ADR-001/ADR-002 (build the per-slug `ServiceLayer` whose handles this borrows), ADR-004
(the job that mutates only `ctx`), ADR-005 (per-slug `TickMetadata` in the context).
