# Test Plan — Bundle Decoder (JS)

> Component: `packages/unimatrix/lib/hook-client/bundle.js` · Surface: `packages/unimatrix/test/remote-client.test.js` (consumes `bundle-golden.json`) · Risks: R-03, R-04 (Crit) · AC-05

## Scope
JS DECODES only (never encodes). `decodeBundle(raw)` strict-validates `v:2` and returns `{v, mcp_url, observe_url, token, fp}` with URLs the client posts verbatim. `EXPECTED_KEYS = ["v","mcp_url","observe_url","token","fp"]`; `obj.v !== 2` reject. Zero-dependency (NFR-08). Guard ordering preserved: length → scheme → base64url → JSON → strict schema.

## Unit Test Expectations

### Golden round-trip (R-03 — JS golden NEVER hand-written)
- `decodes every v:2 golden row` — for each row in `bundle-golden.json`, `decodeBundle(row.wire)` deep-equals `row.fields` (`{v:2, mcp_url, observe_url, token, fp}`). This is the JS half of the cross-language parity oracle; it FAILS the instant the Rust encoder drifts.

### Strict-reject matrix on the JS side (R-03 — mirror of Rust)
Each throws `BundleError`, no partial accept:
- `rejects missing key` — drop `observe_url`.
- `rejects extra key` — add a 6th key (`keysAreExactly` fails).
- `rejects wrong-type key` — `v` as string / `mcp_url` non-string.
- `rejects non-https url` — `mcp_url` starting `http://` or non-URL → reject (assert `https://`-only validation on BOTH url fields).
- `rejects unknown major version` — `obj.v === 3`.

### v:1 hard-cut with actionable message (R-04)
- `rejects v:1 bundle with re-issue message` — feed a well-formed `v:1` bundle (`{v:1, base_url, token, fp}`); assert `obj.v !== 2` → `BundleError` whose message tells the operator to re-issue a `v:2` bundle (actionable, NOT a silent/opaque throw).
- `no v:1 fallback decode path` — assert there is no `base_url` acceptance branch; a `v:1` bundle never silently decodes.

### Guard ordering (R-03 / NFR-08 — security boundary)
- `MAX_RAW_LEN cap runs first` — a raw paste > `MAX_RAW_LEN` throws the length-cap error BEFORE any base64/JSON parse (DoS guard). Assert order via an over-length-but-otherwise-invalid input still failing on length.
- `boundary: exactly MAX_RAW_LEN accepted, +1 rejected`.

### Zero-dependency invariant (NFR-08)
- `decoder imports no third-party modules` — assert `bundle.js` stays zero-dependency (static check on imports / no new package added).

## Security (bundle = JS client trust boundary)
- Oversized input → length cap first (DoS).
- Strict exact-key + version pin → no field smuggling.
- `https://`-only URL validation → no downgrade/SSRF to an attacker host (the client otherwise posts telemetry/MCP to a hostile endpoint).
- Bundle is `base64url(JSON)` — assert JSON parsing is bounded.

## Coverage Requirement
A `v:1` bundle fails closed with a re-issue message; no silent acceptance, no silent default; the strict-reject matrix mirrors the Rust side exactly (R-03 symmetry); the golden corpus is decoded, never hand-written.
