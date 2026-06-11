# Agent Report: vnc-034-agent-8-certpin-fix

## Task
Fix BLOCKING security finding F1 (HIGH) from PR #734: the C2 cert-fingerprint pin
was non-functional over a live TLS handshake. `applyCertPin` set
`rejectUnauthorized:true` + `ca:undefined` + a custom `checkServerIdentity`; Node
runs CA-chain verification BEFORE `checkServerIdentity`, so the self-signed leaf
failed with `DEPTH_ZERO_SELF_SIGNED_CERT` first — legitimate connections rejected
and the pin was dead code. No test exercised a real TLS handshake, so it went green.

## Files Modified
- `packages/unimatrix/lib/hook-client/cert-pin.js` (modified)
- `packages/unimatrix/lib/hook-client/transport-http.js` (modified)
- `packages/unimatrix/test/remote-client.test.js` (modified — updated stale shape
  assertion + added verifyPeerFingerprint unit tests)
- `packages/unimatrix/test/cert-pin-tls.test.js` (new — real-TLS regression)
- `packages/unimatrix/test/fixtures/tls/cert.pem`, `key.pem` (new — committed
  self-signed fixture; pinned fp is COMPUTED from its real DER, never hand-written)

## The Fix
1. `cert-pin.js`:
   - `applyCertPin` now sets `rejectUnauthorized:false` for pinned TLS so the
     self-signed handshake COMPLETES. CA-trust cannot be used — we hold only the
     fingerprint, not the cert.
   - New `verifyPeerFingerprint(socket, pinnedFp) -> Error|null`: reads
     `socket.getPeerCertificate(true).raw`, computes `computeFingerprint`, compares
     to the pin; returns `null` on match, the existing diagnosable expected-vs-
     presented Error on mismatch, or an Error if no cert is presented.
   - `computeFingerprint` and the diagnosable mismatch message preserved verbatim;
     `makeCheckServerIdentity` retained for direct unit verification (documented as
     NOT the live-handshake mechanism).
2. `transport-http.js`:
   - Pinned-TLS path (`pinned = isTls && !!config.pinnedFp`) does NOT flush the
     body. On `secureConnect`, it calls `verifyPeerFingerprint`; on a non-null
     Error it `req.destroy(err)` and settles a `connect` SendResult carrying the
     diagnosable `message` — then returns WITHOUT writing the body. Only on a
     passing pin does it call `req.end(body)`.
   - `pingForInit` now surfaces a carried `res.message` verbatim (the diagnosable
     mismatch) instead of the generic "cannot reach host" line — the ONE loud
     path (FR-A11 / AC-CT-ROT), classified as connect.

## Ordering Proof (token never written on mismatch)
The token rides in `Authorization: Bearer` inside the request body. For pinned TLS
the body is flushed via `req.end(body)` ONLY inside the `secureConnect` handler,
AFTER `verifyPeerFingerprint` returns `null`. On mismatch the handler calls
`done(...)` (once-guarded) and `req.destroy(err)` and returns — `req.end(body)` is
never reached, so the request is never flushed. This does not rely on Node's flush
timing: an un-ended request cannot transmit its body. Empirically proven by test
(b') which hammers 20 mismatch attempts and asserts the server's observed
`Authorization` headers never contain the token and the request count never
advances.

## Tests
- New real-TLS suite `cert-pin-tls.test.js` (5 tests, all pass): spins up a real
  `https.createServer` with the fixture self-signed cert.
  - (a) good pin connects over real TLS, receives 200 Pong, token reaches server.
  - (b) wrong pin rejected with diagnosable expected-vs-presented error; server
    receives NO request and the token never appears in anything the server saw.
  - (b') 20x mismatch hammer — token never leaks across attempts (ordering proof).
  - (c) `pingForInit` surfaces the mismatch verbatim, classified connect.
  - (c') `pingForInit` good pin yields a clean Pong over real TLS.
- `remote-client.test.js`: updated `test_pin_completes_self_signed_handshake`
  (asserts `rejectUnauthorized:false` now, the F1 contract) + 3 new
  verifyPeerFingerprint unit tests. Parity assertion vs the committed Rust corpus
  (`crates/.../c1c2-parity/`) unchanged and still holds (SR-02). 42/42 pass.
- Relevant package suite (all `*.test.js` + hook-client unit tests, excluding the
  slow cargo-server Layer 2 integration tests): **818 tests, 818 pass, 0 fail,
  1 skipped** — verified stable across 3 consecutive runs.

## Gates
- Size gate: PASS — stripped 82441/100000 (17.5 KB headroom), raw 143400/160000.
- Zero-deps: PASS — package.json has no runtime deps; all 18 hook-client modules
  use Node built-ins / relative paths only. `package.json`/lockfile unchanged.
- Touched ONLY `packages/unimatrix/**`. No Rust file or committed corpus modified.

## Issues / Non-blockers
- Two pre-existing flaky failures observed in the FULL suite, both reproduced on
  PRISTINE code (changes stashed), neither related to cert-pinning:
  1. Layer 2 `parity-layer2-concurrency` (`test_l2_concurrency_attribution`,
     `test_l2_raw_session_id_on_wire_server_mints_http_prefix`): the real cargo
     server times out at 60s under load in this sandbox. Uses transport-uds/delta,
     not transport-http/cert-pin.
  2. `test_foreign_hooks_preserved_incl_foreign_node` / `bundled-only` ENOENT: a
     test-isolation race — `init-integration.test.js` mutates the shared package
     `skills/` dir (creates+rm `skills/bundled-only` in a finally) while
     `node --test` runs files in parallel; a parallel scandir races the rm. Non-
     deterministic (0 failures across 3 consecutive relevant-set runs). Adding a
     test file shifts parallel scheduling but does not cause the defect.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-002 (entry 4948) and
  hook-client test-trap patterns (4768/4774/4823); confirmed the
  oracle-corpus-never-hand-written rule and the stub-server-is-plain-TCP gap that
  let F1 through.
- Stored: pattern below (Node fingerprint pinning live-handshake trap) via
  /uni-store-pattern.
