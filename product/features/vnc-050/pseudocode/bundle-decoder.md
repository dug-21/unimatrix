# C8 JS Bundle Decoder — `lib/hook-client/bundle.js`

ADR-005 (#5815). No dependencies. Smallest change: one guard.

## Change
GUARD 5 fp check:
```
if (obj.fp !== null && (typeof obj.fp !== "string" || !FP_RE.test(obj.fp))) {
  throw new BundleError("fp must be null or sha256:<64 hex>");
}
return { v: 2, mcp_url, observe_url, token, fp: obj.fp };   // fp: string | null
```
`keysAreExactly` unchanged: `fp` key must be present (missing key still rejects; `undefined` never reaches here). JSDoc return type → `fp: (string|null)`.

## Consumers
- `init.js resolveRemoteTarget`: `pinnedFp: b.fp` already passes through; credstore write stores `fingerprint: null` for a CA bundle (`isBundlePath ? pinnedFp : null` → null). No change needed; add a test.
  Gap flagged: implementer must check `init.js` MCP-registration branches keyed on `isBundlePath`/pin presence (pattern #5129: pinned self-signed not registered as native http MCP). A CA bundle must register the bridge the same way a pinned bundle does; if `init.js` keys bridge registration on a truthy `pinnedFp`, change it to key on `isBundlePath`.
- C9 `resolveRemote` (bundle tier).

## Error Handling
Throws `BundleError`; message never contains token or raw blob.

## Key Test Scenarios
- `fp:null` accepted → `{fp:null}`; `""`, `"sha256:"`, `"SHA256:…"`, uppercase hex, number, `false`, missing key, extra key rejected (parity with Rust corpus).
- Pre-vnc-050 fixture bundle decodes identically (AC-24).
- `init --bundle <ca-bundle>` writes `fingerprint:null` to remote.json; bridge then uses CA branch (R-10.2).
