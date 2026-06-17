# vnc-038 Component 2 — Bundle Decoder (JS) — Agent Report

**Agent:** vnc-038-agent-3-bundle-decoder-js · **Role:** uni-js-dev · **ADR:** ADR-002 (#5081), ADR-001 (#5080) · **AC:** AC-05 · **Risk:** R-03, R-04

## Outcome: COMPLETE

Implemented the JS `v:2` bundle decoder, byte-parity with the Wave-1 Rust encoder. Full Rust↔JS parity corpus is green.

## Files Modified

- `packages/unimatrix/lib/hook-client/bundle.js` — v:2 decoder.
- `packages/unimatrix/test/hook-client/bundle.test.js` — NEW focused decoder suite (parity, strict-reject, guard-ordering, v:1 hard-cut, secret hygiene, zero-dep).
- `packages/unimatrix/test/remote-client.test.js` — removed the now-migrated bundle-decode `describe` blocks and the RETIRED slug-allowlist block; dropped the `assertSlugAllowlist` import.

## What Changed in `bundle.js`

- `EXPECTED_KEYS = ["v","mcp_url","observe_url","token","fp"]` (was `["v","base_url","token","fp"]`).
- `obj.v !== 2` reject with an actionable re-issue message (R-04); a v:1 bundle fails closed.
- `mcp_url` + `observe_url` each required to be `https://` strings; returned verbatim (ADR-001).
- Decoder returns `{v:2, mcp_url, observe_url, token, fp}`.
- LOCKED guard ordering preserved: length cap 4096 (first, on raw bytes) → scheme strip → base64url no-pad (round-trip re-encode check) → JSON → strict exact-key schema.
- RETIRED `assertSlugAllowlist` / `SLUG_RE` and removed them from exports (ADR-001 — the client derives no slug).
- Token never appears in any thrown message (NFR-06).

## Tests

- `test/hook-client/bundle.test.js`: **23 pass / 0 fail.** Includes the Rust↔JS parity corpus consumer (decodes every committed v:2 golden row from `crates/unimatrix-server/tests/fixtures/c1c2-parity/bundle-golden.json` to its exact fields). No hand-written vectors.
- Size gate: **PASS** — stripped 82476/100000 (17.5 KB headroom), raw 144054/160000 (15.9 KB headroom). `bundle.js` shrank (slug code removed).
- Zero-deps: **PASS** — package.json/lock unchanged; all 18 hook-client modules require only Node built-ins.

## Issues / Coordination Notes (NOT blockers for this component)

- **Atomic-coupling failures in `remote-client.test.js` (11 `initRemote` tests).** These are Component 3 (init.js) surface — they reference `base_url` + slug-append + v:1 `resolveRemoteTarget`. They fail by construction (C-03 strict exact-key guard / corpus now v:2) until `init.js` lands its v:2 rework in the same atomic landing. NOT my scope to edit init.js's tests; flagged for the init/transport agents.
- **`init.js` still imports `assertSlugAllowlist`** (line 14/304). My ADR-001-mandated export removal will break init.js until Component 3 deletes the slug-append composition sites. This is the documented atomic dual-side change.
- **Parity corpus was stale v:1 at the start of my run** and the server crate did not compile (Wave 2 `ProjectKey::Default` removal in flight), so I could not regenerate it myself. It was regenerated to v:2 by the Rust dev / Delivery Leader during the swarm; my parity test then went green with zero changes.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing/context_search + context_get(5081 ADR-002, 4961 length-cap pattern) — found the v:1 length-cap guard-ordering trap (#4961) and ADR-002 atomicity contract; applied both.
- Stored: entry #5092 "v:2 bundle decoder: dual-side atomic coupling — focused test file + corpus-gated parity, never hand-write goldens" via /uni-store-pattern.
