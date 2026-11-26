# nan-024 Test Strategy Overview

Feature: Every harness on the Node client, portable wiring (GH #1023). Source: RISK-TEST-STRATEGY.md rev 6, ACCEPTANCE-MAP.md, DECISIONS.md D-001..D-091 (later wins; D-077..D-091 are the rework set; D-072 = Windows MCP is the D-047 fallback form; D-073 = Codex passes the hook string raw to `cmd.exe /C`).
ADRs consulted (binding): ADR-001..ADR-006 (cited by number per D-067; Unimatrix IDs per architecture/ADR-INDEX.md).

## Strategy

Three layers, all in `packages/unimatrix/` (node:test, zero new deps):

1. **Unit** (per component, file-per-component under `test/`): pure functions, fixture dirs only.
2. **Assembled-path integration** (new set `wiring-e2e`): run `init`/`wire` into a temp HOME + fixture repo, read the written command *from the written file*, execute it through the real harness shell (`sh -c`, `$SHELL -lc`, `cmd.exe /C`, Git Bash), assert stored observation / exit code / stdout / stderr. This is the floor test for AC-01/02/04/07; hand-built command strings or direct `main()` calls are NOT acceptable proof (principle 6: #917/#918/#930).
3. **CI-only platform proof** (Linux-local gates cannot see Windows/macOS): E2E matrix, latency gate, Q9 proof job, parity-layer2 gate.

Fixture rule (NFR-05): every test builds its own `HOME`/`USERPROFILE`/`XDG_*` temp dir and fixture repo via `test/helpers/` (extend, do not fork); never touches this repo's `.codex/hooks.json`, `.mcp.json`, `.opencode/`, or real `~/.unimatrix`. A guard helper asserts `HOME` under `os.tmpdir()` at test start.

## Test-infra registration (lessons #5790, #5781, #4779)

- All new suites are `.test.js` at top level of `test/` (auto-claimed by `package` set, non-recursive) or in `test/hook-client/` (default set). No new subdirectory (would be an orphan).
- **Windows gap**: `--set package` and `opencode-plugin` exit 0 on win32 by design. Windows coverage therefore needs a NEW explicit file-list set `wiring-e2e` in `test/run-hook-client.js` (serial, cross-platform, no Unix-shape fakes: stubs are `process.execPath` scripts). Files in `wiring-e2e` are listed explicitly and ALSO excluded/claimed in `package` so they are not double-run on ubuntu/macOS (resolved: exclusive). Audit set updated so every new file is claimed by exactly one set; `npm test` script and `test:wiring-e2e` script added.
- POSIX-only cases (sh shim, UDS, umask, mode bits, symlinks) carry `process.platform === 'win32'` skip-guards; `.cmd`/icacls cases carry `!== 'win32'` skip-guards with a visible skip reason. Byte-compare/fixture suites stay unguarded.
- macOS traps (#5784): resolve coreutils by PATH, no `realpath -m`, UDS path <104B (use short `os.tmpdir()` roots), `/tmp` symlink to `/private/tmp` (realpath fixtures).
- async `spawn`, never `spawnSync`, when a stub server runs in-process (#4774). Explicit `node:test` imports.
- NFR-04 / R-13: a CI step records `it(` / test counts of touched suites on the base commit vs PR and fails on decrease (script `test/check-test-count.js`, base counts committed in `test/fixtures/test-counts.json`). Old-string tests are updated, not deleted (C-10 superseded D-008).
- Size gates: `lib/hook-client/` additions (last-hook write) must stay under the 100 KB / zero-dep gates (`check-hook-client-size.js`, `check-zero-deps.js`).

## CI jobs (the only proof for Windows/macOS) 

| Job | OS x Node | Required | Proves |
|---|---|---|---|
| `q9-windows-mcp-proof` (exists) | windows | gate, first | D-047 fallback MCP argv via real Claude/Codex spawners, USERPROFILE with space; hook `.cmd` form via `cmd /C` (D-073). Add: assert Windows MCP written entry == D-047 form |
| `wiring-e2e` | {ubuntu, macos, windows} x Node {20, 24} (18/22 optional) | required once green (land non-required first per #5790) | AC-01/04/07, R-17/18/26/34/37/38/39 literal-command E2E, HTTPS test endpoint + `NODE_EXTRA_CA_CERTS`, space-in-home case |
| `latency-gate` | 3 OS | required | AC-06/ADR-005: `node -e 0` baseline same job; hook p95 <= max(base+15, 50) ms; not-a-project path; `mcp-stdio` not-a-project start-to-exit; 50 iters + 5 warmups, cold, through `sh -c`/`cmd /C` |
| `parity-layer2` | ubuntu | required (AC-08) | UDS, PreCompact, concurrency; needs cargo server build |
| `package-suite`, `hook-client-matrix`, `hook-client-audit` (exist) | existing grid | existing | updated suites, orphan audit, size/zero-dep, test-count diff |
| linux local E2E (UDS) | ubuntu | required | local transport, zero Rust exec (process trace), `LD_LIBRARY_PATH` pass-through |

Windows/macOS runners must NOT depend on a Rust binary (cloud-only, D-032): HTTPS stub endpoint is a Node `https` server in the test process with a generated CA passed via `NODE_EXTRA_CA_CERTS`.

## Risk-to-test mapping (component file in parentheses)

| Risk | Pri | Primary tests | Component plan |
|---|---|---|---|
| R-03 | Crit | positive/negative/property tables, round-trip with `buildShimCommand`, L1-L6 seeded re-init | hook-ownership, merge-settings |
| R-04, R-18 | High | same event via 3 paths -> identical record; `.cmd` exit/stdin | shim, shim-entry, hook-client |
| R-07, R-39 | High | spawn written MCP entry, first byte `{`; not-a-project silent + zero network | launcher, wire-init |
| R-08, R-36, R-37, R-38, R-40 | High/Med | foreign preservation, dry-run == real, concurrent writer, key normalisation table, migration | wire-init, merge-settings, codex-install, opencode-install |
| R-17 | High | quoting matrix across shells/OS, refused chars | shim, merge-settings |
| R-26, R-11, R-21, R-22 | High/Med | CI config check test + E2E/latency jobs | ci-and-docs, trust |
| R-28, R-29, R-34 | High | shared fixture table across 3 predicate copies + shell pre-check; headless cross-repo | project-state, shim-entry, shim |
| R-01, R-23 | Med | exit-outcome matrix (never 2; stdout empty) | launcher |
| R-05, R-10, R-15 | Low/Med | resolver precedence matrix, pass-through env | resolve-runtime, launcher |
| R-06, R-30, R-09, R-24 | Med | trust matrix, install-store atomicity | trust, install-store |
| R-12, R-13, R-14, R-16, R-19, R-20, R-25, R-27, R-31..R-33, R-35 | Low/Med | see component files | per file |
| R-02 | Low | shell matrix in wiring-e2e (bash/zsh/fish/sh/cmd), PowerShell documented | codex-install, ci-and-docs |

All 40 risks are mapped in component files; none uncovered.

## Acceptance map -> tests

AC-01 wiring-e2e byte-compare (wire-init); AC-02 spawn trace + scan (wire-init, ci-and-docs audit); AC-03 launcher-hook; AC-04 spread (shim, trust, launcher, install-store, merge-settings); AC-05 hook-ownership + merge-settings table; AC-06 latency-gate; AC-07 wiring-e2e matrix; AC-08 workflow gating check (ci-and-docs); AC-09 opencode-install; AC-10/14 resolve-runtime + launcher; AC-11 docs grep test; AC-12 suite list + count diff; AC-13 cross-scope dedupe; AC-15 headless; AC-16 migration tests.

## Headless contract negative case (D-053, AC-15, R-35)

`headless-smoke.test.js`: (a) positive: env with `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*` + `UNIMATRIX_PROJECT_ROOT=<fixture>` + cwd inside -> `initialize` + search OK; (b) NEW negative: same env, `UNIMATRIX_PROJECT_ROOT` removed -> `mcp-bridge` exit 0, empty stdout/stderr, recording stub endpoint receives 0 requests (test FAILS if variable is not honoured); (c) cwd outside root, sibling-prefix (`/fx/repoA-other`), empty, relative, symlinked root. Plus grep test that `docs/client-setup.md` lists `UNIMATRIX_PROJECT_ROOT`.

## Integration harness plan (product/test/infra-001)

infra-001 drives the Rust `unimatrix-server` over MCP JSON-RPC. nan-024 changes no Rust server, tool, schema, or confidence logic (Rust `hook.rs` and MCP stdio/daemon unmodified). The behavior is JS client/installer, exercised by the `packages/unimatrix` node suites above, not infra-001.

- **Mandatory gate**: `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` (needs `cargo build --release`). Regression only.
- **Relevant suites to run**: `protocol` (MCP handshake unchanged; Rust stdio still the MCP server), `tools` + `lifecycle` (store->search, restart persistence; the Node client now writes the hook records these read), `test_headless_access.py` and `test_project_isolation.py` (vnc-050 scoped tokens and slug isolation: D-052 changes the headless env tier client-side; must stay green), `test_https_uds_parity*.py` and `test_transport_health.py` (HTTPS/UDS parity: client is the only change). `security` quick run (trust/PII unchanged).
- **Gaps / new infra-001 tests**: none required; MCP-visible behavior is unchanged and the new behavior is not reachable through the Python harness (no Node shim, no harness shells, no Windows). Optional single addition if cheap: in `test_headless_access.py`, a case that a recording request from the Node bridge with `UNIMATRIX_PROJECT_ROOT` unset reaches zero slugs; only if the existing fixture already spawns the Node bridge (check in Stage 3c; otherwise covered by `headless-smoke.test.js`). Not planned as mandatory.
- Triage per USAGE-PROTOCOL: pre-existing failures -> GH issue + xfail, never fixed in this PR.

## Stage 3c local run order (Linux)

1. `cargo test --workspace` (hardened convention) 2. `bash product/test/infra-002/check-workspace-link-smoke.sh` 3. `cargo build --release` + infra-001 smoke + suites above 4. `npm test` in `packages/unimatrix` (all sets incl. `wiring-e2e`, `audit`) 5. read CI results for the Windows/macOS/latency jobs (cannot be run locally: report as CI-evidence, not local PASS).

## Rework (D-077..D-083)

### Added risk/AC mapping
| Decision | Risk / AC | Tests | Component plan |
|---|---|---|---|
| D-077 mcp-stdio spawn, inherited stdio, exit code + signal mirrored, zero Node stdout bytes pre-spawn | R-07, R-39, AC-07 | `mcp-stdio-spawn.test.js` | launcher, shim-entry |
| D-078 platform binary in `pkg/<version>/node_modules`; resolver independent of npx cache | R-05, R-24, R-09, AC-10 | `pkg-binary-copy.test.js` (linux), size measurement | install-store, resolve-runtime |
| D-079 pre-Node clause (d) + loosened headless; MANDATORY superset test (verdict != null => Node started); Node D-052-strict | R-28, R-29, R-34, R-21, AC-06 | `shim-superset.test.js` (fixture matrix + seeded property) | shim, shim-entry, project-state |
| D-080 `allowSelf`, `buildShimCommand` home (space round trip), `isInstalled {env, home}` | R-17, R-03, R-23, AC-04 | resolve-runtime, merge-settings, project-state additions | per file |
| D-081 Codex detection matrix + `$CODEX_HOME` destinations; invalid CODEX_HOME ignore+warn (decided D-085); Codex MCP write uses same detection (D-084) | R-19, R-20, R-02, AC-04/13 | `codex-detect.test.js` | codex-install, wire-init |
| D-082 init exit 1 if node unrecorded; wire/upgrade preflight | R-09, R-24, R-08 | `init-preflight.test.js` | install-store, wire-init |
| D-083 AC-11 grep rule + PowerShell statement | R-16, R-25, AC-11 | `docs-grep.test.js` positive/negative fixtures | ci-and-docs |

### H1 decided (Rework 3): D-090, D-091 are in scope
`effectiveEnv` (D-091) and sibling `trust.js` (D-090) are normal contract tests. Invalid/relative `$CODEX_HOME` -> ignore + warn is D-085.

### Stage 3c additions
- Measure `pkg/<version>` size and the platform-binary subtree on Linux (`test/measure-pkg-size.js`); RISK-COVERAGE-REPORT must carry a "D-078 package size" row reporting the numbers only (D-087: no cap, no pass/fail threshold).
- RISK-COVERAGE-REPORT must list the superset-test fixture count and the seed used, and cite CI run IDs for Windows/macOS/latency jobs.

### Resolved earlier open questions
1. `wiring-e2e` files run only in their own set/job (no double-run in `package`; audit claims each file once).
2. Test-count baseline: `test/fixtures/test-counts.json` + `test/check-test-count.js` diff script.
3. R-38 key normalisation: still verified against a captured fixture, not a live Claude (residual, stated in the report).
4. New CI jobs (`wiring-e2e`, `latency-gate`) land non-required until the full grid is green once, then promoted.

## Open questions
1. (Resolved) H1: D-090 and D-091 accepted by human.
2. (Resolved) D-085 CODEX_HOME ignore+warn; D-087 no size cap; D-086 Windows exit-code-only.
3. If the latency gate forces the Windows pre-Node check on (D-088), escalate to the primary.

## Rework 2 (D-084..D-089)
- D-084: Codex MCP write gated by D-081 detection (codex-install M1-M3).
- D-085: invalid `$CODEX_HOME` ignore+warn, decided.
- D-086: Windows exit code only; POSIX re-raises signal (launcher.md, win32 skip-guard).
- D-087: package size reported only in RISK-COVERAGE-REPORT; no threshold.
- D-088: Windows pre-Node check disabled; shim-cmd test asserts it (shim.md); escalate if latency gate forces on.
- D-089: `$HOME` Gemini form accepted (ci-and-docs positive fixture retained; not an AC-11 narrowing).


## Rework 3 (D-090, D-091)
| Decision | Risk / AC | Tests | Component plan |
|---|---|---|---|
| D-090 sibling `trust.js`: self-contained (static no-non-builtin-require + empty-dir dynamic run), atomic tmp+rename refresh (interrupt leaves old intact, byte-identical to package), shim path/hook command byte-equal across upgrade (D-028 test extended) | R-06, R-22, R-30, R-09, R-24, AC-04 | `trust-selfcontained.test.js`, trust, install-store, merge-settings, codex-install, shim-entry | trust, shim-entry, install-store, merge-settings, codex-install |
| D-091 `effectiveEnv` governs hooks + mcp-bridge; `status` cloud env text (4 variants) + `--json cloudEnv` (between transport and trust), sorted names, no values, raw shell env, exit 0 | R-29, R-34, R-35, R-28, AC-15, AC-10/14 | resolve-runtime, project-state parity, `headless-smoke.test.js`, `status-cloud-env.test.js` | resolve-runtime, project-state, launcher |
- D-086 Windows exit-code-only launcher branch tests retained unchanged (launcher.md).
- Headless-smoke (b)/(c) cases double as the bridge half of the D-091 effectiveEnv proof.

## Rework 4 (gate 3a iteration 1)
- R-39 evidence item: record, per harness (Claude Code, Codex, OpenCode, Gemini), what is displayed for an MCP server that exits 0 outside a project (stderr silent, exit 0, before `initialize`). Captured as CI log excerpts / manual notes in RISK-COVERAGE-REPORT; observation only, no pass/fail beyond exit 0 + empty stdout (launcher.md, opencode-install.md).
- AC-12 suites enumerated in ci-and-docs.md "AC-12 suite list".
- D-082 branch matrix (warn / fail-no-PATH / fail-Windows) in wire-init.md; D-087 size report-only; symlinked trust inputs refused (trust.md).
