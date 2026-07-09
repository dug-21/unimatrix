# C13 — Freeze-outcome trace (listener, NON-GATING)

> File: `crates/unimatrix-server/src/uds/listener.rs` step-5 spawn. `tracing` line distinguishing
> wrote-set vs frozen-skip; NOT returned to caller.
> Risks: R-16 (Low). ACs: AC-09 (trace leg).
> **NON-GATING. The frozen-skip outcome is operator-visible ONLY — no assembled-path proof required.**

## Test expectations (best-effort)
- `test_listener_traces_wrote_set` — a first tag-bearing Start (wrote the set) emits a `tracing` line
  identifying the wrote-set path (log-assertion via a tracing capture subscriber, or manual
  observation noted in the coverage report).
- `test_listener_traces_frozen_skip` — a later Start whose whole set was skipped (EXISTS guard hit)
  emits a `tracing` line identifying the frozen-skip path. This is the ONLY observation point for
  frozen-skip (it is fire-and-forget, not caller-returnable).

## Constraints
- The trace is `tracing`-only; no caller signal, no new interface, no read-back.
- Because frozen-skip is not caller-returnable, do NOT require an assembled MCP→review proof for it
  (RISK-TEST-STRATEGY R-16 coverage requirement) — a listener log line suffices.

## Gate posture
Explicitly NON-GATING. Verify the trace lines if implemented; a miss MUST NOT block a gate. Record
in RISK-COVERAGE-REPORT.md. If log-capture is impractical, manual observation or code-review sign-off
of the two `tracing` calls is acceptable.
