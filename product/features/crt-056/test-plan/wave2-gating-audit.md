# crt-056 — Wave-2 Gating Audit (A1 per-op + verify-the-funnel)

> **This is a Wave-2-GATING precondition, executed as the FIRST ACT of Wave 2 — before any
> Wave 2 code is written.** It is NOT an end-gate check. (RISK R-01.2 / R-02.2; AC-wave2-gate.)
>
> Rationale (from the brief and #4974): if even one tick op carries a hidden global-handle write
> the probes missed, AC-4 can pass for the clean ops while the missed op corrupts B's state. The
> per-slug funnel can only be proven *sole* once every one of the 9 ops is confirmed
> store-parameterized **before** code is written. The whole "MODERATE not HARD" verdict and the
> per-slug funnel rest on this audit passing first.

Grounding evidence: Unimatrix **#4974** (verify-the-funnel 5-point checklist; vnc-034 `let _store`
ceremonial-seam precedent). This audit is the crt-056 application of that checklist.

---

## How this audit is run and recorded

- **Method:** source review + `Grep` (no behavioral test substitutes for it; it precedes the code).
- **Owner:** Wave 2 implementer, gated by the Stage-3a Gate (this file must exist and be complete)
  and re-confirmed at Gate 3c.
- **Output:** each checklist row below is filled with `PASS` + the source location/evidence, or
  `FAIL` + the offending location. Any FAIL **blocks Wave 2 code** until resolved.
- **Re-run trigger:** if any of the 9 ops or the job `run` path is edited during Wave 2, re-run the
  affected rows before the change lands.

---

## Part A — A1 per-op source audit (the 9 tick ops are store-parameterized)

For each of the 9 tick operations dispatched inside `run_single_tick`
(`crates/unimatrix-server/src/background.rs`, fn def ~`413-804`; call sites at
`463`/`552`/`566`/`629`/`706`/`794`), **source-confirm by closure-check** that the op:

1. takes `&Store` (the per-slug store) as its store input, AND
2. writes **only** the passed-in handle (the one supplied via `PerSlugTickContext`), AND
3. closes over **no** global/`static` analytics handle and **no** store singleton.

A single op reaching a global handle silently re-globalizes the funnel and would let AC-4 pass for
the clean ops while the missed op corrupts B's state (R-02).

| # | Op (call site) | Takes `&Store`? | Writes only passed-in handle? | No global/static closure? | Verdict + evidence |
|---|----------------|-----------------|-------------------------------|---------------------------|--------------------|
| 1 | `maintenance_tick` (~`463`) | | | | |
| 2 | co-access promotion (~`552`) — runs AFTER orphaned-edge compaction `~496-540`, BEFORE TypedGraph rebuild `566` | | | | |
| 3 | `TypedGraphState::rebuild` (~`566`) | | | | |
| 4 | `PhaseFreqTable::rebuild` (~`629`) | | | | |
| 5 | contradiction scan (~`706`, interval-gated `EveryN`) | | | | |
| 6 | extraction tick (~`744`) | | | | |
| 7 | NLI graph inference (~`780`) | | | | |
| 8 | graph enrichment (~`794`) | | | | |
| 9 | effectiveness maintenance (within `maintenance_tick` scope / `~441-803`) | | | | |

> The op set and exact line ranges are confirmed against live source during the audit; the table
> above lists the architecture's expected set (ARCHITECTURE.md §2, brief Function Signatures). If
> the live op count/boundaries differ, the auditor reconciles the table to source and notes the
> delta — **the audit covers every op actually dispatched, not just these rows.**

**Ordering invariant to preserve when wrapping as jobs** (`background.rs:547-550`): co-access
promotion (`552`) runs AFTER orphaned-edge compaction (`~496-540`) and BEFORE
`TypedGraphState::rebuild` (`566`). Registry order IS the ordering invariant — the audit confirms
the registered job order reproduces this.

---

## Part B — verify-the-funnel source audit (the per-slug handle set is the SOLE mutation route)

Applies #4974's 5-point checklist to the crt-056 job `run` path and the `main.rs` multi-project boot.

| # | #4974 checklist item | crt-056 application | Verdict + evidence |
|---|----------------------|---------------------|--------------------|
| 1 | **No discarded resolved handle.** Grep the job `run` path for `let _`, `let _ =`, unused binding that drops a resolved per-slug handle. | A discarded resolved handle ⇒ ceremonial funnel (the vnc-034 `let _store` trap). | |
| 2 | **No parallel/global write path beside the seam.** Grep for any global/`static`/shared analytics-handle write path beside `PerSlugTickContext`. | The five pre-crt-056 singletons (`main.rs:957-961`, threaded into `spawn_background_tick` `968-991`) MUST be removed from the multi-project path — not supplemented. | |
| 3 | **Per-slug handle set is the SOLE route.** Confirm the only mutation route for analytics handles is via `PerSlugTickContext` accessors. | Eliminate the discard AND remove the parallel path entirely (do not merely add the new path beside it). | |
| 4 | **No trait-default `{ }` bypass.** `BackgroundJob::run` MUST have no trait-default impl that could reintroduce a no-op/bypass. | Mirrors vnc-034 giving `adapter_for` no default impl so a `{ None }` bypass cannot reappear. | |
| 5 | **Resolve and dispatch tied to the same source.** The handles the tick mutates are the SAME `Arc` instances the slug's `ServiceLayer` accessors return (proved structurally by `Arc::ptr_eq`, AC-5). | Resolution (build) and mutation (tick) cannot diverge from the serving read. | |

---

## Gate outcome

- **All Part A rows PASS** AND **all Part B rows PASS** ⇒ Wave-2-gate **CLEARED**; Wave 2 code may
  begin.
- **Any FAIL** ⇒ Wave 2 code is **blocked**. Resolve the offending op/path, re-run the affected
  rows, then re-evaluate.

This file is consumed at: Wave 2 start (precondition), Gate 3a (must exist + be complete as a plan),
and Gate 3c (must be filled with PASS evidence). AC-wave2-gate in RISK-COVERAGE-REPORT.md cites this
file.
