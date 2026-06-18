# C5 — `lib/hook-client/config.js` `resolve()` + `okHttp` (MODIFIED, Scope B)

**Purpose.** Repoint the file-mode branch of `resolve()` from in-tree
`<root>/.claude/settings.local.json` to the out-of-tree store
`~/.unimatrix/<projectHash>/remote.json` (via `credstore.read`); read the canonical schema
(`observe_url`+`fingerprint`, not `url`); add `pinnedFp` to `okHttp` so file-mode remote observe runs
over PINNED HTTPS. This fixes a CURRENT break (file-mode remote observe reads a never-written `url`
key, falls through to UDS, and would run unpinned). Sources: ADR-004 (schema+reconcile), R-06, R-13,
R-15. New import: `const credstore = require("./credstore.js");`.

> CRITICAL: this is a break TODAY (ADR-004 / R-06), not a latent risk. A faithful port of the old
> read keeps the break and is a GATE FAILURE. Validation is BEHAVIORAL (the observe POST actually
> transits pinned HTTPS), not field-presence (AC-08d).

## Change 1 — `okHttp` gains a `pinnedFp` field (`config.js:203`)

```
function okHttp(url, token, timeouts, source, projectRoot, projectHash, stateDir, pinnedFp):
    return {
        ok: true, mode: "http", url, token, timeouts, source,
        projectRoot, projectHash, stateDir, urlHost: safeHostOf(url),
        pinnedFp: pinnedFp || null,          // NEW — threaded to transport.post (config.pinnedFp, R-06)
    }
```
- `transport-http.js:117` already reads `config.pinnedFp` to gate the pinned flush; today it is never
  set on the file path (the bug). Adding it here is the minimal fix that makes the observe path pin.
- Existing callers (env path, UDS via `okUds`) are unaffected: env passes `pinnedFp` absent/null
  (env stays unpinned by design — ADR-004 precedence note); `okUds` is unchanged.
- Backward-compat: append `pinnedFp` as the LAST positional arg so the env-path call
  (`okHttp(envUrl, envTok, …, stateDir)`) keeps working with `pinnedFp` undefined → coerced to null.

## Change 2 — repoint the file-mode branch of `resolve()` (`config.js:275-306`)

Precedence order is PRESERVED EXACTLY (ADR-004 §reader-repointing): env pair → store file → UDS.
Only the middle branch changes (source + schema + pin).

```
function resolve(cwd):
    startDir = nonEmpty(cwd) ? cwd : safeProcessCwd()
    projectRoot = walkToProjectRoot(startDir)            // unchanged read-side root (config.js:44)
    projectHash = computeProjectHash(projectRoot)        // SAME oracle as the writer (R-07)
    stateDir = stateDirFor(projectHash)

    // 1. ENV PAIR -> http, wins outright (UNCHANGED, config.js:263-273). Env stays UNPINNED.
    if nonEmpty(envUrl) and nonEmpty(envTok):
        return okHttp(envUrl, envTok, mergeTimeouts(null), "env", projectRoot, projectHash, stateDir, null)
    if nonEmpty(envUrl) or nonEmpty(envTok):
        return { ok:false, reason:"partial_env", projectRoot, projectHash, stateDir }     // UNCHANGED

    // 2. STORE FILE (CHANGED): was <root>/.claude/settings.local.json key unimatrix.remote.url.
    //    Now: out-of-tree ~/.unimatrix/<projectHash>/remote.json via credstore.read (ADR-004).
    let cred
    try:
        cred = credstore.read(projectHash)               // null on ENOENT/no-home; THROWS on malformed/unknown ver
    catch e:
        // Parse failure / unknown schema_version signals intent to use remote -> TERMINAL (R-13).
        // (Same posture the old code gave on non-ENOENT JSON parse failure: reason "malformed".)
        return { ok:false, reason:"malformed", projectRoot, projectHash, stateDir }
    if cred === null:
        // ENOENT (or no home): NOT a misconfig -> fall through to local UDS (UNCHANGED semantics).
        return resolveUds(projectRoot, projectHash, stateDir)

    // Read the CANONICAL schema: observe_url (NOT url) as post target; fingerprint -> pinnedFp.
    observeUrl = cred.observe_url
    token = cred.token
    if not nonEmpty(observeUrl) or not nonEmpty(token):
        // Incomplete entry (missing observe_url/token) -> UDS fall-through (UNCHANGED "incomplete" semantics, R-13).
        return resolveUds(projectRoot, projectHash, stateDir)

    timeouts = mergeTimeouts(cred.timeouts)              // absent -> DEFAULT_TIMEOUTS (config.js:135)
    pinnedFp = nonEmpty(cred.fingerprint) ? cred.fingerprint : null   // null on legacy -> UNPINNED (R-15)
    return okHttp(observeUrl, token, timeouts, "file", projectRoot, projectHash, stateDir, pinnedFp)

    // 3. (handled above by resolveUds calls) UDS fall-through — UNCHANGED.
```

