# Test Plan: `PerSlugTickContext` (borrow bundle)

> Component: new (location TBD in `background.rs` or new module), ADR-003.
> A thin **borrow** bundle of one slug's `{ slug, store, vector_index, 5 analytics handles,
> tick_metadata }`. Handles are `Arc::clone`s of the slug's `ServiceLayer` `*_handle()` accessors
> (`services/mod.rs:274-316`) — the **SAME** `Arc<RwLock<_>>`, NOT new instances.
> Risks: **R-03** (serve/tick handle divergence), **R-02** (cross-slug corruption).
> ACs: **AC-5** (handle identity half), feeds AC-3/AC-4. FR-11, FR-15.

The single most load-bearing integration point: one handle set per slug, **built** at boot,
**read** by serving, **written** by the tick. Divergence at any of the three produces a green-but-
broken state (R-03). `Arc::ptr_eq` is the structural guard; AC-5 (behavioral) is in
`multi-slug-harness.md`.

---

## Unit test expectations

### AC-5 (structural) — handle identity: context handles ARE the ServiceLayer's
- `test_per_slug_context_handles_are_service_layer_arcs`
  - **Arrange:** build a slug's `ServiceLayer`; construct its `PerSlugTickContext`.
  - **Act / Assert (`Arc::ptr_eq` for all 5):**
    - `ctx.confidence` ptr_eq `service_layer.confidence_state_handle()`
    - `ctx.effectiveness` ptr_eq `service_layer.effectiveness_state_handle()`
    - `ctx.typed_graph` ptr_eq `service_layer.typed_graph_handle()`
    - `ctx.contradiction` ptr_eq `service_layer.contradiction_cache_handle()`
    - `ctx.phase_freq` ptr_eq `service_layer.phase_freq_table_handle()`
  - **Crucial:** these must be clones of the same `Arc<RwLock<_>>`, **not** clones of the underlying
    state into *new* `RwLock`s. A new `RwLock` would pass an "exists/changed" test (AC-3) while
    serving reads a stale instance (R-03). `ptr_eq` is the only assertion that catches this.

### FR-11 — one context per registered slug; store identity
- `test_per_slug_context_store_is_slug_store`
  - **Assert:** `ctx.store` is the slug's own store (`Arc::ptr_eq` to the slug store from the
    routing registry), not a global/default store. (Guards against a context built over the wrong
    store — a corruption vector adjacent to R-02.)

### ADR-005 — per-slug TickMetadata
- `test_per_slug_context_owns_distinct_tick_metadata`
  - **Arrange:** two contexts for two slugs.
  - **Assert:** `ctx_a.tick_metadata` is NOT `Arc::ptr_eq` to `ctx_b.tick_metadata` — each slug owns
    its own `Arc<Mutex<TickMetadata>>` (per-slug counter falls out for free, R-07).

---

## Edge case / failure mode

- **Concurrent MCP query during a tick (same slug):** serving reads the handle mid-rebuild. Because
  context and serving share the SAME `Arc<RwLock<_>>` (proved above), `RwLock` semantics yield a
  consistent pre- or post-rebuild snapshot, never torn state. *Testable (light):* a read-lock taken
  during a write-lock window observes either the old or the new value, never a partial — assert via
  `RwLock` guard ordering (no separate instance to desync).

## Coverage requirement

AC-5's structural half = `Arc::ptr_eq` between context handles and `ServiceLayer` accessors for all
five handles (R-03). Distinct per-slug `tick_metadata` (R-07). Slug-store identity (R-02 adjacent).
The behavioral half of AC-5 (search reflects tick) is in `multi-slug-harness.md`.
