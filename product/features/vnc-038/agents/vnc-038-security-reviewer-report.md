# Security Review: vnc-038-security-reviewer

## Risk Level: low

## Summary
vnc-038 makes project identity mandatory at the cloud HTTP entrypoint. Reading the
diff cold, the security-critical surfaces are well-constructed: slug input is an
allowlist newtype that makes path traversal and TOML injection structurally
impossible, the `v:2` bundle codec is a strict trust boundary mirrored byte-for-byte
on both Rust and JS sides, observe now resolves per-request through the *same*
`resolve_store` funnel as MCP (closing the cross-pollination hazard at its root), the
first-boot token is never emitted, and no new dependencies are introduced. No
blocking findings.

## Findings

### F1 — Slug allowlist makes traversal/injection unrepresentable (positive)
- **Severity**: informational (strength)
- **Location**: `crates/unimatrix-server/src/http/router/seam.rs` (`ProjectSlug::try_from`)
- **Description**: `^[a-z0-9][a-z0-9-]{0,62}$` enforced at the parse edge before any
  filesystem or TOML use. `.`, `/`, `\`, `%`, whitespace, uppercase, `..`, `%2f`,
  `%2e` cannot pass the charset, so a slug-derived path cannot escape the per-slug
  data dir and no TOML metacharacter can break the `[[projects]]` stanza. Validation
  is centralized in one newtype constructed only via `TryFrom`. This is
  defense-by-construction, not runtime rejection — the strongest form.
- **Recommendation**: none.
- **Blocking**: no

### F2 — `v:2` bundle decode: strict, length-capped, https-only, token-safe (positive)
- **Severity**: informational (strength)
- **Location**: `client_bundle.rs` (Rust decode), `packages/unimatrix/lib/hook-client/bundle.js` (JS decode)
- **Description**: Both decoders enforce identical guard ordering — 4 KB raw length
  cap FIRST (DoS pre-filter), scheme, base64url round-trip, JSON, then the
  load-bearing strict schema: exactly 5 keys, `v == 2`, both URLs `https://`-only,
  token/fp regex. A `v:1` bundle fails closed with an actionable re-issue message
  (no silent compat arm). The `https://`-only check blocks SSRF/downgrade to an
  attacker `http://` host. The token never appears in any error message (NFR-06).
  The JS decoder is zero-dependency.
- **Recommendation**: none. Note: the `https://` check is a prefix test
  (`startsWith`), which is correct for this purpose — the URL is server-composed and
  posted verbatim; the client deliberately performs no further URL parsing/normalization.
- **Blocking**: no

