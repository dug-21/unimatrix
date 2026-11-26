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
