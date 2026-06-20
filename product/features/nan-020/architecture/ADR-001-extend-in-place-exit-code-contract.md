## ADR-001: Extend docker-http-posture-smoke.sh In Place; New Failures Fold Into the Existing exit-1 Contract

### Context

D-2 LOCKS the doc-test as an in-place extension of `docker-http-posture-smoke.sh`, not a
sibling script. That script carries a load-bearing, fragile exit-code contract from nan-019
(lesson #5208 / #4873): `run_smoke_gate` in `release-gate-lib.sh` discriminates `0` (pass) /
`1` (`fail()`) / `3` (Docker absent → HARD fail) / `4` (`IMAGE=` tag unpullable → HARD fail),
then asserts the anchored terminal run-marker `[783-smoke] ALL GATES PASSED`. This is the
project's primary release guard (SR-04). The bundle round-trip adds new failure modes
(emit failed, init failed, route absent, store didn't grow). SR-08 asks each to be a
"distinct hard-fail exit code." The question: do new modes get new numeric exit codes
(5/6/7), forcing a `run_smoke_gate` edit, or fold into the existing contract?

### Decision

**Extend the script append-only; new bundle-attach failures use the existing `fail()`
(exit 1) with distinct, attributable messages. `run_smoke_gate` and the exit-code numerics
are NOT modified.**

- The original Gates 1–4 (boot HTTP-on, register slug, per-slug observe 204, store-grew /
  hash-unchanged) run unchanged and FIRST. New Gates 5–7 run only after Gate 4 passes — so
  the original per-slug-observe assertion is the literal precondition of the new path and
  acts as the regression guard: if it regresses, the script fails at Gate 4 exactly as
  today, before any new code runs.
- New failure modes each call `fail()` (exit 1) with a UNIQUE message prefix naming the
  failing step, e.g. `"client-bundle emit failed (rc=N) — subcommand renamed/absent in
  shipped image?"`, `"init --bundle failed (rc=N)"`, `"documented bundle attach observe
  returned HTTP C (expected 204)"`.
- Node-absence on the host is a `fail()` (exit 1), NOT an `exit 3`: a missing `node` is a
  mis-provisioned lane (same class as Docker-absent), so it must HARD-fail, never self-skip.
- The terminal marker `[783-smoke] ALL GATES PASSED` stays the single run-marker; new gate
  log lines print between Gate 4 and that final line.

Rationale for folding rather than minting exit codes: SR-08's requirement is *never
silent-green + attributable cause*. Every conceivable new code (5/6/7) would also just
`return 1` inside `run_smoke_gate` — adding them buys zero gate-behavior change while
forcing an edit to the load-bearing wrapper (widening SR-04 blast radius). Distinctness
that matters lives in the message; the only load-bearing numeric distinction
(skip 3 / fail 1 / acquire 4) is preserved byte-for-byte. Minting codes here is the
gold-plating C-3 forbids.

### Consequences

- Easier: the load-bearing wrapper and its 0/1/3/4 contract are untouched, so nan-019's
  release gate cannot regress through wrapper changes; the extension is reviewable as a pure
  append; the regression guard is structural (Gate 4 precedes the new gates).
- Harder: failure diagnosis relies on reading the `fail()` message rather than the numeric
  code — accepted, because the messages are explicit and CI surfaces the full smoke log.
- Constraint on implementers: the extension MUST be strictly append-only after Gate 4; no
  reordering, no edits to Gates 1–4, the `IMAGE=` acquisition arm, the exit-3 preflight, or
  the terminal marker. (Cross-ref ADR-002 for what the new gates do; SR-04/SR-08.)