### F3 — Observe folded onto the per-request funnel closes the cross-store hazard (positive)
- **Severity**: informational (strength)
- **Location**: `crates/unimatrix-server/src/http/router/handlers.rs` (`route_observe`)
- **Description**: The highest-stakes risk (R-09: a request bound to project B
  reaching project A's store, corrupting the unrollbackable hash chain). The new
  observe handler derives identity via the same `parse_project_key` as MCP and
  resolves the store per-request via `observe_ctx.resolver.resolve_store(&key)` —
  there is no boot-bound or parallel observe store. `ProjectKey` is now a
  single-variant `Slug(_)` enum: the `Default` variant, the resolver's `default`
  field, and every `Default` match arm are deleted, so a default-store fall-through
  is not merely rejected but unrepresentable. Unknown slug → loud `UnknownProject`
  (404), never a default.
- **Recommendation**: none.
- **Blocking**: no

### F4 — Atomic, injection-free config write (positive)
- **Severity**: informational (strength)
- **Location**: `crates/unimatrix-server/src/projects/config_write.rs`
- **Description**: `register` writes the `[[projects]]` stanza via read-modify-write
  of the parsed TOML document, serialized through the `toml` library (no string
  concatenation, so no injection path), then temp + `fsync` + atomic `rename`. A
  crash yields the old or new complete file, never partial. Idempotent (re-register
  is a no-op). A malformed existing `config.toml` is a loud error, never clobbered.
  Genesis-clobber is prevented: State B (data dir survives) opens the existing store;
  State C (genesis) runs only when `!data_exists` — the hash chain is preserved.
- **Recommendation**: minor/non-blocking — the temp file name is PID-only
  (`.config.toml.<pid>.tmp`). Two distinct daemon processes would use distinct PIDs;
  a same-PID collision is impossible. The atomic rename is the correctness mechanism,
  so this is fine. No change required.
- **Blocking**: no

### F5 — First-boot token never emitted; bundle is the sole channel (positive)
- **Severity**: informational (strength)
- **Location**: `crates/unimatrix-server/src/http/token.rs` (`render_first_boot_notice`),
  `client_bundle.rs` (`render_output`)
- **Description**: `render_first_boot_notice` is a static string that contains
  neither the token hex nor the token-file path — it points to the retrieval command
  only. `client-bundle` emits the opaque blob on stdout and echoes only mcp-url /
  observe-url / cert-fp on stderr; the token lives only inside the base64url blob.
  Golden parity fixtures use obvious placeholder tokens (sequential hex, all-`f`,
  `cloud.example`), not real secrets.
- **Recommendation**: none.
- **Blocking**: no

### F6 — Test-only TLS override is test-scoped (verified)
- **Severity**: informational (verification)
- **Location**: `packages/unimatrix/test/hook-client/parity-layer2-precompact.test.js:70`,
  `packages/unimatrix/test/hook-client/parity-layer2-uds.test.js:119`
- **Description**: `NODE_TLS_REJECT_UNAUTHORIZED=0` appears only in two test-harness
  files, set on a spawned child process's env to accept a localhost self-signed leaf.
  It is gated on `env.UNIMATRIX_REMOTE_URL` (HTTPS-origin spawns only) and never
  appears under `packages/unimatrix/lib/`. The production cert-pinning path
  (`lib/hook-client/cert-pin.js`) uses `rejectUnauthorized:false` deliberately and
  paired with manual leaf-fingerprint verification (pre-existing, unchanged design).
  No production TLS relaxation is introduced.
- **Recommendation**: none.
- **Blocking**: no

### F7 — Legacy `{remote, token}` path retains client-side `/observe` compose and accepts `http:` (observation)
- **Severity**: low (pre-existing, out of scope)
- **Location**: `packages/unimatrix/lib/init.js` (`legacyObserveFrom`, `resolveRemoteTarget`)
- **Description**: The legacy backward-compat path still composes the observe URL
  client-side (`legacyObserveFrom`) and accepts `http:` or `https:`. This is
  pre-existing F3-era behavior explicitly preserved (and now isolated to the legacy
  branch); the dumb-client invariant and https-only posture apply to the `v:2` bundle
  path, which is the cloud surface. Not introduced by this diff, not the #766 surface.
- **Recommendation**: track for eventual legacy-path retirement; no action this PR.
- **Blocking**: no

## Blast Radius Assessment
The stated integrity stake is cross-project pollution of the unrollbackable hash
chain. The worst case — a resolver or codec bug routing project B's write to project
A's store — is structurally guarded, not merely tested:
- `ProjectKey` is a single-variant enum; there is no default store to fall through to.
- MCP and observe share exactly one `resolve_store(parse_project_key(path))` funnel;
  the boot-bound observe store handle is deleted.
- Slug identity is transport-derived (URL path only), never from a request payload,
  so a client has no field to name another project — mis-targeting is unrepresentable.
- The slug newtype forbids traversal at the charset level, so a malicious slug cannot
  reach another project's data dir even if registration validation were bypassed.
A subtle codec bug's worst case is a *failed* attach (fail-closed), not a
mis-targeted write — the strict exact-key/version/https guards reject rather than
silently accept. The most dangerous residual would be a future re-introduction of a
client-side path-composition site or a default arm; both are covered by the test
suite's invariant/grep assertions and the single-variant enum.

## Regression Risk
The hard cutover deletes the no-slug/default HTTP route and changes the bundle wire
form (`v:1` → `v:2`, no compat). Per the architecture, the cloud/container HTTP
surface has zero current deployments, and local STDIO/UDS is explicitly not routed
through the resolver — confirmed: the main.rs diff touches the local UDS/STDIO boot
binding only in a comment (the binding code is unchanged), and no new dependency or
manifest change is present. A `v:1` bundle held by any client fails closed with an
actionable re-issue message on both sides. The regression surface is bounded to the
HTTP path; local stores need no migration. Residual regression risk is low and
well-contained.

## Dependency Safety
No new dependencies — `Cargo.toml`, `Cargo.lock`, `package.json`, and
`package-lock.json` are unchanged on the branch. The JS bundle decoder remains
zero-dependency (Node built-ins only). No newly introduced transitive advisories.

## PR Comments
- Posted 1 review comment on PR #772 (approve / no change requested)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- the governing security patterns here
  (allowlist-newtype to make traversal/injection unrepresentable, strict
  length-cap-first codec at the trust boundary, single-funnel store resolution to
  prevent cross-tenant store access, secret-never-to-stdout) are already established
  in this codebase and applied cleanly; no recurring cross-feature anti-pattern
  surfaced in this diff.
