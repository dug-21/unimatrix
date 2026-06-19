# Test Plan: Multi-slug test harness (Layer-2, rayon panic_handler)

> Component: extend the cumulative Layer-2 / multi-project Rust harness
> (`crates/unimatrix-server/tests/project_routing_integration.rs`), NFR-7 / C-9.
> This is the home of the **load-bearing behavioral trio AC-3 / AC-4 / AC-5** plus the AC-harness
> infra obligation. **No isolated scaffolding** — extend `build_server()`/`wired_router()` and use
> `RayonPool` (which already installs the `panic_handler`, `src/infra/rayon_pool.rs`, #2543).
> Risks: **R-01, R-02, R-03, R-10, R-11, R-04** (cross-slug inference). ACs: **AC-3, AC-4, AC-5,
> AC-harness**, AC-7 (behavioral registry).

> **AC-4 at N=2 is the single non-substitutable proof** — corruption guard + cross-tenant
> data-isolation + AC-7 concurrency-readiness. N=1 cannot distinguish a real per-slug funnel from a
> global-handle bypass (#4974). No N=1 test stands in for it under any priority.

---

## Harness extension (cumulative — NFR-7)

Add a `TickTestHarness` to / beside `project_routing_integration.rs`, building on the existing
`build_server()` / `wired_router()` (which already give N per-slug stores with isolation):

- Per slug: build the **config-driven** `ServiceLayer` + its `PerSlugTickContext`.
- **One shared** `RayonPool` (panic_handler installed via `RayonPool::new`) and **one** loaded
  `nli_handle` + embedding model `Arc`, shared read-only across slugs.
- Helpers:
  - `tick_slug(&harness, idx) -> Result<(), String>` — runs one tick over slug idx's context.
  - `snapshot_handles(&ctx) -> HandleSnapshot` — deterministic snapshot of the four AC-4 states.
  - `assert_handles_unchanged(before, after)` — byte-for-byte (or stable-hash) equality.
  - `search_slug(&harness, idx, query) -> Vec<Result>` — drives a real search on slug idx.

> **Open question (see OVERVIEW §8):** the four AC-4 states need a stable comparison surface
> (`PartialEq` / serialized snapshot / hash). If absent, delivery adds a deterministic test-visible
> snapshot. Flag to the Wave 2 implementer.

---

## Behavioral test expectations

### AC-3 — analytics maintained (R-03 part, FR-13)
- `test_tick_maintains_slug_a_analytics`
  - **Arrange:** running multi-project harness; write entries to slug A.
  - **Act:** run one tick over A.
  - **Assert:** A's `ConfidenceState`, co-access graph (`TypedGraphState`), `PhaseFreqTable`, and
    `ContradictionScanCache` **changed to reflect the write** — not merely "the handle exists." Use
    `snapshot_handles` before/after and assert a real delta keyed to A's content.

### AC-4 ★ — isolation / corruption guard (R-01 + R-02; N=2)
- `test_tick_b_leaves_a_unchanged` **(N=2, load-bearing)**
  - **Arrange:** two real registered slugs A and B, populated **differently** (A with content that
    produces a non-default `ConfidenceState`/`PhaseFreqTable`; B differently — include the **empty-B**
    case in a variant).
  - **Act:** tick A, snapshot A's four states; tick B; re-snapshot A.
  - **Assert:** A's `TypedGraphState`/`PhaseFreqTable`/`EffectivenessState`/`ConfidenceState` are
    byte-for-byte unchanged by B's tick. **And vice versa** (`test_tick_a_leaves_b_unchanged`).
  - **Why N=2:** at N=1 a global-handle bypass and a real per-slug funnel are indistinguishable
    (both point at the one handle set) — every N=1 test is green. Only a second, differently-
    populated slug surfaces a residual cross-slug write (#4974 checklist 5). This test doubles as the
    AC-7 concurrency-readiness proof and the cross-tenant data-isolation (security) proof.
- `test_distinct_state_survives_other_tick` (R-02.3)
  - **Arrange:** populate A to a non-default state; tick B (empty or different).
  - **Assert:** re-read A's four states == the pre-B-tick snapshot.

### AC-5 — serving reads maintained state (R-03; N=2)
- `test_search_a_reflects_tick_unaffected_by_b`
  - **Arrange:** after ticking A and B (differently populated).
  - **Act:** run a **search** on slug A.
  - **Assert:** results reflect A's post-tick state — phase blending + confidence applied, a
    **ranking/score delta vs stale defaults** (not "handle exists/changed"). A search on B is
    unaffected by A's state.
  - **Pairs with** the `Arc::ptr_eq` handle-identity assertion in `per-slug-tick-context.md` — the
    structural guarantee that serving reads the SAME instance the tick wrote.
  > **Open question (OVERVIEW §8):** seed A so post-tick blending yields a *deterministic, observable*
  > ranking change. Pseudocode should specify the seed content.

### AC-harness — rayon panic_handler installed (R-10)
- `test_panicking_job_caught_no_sigabrt`
  - **Arrange:** register a deliberately-panicking `BackgroundJob`; tick a slug.
  - **Assert:** the test fails cleanly (the panic is contained → the job returns an error / the loop
    isolates it) with **no SIGABRT** — because the harness's rayon work runs on `RayonPool` whose
    `panic_handler` (#2543) disables the abort-on-panic propagation. Manifestation to guard against:
    "signal: 6, SIGABRT" in cargo output.
- **Assert (structural):** the harness constructs its pool via `RayonPool::new(...)`, never a bare
  `rayon::ThreadPoolBuilder` without `.panic_handler`.

### AC-7 (behavioral registry) — no-op job via registry, no loop edit
- `test_noop_job_runs_via_registry` / `test_unregistered_job_does_not_run`
  - Behavioral confirmation (complements the unit form in `background-job-seam.md`): add a no-op
    `BackgroundJob`, register, run a loop pass with zero loop-body edits — it runs; unregister — it
    stops.

### R-11 — adapt_service no cross-slug bleed
- `test_adapt_service_no_cross_slug_bleed`
  - **Arrange:** drive adaptation on slug A.
  - **Assert:** slug B's adaptive state is unchanged (adjacent to AC-4's isolation). `adapt_service`
    is per-slug independent state. **`session_capabilities` is OUT — not asserted.**

### R-04 — cross-slug inference independence (complements the type audit)
- `test_cross_slug_inference_independent`
  - **Arrange:** tick A then B, each exercising NLI inference.
  - **Assert:** B's inference does not alter A's results on a re-query (catches a globally-keyed
    shared mutable cache in `nli_handle`).

---

## Edge cases (behavioral)

- `test_empty_registry_tick_is_noop` — N=0: no panic, no-op.
- `test_empty_slug_store_clean_defaults` — ticking an empty slug leaves clean-default handles, no
  panic (the empty-B variant of AC-4).
- `test_slug_added_after_first_pass_picked_up` — a slug registered after the first pass is ticked on
  the next pass with no loop-body edit (FR-12).
- `test_failing_job_on_a_does_not_abort_b` — per-slug failure isolation (see `per-slug-tick-loop.md`).

---

## Coverage requirement

The behavioral trio AC-3 / AC-4 / AC-5 runs against a **running multi-project server at N=2** using
the extended Layer-2 harness with the rayon `panic_handler` installed (AC-harness). AC-4 is
byte-for-byte over the four states, both directions, including the empty-B case; N=1 is never a
substitute. AC-5 proves a search delta, not handle existence, paired with `Arc::ptr_eq` identity.
