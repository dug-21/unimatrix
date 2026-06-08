# crt-052 Agent Report — C7 TranscriptRetention Gate (retention-gate)

**Agent:** crt-052-agent-3-retention-gate
**Component:** C7 — exhaustive `TranscriptRetention` match in `purge_cycle_transcripts` (`server.rs`)
**Wave:** A (ZERO reference to `transcript_hold.rs`, R-11)

## Summary

The binding contract (ADR-005, AC-10) was already structurally satisfied: vnc-025 (#670)
shipped the exhaustive match in `purge_cycle_transcripts` (server.rs:541) with explicit
`PurgeOnCycleClose` / `RetainDays(_)` arms and a doc comment forbidding a wildcard, plus
both purge-behavior tests; vnc-024 (#4721) shipped the `validate()`-rejects-`RetainDays`
config test. crt-052 C7 is therefore purely additive — I did not re-implement the match.

Additions:
1. Minimal **Wave B seam** comment inside the `PurgeOnCycleClose` arm marking where C8's
   `purge_held_for_feature(feature_cycle)` lands, with no compile-time reference to
   `transcript_hold.rs` (R-11). Reverting Wave B removes only that one line.
2. **`test_retention_match_no_wildcard`** — compile-level AC-10 exhaustiveness guard: a
   standalone `gate_decision` fn whose body mirrors the gate's variant arms in an actual
   exhaustive match (no `_`), so adding a third `TranscriptRetention` variant breaks the
   build. Asserts PurgeOnCycleClose=proceed, RetainDays(30)/RetainDays(0)=skip.

I did NOT edit `distill_handler.rs` or `tools.rs` (C6's distill gate — owned by another
agent). The purge gate (mine) and distill gate (C6) match the SAME variants for lockstep.

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/server.rs`

## Tests
- `test_retention_match_no_wildcard` (new, AC-10 compile-level) — PASS
- `test_cycle_review_purges_under_purge_on_cycle_close` (existing) — PASS
- `test_cycle_review_retain_days_arm_does_not_purge` (existing, R-18 purge-site) — PASS
- `test_cycle_review_clears_only_matching_feature_sessions` (existing) — PASS
- `test_cycle_review_zero_attributed_sessions_noop` (existing) — PASS
- `test_validate_rejects_retaindays_enterprise_only` (existing, validate-rejection) — PASS
- `test_validate_accepts_purge_on_cycle_close` (existing) — PASS

**7 passed / 0 failed.** `cargo build -p unimatrix-server` clean; clippy clean on changed
regions; `cargo fmt` applied.

Note: R-18 `distill_before_purge` returns-`None`-on-RetainDays test is owned by the C6
distill-handler component per the C7 test-plan cross-component note; this plan owns
exhaustiveness + validate-rejection + purge-site inertness, all green.

## Issues / Blockers
- Full-workspace `cargo test` compile got SIGKILL (OOM) in this environment. Worked around
  with `CARGO_BUILD_JOBS=1` + targeted `cargo test -p unimatrix-server --lib <filter>`
  (consistent with MEMORY: prefer targeted `-p` over full-workspace compile).
- Working tree contained unrelated edits from parallel C3/C6 agents
  (`unimatrix-observe/src/lib.rs`, `mcp/mod.rs`); I staged and committed ONLY `server.rs`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-005 (#4851),
  vnc-024 ADR-005 (#4721, the enterprise-seam origin), vnc-025 ADRs. Confirmed the match +
  behavior/validate tests were predecessor-shipped, so C7 is additive only.
- Stored: entry #4865 "Compile-level enum-exhaustiveness guard must be a real match (no _ arm),
  not a runtime assert" via /uni-store-pattern.
