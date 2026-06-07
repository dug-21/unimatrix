# Agent Report: vnc-026-agent-21-tester (Stage 3c Test Execution)

## Outcome: PASS

All unit + Layer 1 + Layer 2 hook-client suites green; integration smoke gate green;
cargo parity generator guards green. Every risk R-01..R-20 and AC-01..AC-16 mapped to executed
tests. One pre-existing out-of-scope failure triaged and filed (GH#695). No vnc-026-owned test
fails.

## Executed

| Step | Command | Result |
|------|---------|--------|
| Unit + Layer 1 parity | `npm run test:hook-client` | 421 tests: 419 pass / 0 fail / 1 skip / 1 todo |
| Layer 2 vs merged F2 server | `npm run test:hook-client:layer2` | 8 tests: 8 pass / 0 fail |
| Parity generator guards | `cargo test -p unimatrix-server --lib parity` | 9 pass / 0 fail / 1 ignored (generator, by design) |
| Integration smoke gate | `pytest -m smoke --timeout=60` (infra-001) | 23 pass / 0 fail |

- Server binary present at `target/release/unimatrix` (C-07: no server production changes, so the
  existing build is valid for Layer 2). Layer 2 ran against it via `test/helpers/real-server.js`.
- Parity corpus: 83 cases; `MANIFEST.json` maps 104 `build_request` arms (R-02); branch-coverage
  guard `test_generator_branch_coverage` passes.
- Zero runtime deps confirmed (`dependencies: {}`) for AC-12.

## Accepted Non-Pass Markers (pre-adjudicated — not reopened)

- **todo** `stdout-subagent-non-entries-fallback` — wire-contract limitation (text/plain erases
  Entries-vs-BriefingContent, ADR-003); server-side fix, C-07 out of scope. Adjudicated Gate 3b.
- **skip** Windows-only root-walk (backslash) — runs on CI Windows OS matrix (AC-12 / R-14).

## Pre-existing Failure Triaged → GH#695

`packages/unimatrix/test/init.test.js::test_creates_mcp_json_on_clean_project` fails in the full
package suite (NOT in hook-client suites; NOT smoke). Verified: failure reproduces; production
`init.js` sets `LD_LIBRARY_PATH` in .mcp.json env since commit `07062006` (2026-03) while the test
still asserts `env: {}`; `git diff main` empty for both files. nan-004 lineage (#221). Per
USAGE-PROTOCOL.md → filed **GH#695**, no fix in this PR. No xfail marker added: the test is outside
vnc-026's gate command, so an xfail edits an unrelated file for no gate benefit (see stored
procedure #4781).

## Risk / AC Coverage

- All 20 risks: Full coverage, PASS. Critical R-01 (83-case corpus + 104-arm manifest + drift
  guard) and R-14 (fd-0 stdin variants + CI OS matrix) covered. R-06 four pinned ADR-008 elision
  items asserted via observable PreCompact body in Layer 2 (`test_l2_elision_mid_session`), pass.
- All 16 ACs: PASS. AC-13 is PASS per the binding Gate-3b adjudication (client-work p50 0.07 ms /
  p95 0.11 ms ≤ 12/20 ms targets, ~100× margin; full-spawn ~25 ms overage = Node interpreter
  cold-start ≈ C-03's "~12 ms spawn floor"). AC-15 evaluated against the amended letter
  (delta-failure → offset-non-advance + NO queue file).
- Full mapping in `testing/RISK-COVERAGE-REPORT.md`.

## ass-071 Freebie — Declined

The Layer 2 harness drives SubagentStart / PreCompact / delta flows; it never produces a real
SubagentStop stdin payload. No authentic payload exists to capture (synthesizing one answers
nothing). Declined for the same reason agent-14 did. One-line rationale, advisory only.

## Gaps

None. Windows-specific R-14 arms run only on the CI Windows runner (by design), not a gap.

## Deliverables

- `product/features/vnc-026/testing/RISK-COVERAGE-REPORT.md`
- GH#695 (pre-existing stale init test)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #4780 (hook-client size lesson), #4775
  (Layer 1 wire-body recovery from expected-stdout.bin), #4515 (gate-3b zero-test failure mode),
  #4725 (dual-transport drop-guard); all consistent with the executed plan.
- Queried: context_search "pre-existing failing test outside owned suites / xfail" -- existing
  entries (#4169, #3918) cover xfail/xpass nuance but not the outside-the-gate decision rule.
- Stored: entry #4781 "Pre-existing failure outside the feature's owned suites: GH Issue, no xfail
  marker needed" via context_store (testing/procedure).
