# Risk-Based Test Strategy: crt-056

> Per-slug intelligence parity — maintained analytics + correct service config on a
> concurrency-clean per-project tick work-unit. GH #787. Capability C5.
>
> Inputs: SCOPE.md, ARCHITECTURE.md (ADR-001..006), SPECIFICATION.md (FR-1..18, AC-1..7),
> SCOPE-RISK-ASSESSMENT.md (SR-01..10).
> Historical evidence: Unimatrix #4974 (ceremonial seam / N=1 false confidence, vnc-034),
> #2535 (rayon monopolisation, crt-022), #2543 (rayon panic SIGABRT in tests),
> #1494 / #3354 (snapshot-before-spawn — interior-mutable closure-capture hazard).

This strategy identifies what could go wrong in **this design** — the ServiceLayer-owns-handles
funnel (ADR-003), the additive `Option<ServiceLayer>` constructor (ADR-001), config-parity
field-by-field threading (ADR-002), and the serial per-slug `BackgroundJob` loop with per-slug
counters and serialized rayon (ADR-004/005). Risks are the tester's mandate; AC-4 is the
load-bearing one and doubles as the concurrency-readiness proof (AC-7).

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Ceremonial BackgroundJob seam** — job `run` resolves the per-slug handle set but mutates via a parallel/global write path; N=1 stays green, contract unproven (direct #4974 precedent). | High | High | **Critical** |
| R-02 | **Cross-slug handle corruption** — a residual shared-singleton write (or a tick op closing over a global handle, A1) lets slug A's rebuild overwrite slug B's `TypedGraphState`/`PhaseFreqTable`/etc. | High | Med | **Critical** |
| R-03 | **Serve/tick handle divergence** — tick rebuilds a different instance than the serving path reads; AC-3 passes ("handle exists / changed") yet search returns stale state (FR-15, SR-08). | High | Med | **High** |
| R-04 | **`nli_handle` interior-mutable cache hazard (A2)** — a "read-only" shared `Arc` carries interior-mutable loaded-model/inference cache state; serial loop hides it, but it is an SR-01-class cross-slug write the moment Step B runs it concurrently — and AC-4 may not detect it under serial execution. | High | Med | **High** |
| R-05 | **Config-parity partial threading** — one of the 8 fields silently keeps a test default (NLI flag flips, default `ConfidenceParams`, empty `CategoryAllowlist`, domain packs, pool size); a representative-subset assertion misses it and ships a degraded slug. | High | Med | **High** |
| R-06 | **Additive constructor regression / cloud-only branch** — `Option<ServiceLayer>` refactor breaks existing test-default call sites, OR `Some`/`None` paths diverge so the daemon and per-slug traverse different parity logic (violates one-isolation-seam, NFR-5). | High | Low | **High** |
| R-07 | **Per-slug counter not actually per-slug** — `tick_counter` still read from a loop-global source; interval gates (`tick % 4`, contradiction cadence) fire synchronously for all slugs (latency spike) and a job reading loop-global state breaks the no-shared-mutable contract (FR-18, SR-09). | Med | Med | **Medium** |
| R-08 | **Rayon monopolisation of the MCP hot path** — a long per-slug tick closure holds the shared pool; MCP latency degrades to single-slug-tick duration; worse if the loop holds rayon across all N slugs in one closure (#2535, NFR-3). | Med | Med | **Medium** |
| R-09 | **Step B leakage** — queue/pool/residency/cadence machinery creeps in under the BackgroundJob banner; ships half a scheduler, inflates blast radius (SR-04, C-2). | High | Med | **High** |
| R-10 | **Rayon panic SIGABRT in the multi-slug harness** — a tick-closure panic without the rayon `panic_handler` aborts the whole test process; AC-3/4/5 become flaky/un-runnable (#2543, NFR-7). | Low | Med | **Low** |
| R-11 | **`adapt_service` / `session_capabilities` parity gap** — per-slug `adapt_service` bleeds across slugs, or `session_capabilities` derived from wrong/empty config, leaving a still-degraded slug despite AC-1 (FR-10, OQ-5). | Med | Low | **Medium** |
| R-12 | **N model copies / unloaded per-slug handle** — a per-slug path constructs a fresh `NliServiceHandle::new()` or clones the model rather than the shared `Arc`; memory blow-up and cold/wrong inference (FR-6, AC-2). | Med | Low | **Medium** |

Priority = Severity × Likelihood. Critical = High×High or the load-bearing contract proof.

---

## Risk-to-Scenario Mapping

### R-01: Ceremonial BackgroundJob seam (the #4974 trap, applied to the tick)
**Severity**: High · **Likelihood**: High · **Priority**: Critical
**Impact**: AC-7's concurrency-clean contract is the whole north-star of the feature. If the
seam proves *shape* (a job trait + registry) but the actual mutation flows through a parallel or
global handle path, every single-slug test is green and the contract is silently deferred —
exactly the `let _store` discard failure of vnc-034. The Step B "1–2 week additive follow-up"
promise becomes false.

**Test Scenarios**:
1. **N=2 funnel proof (AC-4).** Two real registered slugs, populated *differently*; tick A then B;
   assert B's tick leaves A's `TypedGraphState`/`PhaseFreqTable`/`EffectivenessState`/`ConfidenceState`
   byte-for-byte unchanged, and vice versa. N=1 is explicitly insufficient — it cannot distinguish
   funnel from bypass (#4974 checklist item 5).
2. **Verify-the-funnel source audit (#4974 checklist 1–4) — paired with the A1 per-op audit (R-02.2),
   both Wave-2-GATING.** Grep the job `run` path for a discarded resolved handle (`let _`, unused
   binding) and for any global/shared analytics-handle write path beside `PerSlugTickContext`. Assert
   the per-slug handle set is the **sole** mutation route and that `BackgroundJob::run` has no
   trait-default that could reintroduce a `{ }` no-op bypass. This funnel audit and the A1 per-op
   closure audit (R-02.2) run together as the **first act of Wave 2, before any Wave 2 code** — not as
   an end-gate check. Rationale: if even one op carries a hidden global-handle write the probes missed,
   AC-4 can pass for the clean ops while the missed op corrupts B's state; the funnel can only be proven
   sole once every op is confirmed store-parameterized.
3. **Registry-not-loop-hardcode (AC-7a).** Add a no-op `BackgroundJob`, register it, run a loop pass;
   assert it executes with **zero edits to the loop body**. Conversely, unregister a job and assert it
   stops running — proving the iterated set is registry-derived (FR-12, FR-16).

**Coverage Requirement**: AC-4 MUST run at N=2 against a running multi-project server (NFR-7) and is
the accepted concurrency-readiness proof. A source audit confirming a single mutation route MUST
accompany it. The A1 per-op closure audit (R-02.2) is a **Wave-2-gating precondition** run before any
Wave 2 code — paired with this verify-the-funnel source audit (no `let _` discard, no surviving
global-handle write path) — not an end-gate check. No N=1 test may stand in for AC-4.

### R-02: Cross-slug handle corruption (the SCOPE crux)
**Severity**: High · **Likelihood**: Med · **Priority**: Critical
**Impact**: The pre-crt-056 singletons (`Arc<RwLock<_>>` not keyed by slug — extracted at
`crates/unimatrix-server/src/main.rs:957–961`, then handed to `spawn_background_tick` at :968–991)
mean a naive iteration
overwrites each slug's state with the next. Slug A's co-access graph silently becomes slug B's;
retrieval quality is corrupted with no error.

**Test Scenarios**:
1. AC-4 (as R-01.1) is the primary detector.
2. **Per-op global-handle audit (A1 verification) — FIRST ACT OF WAVE 2, before any Wave 2 code.**
   For each of the 9 tick operations dispatched inside `run_single_tick` (`crates/unimatrix-server/
   src/background.rs`, fn at :413 — call sites ~:413–440/463/552/566; the loop body that invokes
   `run_single_tick` is :363–396, NOT the op list), confirm by source closure-check that the op takes
   `&Store` and writes only the passed-in handle — none closes over a global/static handle. A single op
   reaching a singleton defeats the per-slug funnel even with correct context plumbing, and would let
   AC-4 pass for the clean ops while the missed op corrupts B's state. This audit GATES Wave 2: the
   "MODERATE not HARD" verdict rests entirely on all 9 ops being cleanly store-parameterized, so it must
   be source-confirmed before code, not asserted and deferred to the end gate. Pair with the AC-4 N=2
   verify-the-funnel source audit (R-01.2).
3. **Distinct-state-survives-tick.** Populate A with content that produces a *non-default*
   `ConfidenceState`/`PhaseFreqTable`; tick B (empty or differently populated); re-read A's four
   states and assert they equal the pre-B-tick snapshot.

**Coverage Requirement**: AC-4 plus the A1 per-operation closure audit that no tick op closes over a
global handle. The A1 audit is a **Wave-2-gating precondition** (first act of Wave 2, before any Wave 2
code) paired with the AC-4 verify-the-funnel source audit — not an end-gate check. Both required to
consider R-02 mitigated.

### R-03: Serve/tick handle divergence (in-memory hot path)
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: If the tick rebuilds handle instance X but the serving path reads instance Y, AC-3
("handle changed") passes while **search returns stale analytics** — the self-learning surface is
dead at the user-visible layer despite a green maintenance test (SR-08).

**Test Scenarios**:
1. **AC-5 behavioral serve-reflects-tick.** Store to A, tick A, run a *search* on A; assert results
   reflect post-tick phase blending + confidence (ranking/score changes), not stale defaults — and a
   search on B is unaffected by A's state.
2. **Handle-identity assertion.** Assert the `PerSlugTickContext` handles are the **same `Arc`
   instances** the slug's `ServiceLayer` accessors return (`Arc::ptr_eq`), not clones of the
   *underlying state into new `RwLock`s*.

**Coverage Requirement**: AC-5 must prove serving reads post-tick state behaviorally (a search delta),
not "handle exists/changed." Handle-identity (`Arc::ptr_eq`) asserted between tick context and serving
accessors.

### R-04: `nli_handle` interior-mutable cache hazard (A2)
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: ADR-002 assumes the shared `Arc`s (`nli_handle`, `inference_config`, `confidence_params`,
`ml_inference_pool`, `embed_handle`) are truly immutable at inference time. The `nli_handle` carries a
loaded-model state machine; if it holds interior-mutable cache/state, two slugs sharing it mutate
shared state — an SR-01-class hazard that the **serial** loop hides today and AC-4 may NOT catch
(serial execution never overlaps). It surfaces only under Step B, after the contract was "proven."
Mirrors the snapshot-before-spawn closure-capture trap (#1494/#3354).

**Test Scenarios**:
1. **One-model-in-memory (AC-2).** Assert exactly one NLI and one embedding model loaded in process and
   that per-slug handles are the *same `Arc` instances* as the daemon's (no `NliServiceHandle::new()`,
   no N copies).
2. **Interior-immutability audit (A2).** Inspect each shared `Arc<...>` type for interior mutability
   (`RwLock`/`Mutex`/`Cell`/`AtomicX`/unsynchronized cache) on the **inference read path**. If any
   exists, document it as a Step-B concurrency blocker and assert no per-slug tick writes it.
3. **Cross-slug inference independence.** Tick A then B exercising NLI inference on each; assert B's
   inference does not alter A's results on a re-query (catches a shared mutable cache keyed globally).

**Coverage Requirement**: AC-2 plus an explicit type-level audit that each shared `Arc` is free of
interior-mutable state on the read path; any found mutability documented as a Step-B blocker, not
silently accepted.

### R-05: Config-parity partial threading
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: 8 fields are threaded by hand (ADR-002 params-at-end). Any one silently retaining a test
default (NLI off, default fusion/PPR, empty allowlist, built-in-only packs, pool size 1) ships a
degraded slug. A representative-subset assertion is how this slips through (SR-05).

**Test Scenarios**:
1. **AC-1 field-by-field equality** against the daemon's **resolved** config (not a subset): NLI-enabled
   flag, full `InferenceConfig`, full `ConfidenceParams`, `CategoryAllowlist`, domain pack set,
   effective rayon pool size, `session_capabilities`.
2. **NLI flag both directions.** Config NLI-on ⇒ per-slug on; config NLI-off ⇒ per-slug off. Proves the
   flag is threaded, not hardcoded either way (FR-2).
3. **Global-config-only guard (FR-9).** Assert all per-slug servers resolve to the *same* global config
   values; no per-slug override path exists (keeps #785/C6 out — R-09 adjacent).

**Coverage Requirement**: AC-1 asserts every field of the closed parity checklist against the resolved
config. A subset assertion is a coverage gap and must be rejected in review.

### R-06: Additive constructor regression / cloud-only branch
**Severity**: High · **Likelihood**: Low · **Priority**: High
**Impact**: The `UnimatrixServer::new` refactor (`Option<ServiceLayer>`, ADR-001) is the riskiest edit
to existing code. A broken `None` path breaks unit tests; divergent `Some`/`None` parity logic creates a
cloud-only path the local install never exercises (violates NFR-5 one-isolation-seam, the vnc-034
ADR-003 constraint).

**Test Scenarios**:
1. **AC-6 test-default preserved.** Existing `UnimatrixServer::new` unit-test call sites compile and pass
   unchanged; `None` yields NLI-off / pool-1 / default-params behavior byte-for-byte.
2. **Same-path proof.** Assert the single-project daemon builds its `ServiceLayer` through the **same**
   config-driven path as per-slug servers (both go `Some(config-driven)`); only unit tests use `None`.
   No `if cloud { ... } else { ... }` parity branch (FR-7).

**Coverage Requirement**: AC-6 unchanged unit tests + a structural assertion that daemon and per-slug
share one construction path. The `None` path is reachable only from unit tests.

### R-07: Per-slug counter not actually per-slug
**Severity**: Med · **Likelihood**: Med · **Priority**: Medium
**Impact**: If interval gating reads a loop-global counter, all slugs run heavy interval ops on the same
tick (synchronized latency spike) AND a job reading loop-global state breaks the no-cross-context-mutable
contract — quietly failing AC-7's audit clause (SR-09).

**Test Scenarios**:
1. **Independent gate firing.** Register two slugs at different counter offsets; assert an `EveryN(4)`
   gate fires on A but not B on the same loop pass (counters advance independently per `PerSlugTickContext`).
2. **No loop-global read audit (AC-7b).** Audit job bodies: no job reads a shared/loop-global counter;
   each reads only `ctx.tick_metadata` (FR-18).

**Coverage Requirement**: Behavioral proof that interval gates fire per-slug-independently, plus an audit
that no job reads loop-global counter state.

### R-08: Rayon monopolisation of the MCP hot path
**Severity**: Med · **Likelihood**: Med · **Priority**: Medium
**Impact**: The shared rayon ML pool serves both MCP queries and per-slug ticks (#2535). A long per-slug
tick closure monopolises threads; MCP latency degrades to single-slug-tick duration. Worst case: the
loop holds rayon across **all N slugs** in one closure (NFR-3 explicitly forbids this).

**Test Scenarios**:
1. **Rayon-not-held-across-slugs (FR-17).** Assert each slug's tick enters and exits rayon within its own
   tick; no two slugs' closures hold rayon concurrently; the loop does not wrap all N slugs in one rayon
   closure.
2. **Monopolisation envelope documented (NFR-3).** Measure/assert the worst-case single-slug tick
   duration is recorded as the MCP-latency monopolisation envelope (#2535 pattern: document the envelope,
   don't choose the pool size arbitrarily).

**Coverage Requirement**: FR-17 structural test (rayon entered/exited per slug) + a documented
worst-case single-slug-tick monopolisation envelope.

### R-09: Step B leakage
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: The BackgroundJob seam invites "just a small scheduler" — bounded pool, LRU residency,
cadence signals, concurrent rayon. Shipping half a scheduler inflates the feature and the blast radius
(SR-04, C-2).

**Test Scenarios**:
1. **Scope-boundary audit.** Confirm no queue, worker pool, residency/eviction, cadence-signal, or
   concurrent-rayon machinery is present. `ResourceClass` is a *declaration only* (no scheduler reads it);
   `Cadence` is `EveryTick`/`EveryN` only — no cron/signal machinery (ADR-004).
2. **Serial-loop assertion.** The loop is serial over resident registered slugs; no `spawn`/`join` fan-out
   across slugs (NFR-1, C-1).

**Coverage Requirement**: A reviewer audit that the seam is the SHAPE only — any scheduler machinery is a
scope failure and must be rejected.

### R-10: Rayon panic SIGABRT in the multi-slug harness
**Severity**: Low · **Likelihood**: Med · **Priority**: Low
**Impact**: A tick-closure panic without the rayon `panic_handler` SIGABRTs the whole test process,
making the AC-3/4/5 behavioral tests un-runnable/flaky (#2543).

**Test Scenarios**:
1. **Harness installs `panic_handler` (NFR-7, C-9).** Assert the extended Layer-2 multi-slug harness
   installs the rayon `panic_handler`; a deliberately-panicking job is caught (test fails cleanly, no
   SIGABRT).

**Coverage Requirement**: The multi-slug tick harness installs the rayon `panic_handler`; verified by a
controlled-panic test that fails gracefully.

### R-11: `adapt_service` / `session_capabilities` parity gap
**Severity**: Med · **Likelihood**: Low · **Priority**: Medium
**Impact**: Per OQ-5 (ADR-006/FR-10): `adapt_service` is per-slug-independent, `session_capabilities`
per-slug from global config. If `adapt_service` bleeds across slugs or capabilities derive from
empty/wrong config, the slug is still degraded despite AC-1.

**Test Scenarios**:
1. **`session_capabilities` parity.** Per-slug `session_capabilities` equals the daemon's for the same
   config (part of AC-1 checklist, FR-10).
2. **`adapt_service` independence.** Drive adaptation on A; assert B's adaptive state is unchanged (no
   cross-slug bleed) — adjacent to AC-4's isolation guarantee.

**Coverage Requirement**: `session_capabilities` in the AC-1 field-by-field set; an `adapt_service`
no-cross-bleed assertion alongside AC-4.

### R-12: N model copies / unloaded per-slug handle
**Severity**: Med · **Likelihood**: Low · **Priority**: Medium
**Impact**: A per-slug path that constructs `NliServiceHandle::new()` (the current defect) or clones the
model rather than sharing the `Arc` causes memory blow-up and cold/wrong inference (FR-6).

**Test Scenarios**:
1. **AC-2** (as R-04.1): exactly one model in memory; per-slug handles are the daemon's `Arc` instances.

**Coverage Requirement**: AC-2; no `NliServiceHandle::new()` on the per-slug path (source audit).

---

## Integration Risks

The defining integration surface is **handle identity across three consumers** (build / tick / serve)
of one `ServiceLayer` per slug:

- **Build→Serve→Tick handle identity (R-03, R-02).** The single most load-bearing integration point:
  one handle set per slug, written by the tick, read by serving, built at boot. Divergence at any of the
  three points produces a green-but-broken state. `Arc::ptr_eq` between tick context and serving
  accessors is the structural guard; AC-5 is the behavioral guard.
- **8-field config threading boundary (R-05).** `build_project_server`'s params-at-end signature (ADR-002;
  defined at `crates/unimatrix-server/src/http_provision.rs:125`, per-slug call site at
  `crates/unimatrix-server/src/main.rs:1085–1092`) is a hand-threaded boundary — the #2398-class "new
  fields not propagated to all call sites" risk. Field-by-field AC-1 is the only adequate boundary test.
- **Shared `Arc` immutability boundary (R-04).** Five resources cross into every slug as read-only `Arc`.
  The boundary is sound **only if** each is genuinely free of interior-mutable read-path state — a
  type-level invariant, not a runtime one. Must be audited, not assumed (A2).
- **Tick-op `&Store` boundary (R-02, A1).** Each of the 9 reused ops dispatched in `run_single_tick`
  (`background.rs`, fn at :413) must reach no global store singleton; one closing over a global handle
  silently re-globalizes the funnel. The A1 per-op closure audit confirming this is a **Wave-2-gating
  precondition** (first act of Wave 2), not an end-gate check — the whole "MODERATE not HARD" verdict
  depends on it.
- **Constructor `Option` boundary (R-06).** `Some`/`None` must converge on identical parity logic — the
  one-isolation-seam invariant lives here.

## Edge Cases

- **N=0 registered slugs.** Tick loop over an empty registry is a no-op, doesn't panic.
- **N=1.** Behaviorally indistinguishable from a bypass (#4974) — explicitly NOT a valid contract proof;
  AC-4 must be N=2.
- **Empty slug store.** Ticking a slug with zero entries leaves handles at clean defaults, no panic;
  AC-4's "B differently populated" includes the empty case.
- **Slug registered after first tick pass.** The registry-derived iterated set picks it up on the next
  pass without a loop edit (FR-12).
- **NLI config-disabled.** Per-slug NLI off, tick's NLI op no-ops, no spurious rayon work (R-05.2).
- **Interval-gate boundary** (`tick % 4 == 0`): per-slug counter at exactly the gate boundary fires for
  that slug only (R-07.1); contradiction-scan cadence noted (OQ-3).
- **Concurrent MCP query during a tick** (same slug): serving reads the handle mid-rebuild — `RwLock`
  semantics must yield consistent (pre- or post-rebuild) state, never torn.

## Security Risks

crt-056 adds **no new external input surface** — it threads existing config and reuses existing tick ops
over existing per-slug stores established by vnc-034. Assessment of the relevant surfaces:

- **Untrusted input entering this feature:** none new. The config is operator-resolved (trusted); slug
  store contents already entered via the vnc-034 routing path with its existing authz. crt-056 only
  changes *which config* and *which handles* the existing path uses.
- **Blast radius if a component is compromised:** the corruption guard (R-02/AC-4) is itself a
  **confidentiality/integrity boundary** — a cross-slug handle write would leak slug A's analytics
  (co-access graph, confidence) into slug B's serving results. This is the security-relevant failure mode:
  AC-4 is not only a correctness test but the per-project data-isolation proof (one client : one project,
  vnc-034). A residual global-handle write path is a cross-tenant leak, not merely a bug.
- **No path traversal / injection / deserialization** introduced — no new file paths, query parsing, or
  serialization formats. The shared-model `Arc` is in-process, not a deserialization surface.

Net: the dominant security risk is **cross-slug analytics bleed via a corrupted handle funnel (R-02)** —
covered by AC-4 at N=2.

## Failure Modes

- **Per-slug tick failure isolation (ADR design).** A job error on slug A is logged and the loop
  continues to slug B (mirrors `background.rs:393-395`); one slug's failure never aborts another's tick.
  *Testable:* inject a failing job for A; assert B still ticks and A's prior state is intact.
- **Tick-closure panic.** Caught by the rayon `panic_handler` (R-10) and the outer-handle panic→restart
  wrapper; does not SIGABRT (test) or kill the daemon (prod).
- **Config field missing/unresolved at boot.** Per-slug build must fail loudly (not fall back to a test
  default) — a silent fallback re-creates the original defect. *Testable:* a build with an unthreaded
  field should not silently degrade to defaults.
- **Stale serving state.** If the tick hasn't run yet, serving reads clean-default handles (degraded but
  correct), never another slug's state.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (singleton handles overwrite) | R-02 | ADR-003 makes the per-slug `ServiceLayer` handle set the sole mutation route; AC-4 (N=2) + the per-op A1 audit prove it — the A1 audit runs as the Wave-2-gating first act (before any Wave 2 code), not at the end gate. |
| SR-02 (rayon shared pool monopolisation) | R-08 | ADR-005 serializes rayon via the serial loop; FR-17 test (rayon per-slug, never across N) + documented monopolisation envelope (#2535). |
| SR-03 (test-default constructor / cloud-only path) | R-06 | ADR-001 additive `Option<ServiceLayer>`; AC-6 preserves the `None` path; same-path proof enforces one isolation seam (NFR-5). |
| SR-04 (Step B leakage) | R-09 | ADR-004 builds the seam only; scope-boundary audit rejects any queue/pool/residency/cadence machinery. |
| SR-05 (parity under-defined) | R-05, R-11 | ADR-006 closes the checklist; AC-1 asserts every field (incl. `session_capabilities`) against the resolved config. |
| SR-06 (global-config-only blur / #785 leak) | R-05.3 (+ R-09) | FR-9 guard: all slugs resolve to the same global config; no per-slug override path (kept in #785/C6). |
| SR-07 (ceremonial seam / N=1 false confidence) | R-01 | #4974 verify-the-funnel checklist applied; AC-4 at N=2 is the load-bearing contract + concurrency-readiness proof; no parallel global-handle write path. |
| SR-08 (serve/tick handle identity) | R-03 | ADR-003 one handle set per slug; AC-5 behavioral search-reflects-tick + `Arc::ptr_eq` identity assertion. |
| SR-09 (loop-global `tick_counter` gating) | R-07 | ADR-005 per-slug counters; independent-gate-firing test + no-loop-global-read audit (FR-18). |
| SR-10 (rayon panic SIGABRT in harness) | R-10 | NFR-7/C-9: extended Layer-2 harness installs the rayon `panic_handler`; controlled-panic test. |

All ten SR-XX risks trace to an architecture risk (none accepted-without-coverage). The assumptions
A1–A4 are folded into R-02 (A1), R-04 (A2), R-05/R-06 (A3 threading), and R-08/R-09 (A4 cadence
envelope).

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | AC-4 (N=2 funnel proof) + verify-the-funnel source audit + per-op A1 audit (**Wave-2-gating: first act, before any Wave 2 code**) + registry-not-hardcode |
| High | 5 (R-03, R-04, R-05, R-06, R-09) | AC-5 + `Arc::ptr_eq`; AC-2 + interior-immutability audit; AC-1 field-by-field + NLI both-directions; AC-6 + same-path proof; Step B scope-boundary audit |
| Medium | 4 (R-07, R-08, R-11, R-12) | per-slug gate firing + counter audit; rayon-per-slug + envelope doc; `session_capabilities` parity + adapt independence; AC-2 source audit |
| Low | 1 (R-10) | harness `panic_handler` + controlled-panic test |

**Load-bearing test:** AC-4 at N=2 (R-01 + R-02 + the security data-isolation boundary + the AC-7
concurrency-readiness proof). N=1 is not an acceptable substitute under any priority.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search for ceremonial-seam / N=1 false confidence and rayon
  monopolisation/panic patterns -- found #4974 (verify-the-funnel checklist; directly grounds R-01/SR-07),
  #2535 (monopolisation envelope; grounds R-08/SR-02), #2543 (panic_handler SIGABRT; grounds R-10/SR-10),
  #1494/#3354 (snapshot-before-spawn interior-mutable hazard; grounds R-04/A2), #2398 (call-site
  propagation gap; grounds R-05 integration boundary).
- Stored: nothing novel to store -- the recurring patterns (ceremonial seam, rayon monopolisation,
  interior-mutable shared-Arc hazard) are already first-class Unimatrix entries; this strategy applies
  them rather than discovering a new cross-feature pattern.
