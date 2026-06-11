# Security Re-Review: vnc-034-security-reviewer-2 (F1 verification)

## Risk Level: low

## F1 Status: CLOSED

## Summary
Focused re-review of commit `46d3aa6c` against the prior BLOCKING finding F1
(HIGH): the JS C2 cert-fingerprint pin was non-functional over a live TLS
handshake. The fix genuinely closes F1 — it restores legitimate pinned
connections, enforces the pin via a manual `secureConnect` peer-cert check, and
provably does NOT leak the Bearer token on mismatch. No new weakness introduced.
Zero new runtime deps. Verified by reading the code cold AND running the suite +
an independent Node-internals probe.

## Verification Results

### 1. F1 closed — happy path (PASS)
`applyCertPin` now sets `options.rejectUnauthorized = false` (previously `true`
with `ca: undefined`), so the self-signed handshake COMPLETES instead of being
rejected with `DEPTH_ZERO_SELF_SIGNED_CERT`. A client pinned to the server's
REAL fingerprint connects and receives a 200. Confirmed by real-TLS tests (a)
and (c') — both pass against a live `https.createServer`.

### 2. F1 closed — pin actually enforced (PASS)
With `rejectUnauthorized:false` Node does not invoke `checkServerIdentity`. The
fix enforces the fingerprint manually: transport-http.js registers
`s.once("secureConnect", ...)` which calls
`verifyPeerFingerprint(socket, pinnedFp)` →
`computeFingerprint(socket.getPeerCertificate(true).raw)` and compares to the
pin. A wrong/MITM cert returns a diagnosable expected-vs-presented Error and the
socket is destroyed. Confirmed by tests (b) and (c). The retained
`makeCheckServerIdentity` is now correctly documented as unit-only / not the
live mechanism — not dead-but-claimed-live code.

### 3. No token leak on mismatch — CRITICAL (PASS)
The ordering claim holds. In the pinned branch:
- The bottom-of-function flush is guarded: `if (!pinned) req.end(body)`
  (transport-http.js:210) — the pinned path does NOT flush there.
- `req.end(body)` is called ONLY inside the `secureConnect` handler, AFTER
  `verifyPeerFingerprint` returns `null` (transport-http.js:165).
- On mismatch: `req.destroy(err)` with the body never written
  (transport-http.js:161).

I independently probed Node internals (not just trusting the test): the request
`socket` event fires BEFORE the TLS handshake completes (`authorized=false` at
socket-event time), so `s.once("secureConnect")` cannot miss the event; and a
request destroyed without `req.end()` transmits NO bytes — the server's request
handler never fires and never sees the `Authorization` header. The 20x
mismatch-hammer test (b') and the `observedAuth` assertions in (b) confirm the
token never reaches the server across repeated attempts.

### 4. No new weakness (PASS)
- `rejectUnauthorized:false` is set ONLY inside `if (isTls && pinnedFp)` in
  `applyCertPin` (cert-pin.js:132–137). Unpinned-TLS and plain-http requests get
  a no-op — Node's default `rejectUnauthorized:true` (full CA verification)
  still applies to any non-pinned https path. Asserted by the unit test
  (`applyCertPin(unpinned, true, null)` → `rejectUnauthorized === undefined`;
  `applyCertPin(plain, false, fp)` → `undefined`).
- The transport `pinned = isTls && !!config.pinnedFp` guard gates the
  manual-verify/deferred-flush branch identically, so the relaxed handshake and
  the manual check are coupled — there is no state where the handshake is
  relaxed but the pin is skipped.
- Zero new runtime deps: `cert-pin.js` uses only `crypto`; `transport-http.js`
  uses only `http`/`https` (Node built-ins). No `package.json` change in the
  commit.

### 5. Regression test is real (PASS)
`test/cert-pin-tls.test.js` uses an actual `https.createServer` (not plain TCP)
with a committed self-signed fixture (`test/fixtures/tls/{cert,key}.pem`, both
tracked at HEAD), exercising a live handshake for match, mismatch (incl. the
no-token-leak assertion), and pingForInit cases. The pinned fingerprint is
COMPUTED from the fixture DER via the production `computeFingerprint`
(self-consistent, not hand-written).

## Test Evidence
- `node --test test/cert-pin-tls.test.js` → 5/5 pass.
- `node --test test/remote-client.test.js` → 37/37 pass.
- Independent Node-internals probe → confirmed socket-event-before-handshake +
  no header transmission without `req.end()`.

## Blast Radius Assessment
Materially improved vs. the F1 state. Previously the broken pin caused
safe-but-silent loss of the entire remote HTTPS path (fail-open telemetry loss).
With the fix, the happy path works and the failure mode on a genuine cert
rotation/MITM is a loud, diagnosable connect failure that surfaces verbatim
through `pingForInit` at `init --remote`. Worst case if the deferred-flush
ordering had a subtle bug would be token disclosure to a MITM server — but that
path is closed: an unwritten request cannot leak headers, verified empirically.

## Regression Risk
Low. The change is confined to the two hook-client files and is gated behind the
`pinned` predicate. Unpinned-TLS and plain-http transport behavior is unchanged
(Node defaults preserved). No server-side Rust changes in this commit.

## New Findings
None. Prior non-blocking notes from the first review remain out of scope and
unchanged: N1 (server first-boot token on stdout — `token.rs` untouched) and N3
(transitive `rsa 0.9.10` RUSTSEC-2023-0071 via unused sqlx-mysql path). Both
non-blocking.

Note: the two pre-existing flaky JS failures (`parity-layer2-concurrency`
timeout; `init-integration` skills-dir isolation race) are unrelated to
cert-pinning and were not introduced by this fix.

## PR Comments
- Posted 1 verification comment on PR #734.
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the generalizable anti-pattern ("Node
  fingerprint pinning must use `rejectUnauthorized:false` + a manual
  `secureConnect` `getPeerCertificate(true).raw` check, and must defer
  `req.end(body)` until after the pin passes to avoid leaking the bearer token")
  is now captured directly in the cert-pin.js/transport-http.js code comments and
  a real-TLS regression test. Recommend promoting to a Unimatrix lesson only if
  the same pin mistake recurs in another feature.
