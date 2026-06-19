# Risk Coverage Report: crt-056 — Per-Slug Intelligence Parity

> Stage 3c test execution. Per-slug MCP servers reach functional parity with the single-project
> daemon — config (Wave 1) + maintained analytics (Wave 2) — on a concurrency-clean per-slug tick
> work-unit. GH #787. Capability C5.
>
> Executed 2026-06-19 on branch `feature/crt-056` (HEAD `29585e14`). Tester agent
> `crt-056-agent-6-tester`.

## Executive Summary

- **All gates GREEN. No regressions. No xfails needed — no pre-existing failures surfaced.**
- The load-bearing **AC-4 N=2 cross-slug corruption guard** runs behaviorally against a real
  two-slug, two-store, two-ServiceLayer setup ticked through the SAME serial
  `run_per_slug_tick_pass` the daemon uses — and is **non-vacuous** (asserts A=7 vs B=3 distinct
  states; a global-handle bypass would flip the equality assertion).
- 9 new behavioral tests added cumulatively to the Layer-2 multi-project harness
  (`crates/unimatrix-server/tests/project_routing_integration.rs`) — no isolated scaffolding
  (NFR-7 / C-9). Harness uses `RayonPool` (panic_handler installed, #2543 — AC-harness satisfied).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|------------------|---------|--------|----------|
| R-01 | Ceremonial BackgroundJob seam (#4974 N=1 false-confidence trap) | `test_tick_b_leaves_a_unchanged_n2` (N=2 funnel proof); `test_noop_job_runs_when_registered_unregistered_does_not` (registry-derived, unit); AC-wave2-gate source audit (commit `514d5a8c`) | PASS | Full |
| R-02 | Cross-slug handle corruption | `test_tick_b_leaves_a_unchanged_n2` ★; `test_distinct_state_survives_empty_b_tick`; per-op A1 closure audit (commit `514d5a8c`) | PASS | Full |
| R-03 | Serve/tick handle divergence | `test_serving_accessor_reflects_tick_unaffected_by_b`; `test_handle_identity_tick_ctx_eq_service_layer_n2` (`Arc::ptr_eq`); `test_per_slug_context_handles_are_service_layer_arcs` (unit) | PASS | Full (model-free); search-delta partial — see Gaps |
| R-04 | `nli_handle` interior-mutable cache hazard (A2) | AC-2 unit (`make_server_with_some_layer` handle-identity); one shared `NliServiceHandle` in harness `SharedTickResources`; A2 type-audit = delivery item (Step-B precondition, accepted) | PASS | Full (structural); A2 audit accepted as Step-B precondition |
| R-05 | Config-parity partial threading (8 fields) | AC-1 unit tests (Wave 1, `http_provision`/`server`) | PASS | Full (unit) |
| R-06 | Additive constructor regression / cloud branch | AC-6 unit tests; existing 4237 lib tests compile/pass unchanged via `None` arm | PASS | Full (unit) |
| R-07 | Per-slug counter not per-slug | `test_per_slug_counter_advances_independently_n2`; `test_interval_gate_fires_per_slug_independently[_through_loop]` (unit) | PASS | Full |
| R-08 | Rayon monopolisation | Serial loop (structural, `tick_loop.rs`); job enters/exits rayon within its own slug via `RayonPool::spawn`; never wraps all N | PASS | Full (structural) |
| R-09 | Step B leakage | AC-7-stepb source audit: `ResourceClass` declaration-only, `Cadence` is `EveryTick`/`EveryN` only, serial loop, no spawn/join fan-out | PASS | Full (audit) |
| R-10 | Rayon panic SIGABRT in harness | `test_panicking_job_caught_no_sigabrt` (AC-harness) | PASS | Full |
| R-11 | `adapt_service` / `session_capabilities` parity gap | `test_adapt_service_no_cross_slug_bleed` (per-slug distinct `Arc`); `session_capabilities` OUT (ADR-006) — NOT asserted | PASS | Full (`adapt`); caps out-of-scope |
| R-12 | N model copies / unloaded per-slug handle | AC-2 unit; one shared `NliServiceHandle::new()` handle in harness (never N copies) | PASS | Full (structural) |

★ = the single non-substitutable load-bearing proof.

## Test Results

### Unit Tests (`cargo test -p unimatrix-server`)

Hardened convention: `log="$(mktemp -t uni-test.XXXXXX.log)"; setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-900}" cargo test -p unimatrix-server --jobs 1 > "$log" 2>&1; rc=$?; ...`

> `--jobs 1` was required: this sandbox has 2 GiB swap fully exhausted, and parallel linking of
> multiple large unimatrix-server test binaries OOM-killed `ld` (signal 9 at link, not a test
> failure). Serializing the link step (`--jobs 1`) resolves it deterministically. The hardened
> `setsid -w` + ceiling + file-not-pipe form is preserved.

| Binary | Passed | Failed | Ignored |
|--------|--------|--------|---------|
| `unittests src/lib.rs` | 4237 | 0 | 1 |
| `unittests src/main.rs` | 75 | 0 | 0 |
| `tests/bundle_codec.rs` | 21 | 0 | 1 |
| `tests/cert_provisioner.rs` | 9 | 0 | 0 |
| `tests/client_bundle_e2e.rs` | 4 | 0 | 0 |
| `tests/dockerfile_http_posture.rs` | 2 | 0 | 0 |
| `tests/export_integration.rs` | 21 | 0 | 0 |
| `tests/fingerprint_parity.rs` | 12 | 0 | 1 |
| `tests/graph_subgraph_integration.rs` | 3 | 0 | 0 |
| `tests/import_integration.rs` | 19 | 0 | 0 |
| `tests/pipeline_e2e.rs` | 16 | 0 | 0 |
| `tests/project_routing_integration.rs` | **19** | 0 | 0 |
| **Total** | **4438** | **0** | 4 |

- The 87 crt-056 `background::` component/unit tests (job.rs, jobs.rs, tick_loop.rs) are within the
  4237 lib count and pass.
- `project_routing_integration.rs` grew from 10 → 19: **+9 new crt-056 Wave 2 behavioral tests**.

### New Behavioral Tests (Rust Layer-2 multi-project harness, N=2)

Added to `crates/unimatrix-server/tests/project_routing_integration.rs` (cumulative — extends
`build_server()`/`wired_router()` with a `TickTestHarness`; reuses the SAME real `UnimatrixServer`
+ `Arc<Store>` builders). All 9 PASS:

| Test | AC / Risk | Proves |
|------|-----------|--------|
| `test_tick_maintains_slug_a_analytics` | AC-3 | store→tick→A's `TypedGraphState` rebuilt to reflect the 5-entry write (`all_entries.len()==5`, `use_fallback` cleared); snapshot delta, not "handle exists" |
| `test_tick_b_leaves_a_unchanged_n2` ★ | **AC-4** / R-01 / R-02 | A=7, B=3 differently populated; tick A then B; A's four-state snapshot byte-for-byte unchanged by B's tick AND vice versa |
| `test_distinct_state_survives_empty_b_tick` | AC-4 (empty-B) | A populated to non-default; ticking EMPTY B leaves A intact, B at clean defaults, no panic |
| `test_handle_identity_tick_ctx_eq_service_layer_n2` | AC-5 / R-03 | `Arc::ptr_eq` ctx handles == ServiceLayer accessors (per slug); A's handles ≠ B's (no shared singleton) |
| `test_serving_accessor_reflects_tick_unaffected_by_b` | AC-5 / R-03 | reading through the SERVING accessor reflects post-tick state (A=6, B=2), independent |
| `test_panicking_job_caught_no_sigabrt` | AC-harness / R-10 | a job panicking on the shared `RayonPool` is contained (panic_handler #2543) — reaching the assertion at all is the proof (no SIGABRT) |
| `test_adapt_service_no_cross_slug_bleed` | R-11 | each slug owns a distinct `AdaptationService` `Arc`; ctx's is its own server's |
| `test_empty_registry_tick_is_noop_n0` | Edge N=0 | empty context slice is a no-op, no panic |
| `test_per_slug_counter_advances_independently_n2` | AC-7b / R-07 | A ticked 4× → counter 4; B ticked 1× → counter 1 (independent, not lockstep) |

**Why AC-4 is non-vacuous:** the harness ticks build `TypedGraphState` purely from per-slug store
rows (model-free). A's tick produces `all_entries.len()==7`; B's produces `==3`. A residual
global-handle write (the pre-crt-056 SR-01 defect) would make A's typed-graph reflect B's entry set
(3) after B's tick — flipping `assert_eq!(a_after_b_tick, a_after_a_tick)`. The N=2 distinct
populations are exactly what surfaces a bypass that N=1 cannot (#4974 checklist item 5).

### Integration Tests (infra-001 smoke gate — MANDATORY minimum gate)

```
cd product/test/infra-001
ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so LD_LIBRARY_PATH=/usr/local/lib \
  UNIMATRIX_BINARY=target/release/unimatrix \
  python -m pytest suites/ -v -m smoke --timeout=60
```

Built against `target/release/unimatrix` (cargo build --release, exit 0).

| Suite selection | Total | Passed | Failed | xfail |
|-----------------|-------|--------|--------|-------|
| `-m smoke` (across all 9 suites) | 24 | 24 | 0 | 0 |

Result: **24 passed, 382 deselected in 207.98s.** No failures. The known search-ranking eval flake
did **not** surface in the smoke selection — no triage / no GH Issue / no xfail required this run.

**infra-001 multi-project surface — NOT extended (per Stage 3a plan, OVERVIEW §4):** the Python
harness is single-project / daemon-CLI and has no multi-project tick surface. crt-056's behavioral
contract (per-slug analytics maintenance + cross-slug isolation) is an in-process Rust-handle
concern asserted in the Layer-2 Rust harness where handle identity and byte-for-byte state are
observable. No new Python suites added (NFR-7, avoid isolated scaffolding).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-1 (config parity, 8 fields) | PASS | Wave 1 unit tests (`http_provision`/`server`); within 4237 lib pass. `session_capabilities` OUT (ADR-006) — correctly not asserted. |
| AC-2 (one shared model) | PASS | Wave 1 unit `Arc::ptr_eq` handle-identity tests; harness shares ONE `NliServiceHandle` across slugs (never N copies). |
| AC-3 (analytics maintained) | PASS | `test_tick_maintains_slug_a_analytics` — typed-graph reflects the write behaviorally. |
| **AC-4 (isolation / corruption guard, N=2)** | **PASS** | `test_tick_b_leaves_a_unchanged_n2` ★ + `test_distinct_state_survives_empty_b_tick`. Byte-for-byte four-state snapshot, both directions, incl. empty-B. Doubles as cross-tenant data-isolation + AC-7 concurrency-readiness proof. |
| AC-wave2-gate (A1 per-op + funnel source audit) | PASS | Committed `514d5a8c` — first act of Wave 2; 9/9 ops store-parameterized, funnel sole route. |
| AC-5 (serving reads maintained state) | PASS (model-free) / Partial (search-delta) | `test_handle_identity_tick_ctx_eq_service_layer_n2` (`Arc::ptr_eq`) + `test_serving_accessor_reflects_tick_unaffected_by_b`. Full `search()` ranking-delta is model-bound + crate-private — see Gaps. |
| AC-6 (test path preserved) | PASS | 4237 lib + 75 main tests compile/pass unchanged via the `None` constructor arm; no `if cloud` branch (Wave 1 same-path proof). |
| AC-7 (registry-based, concurrency-clean work-unit) | PASS | `test_noop_job_runs_when_registered_unregistered_does_not` + `test_per_slug_counter_advances_independently_n2`; AC-4 stands as the concurrency-readiness proof. |
| AC-7-stepb (Step B scope boundary) | PASS | Source audit (committed) + `job.rs`: `ResourceClass` declaration-only, `Cadence` `EveryTick`/`EveryN` only, serial loop, no spawn/join fan-out, no queue/pool/residency/cadence-signal. |
| AC-harness (rayon panic_handler) | PASS | `test_panicking_job_caught_no_sigabrt`; harness builds the pool via `RayonPool::new` (panic_handler #2543), never a bare `ThreadPoolBuilder`. |

## Gaps

1. **AC-5 full search-delta (a ranking/score change vs stale defaults) is NOT run over the wire in
   this report.** The serving `SearchService::search()` is `pub(crate)` (unreachable from the
   external `tests/` integration crate) and requires a LOADED ONNX embedding model (the Layer-2
   harness is deliberately model-free, like the vnc-034 routing tests). AC-5 is therefore covered
   here by its **structural guarantee** — `Arc::ptr_eq` between the `PerSlugTickContext` handles and
   the slug's `ServiceLayer` serving accessors (the SAME instance the tick writes is the one serving
   reads) — plus a model-free serving-accessor read that reflects the post-tick per-slug entry set.
   The phase-blending/confidence ranking math itself is exercised by the in-crate unit search tests.
   This is a coverage-altitude note, **not an uncovered risk**: R-03 (serve/tick divergence) is fully
   covered because handle identity is the mechanism that makes serving reflect the tick; a divergent
   instance would fail `Arc::ptr_eq`. No GH Issue filed — extending the Python harness to a
   multi-project, model-loaded search-delta surface is a deferred infra enhancement (OVERVIEW §4),
   not a feature bug.

2. **ConfidenceState is not mutated by a model-free tick (by design).** No background job in
   `run_per_slug_tick_pass` writes `ConfidenceState`; per-entry confidence recompute is a
   fire-and-forget serving-path concern. Consequently AC-4's "ConfidenceState unchanged by B's tick"
   is *structurally* true and is asserted (the four f64 fields are part of the snapshot). AC-3's
   "confidence reflects the write" is realized through the serving path, not the tick — captured
   under Gap 1. No risk is left uncovered: the corruption guard still asserts the four states
   including confidence are byte-for-byte unchanged cross-slug.

3. **A2 interior-immutability of shared `Arc`s on the inference read path** is an accepted Step-B
   precondition (IMPLEMENTATION-BRIEF Load-Bearing Items; HQ-2 ACCEPTED). AC-2 covers it structurally
   (one shared model handle, same `Arc` instances). The serial loop never overlaps two slugs'
   inference, so any interior-mutable read-path cache cannot manifest cross-slug under crt-056's
   serial execution. The type-level audit is recorded as a Step-B concurrency precondition, not
   silently accepted, and not a crt-056 blocker. No mutability was relied upon by any per-slug tick
   write.

## Pre-Existing Failures / xfails / GH Issues

**None.** No integration test failed; no `@pytest.mark.xfail` was added; no GH Issue was filed. The
known search-ranking eval flake (passes in isolation) did not appear in the smoke selection and was
confirmed unrelated to this feature (crt-056 adds no new external input surface and no scoring math —
SCOPE Security Risks). No integration tests were deleted or commented out.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (task: crt-056 Stage 3c behavioral + N=2 corruption
  guard) — surfaced #5147 (per-slug analytics-maintained capability), #4202/#3935 (lesson:
  test-plan-named tests not implemented by 3b → gate passes vacuously; directly motivated WRITING
  the AC-3/4/5 behavioral tests rather than relying on the existing suite), #724 (behavior-based
  ranking-order assertion pattern), #4258 (hardcoded-output fixtures when scoring changes). All
  applied.
- Stored: pattern stored — the multi-slug `TickTestHarness` shape (extend `build_server()` to borrow
  each config-driven `ServiceLayer` into a `PerSlugTickContext`, share ONE `RayonPool`/`NliServiceHandle`,
  snapshot a stable 4-state surface, prove N=2 byte-for-byte isolation model-free via `TypedGraphState`).
  See agent report block for the stored entry ID.
