# Risk Coverage Report: nan-024

Stage 3c, HEAD 0139b312 (+ `packages/unimatrix/test/measure-pkg-size.js`). Linux dev container, Node 24. Windows/macOS evidence is CI only.

## Headline
- Local: all green (unit, package, wiring-e2e, opencode-plugin, audit, size/zero-dep/test-count, cargo workspace, link smoke, infra-001 smoke + planned suites). 0 failures, 0 new xfails.
- CI: `wiring-e2e` 6/6 cells green x2 runs, `wiring-e2e` linux-local green, `parity-layer2` green, test-count guard green. **`latency-gate` breached the D-094 p95 figures in 5 of 6 cell-runs (different cells each run); per D-096 it is now REPORT-ONLY (AC-06 evidenced, not enforced); Q9 job is red by construction (primary-form step is a designed FAIL).** See CI table.
- Gaps: the 4 plan-named test files now exist and pass (commit 7628b623: mcp-stdio-spawn 4, init-preflight 3, codex-detect 7, pkg-binary-copy 2); the earlier figures below predate them (measured at 0139b312).

## Test Results

### Unit / package (packages/unimatrix, `npm test`, rc=0)
| Set | Tests | Pass | Fail | Skip |
|---|---|---|---|---|
| default hook-client (32 suites) | 976 | 975 | 0 | 1 |
| parity-layer2 (4 suites) | 24 | 24 | 0 | 0 |
| package (38 suites) | 728 | 727 | 0 | 1 |
| wiring-e2e (4 suites) | 29 | 29 | 0 | 0 |
| opencode-plugin (7 suites) | 37 | 37 | 0 | 0 |
| audit | orphan audit PASS, 86/86 suites claimed | | | |
| **Total** | **1794** | **1792** | **0** | **2** |

Gates: `check-hook-client-size` OK (115012/115012 stripped, 203058/203058 raw; at budget, no headroom), `check-zero-deps` OK (30 modules, built-ins only), `check-test-count` PASS (64 suites, 1330 tests, no decrease).
Superset test (D-079, `shim-superset.test.js`): fixed-table positives/negatives/strict-superset/clause-(d) cases + property test of 200 seeded random fixtures; seed `SHIM_SEED` default 20260101; passes.

