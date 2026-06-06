# config.js — Config Resolution, Project Root, Hash, State Dir

## Purpose
Spawn-time resolution of remote URL/token per ADR-006 (env > single root-anchored file),
project-root walk (port of the `detectProjectRoot` walk, non-throwing variant), project
hash (`project.rs::compute_project_hash` algorithm), and state-dir path (ADR-003). The
SAME `projectRoot` string feeds both config lookup and the state-dir hash — config
identity and state identity can never disagree.

## Constants
```
ENV_URL   = "UNIMATRIX_REMOTE_URL"      // pinned at gate (Delivery Notes 4)
ENV_TOKEN = "UNIMATRIX_REMOTE_TOKEN"
DEFAULT_TIMEOUTS = { connectMs: 750, syncMs: 2000, fnfMs: 3000 }   // ADR-005
```

## Functions

### resolve(cwd) -> ResolvedConfig
```
function resolve(cwd):
  projectRoot = walkToProjectRoot(cwd)           // never throws; falls back to cwd
  projectHash = computeProjectHash(projectRoot)
  stateDir    = path.join(os.homedir(), ".unimatrix", projectHash, "hook-client")
                // os.homedir() throw/empty (no HOME) → stateDir = null; state.js treats
                // null stateDir as "all persistence disabled, sends still attempted"

  envUrl = process.env[ENV_URL]; envTok = process.env[ENV_TOKEN]
  if bothNonEmpty(envUrl, envTok):
    return ok(envUrl, envTok, DEFAULT_TIMEOUTS, "env", ...)   // env wins outright
  if exactlyOneNonEmpty(envUrl, envTok):
    return { ok:false, reason:"partial_env", projectRoot, projectHash, stateDir }
    // ADR-006: misconfiguration, breadcrumb class "auth", exit 0

  // single file, single read — no probing (ADR-006)
  filePath = path.join(projectRoot, ".claude", "settings.local.json")
  try: parsed = JSON.parse(fs.readFileSync(filePath, "utf8"))
  catch (ENOENT)        -> return { ok:false, reason:"missing", ... }
  catch (anything else) -> return { ok:false, reason:"malformed", ... }

  remote = parsed?.unimatrix?.remote
  if remote is not object or remote.url/remote.token not non-empty strings:
    return { ok:false, reason:"missing", ... }     // file present, key absent → same as missing

  timeouts = mergeTimeouts(remote.timeouts)        // see below
  return ok(remote.url, remote.token, timeouts, "file", ...)

function ok(url, token, timeouts, source, projectRoot, projectHash, stateDir):
  urlHost = safeHostOf(url)                        // new URL(url).host; parse failure → ""
  return { ok:true, url, token, timeouts, source, projectRoot, projectHash, stateDir, urlHost }
  // NOTE: an unparseable URL is NOT rejected here — transport.post classifies it as
  // "connect" failure at send time (fail-open; init is the loud validation point)
```

### mergeTimeouts(t) -> {connectMs, syncMs, fnfMs}
Config-overridable per ADR-005. Key shape (delivery-pinned here, no ADR names the keys —
flagged in OVERVIEW open questions):
```
{"unimatrix":{"remote":{"url","token","timeouts":{"connect_ms","sync_ms","fnf_ms"}}}}

function mergeTimeouts(t):
  out = clone(DEFAULT_TIMEOUTS)
  if t is object:
    for [src, dst] of [["connect_ms","connectMs"],["sync_ms","syncMs"],["fnf_ms","fnfMs"]]:
      if t[src] is a finite number and 1 <= t[src] <= 600_000: out[dst] = floor(t[src])
  return out                                      // invalid values silently ignored (fail-open)
```

### walkToProjectRoot(startDir) -> string
Non-throwing port of the init.js walk / Rust `detect_project_root(Some(cwd))`:
```
function walkToProjectRoot(startDir):
  try: current = path.resolve(startDir)
  catch: return startDir
  loop:
    if fs.existsSync(path.join(current, ".git")): return current   // dir OR file (worktree)
    parent = path.dirname(current)
    if parent === current: return path.resolve(startDir)           // no .git → resolved cwd (ADR-006)
    current = parent
// Documented divergence: Rust resolves `.git` worktree FILES to the real gitdir; this
// walk stops at the directory containing `.git`. The hash is consumed only by THIS
// client (state-dir identity), so internal consistency is what matters. Worktree users
// get a per-worktree state dir — accepted.
```

### computeProjectHash(projectRoot) -> string
Identical algorithm to `project.rs::compute_project_hash` (first 16 hex of SHA-256 of the
path string):
```
function computeProjectHash(projectRoot):
  return crypto.createHash("sha256").update(projectRoot, "utf8").digest("hex").slice(0, 16)
// Input is the path string exactly as produced by walkToProjectRoot (platform-native
// separators). Same-root spawns hash identically regardless of subdirectory cwd.
```

## Error Handling
- This module NEVER throws to callers; every fs/env access is wrapped.
- All failure shapes return `{ ok:false, reason }` so index.js maps reason → breadcrumb
  class (`partial_env` → `auth`; others → `connect`-family "config" stderr line).
- No network I/O here, ever.

## Key Test Scenarios (FR-06 matrix / R-09)
1. Spawn from subdirectory cwd; stdin `cwd` ≠ `process.cwd()`; stdin `cwd` empty.
2. Missing file; file without `unimatrix.remote`; malformed JSON → `{ok:false}`, no throw.
3. Env pair beats a present file; exactly one env var → `partial_env`.
4. Nested-`.git` monorepo → nearest root; assert config path and `projectHash` derive
   from the SAME string (split-brain assertion).
5. Timeout overrides applied; junk override values ignored.
6. No HOME → `stateDir = null`, resolution still returns, sends still possible.
7. Windows: backslash roots hash deterministically; `.git` file (worktree) accepted.
