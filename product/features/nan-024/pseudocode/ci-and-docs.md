# ci-and-docs — workflows, docs, audit, and the headless-contract task

Languages: Part A and C are yml/md/bash (no JS); **Part B (headless contract, D-053) is JS + md** inside packages/unimatrix — flagged mixed; recommend dispatching Part B to the JS developer and Parts A/C to a CI/docs agent. Nothing here touches `crates/`.
The Q9 proof workflow ALREADY EXISTS (`.github/workflows/q9-windows-mcp-proof.yml` + `.github/q9-proof/`); result recorded in D-072 (primary form failed, D-047 fallback adopted). This component does NOT redo it. Optional follow-up only: once the real shim exists, re-point its stub shim at the rendered `lib/shim/unimatrix.cmd` (non-blocking).

## Part A — Stage 1 OQ-6 audit (D-035, D-070; read-only, first task)
Search for direct Rust `hook` invocations outside init-written files, covering: repo tree; docs; installers/postinstall; packed npm tarball (`npm pack` in `packages/unimatrix`, extract into a scratch dir, grep the extraction — never run code from it); OpenCode plugin build output (`opencode-plugin` dist). Patterns: `unimatrix(-server)? hook`, `LD_LIBRARY_PATH=.* hook`, `/unimatrix hook`. Report each hit as: path, line, class (init-written / reference config / docs / shipped artifact). Known dispositions: `.codex/hooks.json` EXEMPT (D-058/D-048 -> #1025); `.gemini/settings.json` planned work in Part C (D-074); `opencode-plugin/lib/emit.js` known exception left to #998, not in tarball (D-075). Any other shipped-artifact hit = escalate to the Delivery Leader. Output goes in the delivery agent report, not a repo file.

## Part B — Headless contract (D-052/D-053, AC-15, R-34, R-35) — NAMED TASK
Files: `packages/unimatrix/lib/hook-client/remote-resolve.js`, `.../mcp-bridge.js`, `bin/unimatrix.js` (mcp-bridge route, owned by launcher), `docs/client-setup.md`, `packages/unimatrix/test/headless-smoke.test.js`. (Paths in the brief omit `hook-client/`.)
1. `remote-resolve.js` single enforcement point:
```
resolveRemote(env, opts):
  env = projectState.effectiveEnv(env, opts.cwd ?? process.cwd(), opts.home ?? os.homedir())   // top of function; strips UNIMATRIX_BUNDLE / UNIMATRIX_REMOTE_* outside the root
  ...existing logic unchanged...
```
`config.js resolve` passes `cwd: startDir`; the bridge passes `cwd: process.cwd()`. `project-state.js` lazy-requires `./hook-client/config.js` inside `isUnimatrixProject` to avoid a require cycle (config -> remote-resolve -> project-state).
2. `mcp-bridge.js main(argv, deps)`:
```
env = deps.env ?? process.env; cwd = deps.cwd ?? process.cwd(); home = deps.home ?? os.homedir()
outside = headlessEnvSet(env) && !headlessRootContains(env, cwd)
projectHash = argv[2]
if outside and !isNonEmptyString(projectHash): exit(0) with no stdout/stderr, no request     // headless launch outside root: not a project (D-043/D-052)
if !isNonEmptyString(projectHash) and !headlessEnvSet(env) : usage error exit 2 (unchanged)
buildSession(projectHash, deps)    // resolveRemote now sanitises env; legacy `mcp-bridge <hash>` with store credential keeps working even with the env exported elsewhere
```
`bin/unimatrix.js mcp-bridge` route: compute `headless` via `headlessEnvSet(process.env)` (not just BUNDLE/REMOTE_MCP_URL), and let bridge `main` own the gate.
3. `docs/client-setup.md`: in the env var table (~line 131) add row `UNIMATRIX_PROJECT_ROOT | Required with any headless variable. Absolute path of the repository the headless credential belongs to; headless env applies only when the working directory is inside it. Without it, hooks and the MCP bridge treat every directory as not a Unimatrix project and exit 0 silently.` Add it to the MCP config `env` example (~line 186) and the CI workflow example (~line 203: `UNIMATRIX_PROJECT_ROOT: ${{ github.workspace }}`). State PowerShell is not supported as a hook shell (D-041). Keep the existing conflict rule text.
4. `headless-smoke.test.js`: every `mcpSession(...)` and hook invocation env gains `UNIMATRIX_PROJECT_ROOT: <tmp project dir>` with cwd inside it; new negative case: same env WITHOUT `UNIMATRIX_PROJECT_ROOT` -> bridge exits 0, no stdout/stderr, zero requests reach the recording endpoint (the test fails if the variable is removed from the positive case); cwd outside the root (sibling sharing a prefix) -> same silent result; extend the existing docs assertion (line ~283) to also require `UNIMATRIX_PROJECT_ROOT` in `docs/client-setup.md` (R-35 grep test).
Error handling: bridge silent exit is the only new behaviour; no new exit codes. Test scenarios: R-34 scenarios 1-4 via real entry points against a recording HTTPS stub; parity table rows for D-052 shared with project-state tests.

## Part C — CI workflows and docs (repo `.github/workflows/`, README, `.gemini/settings.json`, release notes)
Workflows (extend `ci.yml` or add `nan-024-e2e.yml`; mark required in branch protection by the human):
- `shim-e2e` matrix {ubuntu, macos, windows}: fixture repo + temp `HOME`/`USERPROFILE` WITH A SPACE; HTTPS recording stub with a test CA passed through `NODE_EXTRA_CA_CERTS`; run init (Linux: local with stub binary and cloud; macOS/Windows: cloud attach); read the literal hook command from `settings.local.json` and `~/.codex/hooks.json` and execute it through the harness shells (`sh -c`, `$SHELL -lc`, `cmd.exe /C "<line>"` with Codex wrapping, Git Bash); assert stored observation + provider; non-Unimatrix cwd run (exit 0, no stderr, no observation); Windows: run the D-047 fallback MCP entry (`initialize` succeeds); Claude `~/.claude.json` project-key spelling check (R-38). Codex trust bypassed in CI (`--bypass-hook-trust` equivalent / fixture).
- `latency-gate` matrix {ubuntu, macos, windows}: measure `node -e 0` baseline p95, then 50 iterations (5 warm-ups) per event class of the literal command for paths 1-3 (ADR-005), plus `mcp-stdio` not-a-project start-to-exit; fail if p95 > max(baseline + 15 ms, 50 ms). Publish numbers as a job artifact; a breach on the not-a-project path means flip `PRENODE_ENABLED` (install-store.md) and re-run.
- `parity-layer2` gating job (AC-08): Ubuntu, cargo-build the server, run `parity-layer2*` suites; required, not advisory (current ci.yml excludes Layer 2).
- Test-count guard (AC-12/NFR-04): a CI step compares per-suite test counts of the touched suites (`merge-settings`, `dogfood-effect`, `init-remote`, `codex-install`, `opencode-install`, `wire`, `resolve-binary`, `cli-routing`, `shim`) against a committed baseline JSON and fails on decrease.
- Ensure `hook-client-matrix` keeps passing with new `lib/*` files and that `test/check-zero-deps.js` / `check-hook-client-size.js` accept the `config.js -> ../project-state.js` require (open question OQ-3).
Docs:
- `README.md`: rewrite the hook-driven delivery paragraph (~line 249 "Single binary — the `hook` subcommand connects ... via UDS"), the multi-provider paragraph (~252: reference configs / "`--provider codex-cli` flag on the `unimatrix hook` subcommand" / provisioning claims), the OpenCode paragraph (~254: plugin provisioned by init -> OpenCode = retrieval only; hooks deferred to #998), and the model-identity sentence (~280 `unimatrix hook --model`) so no text describes the Rust `unimatrix hook`; describe the Node client, user-level shim `~/.unimatrix/bin/unimatrix`, per-machine config, macOS/Windows cloud-only, `unimatrix status`. State PowerShell unsupported as a hook shell (D-041).
- `.gemini/settings.json`: replace the four `unimatrix hook ... --provider gemini-cli` commands with the absolute-shim form placeholder `"<home>/.unimatrix/bin/unimatrix" hook <Event> --provider gemini-cli` plus a `_comment` key pointing to `unimatrix status`; Gemini writer stays out of scope (open question OQ-4: valid JSON comment key and exact replacement need confirmation).
- Release notes (CHANGELOG or release body): Rust hook client retired from shipped wiring; per-machine wiring; OpenCode = retrieval only; headless contract now requires `UNIMATRIX_PROJECT_ROOT`; macOS/Windows cloud-only; migration (re-run `npx unimatrix init`).
- AC-11 grep test (new JS test, owned by the test plan): over README and `.gemini/settings.json` fail on `unimatrix(-server)? hook` not preceded by `.unimatrix/bin/`, on `LD_LIBRARY_PATH=` near `hook`, and require the PowerShell statement in docs. Note: `isUnimatrixCommand` cannot be used for this because the abs-shim form is itself "owned".

## Error handling / failure modes
CI: any breach or missing platform job fails the build (R-26: macOS/Windows jobs must run the literal command through the shell, not an emulation). If the Windows `.cmd` hook form fails in the proof job there is no fallback: stop and escalate (RTS failure modes).

## Key test scenarios
Part A audit table complete; Part B tests as above; Part C: CI config check (jobs exist, required, space-in-home, `NODE_EXTRA_CA_CERTS`), latency artifact present, test-count guard fails on a deliberately reduced fixture.
