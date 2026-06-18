# C1 — `lib/hook-client/credstore.js` (NEW, Scope B)

**Purpose.** Sole owner of the out-of-tree per-`projectHash` credential store
`~/.unimatrix/<projectHash>/remote.json` (mode 0600). Owns the canonical path derivation, the one
schema, read, and idempotent merge-write. No other module hand-rolls store access — both consumers
(C2 bridge, C5 hook client) and the writer (C4 init) go through this module (ADR-004 §end).

Builds first (OVERVIEW dependency graph). Pure Node stdlib only: `fs`, `path`, `os`, `crypto` (no
new deps — AC-02). Sources: ADR-003 (keying+path), ADR-004 (schema+reconcile).

## Module-level constants

```
STORE_SCHEMA_VERSION = 1          // single source of truth; reader + writer share it (R-13)
STORE_FILENAME       = "remote.json"
STORE_MODE           = 0o600
```

## Function: pathFor(projectHash) -> string | null

Mirrors `config.js:socketPathFor` posture exactly (ADR-003): same home-derivation, same null-on-no-home.

```
function pathFor(projectHash):
    try home = os.homedir()
    catch -> return null
    if home is not a non-empty string -> return null
    return path.join(home, ".unimatrix", projectHash, STORE_FILENAME)
```

Notes:
- NO user-supplied path segment — `projectHash` is a 16-hex SHA-256 (no traversal surface, Security
  Risks §). Do not sanitize/normalize the hash; it is a fixed-grammar derived value.
- Colocated with `unimatrix.sock` + `hook-client/` under the SAME `~/.unimatrix/<projectHash>/`
  root (ADR-003). Not XDG, not a separate `credentials.json`, not a global map.

## Function: read(projectHash) -> object | null

Returns the parsed canonical schema object, or `null` on ENOENT (no credential for this project).
THROWS a token-free Error on malformed JSON or unknown `schema_version` (R-13). Callers map the
posture: bridge fail-loud, hook client terminal `malformed` / UDS fall-through (see consumer files).

```
function read(projectHash):
    p = pathFor(projectHash)
    if p === null -> return null            // no homedir: same as ENOENT posture (R-13 §ENOENT)

    raw = fs.readFileSync(p, "utf8")        // may throw fs error
      on error e:
        if e.code === "ENOENT" -> return null     // absent: caller decides fall-through vs loud
        else                   -> throw token-free Error("credential store unreadable at <p>: " + e.code)

    parsed = JSON.parse(raw)                // may throw SyntaxError
      on error -> throw token-free Error("credential store malformed (invalid JSON) at <p>")

    if parsed is not a plain object -> throw token-free Error("credential store malformed at <p>")

    // schema_version gate — unknown version is TERMINAL, never a silent skip (ADR-004)
    if parsed.schema_version !== STORE_SCHEMA_VERSION:
        throw token-free Error(
          "credential store schema_version " + parsed.schema_version +
          " unsupported (this client supports " + STORE_SCHEMA_VERSION + "); re-run init")

    return parsed     // {schema_version, mcp_url, observe_url, token, fingerprint, timeouts?}
```

Contract notes:
- `read` does NOT validate field completeness beyond `schema_version` — each consumer validates the
  fields IT owns (bridge needs `mcp_url`+`token`+`fingerprint`; hook client needs
  `observe_url`+`token`, `fingerprint` may be `null` for legacy). Keeping field-completeness in the
  consumer preserves the ENOENT-vs-incomplete distinction the hook client relies on for UDS
  fall-through (R-13 §incomplete).
- Error messages MUST NOT contain `token` or `fingerprint` values (NFR-06 / R-09). The store path is
  fine to include (it carries no secret).

## Function: write(projectHash, cred, { dryRun }) -> string[]

Idempotent merge-write of the canonical schema at mode 0600. `cred` =
`{ mcp_url, observe_url, token, fingerprint, timeouts? }` (`fingerprint` may be `null` for legacy).
Returns an actions array (mirrors `init.js` action-string convention). THROWS on write failure
(creds must persist — init exit 1, R-12).

