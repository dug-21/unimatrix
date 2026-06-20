# Agent Report: vnc-041-agent-4-tester (Stage 3c — Test Execution)

## Outcome: PASS

All 14 risks (R-01..R-14) and all 6 acceptance criteria (AC-01..AC-06) covered and PASS.
Mandatory infra-001 smoke gate PASS. No vnc-041-caused failures. No GH Issues filed.

## Results

| Layer | Result |
|-------|--------|
| `cargo test -p unimatrix-server --lib` | 4280 passed, 0 failed, 1 ignored |
| `cargo test -p unimatrix-server --bins` | 123 passed, 0 failed |
| infra-001 smoke (`-m smoke`) | 24 passed, 0 failed |
| infra-001 protocol | all passed |
| infra-001 lifecycle | passed; 5 xfailed, 2 xpassed (all PRE-EXISTING markers, unrelated) |

vnc-041 new tests (all PASS): C1=6, C2=10, C3=13, C4=11, C5=14 → 54.

## Deferred gap (C3 / AC-02 empirical round-trip): NOT closed by a new test — documented infeasible

I did NOT add the empirical bin-target `register→resolve_slug_config` round-trip and did NOT
edit any source file. The recommended bin-target landing (`per_slug_loop_tests.rs`) cannot
reach `register`:
- `resolve_slug_config` is binary-target only (`mod http_provision` in `main.rs`, absent from
  `lib.rs`) — unreachable from lib-crate `projects/tests.rs`.
- `register` is private and `with_dirs` is `#[cfg(test)]`+private in lib `projects` — invisible
  to the bin-target build (cfg(test) off for dependency compilation). The only `pub` register
  path needs HOME isolation (forbidden under Rust 2024).
- Closing it requires a NON-TEST production-visibility edit to `projects.rs` (widen `with_dirs`
  + expose `register` under `#[cfg(any(test, feature = "test-support"))]`), which exceeds the
  3c test-execution mandate ("edit ONLY the test file").

The in-scope seam proof is behaviorally equivalent and sufficient: `test_register_seeded_b_path_is_the_resolver_formula`
(b at the resolver's literal probe path) + `test_register_seeded_b_is_resolver_loadable`
(resolver's exact file-present arm reproduced on the register-written file). `resolve_slug_config`
itself is empirically covered against real on-disk (b) by `slug_config_tests.rs` (27) and
`per_slug_loop_tests.rs`. R-05/AC-02 = Full. Full reasoning in RISK-COVERAGE-REPORT.md §Gaps.
**Recommendation for the leader:** if an end-to-end empirical round-trip is required, file a
small follow-up to widen the `projects.rs` test seam — not a 3c change.

## Pre-existing flaky confirmed (not a regression)

`eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` passes in isolation on HEAD
(1 passed) and also passed in the full `--lib` run this session. Pre-existing parallelism flake;
not attributed to vnc-041.

## infra-001 xfail/xpass

All in `test_lifecycle.py`, pre-existing markers (CI tick-interval config / bugfix-491). Not
vnc-041-caused, not authored here; per USAGE-PROTOCOL left as-is, no new GH Issue.

## Files

- Report: `product/features/vnc-041/testing/RISK-COVERAGE-REPORT.md`
- No source files edited. No git commands run.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get(5241)` -- surfaced ADR-005 (#5239),
  resolve_slug_config gotchas (#5212), and #5241 (bin-vs-lib test target trap: `http_provision`
  tests run under `--bins`, not `--lib`) which drove the dual `--lib`/`--bins` run and the
  deferred-round-trip infeasibility analysis.
- Stored: nothing novel to store -- the bin-vs-lib target split and the resolve_slug_config
  test-reach constraint are already captured in #5241; this feature's deferred-round-trip
  infeasibility is a feature-specific instance of that stored pattern, not a new cross-feature lesson.
