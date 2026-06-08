# Component: config-transport-selection (`lib/hook-client/config.js`)

ADR-002 §3, ADR-007. FR-12, FR-13, FR-15, FR-16, AC-02. Risk R-05.
Merge step 3. Existing source: config.js (read in full).

## Purpose

Make `resolve(cwd)` return a transport `mode` ("http" | "uds") and, in UDS mode, a
derived `socketPath` from the SAME `walkToProjectRoot` + `computeProjectHash` already
used for `stateDir` — a single derivation so state dir and socket path can never
disagree (ADR-007 §1). The former terminal `{ok:false, reason:"missing"}` breadcrumb
path is retired: no remote config now means local UDS, not a failure.

## Unchanged (reused as-is — DO NOT reimplement)

- `walkToProjectRoot(startDir)` (config.js:42) — root walk incl. worktree
  `resolveGitFile` and symlink realpath. **Load-bearing for FR-15.**
- `resolveGitFile`, `realpathOrSelf`, `computeProjectHash` (sha256(root).hex[..16]).
- `stateDirFor(projectHash)` → `~/.unimatrix/{hash}/hook-client`.
- env-pair precedence (env wins outright; partial pair → `partial_env`).
- `mergeTimeouts`, `safeHostOf`, `nonEmpty`, `safeProcessCwd`.

## New: socket-path derivation — `socketPathFor(projectHash)` (ADR-007 §1)

```
FUNCTION socketPathFor(projectHash):
  TRY: home = os.homedir()
  CATCH: RETURN null
  IF NOT nonEmpty(home): RETURN null
  RETURN path.join(home, ".unimatrix", projectHash, "unimatrix.sock")
```

Note: this is the SAME `home` + `projectHash` root as `stateDirFor`. Invariant
(ADR-007 §1, R-05 s3): `dirname(socketPath) === dirname(dirname(stateDir))` — both
live under `~/.unimatrix/{projectHash}/`.

## Modified: `ok(...)` builder — add `mode`/`socketPath`

Extend the success result so HTTP and UDS share one shape (OVERVIEW shared type).
Two constructors keep it readable:

```
FUNCTION okHttp(url, token, timeouts, source, projectRoot, projectHash, stateDir):
  RETURN { ok:true, mode:"http", url, token, timeouts, source,
           projectRoot, projectHash, stateDir, urlHost: safeHostOf(url) }

FUNCTION okUds(socketPath, projectRoot, projectHash, stateDir):
  RETURN { ok:true, mode:"uds", socketPath, source:"local",
           projectRoot, projectHash, stateDir, urlHost: "" }   // urlHost "" (no remote host)
```

`urlHost:""` for UDS keeps `state.recordSendOutcomes`/breadcrumbs working unchanged
(they tolerate empty host).

## Modified: `resolve(cwd)` (config.js:202)

```
FUNCTION resolve(cwd):
  startDir = nonEmpty(cwd) ? cwd : safeProcessCwd()
  projectRoot = walkToProjectRoot(startDir)
  projectHash = computeProjectHash(projectRoot)
  stateDir    = stateDirFor(projectHash)

  // 1. env pair — HTTP wins outright (UNCHANGED, FR-13)
  envUrl = process.env[ENV_URL]; envTok = process.env[ENV_TOKEN]
  IF nonEmpty(envUrl) AND nonEmpty(envTok):
      RETURN okHttp(envUrl, envTok, mergeTimeouts(null), "env", projectRoot, projectHash, stateDir)
  IF nonEmpty(envUrl) OR nonEmpty(envTok):
      RETURN { ok:false, reason:"partial_env", projectRoot, projectHash, stateDir }  // TERMINAL (UNCHANGED)

  // 2. settings.local.json unimatrix.remote (single read, UNCHANGED parse)
  filePath = join(projectRoot, ".claude", "settings.local.json")
  TRY: parsed = JSON.parse(readFileSync(filePath, "utf8"))
  CATCH err:
      IF err.code === "ENOENT":
          // CHANGED (ADR-002 §3): file absent is NOT a misconfig → fall through to UDS.
          GOTO uds
      RETURN { ok:false, reason:"malformed", projectRoot, projectHash, stateDir }   // TERMINAL (UNCHANGED)

  remote = parsed?.unimatrix?.remote   // (same guarded access as today)
  IF remote is a valid object with nonEmpty url AND token:
      RETURN okHttp(remote.url, remote.token, mergeTimeouts(remote.timeouts), "file",
                    projectRoot, projectHash, stateDir)
  // remote key absent/incomplete: CHANGED — no longer reason:"missing"; fall through to UDS.

  // 3. UDS (CHANGED, FR-12): no remote config → local mode
  LABEL uds:
  socketPath = socketPathFor(projectHash)
  IF socketPath === null:
      // No HOME → cannot derive a socket path. Honest terminal misconfig (not "missing").
      RETURN { ok:false, reason:"malformed", projectRoot, projectHash, stateDir }
  RETURN okUds(socketPath, projectRoot, projectHash, stateDir)
```

### Decision precedence (FR-12, FR-13, ADR-002 §3)

```
env pair complete         → mode "http"  (wins even if a local socket is live; no probe)
env partial               → ok:false partial_env (terminal)
settings remote valid     → mode "http"
settings file malformed   → ok:false malformed (terminal)
settings ENOENT / no remote key → mode "uds"   (was reason:"missing", now retired)
no HOME (socketPath null) → ok:false malformed (honest terminal)
```

No local-override knob, no socket liveness probe (FR-13; F5 owns init UX). `partial_env`
and `malformed` stay terminal because they signal intent to use remote (ADR-002 §3).

## Exports

Add `socketPathFor` to module.exports (unit-testable); keep all existing exports.

## Error handling

Never throws (existing contract). All fs/os calls wrapped; failures degrade to a
result object, never an exception. ENOENT on settings is now a normal UDS path, not
an error.

## Key test scenarios (hints for tester)

1. Mode matrix: env pair → http; settings remote → http; ENOENT settings → uds;
   no-remote-key settings → uds; partial env → partial_env terminal; malformed JSON
   → malformed terminal; no HOME → malformed — AC-02.
2. UDS socketPath == `~/.unimatrix/{projectHash}/unimatrix.sock`; the `missing`
   reason is never returned — AC-02.
3. Single-derivation invariant: socketPath and stateDir share `{projectHash}` for
   every layout — R-05 s3.
4. TS-vs-Rust hash fixtures: plain repo, deep subdir, linked worktree, symlinked
   root, non-git dir all match the Rust daemon hash (`0d62f3bf1bf46a0a` in this
   workspace) — FR-15, AC-02, R-05 s1 (corpus lives in parity-corpus-uds.md).
5. Corrupt-worktree fixture pins the documented benign divergence exactly — R-05 s2.
6. HTTP-mode result shape unchanged from F3 (url/token/timeouts present) — AC-12.
