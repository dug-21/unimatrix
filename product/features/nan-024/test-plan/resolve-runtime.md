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

## Rework (D-078, D-080; effectiveEnv PENDING H1)
- `allowSelf` (D-080, init-time): `resolveRuntime({..., allowSelf:true})` with no `install.json` returns clientRoot = the running package root (`clientSource:"install"` or documented `"self"` per pseudocode; assert the pseudocode value), binary per normal rules; `allowSelf` omitted/false with no install -> throws `RuntimeResolutionError`. Additive: all existing matrix rows pass unchanged with the field absent. `allowSelf:true` never overrides a valid redirect or env (precedence preserved).
- D-078: installed binary source is `pkg/<version>/node_modules/<platform pkg>`; deleting the original package location does not change the result.
### SEPARABLE: `effectiveEnv(env, cwd, home)` (PENDING H1; not in contract until decided)
If adopted: with headless env set and cwd outside a valid `UNIMATRIX_PROJECT_ROOT` -> returned env has `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*` stripped; inside root -> unchanged; input object not mutated; same fixture table as project-state D-052 rows. Launcher/hook-client tests that depend on it live under the same marker. If not adopted: delete subsection; D-052 behaviour is covered by `isUnimatrixProject` tests alone.