Key reconciliation points (ADR-004 / R-06):
- **`observe_url` replaces `url`.** The old guard required `remote.url` (never written) → silent UDS
  fall-through. Now the post target is `observe_url` and the path resolves (R-06 §no-UDS-fallthrough).
- **`fingerprint` newly read** → `pinnedFp` → `transport.post` pins the observe POST (R-06 §pinned).
  `fingerprint: null` (legacy) → `pinnedFp: null` → unpinned, preserving today's legacy behavior
  (R-15; NOT a pin-or-fail on null).
- **No `url` key anywhere.** Assert the hook client never reads `url` (R-06 §old-key-absence).
- **Posture parity (R-13):** `credstore.read` throwing (malformed/unknown version) maps to terminal
  `malformed`; `null` (ENOENT) and incomplete entry map to UDS fall-through — exactly the old
  ENOENT-vs-parse-error split, now sourced from `credstore`.

## Exports

No new export required for `resolve`/`okHttp` (internal). `credstore` is imported. Keep the existing
`module.exports` set; no signature change is observable to existing importers of `resolve`.

## Data flow

IN: `cwd` (from hook stdin). Derives `projectHash` (same oracle as C4 writer) → `credstore.read`.
OUT: `okHttp(observe_url, token, timeouts, "file", …, pinnedFp=fingerprint)` → `transport.post`
posts to `observe_url` over pinned HTTPS (good-pin) or unpinned (legacy null). UDS fall-through on
ENOENT/incomplete; terminal `malformed` on parse/unknown-version.

## Error handling (R-06, R-13, R-15)

| Condition | Posture (hook client = FAIL-OPEN) |
|-----------|-----------------------------------|
| store ENOENT / no home (read→null) | UDS fall-through (resolveUds) — unchanged |
| store malformed JSON / unknown schema_version (read throws) | terminal `malformed` |
| incomplete entry (no observe_url/token) | UDS fall-through |
| `fingerprint` present | `pinnedFp` set → observe POST pinned (good-pin delivers; wrong-pin connect-class fail → exit 0, breadcrumb) |
| `fingerprint` null (legacy) | `pinnedFp` null → observe POST unpinned (preserves legacy) |

## Key test scenarios (hints; full plan in test-plan/config-resolve.md)

- **AC-08c/R-06 regression**: seed `remote.json` (canonical schema) → `resolve()` returns
  `url === observe_url` (post target) AND `pinnedFp` populated from `fingerprint`. The load-bearing
  regression assertion.
- **AC-08d/R-06 BEHAVIORAL (break-fix proof)**: local pinned `https.createServer` self-signed leaf;
  seed store with its `observe_url`+`fingerprint`+token; drive a hook event through file-mode
  `resolve()` → POST lands on the HTTPS server (NOT UDS); good-pin delivers; wrong-pin connect-class
  fail with token never on the wire; `config.pinnedFp` populated. `pinnedFp`-set alone is NOT
  sufficient — must prove the request transits pinned HTTPS (the vnc-034 dead-pin lesson).
- **R-06 no-UDS-fallthrough**: valid file-mode credential present → observe does NOT silently fall
  through to UDS (the current break); it targets `observe_url`.
- **R-06 old-key-absence**: hook client never reads `url`; store has no `url` key.
- **R-06 both-consumers-one-schema**: bridge reads `mcp_url`/token/`fingerprint`; hook client reads
  `observe_url`/token/`fingerprint`/`timeouts` from the SAME file — no per-consumer dialect.
- **R-15**: `fingerprint: null` legacy entry → resolves with `pinnedFp` unset → unpinned observe; a
  bundle entry (`fingerprint` present) → `pinnedFp` set → pinned. No crash on null.
- **R-13**: malformed JSON / unknown `schema_version` → terminal `malformed`; ENOENT → UDS
  fall-through.
- **Env precedence (regression)**: env pair still wins outright, unpinned, before the store is read.
- **R-07 round-trip**: write store for project P (C4) → C5 reads it back keyed by P's `projectHash`.
