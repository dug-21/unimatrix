# C9 Remote Resolver — `lib/hook-client/remote-resolve.js` (new)

ADR-006 (#5818), ADR-007 (#5817), FR-19..24, FR-28..30, FR-40, HD 3b/7.2/8. Depends on C8 (`decodeBundle`), `credstore.read`. Node stdlib only. Target < 250 lines.

## Purpose
Single credential resolution for both the MCP bridge and the observe path: bundle > env pair > credential store, per consumer; explicit conflict list; empty = unset; strict extra-header parsing. Never logs or embeds a token/header value in any error.

## Exports
```
module.exports = { resolveRemote, parseExtraHeaders, ResolveError, RESERVED_HEADERS, VARS }
VARS = { BUNDLE: "UNIMATRIX_BUNDLE", MCP_URL: "UNIMATRIX_REMOTE_MCP_URL", URL: "UNIMATRIX_REMOTE_URL",
         TOKEN: "UNIMATRIX_REMOTE_TOKEN", FP: "UNIMATRIX_REMOTE_FINGERPRINT", HEADERS: "UNIMATRIX_REMOTE_HEADERS" }
const BUNDLE_CONFLICTS = [VARS.MCP_URL, VARS.URL, VARS.TOKEN, VARS.FP]      // FR-20 exact list
const FP_RE = /^sha256:[0-9a-f]{64}$/                                        // vnc-034 ADR-002
RESERVED_HEADERS = new Set(["authorization","mcp-session-id","host","content-length","content-type","transfer-encoding","connection","accept"])
const NAME_RE = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/
const MAX_HEADERS = 32, MAX_VALUE_BYTES = 8192

class ResolveError extends Error {
  constructor(code, message, varName) { super(message); this.name = "ResolveError"; this.code = code; this.varName = varName || null; }
}
// codes: "conflict" | "partial_pair" | "fp_without_url" | "bad_fingerprint" | "bad_bundle" | "bad_headers" | "store_malformed"
```

## Helpers
```
function nz(v) { return typeof v === "string" && v.length > 0 }      // FR-40: empty = unset; NO trim
function absence(env, name) { return env[name] === "" ? "is empty" : "is not set" }
```

## `resolveRemote(env, { projectHash, readStore })`
`readStore` defaults to `credstore.read` (injectable for tests). Returns `{ mcp, observe, headers, source }`; `source` is `"bundle" | "env" | "store" | "mixed" | "none"` for breadcrumbs only.
```
headers = parseExtraHeaders(env[VARS.HEADERS])             // throws bad_headers; applies to all tiers

// Tier 1 — bundle
if nz(env[VARS.BUNDLE]):
  for name of BUNDLE_CONFLICTS:
    if nz(env[name]): throw ResolveError("conflict", `${VARS.BUNDLE} cannot be combined with ${name}; unset one of them`, name)
  let b
  try { b = decodeBundle(env[VARS.BUNDLE]) } catch (e) { throw ResolveError("bad_bundle", `${VARS.BUNDLE} is invalid: ${e.message}`, VARS.BUNDLE) }  // BundleError messages are token-free
  return { mcp: {url: b.mcp_url, token: b.token, pinnedFp: b.fp},
           observe: {url: b.observe_url, token: b.token, pinnedFp: b.fp}, headers, source: "bundle" }

// Tier 2 — env pairs
mcpUrl = env[VARS.MCP_URL], obsUrl = env[VARS.URL], tok = env[VARS.TOKEN], fpRaw = env[VARS.FP]
if nz(fpRaw) && !FP_RE.test(fpRaw): throw ResolveError("bad_fingerprint", `${VARS.FP} must be sha256:<64 lowercase hex>`, VARS.FP)
if nz(fpRaw) && !nz(mcpUrl) && !nz(obsUrl): throw ResolveError("fp_without_url", `${VARS.FP} is set but neither ${VARS.MCP_URL} nor ${VARS.URL} is set`, VARS.FP)
if nz(tok) && !nz(mcpUrl) && !nz(obsUrl):
  throw ResolveError("partial_pair", `${VARS.URL} ${absence(env, VARS.URL)}; ${VARS.TOKEN} is set`, VARS.URL)
for urlVar of [VARS.MCP_URL, VARS.URL]:
  if nz(env[urlVar]) && !nz(tok):
    throw ResolveError("partial_pair", `${VARS.TOKEN} ${absence(env, VARS.TOKEN)}; ${urlVar} is set`, VARS.TOKEN)
pinnedFp = nz(fpRaw) ? fpRaw : null                         // absent fp = CA trust
mcp     = nz(mcpUrl) ? {url: mcpUrl, token: tok, pinnedFp} : null     // MCP env tier only when _MCP_URL set
observe = nz(obsUrl) ? {url: obsUrl, token: tok, pinnedFp} : null     // unchanged observe meaning

// Tier 3 — credential store, per consumer still null (lazy, at most one read)
if (mcp === null || observe === null) && nz(projectHash):
  let cred
  try { cred = readStore(projectHash) } catch (e) { throw ResolveError("store_malformed", `credential store is unreadable: ${e.message}`, null) }  // credstore messages are token-free
  if cred !== null:
    fp = nz(cred.fingerprint) ? cred.fingerprint : null     // stored null now = CA trust (was refusal in bridge)
    if mcp === null && nz(cred.mcp_url) && nz(cred.token): mcp = {url: cred.mcp_url, token: cred.token, pinnedFp: fp}
    if observe === null && nz(cred.observe_url) && nz(cred.token): observe = {url: cred.observe_url, token: cred.token, pinnedFp: fp, timeouts: cred.timeouts || null}
source = derive from which tiers supplied mcp/observe
return { mcp, observe, headers, source }
```
Shape extension (flagged): each non-null `mcp`/`observe` tuple carries `tier: "bundle"|"env"|"store"` (set where the tuple is built), and `observe` carries an extra `timeouts` field (store tier: `cred.timeouts`; bundle/env: `null`) so `config.js` keeps applying stored timeouts via `mergeTimeouts` exactly as today. Architecture's tuple `{url, token, pinnedFp}` is otherwise unchanged.
Consumers decide what `null` means: bridge → "no credential" exit 1; observe → UDS fall-through (today's behavior).
Pinned-fp format for stored credentials is validated downstream exactly as today (cert-pin.js), unchanged (R-11).

## `parseExtraHeaders(raw) -> Array<[string, string]>`
```
if !nz(raw): return []                                     // unset or empty = no headers (FR-40)
let obj
try { obj = JSON.parse(raw) } catch { throw bad("UNIMATRIX_REMOTE_HEADERS is not valid JSON") }
if obj === null || typeof obj !== "object" || Array.isArray(obj): throw bad("… must be a JSON object")
entries = Object.entries(obj)
if entries.length > MAX_HEADERS: throw bad(`… has more than ${MAX_HEADERS} entries`)
seen = new Set()
out = []
for [name, value] of entries:
  if !NAME_RE.test(name): throw bad(`header name ${safeName(name)} is not a valid token`)
  lc = name.toLowerCase()
  if RESERVED_HEADERS.has(lc): throw bad(`header ${name} is reserved and cannot be overridden`)
  if seen.has(lc): throw bad(`header ${name} is duplicated (case-insensitive)`)
  seen.add(lc)
  if typeof value !== "string": throw bad(`header ${name} value must be a string`)
  if /[\r\n\0]/.test(value): throw bad(`header ${name} value contains CR, LF or NUL`)
  if /[^\t\x20-\x7e]/.test(value): throw bad(`header ${name} value must be printable ASCII (HTAB, 0x20-0x7E)`)
  if Buffer.byteLength(value, "utf8") > MAX_VALUE_BYTES: throw bad(`header ${name} value exceeds 8 KiB`)
  out.push([name, value])
return out
// bad(msg) = new ResolveError("bad_headers", msg, VARS.HEADERS)
// safeName: names failing NAME_RE are printed JSON-escaped and truncated to 64 chars (a name is not a secret, but CR/LF must not reach stderr raw)
```
Non-ASCII policy (ADR-007 strict parsing; P3): values must be HTAB or 0x20–0x7E. Anything else — other C0 controls, DEL, latin1 0x80–0xFF, any code point > 0xFF — is rejected with `bad_headers` at resolve time. Rationale: Node `http` accepts 0x80–0xFF and transmits each char as one latin1 byte (a UTF-8 intent is silently mangled) and throws `ERR_INVALID_CHAR` mid-request for > 0xFF; rejecting up front gives one loud, named error before any connection. Because values are ASCII, the byte-length check equals char length. Test rows: `é` (U+00E9), `€` (U+20AC), emoji, `\x7f`, `\x01` → `bad_headers` naming the header, value absent; `\t` inside value → accepted.

Values never appear in any message. Note: `JSON.parse` keeps the last of exact-duplicate keys; only case-variant duplicates are detectable — accepted, documented in a code comment.

## Error Handling
Only throws `ResolveError`. Callers map: bridge → stderr `mcp-bridge: <message>` + exit 1; hook (`config.js`) → `{ok:false, reason, detail: message}` (exit 0 contract, see bridge-transports.md).

## Key Test Scenarios (R-07, R-08, R-18, AC-28, AC-29)
- Bundle only; env MCP pair only; env observe pair only (bridge falls to store — compat row 5); store only; none.
- Bundle + each of 4 conflict vars → `conflict` naming var, value absent from message; bundle + each conflict var `""` → no conflict (AC-28); bundle + HEADERS → accepted.
- `_URL` set + `_TOKEN=""` → message exactly `UNIMATRIX_REMOTE_TOKEN is empty; UNIMATRIX_REMOTE_URL is set`; reverse; same for `_MCP_URL`; both empty → store used (AC-29).
- Whitespace-only `UNIMATRIX_REMOTE_TOKEN=" "` is non-empty (no trimming).
- FP without URL; bad FP format; FP + env pair → pinnedFp set on both consumers.
- Stored `fingerprint:null` → pinnedFp null (CA).
- Headers: invalid JSON, array, number value, CR, LF, NUL, 8193-byte value, 33 entries, bad name, each reserved name in mixed case, `X-A`+`x-a` → `bad_headers`; `""` and `{}` → []; sentinel value never in any thrown message.