```
function write(projectHash, cred, opts):
    dryRun = opts?.dryRun === true
    actions = []
    p = pathFor(projectHash)
    if p === null:
        throw Error("cannot resolve credential store path (no home directory)")   // loud (R-12)

    // Idempotent merge: read existing (tolerate absent/malformed by starting fresh on a NEW write,
    // but do NOT silently discard a readable existing entry's unknown fields).
    existing = {}
    try:
        existing = read(projectHash) || {}        // null on ENOENT -> {}
    catch:
        existing = {}    // malformed existing file: a write replaces it with a valid one (recovery)

    merged = Object.assign({}, existing, {
        schema_version: STORE_SCHEMA_VERSION,     // ALWAYS stamp current version
        mcp_url:     cred.mcp_url,
        observe_url: cred.observe_url,
        token:       cred.token,
        fingerprint: (cred.fingerprint === undefined ? null : cred.fingerprint),  // null tolerated (legacy)
    })
    if cred.timeouts is a non-null object:
        merged.timeouts = cred.timeouts
    // else: leave merged.timeouts as-is (absent => consumer DEFAULT_TIMEOUTS, ADR-004)

    if dryRun:
        actions.push("[dry-run] Would write credential store " + p + " (mode 0600)")
        return actions

    // Write with 0600 then re-assert mode (the writeRemoteSettingsLocal pattern, init.js:242-253)
    fs.mkdirSync(path.dirname(p), { recursive: true })
    fs.writeFileSync(p, JSON.stringify(merged, null, 2) + "\n", { mode: STORE_MODE })
    try: fs.chmodSync(p, STORE_MODE)              // re-assert (pre-existing looser perms); best-effort
    catch: /* no-op on Windows/unsupported fs; must not abort */

    actions.push("Wrote credential store " + p + " (mode 0600)")
    return actions
```

Write notes:
- **Idempotent re-init** is a single-file rewrite (ADR-003): re-`init` for the same project
  overwrites `remote.json` in-place; two projects produce two hash directories — no cross-project
  map merge (AC-08b satisfied by directory separation).
- **Merge-preserving across versions only** — unknown future fields in an existing entry survive via
  `Object.assign(existing, …)`. The four canonical content fields are always overwritten.
- Action strings carry the path but NEVER `token`/`fingerprint` (NFR-06).
- `mode: 0o600` on `writeFileSync` is applied subject to umask; the explicit `chmodSync` re-assert
  guarantees 0600 even if the file pre-existed (NFR-04).

## Module exports

```
module.exports = { pathFor, read, write, STORE_SCHEMA_VERSION }
```

## Data flow

- IN (write): `projectHash` (from C4 via `computeProjectHash`), `cred` (from C4's `decodeBundle` /
  legacy resolve). OUT: file at `pathFor(projectHash)`, mode 0600.
- IN (read): `projectHash` (C2 from `argv[2]`; C5 from `computeProjectHash(walkToProjectRoot())`).
  OUT: canonical object | null | throw.

## Error handling (R-12, R-13, NFR-06)

| Condition | Behavior |
|-----------|----------|
| read ENOENT (or no homedir) | return `null` (caller decides) |
| read malformed JSON / unknown schema_version | throw token-free Error |
| write no homedir | throw (loud; init exit 1) |
| write fs failure | propagate throw (init exit 1; no token-bearing partial in tree — R-12) |
| chmod failure | swallow (best-effort; Windows) |

## Key test scenarios (hints; full plan in test-plan/credstore.md)

- `pathFor` returns `~/.unimatrix/<hash>/remote.json`; null when `os.homedir()` empty/throws (R-13).
- `write` then `read` round-trips the canonical schema by the SAME `projectHash` (R-07 round-trip).
- `write` creates file at mode `0600`; `stat` confirms (NFR-04, AC-08).
- Re-`write` for the same hash overwrites idempotently; a different hash writes a separate dir (AC-08b).
- Existing entry with an extra/unknown future field: that field survives a re-write (merge-preserve).
- `read` of malformed JSON → throws; `read` of `{schema_version: 99,…}` → throws (R-13).
- `read` ENOENT → `null` (R-13 §ENOENT).
- No token/fingerprint string in any thrown message or action string (NFR-06, R-09).
- `write` with `fingerprint: null` (legacy) persists `fingerprint: null`, not omitted (R-15).
- `write` with no `timeouts` omits the key; with `timeouts` persists it (ADR-004).
