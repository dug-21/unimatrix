# crt-056 — Test Plan Overview: Per-Slug Intelligence Parity

> Per-slug MCP servers reach functional parity with the single-project daemon — correct service
> config (Wave 1) AND maintained analytics (Wave 2) — on a concurrency-clean, registry-based
> per-slug tick work-unit. GH #787 · Capability C5. Tester Stage 3a.
>
> Rooted in: RISK-TEST-STRATEGY.md (R-01..R-12), ACCEPTANCE-MAP.md (AC-1..AC-7 + AC-wave2-gate,
> AC-7-stepb, AC-harness), SPECIFICATION.md (FR-1..18, NFR-1..9), ARCHITECTURE.md (ADR-001..006).

---

## 1. Test Strategy

Three test altitudes, each tied to a risk class:

| Altitude | What it proves | crt-056 use |
|----------|----------------|-------------|
| **Unit** (`#[test]` / `#[tokio::test]`) | Component logic in isolation: constructor `Some`/`None` arms, `Cadence::fires`, registry derivation, per-slug counter independence, handle-identity (`Arc::ptr_eq`). | R-05, R-06, R-07, R-09, R-12; AC-1 (field equality), AC-6, AC-7a/b. |
| **Integration / behavioral** (extend the Layer-2 multi-project Rust harness) | Real multi-project server, **N=2** two-slug tick: corruption guard, serve-reflects-tick, isolation. | R-01, R-02, R-03; **AC-3, AC-4, AC-5** (the load-bearing behavioral trio). |
| **Source audit** (`Grep` + review, NOT a behavioral test) | Funnel is sole mutation route; ops store-parameterized; no Step-B machinery; interior-immutability of shared `Arc`s. | R-01.2, R-02.2, R-04, R-08, R-09; **AC-wave2-gate, AC-7-stepb**, A2 audit. |

