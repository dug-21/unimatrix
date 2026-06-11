# Security Review: vnc-034-security-reviewer

## Risk Level: high

## Summary
The change set is careful and well-defended across secrets handling, the bundle
trust-boundary parser, the `ProjectSlug` allowlist, the `resolve_store` funnel,
dependency surface, and container posture. One BLOCKING defect: the JS cert-pin
path sets `rejectUnauthorized: true` with `ca: undefined`, so Node rejects the
server's own valid self-signed cert (`DEPTH_ZERO_SELF_SIGNED_CERT`) *before*
`checkServerIdentity` runs — the OSS fingerprint-pinning trust model does not
function over a live TLS handshake. The test suite misses it because no test
performs a real TLS handshake.

## Findings

### F1 — Cert pin rejects the valid self-signed cert before the pin runs
- **Severity**: high
- **Location**: `packages/unimatrix/lib/hook-client/cert-pin.js:75-82` (`applyCertPin`)
- **Description**: `applyCertPin` sets `rejectUnauthorized = true` AND
  `ca = undefined`, then attaches the custom `checkServerIdentity`. Node only
  calls `checkServerIdentity` after CA-chain verification succeeds; with
  `ca: undefined` the self-signed leaf fails chain verification with
  `DEPTH_ZERO_SELF_SIGNED_CERT` first, so the pin function is dead code on the
  happy path. The code comment ("CA-chain validation is bypassed by design")
  contradicts the implementation. Empirically confirmed against a real
  `https.createServer` + openssl self-signed leaf: the connection is REJECTED
  and the pin never runs. Also verified that `rejectUnauthorized: false` does
  NOT invoke `checkServerIdentity` at all (a wrong pin is silently accepted), so
  the fix must move the fingerprint check to a `secureConnect` handler (manual
  `socket.getPeerCertificate(true).raw` compare + `destroy` on mismatch) OR ship
  the pinned leaf as `ca` (wire-format change).
- **Why green**: JS tests assert only the options-object shape
  (`rejectUnauthorized === true`, `checkServerIdentity` is a function) and call
  the pin directly with a synthetic `cert.raw`. No real handshake is exercised —
  `test/helpers/real-server.js` uses plain `net.createServer()` (TCP).
- **Recommendation**: implement fingerprint pinning correctly (secureConnect
  manual check with `rejectUnauthorized: false`, since the client only holds the
  fingerprint, not the cert) and add a real-TLS-handshake regression test
  (self-signed server: good-pin accepts, wrong-pin rejects).
- **Blocking**: yes

### N1 — Token on server first-boot stdout (pre-existing, out of scope)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/http/token.rs:101`
- **Description**: `println!("[UNIMATRIX TOKEN] {hex}")` on first-boot
  generation. `token.rs` is UNCHANGED in this PR; the `client-bundle` subcommand
  correctly uses read-only `read_token_hex` (no print), so the bundle blob is
  uncontaminated. NFR-06 ("token never in stdout") is technically violated by
  the server's own first-boot log line, but this is pre-existing.
- **Recommendation**: follow-up to redact/relocate to a 0600-file-only notice.
- **Blocking**: no

### N2 — Pin comparison non-constant-time
- **Severity**: informational
- **Location**: `packages/unimatrix/lib/hook-client/cert-pin.js:46`
- **Description**: `presented !== pinnedFp` is not constant-time. Acceptable —
  the fingerprint is public, not a secret; no useful timing leak.
- **Blocking**: no

### N3 — cargo audit: 1 pre-existing medium
- **Severity**: medium (pre-existing, not introduced)
- **Location**: `rsa 0.9.10` via `sqlx-mysql` (RUSTSEC-2023-0071, Marvin Attack)
- **Description**: Transitive through sqlx's mysql feature path (unused — project
  is sqlite). No fixed upgrade upstream. New deps `rcgen 0.13.2` and
  `base64 0.22.1` are clean; `base64 0.22.1` is the only new lockfile entry.
- **Blocking**: no

## Blast Radius Assessment
F1: total loss of the remote HTTPS path. `init --remote` Ping fails; every
hook-client POST fails `connect`. Because the hook client is fail-open (resolves
`connect`, never throws), the production symptom is SILENT telemetry loss rather
than a loud error — making the broken pin harder to notice in the field. No data
corruption or disclosure; the failure mode is safe-but-silent unavailability of
the remote feature.

## Regression Risk
Server-side Rust path is low-risk: `/health` (no auth) and `/observe` are split
off by `PathRouter` before the MCP arm; only the MCP fall-through routes through
`SlugRouter`. The rmcp `allowed_hosts` CVE-2026-42559 default is untouched
(asserted by T-RO-05/06). `/observe` acquires its store through the funnel at
boot. The `UNIMATRIX_HTTP_ENABLED` override is total (no panic on garbage) and
the compiled default `http.enabled=false` is preserved, so local UDS installs
are unaffected. The only behavioral regression is F1 (client cannot establish
the pinned TLS connection).

## Verified GOOD (no action)
- Bundle parser guard ordering (4 KB raw cap before decode; strict 4-key schema;
  https/64-hex/sha256 grammar); token-free error messages.
- `ProjectSlug` allowlist enforced at the parse edge before any path join; path
  traversal structurally unrepresentable.
- `resolve_store` funnel: `Slug(_)` -> `UnknownProject`, never default fallback;
  `ProjectKey` transport-derived only, no payload-named project.
- Cert/key provisioning: key `0600` via `O_CREAT|O_EXCL`, cert `0644`, fail-loud
  partial state, idempotent reload (no silent rotation), bounded validity; files
  on data volume, never a DB.
- stdout = opaque blob only; stderr = base-url + fp echo, token absent.
- Container: TLS-only EXPOSE 8443, distroless nonroot UID 65532, secrets stay
  0600 on /data, never in image layers.

## PR Comments
- Posted 1 comment on PR #734 (full findings).
- `gh pr review --request-changes` was BLOCKED by GitHub (cannot request changes
  on own PR — author == reviewer account); posted as a `gh pr comment` instead.
- Blocking findings: yes (F1).

## Knowledge Stewardship
- Stored: nothing novel to store — F1 is a one-PR correctness defect (the
  generalizable lesson "Node fingerprint pinning cannot use rejectUnauthorized:true
  + ca:undefined" is specific enough to belong in the PR fix + a regression test,
  not Unimatrix knowledge yet; revisit if the same pin mistake recurs).
