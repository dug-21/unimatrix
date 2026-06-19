# Test Plan: Per-slug tick loop (serial, per-slug counter, serialized rayon)

> Component: new (`background.rs`), ADR-005. Serial loop over resident registered
> `PerSlugTickContext`s; per-slug `tick_counter` (each context's own `Arc<Mutex<TickMetadata>>`);
> rayon serialized via the serial loop, never held across all N in one closure; per-slug failure
> isolation; outer panic→restart wrapper + rayon `panic_handler` retained.
> Risks: **R-07** (counter not per-slug), **R-08** (rayon monopolisation), **R-01** (serial-loop
> shape), R-09 (serial assertion). ACs: **AC-3** (driven here), **AC-4** (driven here), **AC-7**
> (b: counter/rayon audit). FR-12, FR-17, FR-18.

The behavioral AC-3/AC-4 proofs are executed through this loop in `multi-slug-harness.md`; this plan
covers the loop's structural/audit obligations.

---

## Unit / structural test expectations

### AC-7b — per-slug counter independence (R-07)
- `test_interval_gate_fires_per_slug_independently`
  - **Arrange:** two contexts with counters at different offsets (A at tick where `t % 4 == 0`, B at
    `t % 4 == 1`).
  - **Act:** run one loop pass.
  - **Assert:** the `EveryN(4)` gate fires the contradiction-scan job on A but **not** B on the same
    pass — counters advance independently per `PerSlugTickContext`. (Pre-crt-056 risk: a loop-global
    counter fires the heavy interval op synchronously for all slugs, a latency spike.)
- `test_each_context_counter_advances_independently`
  - **Assert:** after one loop pass, each context's `tick_counter` incremented by exactly 1, read
    from its OWN `tick_metadata` (no shared counter).

### AC-7b — no loop-global counter read (R-07, audit)
- **Source audit:** no job body reads a shared/loop-global counter; each reads only
  `ctx.tick_metadata`. A job closing over a loop-global `current_tick` breaks the
  no-cross-context-mutable contract (FR-18). (Note: the pre-crt-056 `run_single_tick` takes
  `current_tick: u32` as a param — confirm the per-slug loop sources it from `ctx.tick_metadata`,
  not a loop-global variable.)

### AC-7 / R-09 — serial loop, no fan-out
- **Source audit:** the loop is serial over resident registered slugs; no `spawn`/`join`/
  `par_iter` fan-out across slugs (NFR-1, C-1). One slug's tick completes before the next begins.

### FR-12 — registry-derived iterated set
- `test_loop_visits_each_registered_context_once`
  - **Assert:** N registered contexts ⇒ each visited exactly once per pass; adding/removing a
    registered slug changes the iterated set with no loop-body edit (behavioral in
    `multi-slug-harness.md`).

---

## Rayon (R-08, FR-17)

### Structural
- `test_rayon_not_held_across_slugs`
  - **Assert:** each slug's tick enters and exits rayon within its own tick; no two slugs' closures
    hold rayon concurrently; the loop does NOT wrap all N slugs in one rayon closure (NFR-3 forbids
    this explicitly). Under the serial loop this is automatic — the test guards against a refactor
    that hoists rayon outside the per-slug body.

### Envelope documentation (NFR-3, #2535)
- The worst-case single-slug tick duration is the MCP-latency monopolisation envelope and MUST be
  **documented** (not chosen arbitrarily). Record the measured/asserted worst-case single-slug-tick
  duration in RISK-COVERAGE-REPORT.md (R-08 row). Pattern: document the envelope, per #2535.

---

## Failure modes

### Per-slug failure isolation (mirrors `background.rs:393-395`)
- `test_failing_job_on_a_does_not_abort_b` (behavioral, `multi-slug-harness.md`)
  - **Arrange:** inject a failing job for slug A.
  - **Assert:** the error is logged, the loop continues to B, B still ticks, and A's prior state is
    intact. One slug's failure never aborts another's tick.

### Tick-closure panic
- Caught by the rayon `panic_handler` (R-10, `multi-slug-harness.md`) and the outer panic→restart
  wrapper (`background.rs:286-301`); does not SIGABRT the test or kill the daemon.

---

## Edge cases

- **N=0 registered slugs:** `test_empty_loop_is_noop_no_panic` — loop over an empty context vec is a
  no-op, does not panic.
- **Interval-gate boundary** (`tick % 4 == 0`): the per-slug counter at exactly the gate boundary
  fires for that slug only (R-07.1); contradiction-scan cadence is the existing `EveryN`.
- **Empty slug store:** ticking a slug with zero entries leaves its handles at clean defaults, no
  panic (this is AC-4's "B differently populated / empty" case).
- **NLI config-disabled:** the NLI op no-ops, no spurious rayon work.

## Coverage requirement

AC-3/AC-4 driven behaviorally here (proofs in `multi-slug-harness.md`). AC-7b = per-slug counter
independence (behavioral gate-firing) + no-loop-global-counter-read audit + serial-loop/no-fan-out
audit. R-08 = rayon-per-slug structural test + documented monopolisation envelope. Per-slug failure
isolation proven by an injected-failure test.