**Load-bearing principle (from #4974 + brief):** the seam's *shape* passing is NOT the contract
passing. AC-4 at **N=2** is the single non-substitutable proof; N=1 cannot distinguish a real
per-slug funnel from a global-handle bypass. Every "green" structural test is suspect until AC-4
runs at N=2 against a running multi-project server.

**Sequencing:** Wave 1 tests (AC-1, AC-2, AC-6) gate on Wave 1 code. Wave 2 behavioral tests
(AC-3/4/5/7) gate on Wave 2 code — but the **AC-wave2-gate source audit runs FIRST, before any
Wave 2 code** (see `wave2-gating-audit.md`).

---

## 2. Risk → AC → Test Mapping

| Risk | Pri | AC(s) | Test scenario (component plan) | Type |
|------|-----|-------|--------------------------------|------|
| R-01 ceremonial seam | **Critical** | AC-4, AC-wave2-gate, AC-7a | N=2 funnel proof; verify-the-funnel audit; registry-not-hardcode | integration + audit + unit |
| R-02 cross-slug corruption | **Critical** | AC-4, AC-wave2-gate | N=2 isolation; A1 per-op closure audit; distinct-state-survives-tick | integration + audit |
| R-03 serve/tick divergence | High | AC-5 | search-reflects-tick (N=2); `Arc::ptr_eq` handle identity | integration + unit |
| R-04 nli_handle interior-mutable | High | AC-2 | one-model-in-memory; interior-immutability type audit; cross-slug inference independence | unit + audit + integration |
| R-05 config partial threading | High | AC-1 | 8-field equality vs resolved config; NLI both directions; global-config-only guard | unit |
| R-06 constructor regression / cloud branch | High | AC-6 | test-default preserved byte-for-byte; same-path (`Some`) proof, no `if cloud` branch | unit + audit |
| R-07 counter not per-slug | Med | AC-7b | independent gate firing at N=2; no-loop-global-read audit | unit + audit |
| R-08 rayon monopolisation | Med | AC-7b | rayon entered/exited per slug, never across all N; documented envelope | unit + audit |
| R-09 Step B leakage | High | AC-7-stepb | scope-boundary audit (no queue/pool/residency/cadence-signal); serial-loop assertion | audit |
| R-10 rayon panic SIGABRT | Low | AC-harness | harness installs `panic_handler`; controlled-panic test fails cleanly | integration |
| R-11 adapt/session_capabilities gap | Med | AC-1, AC-4-adjacent | `adapt_service` no-cross-bleed (`session_capabilities` OUT — NOT asserted) | unit + integration |
| R-12 N model copies | Med | AC-2 | exactly one model; per-slug handles are daemon `Arc` instances; no `NliServiceHandle::new()` on per-slug path | unit + audit |

> **`session_capabilities` is OUT (ADR-006/FR-10).** No test asserts it. AC-1 is the closed
> **8-field** checklist: `nli_enabled`, `nli_top_k`, shared loaded `nli_handle`, `inference_config`,
> `confidence_params`, `category_allowlist`, `observation_registry` (domain packs), effective rayon
> pool size. R-11 is covered ONLY via `adapt_service` independence.

**Coverage completeness:** all 12 risks map to ≥1 scenario; all 7 canonical ACs + the 3 sub-clauses
(AC-wave2-gate, AC-7-stepb, AC-harness) map to a verification. No risk is accepted without coverage.

---

## 3. Cross-Component Test Dependencies

```
Wave 1 substrate            Wave 2 work-unit                 Behavioral proofs (N=2)
─────────────────           ─────────────────                ───────────────────────
unimatrix-server-new  ──┐
build-project-server  ──┼──► per-slug ServiceLayer ──► per-slug-tick-context ──┐
daemon-http-boot      ──┘    (config-driven, AC-1/2/6)     (borrow, Arc::ptr_eq) │
                                                            background-job-seam   ├──► AC-3 maintained
                                                            (registry, Cadence)   │    AC-4 isolation ★
                                                            per-slug-tick-loop ───┘    AC-5 serve-reflects
                                                            (serial, counter)          (multi-slug-harness)
```

- **Build→Serve→Tick handle identity** is the single most load-bearing integration point (R-03,
  R-02). `Arc::ptr_eq` between `PerSlugTickContext` handles and the slug `ServiceLayer` accessors
  is the structural guard; AC-5 is the behavioral guard. Tested across
  `per-slug-tick-context.md` (identity) and `multi-slug-harness.md` (behavior).
- **8-field config boundary** (R-05) spans `build-project-server.md` + `daemon-http-boot.md`
  (hand-threaded params-at-end; #2398-class call-site propagation risk).
- **Shared `Arc` immutability boundary** (R-04) is a type-level audit spanning
  `background-job-seam.md` (`SharedTickResources`) — assumed nowhere, audited.
- **AC-4 at N=2** depends on `multi-slug-harness.md` providing a real two-slug running server with
  the rayon `panic_handler` installed (AC-harness is its prerequisite infra).

---

## 4. Integration Harness Plan (Layer-2 / multi-project — CUMULATIVE)

**Extend, do not scaffold (NFR-7, C-9).** The existing infra and the gaps:

### Existing infrastructure (reuse)

| Asset | Path | What it gives us |
|-------|------|------------------|
| Layer-2 routing harness | `crates/unimatrix-server/tests/project_routing_integration.rs` | `build_server()`, `wired_router()` (N slug stores + owned `Vec<Arc<Store>>`), `drive()`, `collect_resp()`, `reached_mcp()`/`funnel_rejected()`. Proves two-slug routing + per-slug store isolation **at the transport layer**. |
| Rayon pool w/ panic_handler | `crates/unimatrix-server/src/infra/rayon_pool.rs` | `RayonPool::new(num_threads, name)` already installs `.panic_handler(\|_\| {})` (#2543). AC-harness is satisfied by *using `RayonPool`*, not a bare `ThreadPoolBuilder`. |
| Background tick | `crates/unimatrix-server/src/background.rs` | `run_single_tick` (`413-804`), the 9 ops, `TickMetadata.tick_counter`. The reused work. |
| ServiceLayer accessors | `crates/unimatrix-server/src/services/mod.rs:274-316` | the borrow surface for `PerSlugTickContext`. |

### Gap (no existing coverage through this surface)

The Layer-2 harness exercises routing/store isolation but **does NOT run a background tick** and
has **no multi-slug tick context**. There is no existing test proving the per-slug *analytics*
funnel (only the per-slug *routing* funnel from vnc-034). This is exactly the crt-056 gap.

### New integration tests to add (Stage 3c)

All added to / alongside `project_routing_integration.rs` (cumulative; same module conventions —
naming `test_{concept}_{behavior}`):

1. **`TickTestHarness`** — extend the routing harness to also build, per slug, the config-driven
   `ServiceLayer` + its `PerSlugTickContext`, sharing **one** `RayonPool` (panic_handler installed)
   and **one** loaded `nli_handle`/embedding model across slugs. Helpers:
   `tick_slug(&harness, idx)`, `snapshot_handles(&ctx)`, `assert_handles_unchanged(before, after)`.
2. **`test_tick_maintains_slug_a_analytics`** (AC-3) — write to A, tick A, assert A's four handle
   states reflect the write.
3. **`test_tick_b_leaves_a_unchanged`** (AC-4 ★, **N=2**) — populate A and B *differently*, tick A
   then B, assert A's `TypedGraphState`/`PhaseFreqTable`/`EffectivenessState`/`ConfidenceState` are
   byte-for-byte unchanged by B's tick, and vice versa. Includes the empty-store B case.
4. **`test_search_a_reflects_tick_unaffected_by_b`** (AC-5) — after ticking A and B, a *search* on
   A reflects post-tick phase/confidence (ranking/score delta vs stale defaults); a search on B is
   unaffected by A.
5. **`test_handle_identity_tick_ctx_eq_service_layer`** (AC-5) — `Arc::ptr_eq` between the
   `PerSlugTickContext` handles and the slug `ServiceLayer` accessors.
6. **`test_panicking_job_caught_no_sigabrt`** (AC-harness, R-10) — register a deliberately-panicking
   job; assert the test fails cleanly (no SIGABRT) because the harness uses `RayonPool`'s
   panic_handler.
7. **`test_noop_job_runs_via_registry`** / **`test_unregistered_job_does_not_run`** (AC-7a) — add a
   no-op `BackgroundJob`, register it, run a loop pass with **zero loop-body edits**; unregister it,
   assert it stops.
8. **`test_interval_gate_fires_per_slug_independently`** (AC-7b, R-07) — two slugs at different
   counter offsets; an `EveryN(4)` gate fires on A but not B on the same pass.
9. **`test_adapt_service_no_cross_slug_bleed`** (R-11) — drive adaptation on A; assert B's adaptive
   state unchanged.
10. **`test_empty_registry_tick_is_noop`** / **`test_slug_added_after_first_pass_picked_up`**
    (edge cases) — N=0 no-op no panic; a slug registered after the first pass is ticked next pass
    with no loop edit.

### infra-001 Python harness — NOT extended

The infra-001 pytest harness is **single-project / daemon-CLI** and has no multi-project tick
surface. crt-056's behavioral contract (per-slug analytics maintenance + cross-slug isolation) is
an in-process Rust-handle concern not visible through the MCP wire in a way the existing Python
suites assert. **Plan: run infra-001 smoke as the mandatory minimum gate (regression guard for the
constructor refactor / boot changes), but add NO new Python suites.** New behavioral coverage lives
in the Rust Layer-2 harness where handle identity and byte-for-byte state are assertable. Filing a
GH Issue for a future multi-project Python harness is the correct path if MCP-wire multi-project
coverage is later wanted — not isolated scaffolding here.

**Smoke-gate suites (Stage 3c, minimum):** infra-001 `-m smoke` (constructor/boot regression guard).
No feature-specific Python additions.

---

## 5. Component Test Plan Index

| Component | Test Plan | Primary risks / ACs |
|-----------|-----------|---------------------|
| `UnimatrixServer::new` (additive `Option<ServiceLayer>`) | `unimatrix-server-new.md` | R-06; AC-6 |
| `build_project_server` (config-parity threading) | `build-project-server.md` | R-05, R-12; AC-1, AC-2 |
| Daemon HTTP boot (thread Arcs, collect contexts) | `daemon-http-boot.md` | R-05, R-06, R-02; AC-1, AC-6 |
| `PerSlugTickContext` (borrow bundle) | `per-slug-tick-context.md` | R-03, R-02; AC-5 (`Arc::ptr_eq`) |
| `BackgroundJob` trait + registry + `Cadence`/`ResourceClass`/`SharedTickResources` | `background-job-seam.md` | R-01, R-04, R-09; AC-7, AC-7-stepb |
| Per-slug tick loop (serial, per-slug counter, serialized rayon) | `per-slug-tick-loop.md` | R-07, R-08, R-01; AC-3, AC-4, AC-7 |
| Multi-slug test harness (Layer-2, rayon panic_handler) | `multi-slug-harness.md` | R-10; AC-3, AC-4, AC-5, AC-harness |
| Wave-2 gating audit (A1 per-op + funnel) | `wave2-gating-audit.md` | R-01.2, R-02.2; AC-wave2-gate |

---

## 6. Edge Cases (RISK §Edge Cases — covered across plans)

| Edge case | Where covered | Expected |
|-----------|---------------|----------|
| N=0 registered slugs | `per-slug-tick-loop.md`, harness #10 | loop is no-op, no panic |
| N=1 | — | **explicitly NOT a valid contract proof** (#4974); AC-4 MUST be N=2 |
| Empty slug store | `multi-slug-harness.md` (AC-4 "B empty" case) | handles at clean defaults, no panic |
| Slug registered after first pass | harness #10 | picked up next pass, no loop edit (FR-12) |
| NLI config-disabled | `build-project-server.md` (NLI both directions) | per-slug NLI off, NLI op no-ops, no spurious rayon |
| Interval-gate boundary (`tick % 4 == 0`) | `per-slug-tick-loop.md` (R-07.1) | fires for that slug only at its counter boundary |
| Concurrent MCP query during tick (same slug) | `per-slug-tick-context.md` | `RwLock` yields consistent pre/post state, never torn |

---

## 7. Test Conventions

- **Arrange/Act/Assert** structure; deterministic (no flaky tests).
- Naming: `test_{component}_{scenario}_{expected}` (Rust) / `test_{concept}_{behavior}` (harness).
- Async: `#[tokio::test]`.
- Cumulative infra: extend `project_routing_integration.rs` + `RayonPool`; **no isolated
  scaffolding** (NFR-7, C-9).
- Field-by-field, never representative-subset (AC-1 closed 8-field; AC-4 four-state byte-for-byte).
- N=2 minimum for the isolation contract; N=1 never substitutes (#4974).

---

## 8. Open Questions

1. **AC-4 state-equality mechanism.** "Byte-for-byte unchanged" on the four handle states needs a
   stable comparison surface (`PartialEq`/serialized snapshot/hash) on `TypedGraphState`,
   `PhaseFreqTable`, `EffectivenessState`, `ConfidenceState`. If any lacks one, delivery must add a
   test-visible snapshot (deterministic) — flagged for the Wave 2 implementer; not a blocker.
2. **AC-5 search-delta determinism.** The "ranking/score delta vs stale defaults" assertion needs a
   fixture where post-tick phase blending + confidence produce a *deterministic, observable* ranking
   change. The harness must seed A with content guaranteeing a non-default `PhaseFreqTable`/
   `ConfidenceState` (same as AC-4's "differently populated"). Pseudocode should specify the seed.
3. **A2 interior-immutability audit surface.** R-04's type-level audit (`RwLock`/`Mutex`/`Cell`/
   `AtomicX`/unsynchronized cache on the inference read path of `nli_handle`, `inference_config`,
   `confidence_params`, `ml_inference_pool`, `embed_handle`) is a delivery item. Any mutability
   found is documented as a **Step-B blocker**, not silently accepted. Confirm where the audit
   record lands (proposed: `background-job-seam.md` audit section + RISK-COVERAGE-REPORT.md gap note).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` (crt-056 ADRs #5164/#5165/#5168;
  ceremonial-seam **#4974** verify-the-funnel 5-point checklist; panic_handler **#2543**;
  interior-mutable hazard #1494/#3354) -- all directly ground the AC-wave2-gate, AC-harness, and
  R-04 plans below.
- Stored: nothing novel at plan time -- the load-bearing patterns (ceremonial seam, panic_handler,
  interior-mutable shared-`Arc`) are existing first-class entries this plan applies. Any new test
  fixture pattern discovered in Stage 3c (e.g. the multi-slug TickTestHarness shape) is a candidate
  for `/uni-store-pattern` at execution time.
