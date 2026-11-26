# hook-client (`lib/hook-client/index.js`, `lib/hook-client/config.js` env hook-up) — JS

## Purpose
Make the existing Node client callable in-process and observable: `main(argv)` export with `require.main === module` guard (already present), `last-hook.json` write after each handled event (ADR-004, FR-12g), and honour D-052 env sanitisation. No transport/normalisation behaviour changes.

## Changes
### 1. main(argv)
```
async function main(argv = process.argv):       // existing body, parseHookArgs(argv) instead of process.argv
  // argv[2] = event, rest scanned for --provider (unchanged parseHookArgs contract). Launcher passes ["node", <path>, EVENT, ...rest].
```
`readStdin()` unchanged (fd 0). Guard at bottom unchanged: `if (require.main === module) main()` (direct `node hook-client/index.js E` still works). Still never sets a nonzero exit for connection failures, never stdout on failure (D-042). `main` must remain resolve-only (never reject) because the launcher awaits it.

### 2. last-hook.json (R-32)
After the event is dispatched (success or best-effort completion; not on config failure), call `recordLastHook(config.projectHash, input.provider)`:
```
function recordLastHook(projectHash, provider):
  try:
    dir = join(os.homedir(), ".unimatrix", projectHash)
    if !isDir(dir): return                                  // missing/read-only hash dir -> skip silently (event still stored, exit unchanged)
    file = join(dir, "last-hook.json")
    cur = {}; try cur = JSON.parse(read(file)); if not plain object: cur = {}
    cur[provider] = new Date().toISOString()
    tmp = file + "." + process.pid + "." + Date.now() + ".tmp"
    writeFileSync(tmp, JSON.stringify(cur), {mode: 0o600}); renameSync(tmp, file)      // atomic; concurrent writers lose at most one provider key
  catch: ignore                                             // best-effort; never changes exit code
```
Provider key = canonical `input.provider` (`claude-code`, `codex-cli`). Lost-update between two providers is acceptable only if the merge re-reads right before rename (done above); test asserts both keys present after 20 concurrent events in the common case. Budget: ≤ 2 ms p95 (one small stat+read+write+rename); runs after the response is sent / fire-and-forget path so it never lengthens the request.

### 3. D-052 env tier (config.js `resolve`)
```
r = resolveRemote(env, {projectHash, cwd: startDir, readStore: env.UNIMATRIX_REMOTE_URL ? () => null : undefined})   // adds cwd
```
`resolveRemote` applies `effectiveEnv(env, cwd, home)` at its top (ci-and-docs Part B), so hook events from a cwd outside `UNIMATRIX_PROJECT_ROOT` can never use an exported bundle/remote env, even when the launcher is bypassed (`node hook-client/index.js`). The `env.UNIMATRIX_REMOTE_URL ? ...` readStore shortcut must be evaluated on the effective env (compute `effectiveEnv` once in `resolve`, pass it as `env` to `resolveRemote`). `remote-resolve.js` requires `../project-state.js` (relative, no new dependency); `test/check-zero-deps.js` and `test/check-hook-client-size.js` must be checked for compatibility (open question OQ-3).

## Cwd note
Shim predicate uses `process.cwd()`; client uses stdin `cwd` (`resolveCwd`). Harness hooks run with cwd in the project; any divergence is covered by the parity table (subdir/symlink cases) — no code change, test only.

## Error handling
`recordLastHook` swallows every error. `main` keeps its last-resort try/catch (stderr only). `effectiveEnv` never throws.

## Key test scenarios
- `main(["node","x","SessionStart"])` equals direct spawn behaviour; `--provider codex-cli` stamps codex-cli; `require()` does not auto-run.
- last-hook: write creates `{"claude-code": ISO}`; second provider merges; corrupt existing file replaced; read-only dir -> event stored, exit 0; mode 0600; 20 concurrent events from two providers leave valid JSON.
- D-052: headless env exported, cwd in repo B vs root repo A -> no HTTP request, falls to store/uds; cwd inside A -> request sent.
- Existing hook-client suites (`[hc]`, parity layer 1) stay green unchanged.
