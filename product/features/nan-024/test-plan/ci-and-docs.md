# Test Plan: ci-and-docs (workflows, Q9, OQ-6, README, `.gemini/settings.json`, headless contract)

Risks: R-26, R-11, R-13, R-16, R-35, R-02, R-25, AC-06/07/08/11/12. Files: `test/ci-config.test.js` (parses workflows with a tiny YAML-subset reader or regex; no new deps), `test/docs-grep.test.js`, update `headless-smoke.test.js`; add `--set wiring-e2e` + audit claims in `test/run-hook-client.js`.

- Q9 first (D-034/D-072): `q9-windows-mcp-proof.yml` stays passing; add assertion step that the Windows MCP entry written by init equals the D-047 fallback form; `.cmd` hook form via `cmd /C` (D-073) job step; if hook form fails -> stop and escalate (no fallback).
- CI config check (R-26): `ci-config.test.js` asserts macOS and Windows E2E jobs exist, are required (promotion note), read the command from the hook destination, include a space-in-home case, set `NODE_EXTRA_CA_CERTS`; `parity-layer2*` job present, not `continue-on-error` (AC-08); latency job present on 3 OS with baseline step.
- Latency gate (AC-06/R-11/R-21/R-22/R-39): script `test/hook-client/benchmark-wiring.js`: literal command through `sh -c`/`cmd /C`, cold, 50 iters + 5 warmups per event class; `node -e 0` p95 measured in same job; asserts p95 <= max(base+15, 50); cases: project, non-project (0/1/50 store dirs), `mcp-stdio` not-a-project start-to-exit, `mcp-stdio` in-project start-to-initialize reported; reports absolute numbers to job summary. Windows trust paths 2/3 included.
- OQ-6 audit (D-035/D-070): scripted grep over repo, docs, installers, `npm pack` tarball listing contents and `opencode-plugin` dist for Rust `hook` invocations; known exempt: repo `.codex/hooks.json` (D-058), `.gemini/settings.json` (D-074, fixed in scope), `emit.js` (D-075). Output archived in the test report; anything else -> escalate.
- Docs grep (AC-11): `README.md` and `.gemini/settings.json` contain no Rust `unimatrix hook` form; release-notes file contains OpenCode retrieval-only statement; docs state PowerShell unsupported (D-041); `docs/client-setup.md` lists `UNIMATRIX_PROJECT_ROOT` (D-053).
- Headless negative case: see OVERVIEW (D-053); `headless-smoke.test.js` fails when the variable is removed.
- NFR-04: `check-test-count.js` compares per-file test counts for touched suites vs baseline; any decrease fails. Orphan audit (`--set audit`) passes with new files and new set; `npm test` includes `wiring-e2e`.
- Windows/macOS evidence is CI-only: Stage 3c report cites job run IDs, not local results.

## Rework (D-083, D-077, D-078, D-079, OQ resolutions)
### AC-11 grep rule test (`test/docs-grep.test.js`, expands the Docs grep bullet above)
Rule (from ARCHITECTURE "Reference configs and docs"): over `README.md` and `.gemini/settings.json`, FAIL on any match of `(?<!\.unimatrix/bin/)(?<![\w.-])unimatrix(-server)?["']?\s+hook\b` and on any line containing both `LD_LIBRARY_PATH=` and `hook`. The test uses the rule as a regex under test with a fixtures table in `test/fixtures/docs-grep/`:
- Negative fixtures (MUST fail the rule): `unimatrix hook Stop`, `unimatrix-server hook`, `"unimatrix" hook`, `'unimatrix' hook`, `/usr/bin/unimatrix hook`, `LD_LIBRARY_PATH=/x unimatrix hook`, `LD_LIBRARY_PATH=/x \` + `hook` on one line, line-start `unimatrix   hook`.
- Positive fixtures (MUST pass): `"$HOME/.unimatrix/bin/unimatrix" hook BeforeTool --provider gemini-cli`, `~/.unimatrix/bin/unimatrix hook Stop`, `"C:/Users/x/.unimatrix/bin/unimatrix.cmd" hook E`, `the launcher's `hook` verb`, `npx unimatrix mcp-stdio`, `unimatrix.cmd" hook`.
- Real-file run: the rule passes on the real README and `.gemini/settings.json`; `.gemini/settings.json` parses as JSON, has exactly four hook commands (`BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`), each equal to the D-083 string, no `_comment` key; README lines ~252/254/280 contain the D-083 replacement wording (match on the phrase "launcher's `hook` verb").
- Guard: the test asserts the rule does not narrow AC-11 by also running the plain AC-11 check (no `isUnimatrixCommand` match of Rust-form L1/L2 arms in those files); `isUnimatrixCommand` is NOT used as the AC check (shim form is owned).
- Docs state PowerShell is unsupported as hook shell (D-041): grep README or `docs/` for a line containing both `PowerShell` and `unsupported`; absent -> fail. Also states Windows users substitute the absolute `unimatrix.cmd` path.
- OQ-6 audit script treats this rule as its Rust-hook detector; exempt list unchanged (D-058, D-074 now fixed, D-075).

### CI/test-infra decisions (resolved)
- `wiring-e2e` files run ONLY in the `wiring-e2e` set/job (not double-run in `package`); `package` set audit claims exclusion explicitly; orphan audit asserts each file claimed by exactly one set.
- Test-count baseline: `test/fixtures/test-counts.json` + `test/check-test-count.js` diff script (per-file counts, fails on decrease; baseline updated only in a commit that states why).
- New CI jobs (`wiring-e2e`, `latency-gate`) land NON-required; `ci-config.test.js` asserts jobs exist and are not `continue-on-error`; promotion to required is a post-green-grid follow-up (not asserted as required in this PR).

### New assembled tests registered in `wiring-e2e`
`shim-superset.test.js` (D-079), `mcp-stdio-spawn.test.js` (D-077), `init-preflight.test.js` (D-082), `codex-detect.test.js` (D-081), `pkg-binary-copy.test.js` (D-078, linux). Add to the explicit list in `run-hook-client.js`.

## Rework 2 (D-089)
`"$HOME/.unimatrix/bin/unimatrix" hook ... --provider gemini-cli` in tracked Unix `.gemini/settings.json` is accepted (D-027 covers init-written destinations only); keep it as a positive docs-grep fixture.

## Rework 4
- AC-07 local Linux E2E ("linux local E2E (UDS)") uses the BUILT `unimatrix-server` (`cargo build --release`), not a stub: a stub cannot store an observation over UDS. Stubs remain only for macOS/Windows cloud-path rows and exit/argv tests.
- AC-12 suite list (ci-config test asserts each is claimed by a CI job/set, and `check-test-count.js` non-decreasing per file): skills install; DB pre-create; postinstall model; bundle decode; credential 0600; ping; pinned/CA TLS; headless `npx -y @dug-21/unimatrix mcp-bridge` (`headless-smoke.test.js`); Codex TOML upsert; MCP removal preserving foreign servers; SubagentStop opt-in; Rust pass-through `LD_LIBRARY_PATH`; `[hc]`, `[pkg]`, `[oc]` sets; orphan audit. Old-string tests are updated, never deleted.
- R-39 harness-display evidence: Stage 3c records, for each harness, what is shown for an MCP server exiting 0 outside a project (CI job log or manual screenshot/notes); included in RISK-COVERAGE-REPORT, not a pass/fail gate beyond exit 0 + empty stdout.
