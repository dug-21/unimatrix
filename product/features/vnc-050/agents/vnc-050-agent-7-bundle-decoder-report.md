# vnc-050-agent-7-bundle-decoder — C8 JS bundle decoder

## Report
- Files modified: `packages/unimatrix/lib/hook-client/bundle.js` (GUARD 5 accepts `fp:null`, JSDoc `fp:(string|null)`), `packages/unimatrix/test/hook-client/bundle.test.js` (nullable-fp suite: null accept; "", "sha256:", "SHA256:", uppercase hex, number, false, object, missing key, extra key reject; AC-24 pinned golden rows), `packages/unimatrix/test/init-remote.test.js` (`makeBundle(fp)`; `init_bundle_ca_stores_fingerprint_null`, `init_bundle_pinned_unchanged`).
- `init.js`: no change needed — store write and bridge registration already key on `isBundlePath`; Ping gets `pinnedFp=null` → CA branch.
- Parity: existing golden round-trip test deep-compares every corpus row incl. `fp`, so Rust-side `fp:null` rows are covered automatically once C7 regenerates the corpus.
- Tests: bundle/init-remote/remote-client/init — 139 pass, 0 fail (serial). Intermittent cross-file failure under parallel `node --test` (AC-07 / bundle verbatim store) not reproducible serially or per-file; appears to be pre-existing shared-state flake.
- Size gate: OK, stripped 109775/110000 (225 B headroom), raw 196800/200000. Includes concurrent remote-resolve.js.
- No new deps; not committed (per instruction).

## Knowledge Stewardship
- Queried: skipped Unimatrix search — pseudocode/ADR-005 sufficient.
- Stored: nothing novel to store -- one-guard change.
