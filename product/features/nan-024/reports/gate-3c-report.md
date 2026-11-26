# Gate 3c Report: nan-024

> Gate: 3c (Risk Validation)
> Date: 2026-11-26
> Result: PASS (with warnings)
> HEAD: b3fa2c08

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Behavioral-outcome proof (scope lens) | PASS | All 11 SCOPE-RISK-ASSESSMENT outcomes map to passing tests driven from the entry point (wiring-e2e literal command via harness shell, init/wire, launcher, headless smoke). |
| Risk mitigation proof R-01..R-40 | PASS (WARN) | All 40 risks have passing tests; R-11/R-21/R-22 evidenced-not-enforced per D-096. Four plan-named files now exist (7628b623) and pass. |
| Coverage completeness | PASS (WARN) | Partial items are stated residuals. D-078 assembled "wipe npx cache then fire" test not located. |
| Specification compliance | PASS | AC-01..AC-16 PASS; AC-06 evidenced (D-096); AC-11 release-notes OpenCode statement manual. |
| Architecture / ADR / DECISIONS compliance | PASS | D-092/093/094/096 applied; no drift found. |
| Integration tests (infra-001) | PASS (WARN) | Smoke 37/37; relevant suites pass; no infra-001 files changed vs main. |
| CI evidence | PASS | See below. |
| Knowledge stewardship | PASS | Tester reports in agents/ (agent-11, agent-12) carry stewardship; see note. |

## Detailed Findings

### Behavioral-outcome proof
**PASS.** Every row in SCOPE-RISK-ASSESSMENT "User-Facing Entry Points" has a passing test from the real entry point: init/wire (wire, init-integration, codex-install, opencode-install), literal hook command through harness shell (wiring-e2e, 6/6 CI cells + linux-local against built server), legacy `unimatrix hook` (launcher-hook), cache wipe/upgrade (install-store, resolve-runtime), `UNIMATRIX_BINARY`/redirect (resolve-runtime, launcher-hook, AC-10/14), OpenCode (opencode-install, c14-opencode), headless mcp-bridge (headless-smoke + infra-001 test_headless_access 9/9). None rely solely on seam tests or no-op confirmation.

### Risk mitigation
**PASS/WARN.** RISK-COVERAGE-REPORT maps R-01..R-40 to tests, all PASS. Independently re-ran `npm test` in packages/unimatrix: rc=0 (hook-client 976/975 pass/1 skip, parity-layer2 24/24, package 728/727, wiring-e2e 45/45, opencode-plugin 37/37, audit ok). The four rework-set files named in the plan (`mcp-stdio-spawn`, `init-preflight`, `codex-detect`, `pkg-binary-copy`) now exist and pass (4, 3, 7, 2 tests). Therefore R-09, R-20, R-24 "Partial" notes in the report are stale (the report was written at 0139b312, before 7628b623). WARN: RISK-COVERAGE-REPORT not refreshed (Gaps item 2 outdated).
Residual Partials remain stated: R-02 (fish/nu not in CI), R-06/R-24 Windows ACL/rename CI-only, R-38 captured fixture not live Claude, R-39 per-harness display not observable (note present in report).

### D-096: latency (AC-06, R-11, R-21, R-22)
**PASS (evidenced, not enforced).** Per-cell tables for runs A and B (n=50, 5 warm-ups) against D-094 limits are recorded. Medians are within budget in every gated cell; p95 breaches (5 of 12 cell-runs per OS-case sets, differing per run) coincide with runner tail noise (macOS `node -e 0` baseline p95 58.7 vs 208.9). Only macOS mcp-stdio not-a-project breached p95 in both runs (100.7, 104.5 vs 85; median 66.4) -- a budget-tightness observation, not a regression. Latest e2e run 45809180517 (HEAD e37d94b4) has all three latency-gate cells green, with `continue-on-error: true` on the job. Judged per D-096; not a gate failure.

### Specification / ACs
**PASS.** AC-01..AC-05, AC-07..AC-10, AC-12..AC-16 PASS with test evidence; AC-08 parity-layer2 is gating (not continue-on-error, workflow header). AC-11: grep tests pass; release-notes OpenCode statement is manual review (WARN, unchanged from report).

