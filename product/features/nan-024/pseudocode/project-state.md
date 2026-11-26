# project-state (`lib/project-state.js`) — new, JS

## Purpose
Single definition of "Unimatrix project here" (D-045 as amended by D-052), `isInstalled`, transport, and the env-tier sanitizer. Exported copy; `shim-entry.js` carries an inlined copy of `isUnimatrixProject` + minimal installed decision (parity-tested, R-28). Used by launcher hook/mcp-stdio/status, hook-client config, mcp-bridge, wire-init (marker write), pre-Node reference semantics.

## Exports
```
isUnimatrixProject(cwd, env, home) -> Project | null
isInstalled(project, platform, {env, home}) -> {installed, transport, reason}     // {env, home} accepted (D-080)
headlessEnvSet(env) -> boolean
headlessRootContains(env, cwd) -> boolean
effectiveEnv(env, cwd, home) -> env             // PENDING H1 (isolated, see section below); not part of the architecture contract until decided
storeDir(home, hash) -> join(home,".unimatrix",hash)
HEADLESS_VARS  // ["UNIMATRIX_BUNDLE", ...all values of remote-resolve.VARS except HEADERS?]  -- built from remote-resolve.VARS; HEADERS alone does not make headless
```
Imports `walkToProjectRoot`, `computeProjectHash` from `./hook-client/config.js` (same hash semantics as Rust). No other package code.

## Pseudocode
```
function headlessEnvSet(env):
  nz = v => typeof v == "string" && v.trim() != ""
  return nz(env.UNIMATRIX_BUNDLE) or any key starting "UNIMATRIX_REMOTE_" (excluding _HEADERS) with nz value

function headlessRootContains(env, cwd):
  r = env.UNIMATRIX_PROJECT_ROOT
  if !nz(r) or !path.isAbsolute(r): return false              // empty/relative never means "/" (R-34)
  rootReal = realpathOrNull(r); cwdReal = realpathOrNull(cwd)
  if rootReal==null or cwdReal==null: return false
  rel = path.relative(rootReal, cwdReal)                       // win32 relative is case-insensitive
  return rel == "" or (rel != ".." and !rel.startsWith(".."+path.sep) and !path.isAbsolute(rel))   // component match, never string prefix

function isUnimatrixProject(cwd, env, home):
  root = walkToProjectRoot(cwd); hash = computeProjectHash(root)
  sd = join(home,".unimatrix",hash)
  if headlessEnvSet(env) and headlessRootContains(env, cwd): return {hash, kind:"headless", root}
  if isFile(join(sd,"remote.json")): return {hash, kind:"cloud", root}
  if isDir(sd): return {hash, kind:"local", root}
  return null
```
Precedence headless > cloud > local mirrors transport precedence (credential/headless -> cloud). Config-file location is irrelevant (D-044).

```
function isInstalled(project, platform, {env, home}):
  cred = headless-valid(project.kind=="headless") or isFile(storeDir/remote.json)
  hasBundle = project.kind=="headless" && nz(env.UNIMATRIX_BUNDLE)       // bundle honoured only under D-052
  if platform != "linux":                                               // D-032 verbatim
    ok = hasBundle or isFile(storeDir/remote.json)
    return ok ? {installed:true, transport:"cloud"} : {installed:false, transport:null, reason:"missing credential"}
  // linux
  inst = readValidInstallJson(home)    // JSON parse; fields version/packageRoot/node strings; packageRoot exists
  if cred: return {installed:true, transport:"cloud"}
  if inst: return {installed:true, transport:"local"}                    // unresolvable binary surfaces later as Broken (resolveRuntime)
  return {installed:false, transport:null, reason:"no install.json"}
```
```
### effectiveEnv — PENDING H1 (isolated; removable without ripple)
Status: pseudocode extension, NOT in the ARCHITECTURE.md contract (D-080 left it to a human decision, H1). Kept so a decision to keep it needs no redesign; marked so a decision to drop it is a pure deletion.
Single definition: this function (and its export). Single call sites (complete list):
  1. `lib/hook-client/remote-resolve.js` `resolveRemote` first line (ci-and-docs Part B)
  2. `lib/hook-client/config.js` `resolve` (hook-client.md section 3) — passes the effective env on
  3. `lib/hook-route.js` `run` (resolveRuntime env arg) and `runMcpStdio` (bridge env arg) (launcher.md)
Removal recipe if H1 = drop: delete this function and its export; at each of the three sites replace `effectiveEnv(env, cwd, home)` with `env`. Nothing else changes: `isUnimatrixProject` (D-052 containment) still gates every entry point, and the mcp-bridge gate in ci-and-docs Part B still exits 0 for headless env outside the root. What is lost on removal: with a store credential in a different initialised repo AND exported bundle env AND cwd outside `UNIMATRIX_PROJECT_ROOT`, the exported env would win over the store credential for that repo.
```
function effectiveEnv(env, cwd, home):
  if !headlessEnvSet(env) or headlessRootContains(env, cwd): return env
  copy = {...env}; delete UNIMATRIX_BUNDLE and every UNIMATRIX_REMOTE_* ; return copy    // outside root: env tier does not exist
```
This closes the leak where bundle env would otherwise win over a store credential in a different initialised repo. (PENDING H1; see above.)

## Error handling
Never throws; filesystem errors map to "absent". `isInstalled` with unreadable `install.json` = not installed (reason "no install.json"); the launcher's trust check has already refused an unsafe one.

## Parity
`shim-entry.js` inline copy must equal this module over the shared fixture table (root cwd, subdir, symlinked cwd, trailing slash, nested repos, worktree `.git` file, Windows drive-letter case and `\` vs `/`, store dir / remote.json / headless env with root containing / not containing / unset / empty / relative / symlinked root / none) and hash must equal `config.js` and the Rust store dir name.

## Key test scenarios
D-045/D-052 table above; sibling shared-prefix cwd (`/fx/repoA-other` vs root `/fx/repoA`) is outside; [only if H1 keeps it] `effectiveEnv` strips env outside the root while keeping `UNIMATRIX_BINARY`, `UNIMATRIX_PROJECT_ROOT`; macOS/Windows installed table (no bundle, no credential => not installed, reason "missing credential"); Linux: valid install.json => local, credential => cloud, neither => reason "no install.json".
