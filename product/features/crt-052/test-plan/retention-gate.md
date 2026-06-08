# Test Plan — C7 TranscriptRetention Gate

**Component**: exhaustive `TranscriptRetention` match in `server.rs` `purge_cycle_transcripts`
(`:543` `PurgeOnCycleClose`, `:551` `RetainDays(_)`). **ADRs**: ADR-005 §1 (the enterprise seam).
**Wave**: A. **Tests live in**: `server.rs` `#[cfg(test)]` + the helper's gate step (distill-handler).
**Merge gate context**: AC-10 (not itself a listed merge gate, but the four-return helper depends on
this gate being exhaustive).

## Unit Test Expectations (AC-10, R-18)
- `test_retention_match_no_wildcard` — compile-level: the `TranscriptRetention` match is EXHAUSTIVE,
  with NO wildcard `_ =>` arm. `PurgeOnCycleClose` and `RetainDays(_)` are both explicit arms. The
  match IS the enterprise seam — adding a new variant must force a compile error here (not silently
  fall through a wildcard).
- `test_retaindays_rejected_at_validate` — `RetainDays(_)` config is rejected at `validate()` in OSS
  (unreachable in normal flow); assert `validate()` returns an error for a `RetainDays` config.
- `test_retaindays_helper_returns_none` (R-18) — construct a `RetainDays` config in a test (bypassing
  `validate`), call `distill_before_purge` → returns `None`: NEITHER distills NOR purges. The arm is
  structurally dead in OSS but explicit and proven inert.
- `test_purgeoncycleclose_proceeds` — `PurgeOnCycleClose` → the gate proceeds (distill + purge run).
  The only OSS-honored arm.

## Cross-component
- The helper's consumption of this gate (`match cfg.transcript_retention { ... }` as step 1 of
  `distill_before_purge`) is asserted in distill-handler.md (`test_helper_returns_none_on_retaindays`).
  This plan owns the exhaustiveness + validate-rejection; the handler owns the wiring.

## Assertions Summary (concrete)
- No wildcard arm — a future retention variant breaks the build until explicitly handled.
- `RetainDays` rejected at `validate()`; if constructed, the helper neither distills nor purges.
- `PurgeOnCycleClose` is the sole arm that proceeds to distill+purge.
