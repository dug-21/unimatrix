# Test Plan: resolve-runtime (`lib/resolve-runtime.js`)

Risks: R-05, R-10, R-15, AC-10, AC-14. Files: `test/resolve-runtime.test.js`; update `resolve-binary.test.js` (count must not drop).

Matrix {redirect only, `UNIMATRIX_BINARY` only, both, none, invalid redirect, malformed redirect.json, relative `binary`, redirect symlink, `client` lacking `lib/hook-client/index.js`} x transport {local, cloud} x platform {linux, darwin, win32}:
- env + redirect -> binary from env (`binarySource:"env"`), client from redirect (`clientSource:"redirect"`).
- env only -> client = install (R-05); redirect with only `binary` -> client = install.
- present-but-invalid -> throws `RuntimeResolutionError{code,source,path}` (never silently falls back); absent -> falls back (SR-20).
- darwin/win32 cloud: `binary: null`, no throw (R-15); Linux pass-through unresolvable -> error naming every source tried.
- `resolve-binary.js` wrapper: returns same value as `resolveRuntime().binary`; existing resolve-binary tests unchanged and green.
- Integration per caller (assembled): literal hook command and spawned MCP entry with recording stub binary assert executed binary path and that client path is unchanged (AC-10); pass-through (`serve`) records argv + `LD_LIBRARY_PATH` under each source (R-10).
- Redirect removal reverts to install; fixture `git status` clean (no repo writes).

## Rework (D-078, D-080; effectiveEnv D-091)
- `allowSelf` (D-080, init-time): `resolveRuntime({..., allowSelf:true})` with no `install.json` returns clientRoot = the running package root (`clientSource === "install"` exactly), binary per normal rules; `allowSelf` omitted/false with no install -> throws `RuntimeResolutionError`. Additive: all existing matrix rows pass unchanged with the field absent. `allowSelf:true` never overrides a valid redirect or env (precedence preserved).
- D-078: installed binary source is `pkg/<version>/node_modules/<platform pkg>`; deleting the original package location does not change the result.
### `effectiveEnv(env, cwd, home)` (D-091, accepted)
- Headless env set and cwd outside valid `UNIMATRIX_PROJECT_ROOT` (unset, relative, or not containing realpath(cwd)) -> returned env lacks `UNIMATRIX_BUNDLE` and all `UNIMATRIX_REMOTE_*`; inside root -> unchanged; input object not mutated; other vars untouched. Same fixture table as project-state D-052 rows (prefix sibling, symlinked root, empty).
- Governs both consumers: (1) hooks: config/remote resolution, `isInstalled`, transport use the effective env (cloud vars outside root -> local/no-op, zero network); (2) MCP bridge env tier: `mcp-bridge` with the same env + cwd outside root exits 0 silent, recording stub gets 0 requests; inside root works. Assert no other code path reads shell cloud vars (grep test over `lib/` and `bin/`: reads of `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_` only in `effectiveEnv` and `status` raw read).
- Inlined copy in `shim-entry.js` parity with `lib/project-state.js` `effectiveEnv` (shared table, via built file).
