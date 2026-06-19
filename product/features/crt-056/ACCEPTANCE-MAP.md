# crt-056 Acceptance Criteria Map

> Source: SCOPE.md Acceptance Criteria (AC-1..AC-7), verification detail from SPECIFICATION.md
> and RISK-TEST-STRATEGY.md. All AC-3/4/5 are behavioral tests against a **running multi-project
> server**. AC-4 at N=2 is load-bearing and non-substitutable (doubles as the AC-7
> concurrency-readiness proof).
>
> Regenerated 2026-06-19 after design-review rework: AC-1 is the closed **8-field** ADR-006
> checklist (`session_capabilities` is OUT and NOT asserted); the A1 per-op + verify-the-funnel
> source audit is a **Wave-2-gating precondition** (first act of Wave 2), not an end-gate check.

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-1 | Config parity: a per-slug server reports NLI enabled (when config enables it), the daemon's fusion/PPR/confidence params, the operator category allowlist + domain packs, and a rayon pool sized per config. | test | Field-by-field equality assertion vs the daemon's **resolved** config (not a subset) over the closed **8-field ADR-006 checklist**: `nli_enabled`, `nli_top_k`, shared loaded `nli_handle`, `inference_config`, `confidence_params`, `category_allowlist`, `observation_registry` (domain packs), effective rayon pool size. Plus NLI flag both directions (on⇒on, off⇒off). **`session_capabilities` is OUT (FR-10/ADR-006) and is NOT asserted.** `adapt_service` is per-slug independent state (same config). Covers FR-2..FR-5, FR-9, FR-10. | PENDING |
| AC-2 | Shared model: all per-slug servers reference the one loaded NLI/embedding model. | test | Assert exactly one NLI model and one embedding model loaded in process; assert per-slug model handles are the same `Arc` instances as the daemon's (`Arc::ptr_eq`); no per-slug `NliServiceHandle::new()`, no N copies. Covers FR-6, NFR-2. | PENDING |
| AC-3 | Analytics maintained: store to slug A → run a tick → A's confidence/co-access/phase/contradiction caches reflect the write. | test | Behavioral, running multi-project server: write entries to slug A, run one tick, assert A's `ConfidenceState` / co-access graph / `PhaseFreqTable` / `ContradictionScanCache` changed to reflect the write (not "the handle exists"). Covers FR-13. | PENDING |
| AC-4 | Isolation (corruption guard): after ticking A then B, A's `TypedGraphState`/`PhaseFreqTable`/`EffectivenessState`/`ConfidenceState` are unchanged by B's tick. | test | **Behavioral two-slug test at N=2, running multi-project server.** Populate A and B differently, tick A then B, assert A's four handle states are byte-for-byte unchanged by B's tick (and vice versa). N=1 is insufficient (cannot distinguish funnel from bypass, #4974). The per-slug handle set MUST be the sole mutation route. Doubles as the AC-7 concurrency-readiness proof AND the cross-tenant data-isolation proof. Covers FR-14, FR-15. | PENDING |
| AC-wave2-gate | **Wave-2-GATING source audit — FIRST ACT OF WAVE 2, before any Wave 2 code** (paired A1 per-op + verify-the-funnel audit). | grep | **(a) A1 per-op source audit:** for each of the 9 tick ops dispatched inside `run_single_tick` (`background.rs`, fn def ~`413-804`; call sites `463`/`552`/`566`/`629`/`706`/`794`), source-confirm by closure-check the op takes `&Store` and writes only the passed-in handle — none closes over a global/static handle or store singleton. **(b) Verify-the-funnel audit:** grep the job `run` path for a discarded resolved handle (`let _`, unused binding) and for any global/shared analytics-handle write path beside `PerSlugTickContext`; assert the per-slug handle set is the **sole** mutation route and `BackgroundJob::run` has no trait-default `{ }` no-op bypass. Both run together as the first act of Wave 2, **not** as an end-gate check — a single missed global-handle write lets AC-4 pass for the clean ops while the missed op corrupts B's state. (R-01.2, R-02.2) | PENDING |
| AC-5 | Serving reads maintained state: a search on slug A reflects A's maintained analytics (phase blending, confidence) and is unaffected by B's. | test | Behavioral two-slug test, running multi-project server: after ticking A and B, a search on slug A reflects A's post-tick state (phase blending + confidence applied, a ranking/score delta vs stale defaults); a search on B is unaffected by A's state. Plus handle-identity assertion: `Arc::ptr_eq` between `PerSlugTickContext` handles and the slug's `ServiceLayer` accessors (`services/mod.rs:274-316`). Covers FR-15, FR-16. | PENDING |
| AC-6 | Test path preserved: the existing `UnimatrixServer::new` test-default construction still works for unit tests. | test | Existing `UnimatrixServer::new` unit-test call sites compile and pass unchanged; `None` arm yields NLI-off / pool-1 / default-params behavior byte-for-byte. Plus same-path proof: daemon and per-slug both build via the `Some(config-driven)` path; no `if cloud {…} else {…}` parity branch (one isolation seam). Covers FR-7, FR-8, NFR-6. | PENDING |
| AC-7 | Concurrency-clean, registry-based work-unit: registered `BackgroundJob`s touch only their own `PerSlugTickContext` + shared read-only resources + rayon — no cross-context shared mutable state; adding a job is "implement the interface + register," not a loop rewrite. | test + grep | (a) Structural: today's ops are registered jobs each declaring cadence + resource class; add a no-op `BackgroundJob`, register it, run a loop pass — executes with zero loop-body edits; unregister it — it stops running (registry-derived set, FR-12/FR-16). (b) Audit (grep): no cross-context shared mutable state — no global-handle write path, per-slug counters only (no job reads loop-global counter), serialized rayon (entered/exited per slug, never across all N in one closure). (c) Behavioral: AC-4 stands as the concurrency-readiness proof. Covers FR-11, FR-12, FR-16, FR-17, FR-18. | PENDING |
| AC-7-stepb | Step B scope-boundary audit (sub-clause of AC-7). | grep | Confirm no queue, worker pool, residency/eviction, cadence-signal, or concurrent-rayon machinery present; `ResourceClass` is declaration-only (nothing reads it); `Cadence` is `EveryTick`/`EveryN` only; loop is serial with no `spawn`/`join` fan-out across slugs. Any scheduler machinery is a scope failure. (R-09, C-2) | PENDING |
| AC-harness | Multi-slug test harness installs the rayon `panic_handler`. | test | Extended Layer-2 multi-slug harness installs the rayon `panic_handler`; a deliberately-panicking job is caught (test fails cleanly, no SIGABRT). Supporting infra for AC-3/4/5. (NFR-7, C-9, R-10, #2543) | PENDING |

## Notes

- **Wave-2 gating:** AC-wave2-gate (A1 per-op + verify-the-funnel source audit) is a **Wave-2-gating
  precondition** — the FIRST ACT of Wave 2, before any Wave 2 code is written (RISK R-01.2/R-02.2). It
  is NOT an end-gate check. The per-slug funnel can only be proven sole once every one of the 9 ops is
  source-confirmed store-parameterized; a single missed global-handle write lets AC-4 pass for the
  clean ops while the missed op corrupts B's state.
- **Load-bearing:** AC-4 at N=2 is the single most important test — corruption guard + cross-tenant
  data-isolation + concurrency-readiness proof. N=1 is not an acceptable substitute under any priority.
- **`session_capabilities` is OUT (settled, ADR-006)** — not part of the AC-1 parity checklist. AC-1 is
  the closed 8 config-driven fields. `adapt_service` per-slug independence (no cross-slug bleed) is
  asserted adjacent to AC-4's isolation guarantee (R-11).
- **A2 interior-immutability** is a delivery item: AC-2 covers it structurally; a type-level audit of
  each shared `Arc` for interior-mutable read-path state runs alongside, any mutability documented as a
  Step-B blocker (R-04).
- AC-wave2-gate, AC-7-stepb, and AC-harness are sub-clauses/supporting verifications drawn from the
  RISK-TEST-STRATEGY coverage requirements; they are listed explicitly so delivery does not drop the
  source audits behind the green behavioral tests. The seven canonical SCOPE ACs are AC-1..AC-7.
- Edge cases to cover (RISK §Edge Cases): N=0 (no-op, no panic), empty slug store (clean defaults),
  slug registered after first pass (picked up next pass, no loop edit), NLI config-disabled, interval-gate
  boundary, concurrent MCP query during tick (RwLock yields consistent pre/post state, never torn).
