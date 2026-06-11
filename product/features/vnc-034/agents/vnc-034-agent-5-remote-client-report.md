# Agent Report — vnc-034-agent-5-remote-client (RemoteClient, Wave 1, #725)

## Summary
Implemented `init --remote <bundle> [--slug <s>]` in the pure-JS client: C1 bundle
ingestion (decoder mirroring the Rust oracle), C2 cert pinning via a custom
`checkServerIdentity`, slug append, skills copy, CLAUDE.md-block suppression, and
the < 250 KB install gate. The C1/C2 contract is CONSUMED from the committed Rust
parity corpus — no golden values are hand-written (SR-02, ADR-006).

## Files Created
- `packages/unimatrix/lib/hook-client/bundle.js` — C1 decoder. Guard order LOCKED:
  (1) 4 KB RAW-string byte-length cap BEFORE decode/parse (classifies on length,
  not a decode error — AC-W1-C10); (2) scheme strip → base64url-decode (no pad) →
  JSON.parse; (5) strict schema reject (exactly {v,base_url,token,fp}, v===1,
  https base_url, 64-hex token, `^sha256:[0-9a-f]{64}$` fp) — load-bearing
  (AC-W1-C9). `assertSlugAllowlist` enforces `^[a-z0-9][a-z0-9-]{0,62}$` at the
  parse edge. Token never in any thrown message (NFR-06).
- `packages/unimatrix/lib/hook-client/cert-pin.js` — C2 pin. `computeFingerprint`
  mirrors Rust `fingerprint_leaf_der` (`sha256:`+lowercase hex of leaf DER /
  `cert.raw`). `makeCheckServerIdentity` returns a DIAGNOSABLE mismatch Error
  naming expected vs presented `sha256:` + re-bundle remediation (FR-A11 /
  AC-CT-ROT). `applyCertPin` threads the pin into request options, clearing CA
  trust (`ca: undefined`) — the pin IS the trust model (ADR-002).
- `packages/unimatrix/test/remote-client.test.js` — 34 tests; reads the committed
  corpus and asserts byte-identity for fingerprint + bundle goldens.

## Files Modified
- `packages/unimatrix/lib/init.js` — added `resolveRemoteTarget` (bundle path vs
  legacy {remote,token}); extended `initRemote` for the bundle path (pin persist,
  skills copy, pinned Ping); extended `writeRemoteSettingsLocal` to persist
  `unimatrix.remote.fingerprint` (0600); routes `--bundle` to `initRemote`.
- `packages/unimatrix/lib/hook-client/transport-http.js` — threaded `config.pinnedFp`
  into `mod.request` via `applyCertPin`; `pingForInit` accepts/threads `pinnedFp`.
- `packages/unimatrix/bin/unimatrix.js` — parse `--bundle` / `--slug`.

## 1-client:1-project (R-06 / AC-W1-C5)
`resolveRemoteTarget` returns a flat `{remote, token, pinnedFp}` — one endpoint
string. No array/list/second-endpoint field exists; cross-project fan-out is
UNREPRESENTABLE, not runtime-rejected. Asserted structurally.

## Tests
- `node --test test/remote-client.test.js`: 34 pass / 0 fail.
- Full suite `node --test`: 839 pass / 0 fail / 1 skipped (pre-existing).
- `node test/run-hook-client.js` (parity runner): 608 pass / 0 fail / 1 skipped.

## Size / Deps Gates
- `check-hook-client-size.js`: PASS — stripped 81038/100000 (headroom 18962),
  raw 138860/160000 (headroom 21140).
- Install footprint (AC-W1-C3): lib/ + skills/ = 231766 bytes (~226 KiB) < 250 KB;
  asserted by `test_remote_install_under_250kb`.
- `check-zero-deps.js`: PASS — 18 hook-client modules require only Node built-ins.
- `package.json` unchanged; no lockfile exists. Zero added runtime deps.

## Scope / Constraints
- Touched only `packages/unimatrix/**`. No Rust file or committed corpus edited.
- Pure JS, no native binary, copy-install only, all files < 500 lines.
- CLAUDE.md knowledge block NOT appended; `/unimatrix-init` pointer printed only.
- Legacy F3 `{remote, token}` path preserved (backward-compat); all 37 prior
  init-remote tests still green.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern, vnc-034) -- surfaced #4777
  (JS JSON.parse vs serde_json lone-surrogate divergence); applied — strict
  schema regexes neutralize smuggled content, so the schema guard is load-bearing.
- Stored: entry #4961 "vnc-034 bundle decoder: length-cap guard must classify on
  byte length, not a decode error" via context_store (pattern).

## Issues / Blockers
None.
