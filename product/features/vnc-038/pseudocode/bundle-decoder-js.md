# Component 2 — Bundle Decoder (JS)

**File:** `packages/unimatrix/lib/hook-client/bundle.js`
**ADR:** ADR-002 (#5081), ADR-001 (#5080) · **AC:** AC-05 · **Risk:** R-03, R-04

## Purpose

Decode + strict-validate a pasted `v:2` bundle and return finished URLs the caller posts verbatim. Mirror the Rust encoder byte-for-byte (the parity corpus is the shared oracle). Zero-dependency (NFR-08). This is a TRUST BOUNDARY: `raw` is untrusted operator paste.

## Modified Constants

```
KEEP   BUNDLE_SCHEME = "unimatrix-bundle:"
KEEP   MAX_RAW_LEN   = 4096           // GUARD 1, raw bytes
KEEP   TOKEN_RE = /^[0-9a-f]{64}$/,  FP_RE = /^sha256:[0-9a-f]{64}$/
CHANGE EXPECTED_KEYS = ["v", "mcp_url", "observe_url", "token", "fp"]   // was ["v","base_url","token","fp"]
KEEP   class BundleError extends Error
RETIRE SLUG_RE and assertSlugAllowlist   // ADR-001: client no longer branches on a slug; remove from exports
```

## Modified Functions

### `decodeBundle(raw)` (MODIFY — v:2)

```
function decodeBundle(raw):
    if typeof raw !== "string": throw BundleError("bundle must be a string")

    // GUARD 1 — LENGTH CAP FIRST on raw bytes, before decode/parse (DoS; AC-W1-C10)
    if Buffer.byteLength(raw, "utf8") > MAX_RAW_LEN:
        throw BundleError("bundle too long (> 4 KB) — refusing to decode")

    // GUARD 2 — scheme prefix
    if not raw.startsWith(BUNDLE_SCHEME):
        throw BundleError("not a unimatrix bundle (missing 'unimatrix-bundle:' prefix)")
    body = raw.slice(BUNDLE_SCHEME.length)

    // GUARD 3 — base64url decode (no pad) + round-trip re-encode check (kept from v:1)
    try:
        bytes   = Buffer.from(body, "base64url")
        jsonStr = bytes.toString("utf8")
        if bytes.toString("base64url") !== body.replace(/=+$/, ""):
            throw BundleError("bundle payload is not valid base64url")
    catch: rethrow BundleError("bundle payload is not valid base64url")

    // GUARD 4 — JSON parse
    try: obj = JSON.parse(jsonStr)
    catch: throw BundleError("bundle payload is not valid JSON")
    if obj===null or typeof obj!=="object" or Array.isArray(obj):
        throw BundleError("bundle payload is not a JSON object")

    // GUARD 5 — STRICT SCHEMA (load-bearing): EXACTLY {v, mcp_url, observe_url, token, fp}
    if not keysAreExactly(Object.keys(obj)):
        throw BundleError("bundle has unexpected fields (expected exactly v, mcp_url, observe_url, token, fp)")
    if obj.v !== 2:                                                  // R-04: a v:1 holder lands here
        throw BundleError("unsupported bundle version: " + String(obj.v) + " — re-issue a v:2 bundle")
    if typeof obj.mcp_url !== "string" or not obj.mcp_url.startsWith("https://"):
        throw BundleError("mcp_url must be an https URL")
    if typeof obj.observe_url !== "string" or not obj.observe_url.startsWith("https://"):
        throw BundleError("observe_url must be an https URL")
    if typeof obj.token !== "string" or not TOKEN_RE.test(obj.token):
        throw BundleError("token must be 64 lowercase hex chars")    // never echo the token (NFR-06)
    if typeof obj.fp !== "string" or not FP_RE.test(obj.fp):
        throw BundleError("fp must be sha256:<64 hex>")

    return { v: 2, mcp_url: obj.mcp_url, observe_url: obj.observe_url, token: obj.token, fp: obj.fp }
```

### `keysAreExactly(keys)` — UNCHANGED logic (now compares against the 5-key `EXPECTED_KEYS`).

### `module.exports` (MODIFY)

```
module.exports = { decodeBundle, BundleError, BUNDLE_SCHEME, MAX_RAW_LEN }
// REMOVED: assertSlugAllowlist, SLUG_RE  (no client-side slug grammar — ADR-001 closed set)
```

> The removal of `SLUG_RE`/`assertSlugAllowlist` is part of deleting the client-side path-composition closed set. Confirm no other module imports them (grep) before removal; if a non-bundle caller exists, flag rather than break it.

## Error Handling

- Any guard → `BundleError`, hard reject, no partial accept (R-03).
- `v:1` bundle (`obj.v !== 2`) → actionable message telling the operator to re-issue (R-04), not a silent/opaque throw.
- Token never appears in any thrown message (NFR-06 / security).

## Key Test Scenarios (hints)

1. Hex-vector parity: decode the SAME bytes the Rust encoder produced → field equality (R-03 sc.1).
2. Strict reject: missing key, extra key (e.g. a stray `base_url`), wrong-type, non-https `mcp_url`/`observe_url`, bad token/fp (R-03 sc.2).
3. `v:1` bundle (`{v:1, base_url,...}`) → `obj.v !== 2` reject with re-issue message (R-04 sc.1).
4. Guard ordering: over-`MAX_RAW_LEN` non-base64url paste → `TooLong`, not `BadBase64` (R-03 sc.3).
5. Bundle at exactly `MAX_RAW_LEN` and one byte over (edge).
6. Returned object has exactly `{v, mcp_url, observe_url, token, fp}` and the URLs are byte-equal to the payload (feeds R-01 verbatim-post).
