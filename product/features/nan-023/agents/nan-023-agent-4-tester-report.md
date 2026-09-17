# nan-023 — Agent Report: Tester (Stage 3c Test Execution)

**Agent:** uni-tester · **Agent ID:** nan-023-agent-4-tester · **Phase:** Test Execution (Stage 3c)
**Branch:** feature/nan-023 · **Issue:** #990 · **Capability:** C14

## Outcome: GATE-READY (green)

All mandatory gates pass. The two known conditions are dispositioned exactly as mandated.

## Test Results

| Suite | Result |
|-------|--------|
| Unit — authoritative serial (`node --test --test-concurrency=1`, explicit `*.test.js`) | **1225 pass / 0 fail / 2 skip** |
| CI hook-client runner (`npm run test:hook-client`) | **838 pass / 0 fail / 1 skip** |
| C14 verifier (manifest-executing spine) | **21 pass / 0 fail** |
| Integration smoke (`pytest -m smoke`) — MANDATORY gate | **37 pass / 0 fail** |
| tools + lifecycle regression baseline (non-gating) | 0 real failures observed; zero-Rust delta → confirmatory only |
| Size gate (R-09) | PASS — stripped 105243/110000, raw 191416/200000 (not raised) |
| Zero-deps gate | PASS |
| Pre-tag real-server exercise (R-03), real binary local | claude + codex + opencode retrieval-return all PASS |

**C14 non-tautology proof:** all mutation/negative controls behave — broken entry FAILS (×3 harnesses), blanked target FAILS, broken client FAILS, tautology guard confirms presence-passes/execution-fails. The verifier executes the WireLeg manifest, it does not string-match.

**Runner artifact (not a defect):** bare `node --test` (npm test) on Node 24 reports 11 false failures + 1 cancel — a stdin-server fixture recursively discovered as a test (hangs) and parallel test files racing on the shared real `skills/` dir (ENOENT). All pass deterministically under the explicit serial invocation the project's own `run-hook-client.js` already uses. Non-blocking follow-up: guard the fixture with `require.main === module`; isolate skills-mutating tests to temp dirs. Stored as knowledge #5778.

## Gate-0 Disposition (Codex trust-feasibility)

Confirmed **NOT feasible** to seed a trusted, codex-FIRING `.codex/` in CI (codex-cli absent; trust mechanism undocumented in-repo). Consequence:
- codex LOCAL firing, command-level CLOUD firing, and BOTH retrieval-return arms (local + token-free cloud bridge) stay **HARD — all PASS**.
- codex SELF-firing the 7 events in a real trusted `.codex/` is a **DOCUMENTED CONDITIONAL**, recorded per-event as `wired-inactive (untestable-in-CI: codex not installed / trust unconfirmed)` — asserted present as a named gap, never silent-skipped.
- **C14 codex leg claim: proven(local) / partial(cloud)** (partial narrowed to the single "codex raises it" column).

## 7-Event Firing (Q4)

All 7 events (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PreCompact, SubagentStart, Stop) **fire** at command-level in both local (UDS) and cloud (HTTP) — HARD. Provider stamped `codex-cli` on the RecordEvent-family frames (no mislabel, R-05). codex-self-raises-it column = `wired-inactive (untestable-in-CI)` for all 7 (named gap). Full table in RISK-COVERAGE-REPORT.md.

## Two Known Conditions

**(A) Feature-driven test update — DONE (not xfail).** Updated the 3 retired `copySkills` tests in `test/init-integration.test.js` to ADR-004 install-if-absent + installSkills semantics (AC-01/AC-02): new action wording (`Installed skill file:` / `Kept skill file (exists):` / `[dry-run] Would install skill file:`), and rewrote the overwrite test to assert default-keeps-existing + `--force`-overwrites (both arms, coverage expanded). Not xfailed, no coverage deleted. All pass.

**(B) Pre-existing footprint gate — GH Issue + xfail (not fixed here).** `test_remote_install_under_290kb` is RED on main (312005 > 290000); nan-023 adds +47903 legitimate wiring bytes (branch 359908). Filed **GH#994** (exact numbers + precedent #775 for the 250→290KB raise), marked the test `{ skip: "Pre-existing: GH#994 ..." }`. Human gate-raise-vs-trim decision surfaced, not silently absorbed.

## N/A (stated, not skipped)

`cargo test --workspace` and the full-workspace LINK smoke (`check-workspace-link-smoke.sh`) are **N/A** — nan-023 changes zero Rust code.

## Risk Coverage Gaps

**None uncovered.** R-01..R-16 each have ≥1 executed PASS test. The sole documented conditional (codex self-firing) is a stated CI boundary per Gate-0, not a wiring coverage hole.

## Deliverables

- `product/features/nan-023/testing/RISK-COVERAGE-REPORT.md` (risk→AC map, unit+integration counts, 7-event table, feasibility matrix, Gate-0 + footprint dispositions, xfail refs, pre-tag results)
- Test edits: `test/init-integration.test.js` (Condition A), `test/remote-client.test.js` (Condition B skip)
- GH#994 filed

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5327 (infra-001 smoke not a meaningful gate for JS-only edge changes → applied: smoke run as health baseline), #4781 (pre-existing failure outside owned suites → GH Issue + xfail → applied to footprint), #4473/#4177 (warn-continue/tautology → C14 mutation controls verified), #5768 (ADR-006 C14 spine), #5777 (nan-023 C14 commandSource-by-transport-kind).
- Stored: entry #5778 "nan-023 Stage 3c testing gotcha: bare `node --test` recursive-discovery + shared-skills parallel race → false failures; use explicit serial `*.test.js` list" via context_store (pattern / testing).
