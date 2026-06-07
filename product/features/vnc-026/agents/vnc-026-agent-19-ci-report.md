# Agent Report — vnc-026-agent-19-ci (AC-12 CI workflow)

## Scope
AC-12: CI for the TS HTTP hook client — Node 18/20/22/24 x Linux/macOS/Windows
matrix, zero-runtime-dep audit, lib/hook-client/ <100 KB payload check, parity
corpus drift check (R-20 non-vacuity). Commit `b0219522` on `feature/vnc-026`.

## Files modified / created
- `.github/workflows/ci.yml` (modified) — four new jobs appended:
  - `hook-client-matrix` — Node 18/20/22/24 x {ubuntu, macos, windows}, runs
    `npm run test:hook-client`. fd-0 stdin runs honestly on Windows via
    index.test.js (R-14). `fail-fast: false` so all cells report.
  - `hook-client-audit` — zero-dep audit + <100 KB size check (ubuntu).
  - `parity-drift` — Rust generator regen + zero-diff gate with non-vacuity
    guards (ubuntu; goldens are platform-independent, .gitattributes `-text`).
  - `hook-client-layer2` — single Linux cell: `cargo build --release` then the
    3 Layer 2 suites (scoped per test-plan OVERVIEW; real-server.js hard-fails
    when the binary is absent — no skip).
- `packages/unimatrix/package.json` (modified) — added `test`,
  `test:hook-client`, `test:hook-client:layer2` scripts.
- `packages/unimatrix/test/run-hook-client.js` (new) — portable suite runner
  (explicit file list via spawnSync; excludes layer2 + benchmark by default).
- `packages/unimatrix/test/check-zero-deps.js` (new) — no package.json
  `dependencies` + require-graph resolves only Node built-ins.
- `packages/unimatrix/test/check-hook-client-size.js` (new) — sums .js bytes,
  fails at >= 100 KB (decimal).
- `scripts/check-parity-drift.sh` (new) — regen + zero-diff with three
  non-vacuity guards (R-20): generator reported "1 passed"; MANIFEST
  case_count>0 AND mtime advanced; `git diff --exit-code` clean.

## Validation performed
- `actionlint` v1.7.12: PASS (zero issues). YAML parses (pyyaml): OK.
  Matrix = ["18","20","22","24"] x [ubuntu-latest, macos-latest,
  windows-latest] = 12 cells.
- `npm run test:hook-client` (Node 24): 12 suites, 419 pass / 0 fail / 1 skip,
  **exit 0**. (parity-layer1 carries one expected-failure subtest shown with a
  warning marker; the suite handles it and exits 0.)
- `node test/check-zero-deps.js`: PASS — no dependencies; 13 modules require
  only built-ins/relative.
- `bash scripts/check-parity-drift.sh`: PASS — generator ran (`1 passed`,
  83 cases), MANIFEST fresh, zero drift. Regen produced byte-identical goldens
  (no corpus change committed).
- Layer 2 file selection verified (3 suites). Did NOT run integration tests
  (infra-001) or modify them.

## Issues / blockers
- **BLOCKER — payload size exceeds AC-12 budget.** `lib/hook-client/` totals
  **104,240 bytes (101.8 KiB / 104.2 KB)**, over the AC-12 / C-04 "< 100 KB"
  limit on either reading (100,000 B decimal or 102,400 B binary). The
  `hook-client-audit` job will FAIL honestly as wired. I did not inflate the
  limit (vacuous pass) or edit lib/ files (owned by other agents). The lib must
  be trimmed ~4.3 KB (largest: index.js 14.6 KB, build-request-tools.js 13.0 KB,
  transcript.js 11.5 KB) to clear the gate. Escalating to the delivery leader.
- One pre-existing expected-failure parity subtest
  (`stdout-subagent-non-entries-fallback`) is a documented wire-contract
  limitation (commit 537dc261, server-side C-07 out of scope). It does not
  fail the suite/exit code — not a CI blocker.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern/decision)
  -- surfaced ADR-001 (#4751, parity corpus oracle + CI drift, mirrors vnc-024
  ts-rs gate #4726). No prior Node-CI-matrix pattern existed.
- Stored: entry #4779 "node --test directory discovery needs Node >=21 — use an
  explicit file-list runner for an 18/20/22/24 x OS matrix" via
  /uni-store-pattern.
