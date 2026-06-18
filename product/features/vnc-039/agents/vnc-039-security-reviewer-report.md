# Security Review: vnc-039-security-reviewer

## Risk Level: low

## Summary
The bearer-token relocation and the stdio→HTTPS MCP bridge are well-constructed. The token is never placed on argv, in `.mcp.json`, or in any log/summary surface, and the pinned-flush trust boundary correctly destroys the socket before the body byte on a fingerprint mismatch — proven by a real-`https.createServer` live-boundary suite (the #4970 recipe, with a negative control). Out-of-tree store is mode 0600. Zero new runtime dependencies. No blocking findings. Two non-blocking observations: the critical live-TLS suite is gated on `openssl` availability (silent-skip hazard if the validation runner lacks it), and the accepted legacy `fingerprint:null` unpinned path (WARN-1) is bounded and correctly fails the bridge closed.

## Findings

### F1 — Critical live-TLS boundary suite silently skips when openssl is unavailable
- **Severity**: low
- **Location**: `packages/unimatrix/test/helpers/mcp-stub-server.js:173`; consumed by `test/hook-client/mcp-bridge-tls.test.js:34`
- **Description**: `SKIP = GEN ? false : { skip: "openssl unavailable…" }`. The entire live good-pin / wrong-pin / per-socket-repin / cert-swap suite — the single most important security control in the feature (token-never-on-wire) — is skipped wholesale if the runner cannot generate the self-signed fixture. A green run that skipped this suite is exactly the #4970 / #4796 false-green class: a green suite that never exercised the boundary. The code under test is correct; the risk is the *gate* reading green without the boundary having run.
- **Recommendation**: The validating gate must assert openssl is present and these tests RAN (not skipped) before reporting AC-04 validated — verify by name, not by green suite. Generating certs at runtime (vs. committing a key) is itself correct and aligned with lesson #4969.
- **Blocking**: no

### F2 — Legacy path persists an unpinned credential (fingerprint:null) out of tree (accepted WARN-1)
- **Severity**: low
- **Location**: `lib/init.js` initRemote `isBundlePath ? pinnedFp : null`; `lib/hook-client/config.js` resolve (`pinnedFp` null on null fingerprint); `lib/hook-client/mcp-bridge.js:42-46`
- **Description**: On the legacy `--remote/--token` path the credential is relocated out of tree but carries `fingerprint:null`, so the observe POST runs unpinned (bearer over an unverified TLS leaf). This is the accepted WARN-1 and preserves *today's* legacy behavior — it is not a regression introduced here. The blast radius is bounded: (a) the bridge refuses to start on a null-fingerprint credential and exits loud (`mcp-bridge.js:42`), so cloud MCP never sends the bearer unpinned; (b) only the pre-existing legacy observe path stays unpinned, and the diff does not extend it. The bundle path — the only path that gains the new MCP surface — is always pinned.
- **Recommendation**: None required for this PR. The unpinned legacy observe path is a known, documented residual (#773 / ADR-005) to be retired when legacy `--remote/--token` is deprecated. The fail-closed bridge guard is the correct containment.
- **Blocking**: no

### F3 — Store path keyed on projectHash — no path-traversal surface (verification, not a defect)
- **Severity**: informational
- **Location**: `lib/hook-client/credstore.js` pathFor / `config.js` computeProjectHash
- **Description**: The store path is `homedir()` + `.unimatrix` + `String(projectHash)` + `remote.json`. `projectHash` is the first 16 hex of a SHA-256 (fixed grammar, no user-controlled string), so there is no `..`/separator-injection surface even though `pathFor` does not sanitize. `init` computes the write key and both readers compute the read key through the one shared `computeProjectHash` export, so write/read keys cannot diverge (R-07 closed by construction). The slug is payload inside `mcp_url` (posted verbatim by the dumb-client bridge), never a key or a composed path.
- **Recommendation**: None.
- **Blocking**: no

### F4 — Deserialization of untrusted server bytes is bounded (verification)
- **Severity**: informational
- **Location**: `mcp-bridge/dispatch.js`, `mcp-bridge/sse-parse.js`, `mcp-bridge/http-session.js:101`
- **Description**: The bridge is a translator, not an evaluator — it `JSON.parse`s response payloads and passes them through as JSON-RPC; it never executes response content, bounding injection to opaque payload pass-through. Bodies are bounded at 1 MiB on both the JSON (`readBounded`) and SSE (`collect`, `total > limit` → `res.destroy()`) paths. Request bodies are likewise guarded at 1 MiB before flush. Malformed JSON / unexpected content-type degrade to a JSON-RPC error object rather than throwing or hanging. The SSE parser normalizes CRLF and emits only on a record terminator (chunk-split-invariant). Store JSON is parsed under a `schema_version` gate — an unknown version is terminal/diagnosable, not a silent skip, and the bridge fails loud on it (never fails open unpinned).
- **Recommendation**: None.
- **Blocking**: no

## Token-handling verification (the security core)
- **argv**: bridge takes only `<projectHash>`; `bin/unimatrix.js` mcp-bridge route and `.mcp.json` `args:[bridgePath, projectHash]` carry no token. Confirmed.
- **`.mcp.json`**: `writeMcpBridgeEntry` writes `{command, args:[path, hash], env:{}}` — no token, no mcp_url, no fingerprint. Confirmed.
- **logs / printSummary**: store-write actions emit the path + "(mode 0600)" only; no token/fingerprint in any action string. `printSummary` prints actions verbatim — no secret surface. Confirmed.
- **pin-mismatch path**: `mismatchError` (cert-pin.js) names expected vs presented fingerprints only; the live test asserts the token string is absent from the error (`mcp-bridge-tls.test.js:73`). Confirmed.
- **error/throw messages**: credstore read/write Errors carry only the path + `err.code` — explicitly token-free. Confirmed.
- **store at rest**: written with `{mode:0600}` then `chmodSync(0600)` re-asserted (handles pre-existing looser perms). Cleartext-at-rest is accepted by decision (NFR-05); the hardened risks (in-tree leak, token-to-logs) are closed. Confirmed.

## TLS pinned-flush trust boundary
- Token written to the wire ONLY inside `onPin` (`http-session.js:109`); `req.end(body)` is reached from no other path. An unwritten request cannot leak the token.
- `agent:false` on every request → a fresh TLS socket per request; `_pinThenFlush` re-runs `verifyPeerFingerprint` on each socket's `secureConnect` before its first body byte. No connection-pool agent that could flush on an unverified socket. Proven live: per-socket re-pin count == sockets opened, and a mid-session cert swap rejects socket #2 with no body flushed.
- Mismatch fails closed: `req.destroy(err)` → fail-loud (non-zero exit, diagnosable stderr), body never flushed. Mirrors the proven observe path (`transport-http.js:150-176`) verbatim.
- Negative control present (`mcp-bridge-tls.test.js:87`): stubbing the pin to a no-op DOES leak the token, proving the wrong-pin assertion is non-vacuous (closes the #4970 dead-pin false-green class).

## Blast Radius Assessment
Worst case of a subtle bug in the bridge or credential resolution: the cleartext bearer is handed to an attacker-controlled / mis-pinned server. This is the feature's one high-consequence failure and the design centers on it. It is contained by: (a) `req.end` reachable only post-pin; (b) `agent:false` per-socket re-pin (no pooled flush); (c) fail-loud-fail-closed on mismatch; (d) the live good/wrong-pin suite with a negative control. The credential-resolution worst case (wrong-key → resolves nothing) degrades to UDS fall-through (hook) or a loud "no credential" exit (bridge) — a dead surface with a diagnosis, not a silent unpinned send. No data-corruption or privilege-escalation path: the bridge is a stdio↔HTTPS translator that executes no response content.

## Regression Risk
The credential relocation rewrites `config.resolve()` (file branch) and `initRemote()`. Precedence is preserved exactly: env-pair → store-file → UDS fall-through; ENOENT/incomplete still falls to UDS, non-ENOENT parse error still terminal `malformed`. The change *fixes* a pre-existing break (the old reader looked for `unimatrix.remote.url`, never written, and never read `fingerprint` → observe silently fell to UDS and would have been unpinned). Post-fix, file-mode observe now resolves to `observe_url` and populates `pinnedFp` — and the test plan mandates a behavioral wire test (good-pin connects, wrong-pin rejects), not a shape check, guarding against re-introducing a dead pin. Legacy/UDS paths are unchanged. `.mcp.json` writer is idempotent, merge-preserving, dry-run-aware, malformed-throws (mirrors `writeMcpJson`). Stale in-tree `unimatrix.remote` cleanup is merge-preserving and best-effort (never aborts init).

## Dependency Safety
Zero new runtime dependencies confirmed: `packages/unimatrix/package.json` `dependencies` is absent/empty; `optionalDependencies` are the unchanged prebuilt Rust binaries. All new modules import Node stdlib only (`fs`, `os`, `path`, `crypto`, `https`, `url`). AC-02 (zero-dep) holds.

## PR Comments
- Posted 1 review comment on PR #780 (advisory; F1 routed to the gate).
- Blocking findings: no.

## Knowledge Stewardship
- Queried: context_search lesson-learned for cert-pin/trust-boundary precedent (#4970 — the dead-pin false-green that this feature's live-boundary suite + negative control directly answer; #4969 — runtime-generated test certs, satisfied; #4321 — trust-boundary input-validation severity).
- Stored: nothing novel to store -- the trust-boundary-needs-a-live-boundary-test lesson already exists as #4970 (active, edge-linked to ADR-001 vnc-039 #5108) and this PR is a faithful application of it, not a new anti-pattern. The "critical live-TLS suite can silently skip on missing openssl" observation (F1) is a per-PR gate note, not a generalizable recurring anti-pattern across features yet.
