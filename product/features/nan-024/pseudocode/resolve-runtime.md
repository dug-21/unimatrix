# resolve-runtime (`lib/resolve-runtime.js`, `lib/resolve-binary.js`) — JS

## Purpose
The one resolver (ADR-002): client = redirect > install; binary (only when needed) = `UNIMATRIX_BINARY` > redirect > install. Replaces `resolve-binary.js` logic; `resolveBinary()` becomes a thin wrapper.

## resolve-runtime.js
```
class RuntimeResolutionError extends Error { code, source, path }
  codes: redirect_malformed, redirect_binary_invalid, redirect_client_invalid, env_binary_invalid,
         install_missing, install_invalid, client_invalid, binary_unresolvable, unsupported_platform_local

resolveRuntime({env, home, platform, transport, allowSelf=false}) -> {binary, clientRoot, binarySource, clientSource}
```
```
function resolveRuntime(p):
  redirect = readRedirect(p.home)             // absent -> null (falls through). Present but unparseable / non-object / wrong field types -> throw redirect_malformed (names path)
  // ---- client ----
  if redirect?.client != null:
     validateClientRoot(redirect.client, "redirect")      // abs path, dir, contains lib/hook-client/index.js, else throw redirect_client_invalid
     clientRoot = redirect.client; clientSource = "redirect"
  else:
     inst = readInstallJson(p.home)            // missing -> if allowSelf use self root; else throw install_missing
     validateClientRoot(inst.packageRoot, "install")      // throw install_invalid / client_invalid
     clientRoot = inst.packageRoot; clientSource = "install"
  // ---- binary ----
  needBinary = (p.transport == "local")        // pass-through callers pass transport:"local" explicitly (Linux only)
  if !needBinary or p.platform != "linux": return {binary:null, clientRoot, binarySource:null, clientSource}   // D-032: never resolve on macOS/Windows, never error
  if nz(env.UNIMATRIX_BINARY):  b = validateBinary(env.UNIMATRIX_BINARY,"env"); return {..., binary:b, binarySource:"env"}
  if redirect?.binary != null:  b = validateBinary(redirect.binary,"redirect"); return {..., "redirect"}
  b = installedBinary(clientRoot_or_self, p.platform)      // require.resolve("@dug-21/unimatrix-<plat>-<arch>/bin/unimatrix", {paths:[clientRoot]}); allowSelf: from this package
  if b == null: throw binary_unresolvable naming sources tried ["UNIMATRIX_BINARY","redirect","install"]
  return {..., "install"}
```
- `validateBinary(path, source)`: absolute, `fs.statSync(...).isFile()`, executable (`fs.accessSync(X_OK)` on POSIX), return `realpathSync`. Invalid -> throw (present-but-invalid never falls through, SR-20).
- `UNIMATRIX_BINARY` never affects client (D-024): the client branch does not read env.
- `allowSelf` (additive, accepted D-080; not in ADR-002's signature text, contract text is in ARCHITECTURE.md): used ONLY by init/wire/`resolveBinary()` running from an npx cache before any install exists; `clientRoot = path.resolve(__dirname,"..")`, `clientSource = "install"`. Never passed at fire time.
- Trust is not checked here (callers run `checkTrust` first; redirect path set is in `trustSet`).
- Platform-package binary at fire time (D-078): install-store copies the platform package into `pkg/<version>/node_modules/` (see install-store.md) so `require.resolve(..., {paths:[clientRoot]})` finds it. The resolver never looks at the original npx-cache location when `clientSource == "install"` and not `allowSelf`. With a `redirect.client`, the binary is still resolved from the install's `packageRoot` (not the redirected client root) unless `redirect.binary` is set. Key test: after the source package dir is removed, the install binary still resolves (D-078).

## resolve-binary.js (thin wrapper)
```
resolveBinary():
  rt = resolveRuntime({env: process.env, home: os.homedir(), platform: process.platform, transport: "local", allowSelf: true})
  return rt.binary      // null on macOS/Windows (no throw, R-15); throws RuntimeResolutionError (an Error) otherwise
```
`PLATFORMS` export retained. Error message text for env-missing keeps substring "UNIMATRIX_BINARY points to non-existent file" so existing tests update minimally (update tests, do not delete).

## Error handling
Every failure is `RuntimeResolutionError{code, source, path}`; message always names source and path. Callers: hook route / mcp-stdio print message, exit 1.

## Key test scenarios (AC-14 matrix)
{env only, redirect only, env+redirect, none, invalid redirect (target missing, not executable, client lacks entry), malformed redirect JSON} x {hook, mcp-stdio, pass-through}; assert binary and client independently (env: binary from env, client from install; both: binary env, client redirect; redirect with only `binary`: client from install). macOS/Windows: binary null, no error, `UNIMATRIX_BINARY` ignored. Removing redirect reverts to install. Unresolvable binary names sources tried.