### Architecture / ADR / DECISIONS
**PASS.** Gate 3b already PASS. D-092 (hook-client size caps at measured 115012/115012 stripped, 203058/203058 raw; WARN zero headroom), D-093 (440 KB), D-094 (budgets) applied. No contrary evidence in tests or CI. ADRs (#5896, #5900, #5897, #5901, #5898, #5894) not re-fetched individually; relied on gate-3b check plus test evidence.

### Integration test validation
**PASS (WARN).**
- infra-001 smoke 37/37 pass. Suites run: test_headless_access 9, https_uds_parity 13 (11 pass, 2 skip), parity_matrix 29, project_isolation 4, protocol 14, security 29, lifecycle 115 (108 pass, 6 xfail, 1 XPASS non-strict), tools 231 (230 pass, 1 xfail), transport_health 19. 0 failures.
- `git diff main...HEAD` touches no files under product/test/infra-001; no xfail added/removed, no tests deleted or commented out (no deleted files in packages/unimatrix/test per `--diff-filter=D`).
- xfail markers verified pre-existing on main. References: GH#405 (test_tools), GH#291 and GH#406 (lifecycle). WARN: two pre-existing lifecycle xfails (`test_context_status_supports_edge_count_increases_after_tick`, `test_inferred_edge_count_unchanged_by_cosine_supports`) cite "No embedding model in CI" with no GH issue number; not introduced by nan-024, outside this feature's scope.
- RISK-COVERAGE-REPORT includes integration counts.

### CI evidence
**PASS.**
- nan-024-e2e.yml run 45809180517 (success, HEAD e37d94b4; later commit b3fa2c08 is docs only): wiring-e2e 6/6 cells (ubuntu, macOS, Windows x Node 20/24), wiring-e2e linux local, parity-layer2, test-count guard, latency-gate x3, server build -- all success. Flaky parity-layer2 rerun and follow-up issue per spawn prompt.
- Q9 run 45754820291 (success, HEAD 7628b623) after expected-negative handling of the primary-form step (D-034).
- `ci.yml` is PR-triggered and has not run for this branch (will run on PR).

### D-078 package size, R-39, NFR-04, NFR-05
- D-078 size reported, no cap (D-087): pkg/0.11.3 76,706,206 B (73.2 MiB); platform subtree 72.8 MiB; JS+metadata 376,031 B. PASS.
- R-39 harness-display note present. PASS.
- NFR-04: check-test-count PASS (64 suites, 1330 tests, no decrease), CI guard green; counts since increased with the four new files. PASS.
- NFR-05: before/after on real-home files unchanged; `~/.claude.json` change attributed to live session. PASS.

### Knowledge stewardship
**PASS.** Tester/gap agent reports (nan-024-agent-11-tester-report.md, nan-024-agent-12-3c-gaps-report.md) present in agents/. Not exhaustively re-audited for stewardship blocks at this gate beyond presence; no missing-block finding raised.

## Warnings (non-blocking)
1. RISK-COVERAGE-REPORT stale (HEAD 0139b312): Gaps item 2 and the Partial notes for R-09/R-20/R-24 should be refreshed to reflect 7628b623.
2. D-078 assembled test (init, wipe npx cache, literal hook command stores an event) not located; only resolver half (resolve-runtime.test.js:164).
3. check-hook-client-size has zero headroom (D-092).
4. Latency p95 breaches in CI runs A/B (report-only, D-096); macOS mcp-stdio not-a-project consistently near/over 85 ms.
5. Two pre-existing infra-001 xfails lack GH issue references.
6. R-39 harness display, R-38 live-Claude, Windows ACL/rename, AC-11 release notes remain manual/CI-only residuals.

## Rework Required
None.

Unimatrix entries consulted: #5768, #5313, #4311, #5648, #1006, #4515, #5305, #3612, #2758, #5321 (briefing index; #2758 -- grep named test functions at 3c -- applied: the four named test files were verified present and run). ADR entries #5896, #5900, #5897, #5901, #5898, #5894 not fetched individually.