### Rust
- `cargo test --workspace` (hardened convention): rc=0, 7240 passed, 0 failed, 31 ignored.
- `check-workspace-link-smoke.sh` (#878): PASS.

### Integration (infra-001, release binary built from HEAD)
| Run | Tests | Pass | xfail (pre-existing) | skip | Fail |
|---|---|---|---|---|---|
| smoke gate (`-m smoke`) | 37 | 37 | 0 | 0 | 0 |
| test_headless_access | 9 | 9 | 0 | 0 | 0 |
| test_https_uds_parity | 13 | 11 | 0 | 2 | 0 |
| test_https_uds_parity_matrix | 29 | 29 | 0 | 0 | 0 |
| test_project_isolation | 4 | 4 | 0 | 0 | 0 |
| test_protocol | 14 | 14 | 0 | 0 | 0 |
| test_security | 29 | 29 | 0 | 0 | 0 |
| test_lifecycle | 115 | 108 | 6 (+1 XPASS non-strict) | 0 | 0 |
| test_tools | 231 | 230 | 1 (GH#405) | 0 | 0 |
| test_transport_health | 19 | 19 | 0 | 0 | 0 |

Note: the first combined run was killed by my own `timeout 3000` ceiling mid-`test_tools` (~tool test 185); the remainder (tools 175-231 + transport_health) was re-run separately (76/76 pass). All lifecycle/tools xfails are pre-existing markers (GH#405/#406, ONNX/tick environment); none added or removed. No infra-001 tests added: plan states none required (no Rust/MCP-visible change; Node bridge is not driven by the Python harness; headless negative case is in `headless-smoke.test.js`).
No GH issues filed (no pre-existing failures surfaced; CI latency/Q9 findings belong to this feature, reported below for the primary).

## CI evidence (branch feature/nan-024; no PR exists, so `ci.yml` (pull_request only, no workflow_dispatch) has not run)
Runs: A = 45745887970 (push, 16c73608), B = 45746445823 (workflow_dispatch triggered by me, HEAD 0139b312). Q9 = 45745887909.

| Job / cell | Run A | Run B |
|---|---|---|
| wiring-e2e ubuntu Node 20 / 24 | pass / pass | pass / pass |
| wiring-e2e macOS Node 20 / 24 | pass / pass | pass / pass |
| wiring-e2e windows Node 20 / 24 | pass / pass | pass / pass |
| wiring-e2e linux local (built server, UDS) | pass | pass |
| parity-layer2 gating (AC-08) | pass | pass |
| test-count non-decrease (NFR-04) | pass | pass |
| latency-gate ubuntu | FAIL | pass |
| latency-gate macOS | FAIL | FAIL |
| latency-gate windows | FAIL | FAIL |
| Q9 windows MCP proof | FAIL (step 1 of 3) | n/a |

### latency-gate detail (D-094 absolute p95 budgets, ms; 50 iters, 5 warm-ups)
| Cell | Case | Run A p95 / limit | Run B p95 / limit |
|---|---|---|---|
| ubuntu (limits 74 / 50) | cloud FNF | 71.2 / 74 ok | 61.5 / 74 ok |
| | cloud sync | **104.3** / 74 BREACH | 70.5 / 74 ok |
| | not-a-project hook | 32.5 / 50 ok | 28.9 / 50 ok |
| | mcp-stdio not-a-project | 31.1 / 50 ok | 28.5 / 50 ok |
| macOS (126 / 85) | cloud FNF | 72.0 ok | **132.2** / 126 BREACH |
| | cloud sync | 109.5 ok | 115.9 ok |
| | not-a-project hook | **96.6** / 85 BREACH | 75.2 ok |
| | mcp-stdio not-a-project | **100.7** / 85 BREACH | **104.5** / 85 BREACH |
| windows (164 / 81) | cloud FNF | 160.4 ok | **240.5** / 164 BREACH |
| | cloud sync | **167** / 164 BREACH | 127.8 ok |
| | not-a-project hook | **96.1** / 81 BREACH | 75.5 ok |
| | mcp-stdio not-a-project | 78.7 / 81 ok | 61.0 ok |

Why failing: the budgets are p95 over 50 samples on shared runners; the breach set changes run to run (ubuntu breaches in A, passes in B; macOS `node -e 0` baseline itself had p95 208.9 ms in B, 58.7 in A). Medians are within budget in every cell (e.g. win cloud FNF median 128-152, macOS NAP median 53-55). Only macOS `mcp-stdio` not-a-project breached in both runs (100.7, 104.5 vs 85; median 66.4). This is a tail-noise / budget-tightness question for the primary (D-094), not a deterministic regression. Latency-gate cannot be called green; the AC-06 CI proof is therefore not satisfied on any OS by a stable run (ubuntu run B only).

### Full per-cell latency tables (D-096 evidence; ms, n=50, 5 warm-ups; limit = D-094 figure)
Breach = p95 over limit (informational only). Source: CI job logs of runs A and B.

**Run A** (45745887970)

ubuntu:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 29.6 | 32.5 | 50 | PASS |
| cloud project, FNF event (PostToolUse) | 50 | 64.6 | 71.2 | 74 | PASS |
| cloud project, sync event (UserPromptSubmit) | 50 | 67.3 | 104.3 | 74 | BREACH |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 29.2 | 31.1 | 50 | PASS |
| mcp-stdio in-project start-to-initialize (info) | 50 | 62.7 | 66.5 | - | info |
| node -e 0 (baseline) | 50 | 23.7 | 27.7 | - | baseline |
| node -e 0 via sh -c (info) | 50 | 24.2 | 27.2 | - | info |

macos:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 55.2 | 96.6 | 85 | BREACH |
| cloud project, FNF event (PostToolUse) | 50 | 61.8 | 72 | 126 | PASS |
| cloud project, sync event (UserPromptSubmit) | 50 | 66.2 | 109.5 | 126 | PASS |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 66.2 | 100.7 | 85 | BREACH |
| mcp-stdio in-project start-to-initialize (info) | 50 | 117.4 | 154.1 | - | info |
| node -e 0 (baseline) | 50 | 43.8 | 58.7 | - | baseline |
| node -e 0 via sh -c (info) | 50 | 54.8 | 66.3 | - | info |

windows:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 87.6 | 96.1 | 81 | BREACH |
| cloud project, FNF event (PostToolUse) | 50 | 152.1 | 160.4 | 164 | PASS |
| cloud project, sync event (UserPromptSubmit) | 50 | 145.8 | 167 | 164 | BREACH |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 71.8 | 78.7 | 81 | PASS |
| mcp-stdio in-project start-to-initialize (info) | 50 | 121.8 | 135.8 | - | info |
| node -e 0 (baseline) | 50 | 55.5 | 63.1 | - | baseline |
| node -e 0 via cmd.exe /C <line> (info) | 50 | 64.8 | 68.4 | - | info |

**Run B** (45746445823)

ubuntu:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 25.9 | 28.9 | 50 | PASS |
| cloud project, FNF event (PostToolUse) | 50 | 55.9 | 61.5 | 74 | PASS |
| cloud project, sync event (UserPromptSubmit) | 50 | 57.1 | 70.5 | 74 | PASS |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 25.5 | 28.5 | 50 | PASS |
| mcp-stdio in-project start-to-initialize (info) | 50 | 54.9 | 66.4 | - | info |
| node -e 0 (baseline) | 50 | 21 | 22.9 | - | baseline |
| node -e 0 via sh -c (info) | 50 | 21.7 | 23.5 | - | info |

macos:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 52.8 | 75.2 | 85 | PASS |
| cloud project, FNF event (PostToolUse) | 50 | 91.9 | 132.2 | 126 | BREACH |
| cloud project, sync event (UserPromptSubmit) | 50 | 78 | 115.9 | 126 | PASS |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 66.4 | 104.5 | 85 | BREACH |
| mcp-stdio in-project start-to-initialize (info) | 50 | 117.3 | 191.2 | - | info |
| node -e 0 (baseline) | 50 | 75.3 | 208.9 | - | baseline |
| node -e 0 via sh -c (info) | 50 | 52 | 79.2 | - | info |

windows:

| case | n | median | p95 | limit | result |
|---|---|---|---|---|---|
| NOT-A-PROJECT hook (PostToolUse) | 50 | 71.6 | 75.5 | 81 | PASS |
| cloud project, FNF event (PostToolUse) | 50 | 128.2 | 240.5 | 164 | BREACH |
| cloud project, sync event (UserPromptSubmit) | 50 | 120.8 | 127.8 | 164 | PASS |
| mcp-stdio NOT-A-PROJECT start-to-exit | 50 | 56.3 | 61 | 81 | PASS |
| mcp-stdio in-project start-to-initialize (info) | 50 | 93.5 | 108.7 | - | info |
| node -e 0 (baseline) | 50 | 52.5 | 64.3 | - | baseline |
| node -e 0 via cmd.exe /C <line> (info) | 50 | 60.3 | 87.1 | - | info |

Medians are within the D-094 limit in every gated cell of both runs; p95 breaches vary run to run (runner tail noise; macOS `node -e 0` baseline p95 58.7 in A vs 208.9 in B).

### Q9 job (45745887909)
Red because the step "MCP primary form (cmd /d /s /c)" is expected to FAIL (D-034 proof that the primary form breaks: `The network path was not found.`, both Node spawn and Rust `Command.args`). The fallback (D-047), hook `.cmd` (cmd /C + Git Bash), TLS cert, ACL diagnostic and REAL-init steps pass. The workflow has no expected-failure handling for the primary step, so the job conclusion is `failure` on every run. Recommend `continue-on-error` or inverted assertion on that step so the job is a usable gate.

## D-078 package size (report only, D-087: no cap, no pass/fail)
`node packages/unimatrix/test/measure-pkg-size.js` (new; fixture HOME; platform package fixture = real `unimatrix` release binary 43,210,904 B + two `libonnxruntime` copies as in release.yml):
- `pkg/0.11.3` total: **76,706,206 B (73.2 MiB)**
- platform-binary subtree (`node_modules/@dug-21/unimatrix-linux-x64`): 76,330,175 B (72.8 MiB)
- JS + metadata: 376,031 B
Real published ORT is a single ~16.5 MB file duplicated (`.so.1.20.1` + `.so.1`); figure here is representative of the shipped layout.

## R-39 evidence note (MCP server exiting 0 outside a project)
- Observed (automated): `mcp-stdio` outside a project exits 0, empty stdout, empty stderr, no network, before `initialize` (wiring-e2e "non-Unimatrix cwd" on ubuntu/macOS/windows CI x Node 20/24; launcher-hook/launcher-status unit tests; latency gate times it: 28-31 ms ubuntu, 61-105 ms macOS/Windows).
- What each harness DISPLAYS (Claude Code, Codex, OpenCode, Gemini): **not observable** in this environment or CI (no harness UI/clients available headless; the only real-harness spawners exercised are the Q9 Node-spawn and Rust `Command` semantics). No claim made; needs a manual check per harness.

## NFR-05 (no real-home pollution)
Before/after on `~/.unimatrix/install.json` (absent, still absent), `~/.codex` (mtime unchanged), `~/.codex/config.toml` (sha256 unchanged), `~/.codex/hooks.json` (absent, still absent), `~/.claude/settings.json` (unchanged). `~/.claude.json` mtime/hash changed during the window; it is the live file of the running Claude Code session (no `u-size-`/tmp-fixture paths in it), not attributable to the tests. PASS.

## Risk coverage (R-01..R-40)
Result = suite-level result for the named files (all PASS locally); Coverage per test-plan file mapping, E = also exercised by CI wiring-e2e on 3 OS.

| Risk | Description (short) | Tests (packages/unimatrix/test unless noted) | Result | Coverage |
|---|---|---|---|---|
| R-01 | exit outcomes never 2, stdout empty | launcher-hook, launcher-status, wiring-e2e (E) | PASS | Full |
| R-02 | Codex non-POSIX shell | codex-install, wiring-e2e shell matrix; PowerShell documented (docs-grep) | PASS | Partial (fish/nu not run in CI) |
| R-03 | ownership predicate | hook-ownership, merge-settings, c14-* | PASS | Full |
| R-04 | in-process shim-entry argv/hang | shim, shim-entry, launcher-hook, fire-time-parity | PASS | Full |
| R-05 | override source | resolve-runtime | PASS | Full |
| R-06 | trust matrix | trust, trust-selfcontained | PASS | Full (Windows ACL: CI only, via Q9 diag) |
| R-07 | mcp-stdio stdout before exec / cwd | launcher-hook (D-077 spawn, signal), mcp-stdio-spawn (4 tests), wiring-e2e (E) | PASS | Full |
| R-08 | user-level rewrite fidelity | wire, merge-settings (claude-json lib), codex-install, opencode-install | PASS | Full |
| R-09 | node path / init preflight | install-store, wire, shim-entry (D-082 branches), init-preflight | PASS | Full (`init-preflight.test.js`, 3 tests) |
| R-10 | pass-through / LD_LIBRARY_PATH | launcher-hook | PASS | Full |
| R-11 | latency measured via literal command | benchmark-wiring (CI) | Measured; report-only (D-096) | Evidenced, not enforced (D-096) |
| R-12 | local vs cloud identity | wire-init, wiring-e2e byte-compare (E) | PASS | Full |
| R-13 | old tests deleted / parity advisory | check-test-count, parity-layer2 CI gating | PASS | Full |
| R-14 | OpenCode removal | opencode-install, c14-opencode, opencode-plugin set | PASS | Full |
| R-15 | resolve-binary wrapper semantics | resolve-runtime, resolve-binary tests | PASS | Full |
| R-16 | legacy Rust hook strings in artifacts | docs-grep, OQ-6 audit tests | PASS | Full |
| R-17 | shim quoting across shells/OS | shim, merge-settings, wiring-e2e (E, space-in-HOME) | PASS | Full |
| R-18 | `.cmd` exit/stdin | shim (win32), wiring-e2e (windows E), Q9 hook step | PASS | Full (CI) |
| R-19 | Codex approval lost | status / launcher-status, last-hook | PASS | Full (mitigated) |
| R-20 | detection false negative | project-state, wire (CODEX_HOME), codex-detect | PASS | Full (`codex-detect.test.js`, 7 tests) |
| R-21 | pre-Node check cost | shim-superset, benchmark-wiring | PASS / CI latency report-only (D-096) | Evidenced, not enforced (D-096) |
| R-22 | Windows trust cost | trust, trust-selfcontained, latency (win) | PASS | Evidenced, not enforced (D-096; win latency report-only) |
| R-23 | macOS/Windows no bundle/credential | launcher-status, project-state | PASS | Full |
| R-24 | stable copy / prune / concurrent | install-store, pkg-binary-copy | PASS | Full (`pkg-binary-copy.test.js`, 2 tests; Win rename EPERM CI-only) |
| R-25 | Windows MCP argv re-escaped | Q9 job (D-034/D-047), docs-grep | Q9 primary FAIL as designed; fallback PASS | Full (CI) |
| R-26 | CI matrix emulates vs literal | wiring-e2e 3 OS x 2 Node | PASS (CI) | Full |
| R-27 | .gitignore not edited | init, install-store | PASS | Full |
| R-28 | predicate parity (3 copies) | project-state parity, shim-superset, fire-time-parity | PASS | Full |
| R-29 | pre-Node marker check lossy | shim-superset (200 seeded, seed 20260101), shim | PASS | Full |
| R-30 | trust.json staleness | trust, trust-selfcontained | PASS | Full |
| R-31 | noise in every project | launcher-hook, wiring-e2e | PASS | Full |
| R-32 | last-hook.json races/corruption | hook-client/last-hook, launcher-status | PASS | Full |
| R-33 | cloned project no init | launcher-status, wire | PASS | Full |
| R-34 | cross-repo leak | headless-smoke, wiring-e2e not-a-project (E), shim-entry | PASS | Full |
| R-35 | headless contract regression | headless-smoke (positive+negative), docs-grep; infra-001 test_headless_access 9/9 | PASS | Full |
| R-36 | removal preserving foreign servers | wire, codex-install, opencode-install, merge-settings | PASS | Full |
| R-37 | concurrent ~/.claude.json writes | merge-settings (claude-json lib), wire, init-integration (tmp+rename, mtime/mode preserve) | PASS | Full |
| R-38 | projects[root] key mismatch | merge-settings / init `projects[root]` key cases | PASS | Partial (captured fixture, not live Claude; stated residual) |
| R-39 | user-scope MCP outside project | wiring-e2e (E), launcher-status | PASS | Partial (harness display not observable, see note) |
| R-40 | migration edge cases | wire, init, codex-install, opencode-install, status legacy lines | PASS | Full |

## Acceptance criteria
| AC | Status | Evidence |
|---|---|---|
| AC-01 | PASS | wiring-e2e byte-compare, 3 OS CI + local |
| AC-02 | PASS | hook-ownership L1/L2 scan, wiring-e2e spawn trace, docs-grep/audit tests |
| AC-03 | PASS | launcher-hook |
| AC-04 | PASS (Windows ACL/identity parts CI-only) | shim, trust, launcher-*, install-store, merge-settings, wiring-e2e |
| AC-05 | PASS | hook-ownership L1-L5 + prior-shim table, foreign negatives |
| AC-06 | **Evidenced, not enforced (D-096)** | latency job report-only; medians/p95 vs D-094 recorded in the full tables below (runs A, B) |
| AC-07 | PASS | wiring-e2e 3 OS x 2 Node; linux-local against built server (E2E_REQUIRE_LOCAL) |
| AC-08 | PASS | parity-layer2 job green (A, B), gating, not continue-on-error |
| AC-09 | PASS | opencode-install, c14-opencode |
| AC-10 | PASS | resolve-runtime, launcher-hook (hook + mcp-stdio) |
| AC-11 | PASS (automated parts) | docs-grep positive/negative; release-notes OpenCode statement is manual review, not performed here |
| AC-12 | PASS | package/hook-client/opencode sets, audit, check-test-count |
| AC-13 | PASS | wire/codex-install/merge-settings cross-scope dedupe |
| AC-14 | PASS | resolve-runtime matrix, launcher-hook |
| AC-15 | PASS | headless-smoke (a)(b)(c), docs-grep `UNIMATRIX_PROJECT_ROOT`, launcher-status (cloudEnv) |
| AC-16 | PASS | wire, codex-install, opencode-install, status legacy |

## Gaps
1. AC-06 / R-11 / R-21 / R-22: resolved by D-096 (human): latency job report-only; evidenced, not enforced against the D-094 figures.
2. Resolved at 7628b623: the four plan-named files `mcp-stdio-spawn` (4), `init-preflight` (3), `codex-detect` (7) and `pkg-binary-copy` (2) exist and pass. `pkg-binary-copy.test.js` is the D-078 assembled test (init, delete npx cache, literal hook command still stores an event). Unit/package totals above were measured at 0139b312 and exclude these 16 tests.
3. Q9 workflow concludes red by design; no stable green gate.
4. R-39 per-harness display: not observable. R-38: no live-Claude verification (stated residual). Windows trust/ACL and rename-over-open: CI-only. Release-notes OpenCode statement (AC-11) manual.
5. `ci.yml` has not run for this branch (PR-only trigger).
6. `check-hook-client-size` has zero headroom (115012/115012).

## Files
New: `packages/unimatrix/test/measure-pkg-size.js` (report-only, not a `.test.js`; orphan audit unaffected).
