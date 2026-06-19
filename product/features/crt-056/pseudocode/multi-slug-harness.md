# Component: Multi-slug test harness — Layer-2, N=2, rayon `panic_handler`

> Wave 2. ADR-005, NFR-7, C-9. Enables AC-3/AC-4/AC-5 (behavioral, two-slug, running multi-project
> server). Risks R-10 (rayon SIGABRT), and the harness that makes R-01/R-02/R-03 testable at N=2.
> Evidence: #2543 (panic_handler required), #2535 (monopolisation envelope), #4974 (N=2 not N=1).
>
> CUMULATIVE: extend the EXISTING Layer-2 / multi-project harness. Do NOT create isolated scaffolding
> (C-9). This is pseudocode for the harness additions, not a new test framework.

## Purpose

Provide a real two-slug, running-multi-project-server fixture so the load-bearing behavioral ACs run
at N=2: populate slugs A and B differently, run real tick passes, and assert isolation + serve-reflects-
tick. Install the rayon `panic_handler` so a tick-closure panic fails the test cleanly instead of
SIGABRTing the process.

## Harness additions

### `install_rayon_panic_handler()` (NFR-7, R-10)

```text
# Mirror the production rayon pool construction's panic_handler (evidence #2543, #3355).
# Install ONCE per test process (idempotent / OnceCell-guarded) before any rayon pool is built.
fn install_rayon_panic_handler():
    # The shared RayonPool the harness builds for SharedTickResources MUST be constructed with a
    # panic_handler that logs + records, NOT the default (which aborts). Use the same RayonPool::new
    # path production uses with a panic_handler set, so a panicking job surfaces as RayonError::Cancelled
    # (probe pattern #3355) rather than SIGABRT.
    build harness RayonPool with panic_handler installed
```

### `spawn_two_slug_server(populate_a, populate_b) -> MultiSlugFixture` (AC-3/4/5, C-9)

```text
fn spawn_two_slug_server(populate_a, populate_b) -> MultiSlugFixture:
    install_rayon_panic_handler()
    base_dir = temp dir
    register slug "a" and slug "b" (create their stores — register is the sole creator, no auto-create)
    populate_a(store_a)        # differently-populated: produces a NON-DEFAULT ConfidenceState/PhaseFreqTable
    populate_b(store_b)        # different content (or empty — AC-4 includes the empty case)
    # Build resolved config with NLI on (or both directions per R-05.2), real shared nli/embed handles.
    boot the multi-project HTTP server over [a, b] via the same daemon-http-boot path:
        - build_project_server(a, ...resolved config...), build_project_server(b, ...)
        - collect contexts [ctx_a, ctx_b]; build one SharedTickResources (shared rayon pool w/ handler)
    return MultiSlugFixture {
        contexts: [ctx_a, ctx_b],
        shared,
        registry: build_job_registry(),
        server,                 # running multi-project server for search-path assertions (AC-5)
    }
```

### Test-driver helpers

```text
fn tick_once(fixture, ctx):           # run one slug's full registry pass (the loop body for one ctx)
    current = ctx.next_tick()
    for job in fixture.registry:
        if job.cadence().fires(current): job.run(ctx, &fixture.shared).await  # ignore Ok; surface Err

fn tick_pass(fixture):                # run_per_slug_tick_pass over [ctx_a, ctx_b] in order
    run_per_slug_tick_pass(&fixture.contexts, &fixture.registry, &fixture.shared).await

fn snapshot_handles(ctx) -> HandleSnapshot:   # read-lock-clone of the 4 AC-4 states for byte-compare
    { typed_graph: read(ctx.typed_graph), phase_freq: read(ctx.phase_freq),
      effectiveness: read(ctx.effectiveness), confidence: read(ctx.confidence) }
```

## Data flow
- Inputs: two populate closures + resolved config.
- Outputs: a `MultiSlugFixture` exposing the two contexts, shared resources, registry, and a running
  server for search assertions.
- Transformation: real store population + real tick passes (no stubs; the corruption guard requires a
  real two-slug tick, NFR-7).

## Error handling
- A panicking job is caught by the installed rayon `panic_handler` → surfaces as a recordable error /
  `RayonError::Cancelled`, the test FAILS cleanly (no SIGABRT) — verified by a controlled-panic test.
- Per-slug job errors are logged-and-continued by the loop; the driver helpers surface `Err` so a test
  can assert isolation explicitly.

## Key test scenarios this harness enables (hints for tester)
- **AC-4 / R-01.1 / R-02.1 (load-bearing, N=2).** `s0_a = snapshot_handles(ctx_a)`; `tick_once(ctx_b)`;
  assert `snapshot_handles(ctx_a) == s0_a` (B's tick left A's 4 states unchanged); symmetric for B.
- **AC-3 / R-02.3 (analytics maintained).** Write to A; `tick_once(ctx_a)`; assert A's confidence/
  co-access/phase/contradiction caches CHANGED to reflect the write (not "handle exists").
- **AC-5 / R-03.1 (serve reflects tick).** After ticking A and B, a SEARCH on slug A via `fixture.server`
  reflects A's post-tick phase blending + confidence (ranking/score delta), and a search on B is
  unaffected by A's state.
- **R-03.2 handle-identity.** `Arc::ptr_eq(ctx_a.typed_graph, server_a.services.typed_graph_handle())`.
- **R-10 (controlled panic).** Register a deliberately-panicking job; `tick_pass` fails gracefully, no
  SIGABRT; B's tick still ran (isolation) where applicable.
- **R-08.2 (monopolisation envelope).** Measure worst-case single-slug tick duration; record it as the
  documented MCP-latency monopolisation envelope (#2535) — a documented number, not a built limit.
- **R-12 / AC-2.** Assert exactly one NLI + one embedding model loaded across the two-slug fixture;
  per-slug handles are the same `Arc` instances (no `NliServiceHandle::new()`).

## Constraints honored
- **C-9 cumulative + N=2:** extends the existing Layer-2 harness; AC-4 is a real two-slug tick, never a
  unit stub; rayon `panic_handler` installed.
- **No N=1 substitute for AC-4** (#4974): the fixture is always two real, differently-populated slugs.
</content>
