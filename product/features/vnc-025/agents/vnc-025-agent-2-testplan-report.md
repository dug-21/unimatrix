# Agent Report: vnc-025-agent-2-testplan (Stage 3a — Test Plan Design)

## Deliverables

| File | Covers |
|------|--------|
| test-plan/OVERVIEW.md | Strategy, risk-to-plan map, cross-component dependencies, integration harness plan |
| test-plan/transcript-buffer.md | R-01 (Critical), R-02, R-03, R-15, R-05.1; AC-02, AC-07, NFR-09 fuzz |
| test-plan/transcript-block.md | R-14 (pre-move test-name inventory captured, 22 names), R-09 golden parity, R-13 |
| test-plan/registry-wiring.md | R-06 poison/concurrency, R-08 drain/sweep shapes + silently-evicted case, AC-10 |
| test-plan/dispatch-wiring.md | R-04 zero-rows hard gate (5 vnc-024 test names pinned unmodified), R-05.2 sentinel, R-09.4 empty-buffer byte-identity, R-12 #4725 |
| test-plan/purge-audit.md | R-07 (#4379 emission context), AC-08 all three triggers, zero-byte suppression, race case |
| test-plan/config-knob.md | R-11 incl. scenario-5 end-to-end cap chain |
| test-plan/cycle-review-purge.md | R-10 attribution matrix, FR-16 exhaustive match, post-clear pinning for crt-052, AC-09 snapshot |

## Risk Coverage

All 15 risks mapped (OVERVIEW.md table). Hard gates carried as named tests: vnc-024 zero-rows
suite unmodified (R-04), sentinel + static grep both (R-05), golden parity + empty-buffer
byte-identity (R-09), fuzz no-panic + poisoned-mutex (R-02/R-06/NFR-09), silently-evicted
sweep case (R-08.1), hook.rs test-name inventory + constant pins 3000/4 (R-14).

## Integration Harness Plan

Suites for Stage 3c: `smoke` (mandatory), `tools`, `protocol`, `lifecycle` (the three suites
exercising context_cycle_review — the only MCP-visible touchpoint; AC-09 requires output
unchanged). **No new infra-001 tests**: delta ingest/purge/PreCompact are UDS/HTTP surfaces
with no MCP-visible effect; existing cycle_review tests passing unmodified is the MCP-level
AC-09 evidence. Gap (by design, documented): purge audit rows and buffer state unverifiable
through the harness — coverage is Rust-test-only.

## Open Questions

1. R-09.4 / AC-09.4 baseline snapshots (empty-buffer `CompactPayload` response; cycle-review
   output) must be captured BEFORE Stage 3b edits — developer should land the snapshot
   fixtures first or Stage 3c loses the pre-change baseline.
2. FR-16 non-`PurgeOnCycleClose` arm may be untestable in OSS (validate() rejects RetainDays);
   plan falls back to the exhaustive-match compile gate — confirm at Gate 3a this is acceptable.
3. `test_signal_output_shape_unchanged` likewise needs a pre-change serialization fixture.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-001..006 (#4739-4744, already in
  brief), pattern #4725 (transport convergence — incorporated into dispatch-wiring §5), no
  new constraints.
- Stored: nothing novel to store — all techniques used (golden-output #3426, programmatic
  expectations #2984, test-name inventory #3253, transport convergence #4725, emission
  context #4379) are existing Unimatrix patterns, applied not evolved.
