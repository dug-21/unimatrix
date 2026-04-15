# FINDINGS: Transport + Auth Stack Evaluation

**Spike**: ASS-041
**Date**: 2026-04-11
**Approach**: investigation + evaluation
**Confidence**: directional
**Feeds**: W2-2 (HTTPS transport), W2-3 (OAuth identity model), ASS-042 (security architecture)

---

## Findings

### Q0: Is rmcp 0.16's HTTP transport production-ready?

**Answer**: Yes. The `transport-streamable-http-server` feature in rmcp 0.16 is functionally complete and architecturally sound for production use. It requires a tower-service HTTP host and has a dependency consideration with the current workspace (reqwest version), but neither issue blocks production use.

**Evidence**:

rmcp 0.16.0's HTTP transport uses SSE-over-HTTP (POST to send, GET for SSE stream, DELETE to terminate session) — the MCP spec-compliant "Streamable HTTP" transport. `StreamableHttpService<S, M>` implements `tower_service::Service<Request<RequestBody>>`, composing directly with any tower/hyper-based HTTP server. Stateful mode uses a `SessionManager` trait for session lifetime management. SSE keep-alive and retry intervals are configurable.

**Auth middleware hook point**: `StreamableHttpService` does not provide any auth hook — it processes requests directly. Auth must be layered externally as a tower middleware (`tower::ServiceBuilder::layer`) that intercepts `Request<B>` before it reaches `StreamableHttpService`. Auth metadata written to request extensions upstream is readable downstream in tool handlers via `Extension<Parts>`. This is the correct composition pattern.

**No server-side bearer validation in rmcp**: The `auth` feature in rmcp 0.16 is exclusively client-side (PKCE flow, token acquisition). Unimatrix must implement its own bearer token validation middleware in the HTTP path.

**Dependency consideration**: rmcp 0.16.0's `auth` and HTTP transport features can require `reqwest = "^0.13.2"`. The workspace currently has `reqwest 0.12.28`. This conflict does NOT affect the `transport-streamable-http-server-session` feature — that feature depends only on tokio, tower-service, bytes, http, http-body, http-body-util, sse-stream. The conflict only appears if rmcp's `auth` or `reqwest` features are enabled. Since Unimatrix does not use rmcp's client-side auth, this conflict is avoided.

**Migration cost**: The existing `UnimatrixServer` implements rmcp's `Service<RoleServer>` trait. The HTTP path calls the same service via `StreamableHttpService::service_factory`. All existing MCP tool definitions are unchanged. Migration cost: (1) add `StreamableHttpService` wrapping the existing `UnimatrixServer` factory, (2) add a hyper listener binding a TCP socket, (3) add a tower middleware layer for auth.

**Recommendation**: Use rmcp's `transport-streamable-http-server` feature. Enable: `["server", "transport-io", "macros", "transport-streamable-http-server-session", "transport-streamable-http-server"]` in `unimatrix-enterprise/Cargo.toml`. Do NOT enable rmcp's `auth` or `reqwest` features (client-side only; causes reqwest version conflict with workspace).

---

### Q0: Developer cloud auth model — simple token

**Answer**: A 32-byte cryptographically random hex token generated at first startup, written to a volume-persisted file with mode 0600, validated in-memory. Bearer token in the `Authorization` header on every request — same header format as enterprise OAuth so transport code is shared across tiers. No rotation in Wave 2.

**Evidence**:

*Token format*: UUID v4 is not suitable — it provides only 122 bits of effective entropy and leaks format information in its structure. A 32-byte random value encoded as 64 lower-hex characters provides 256 bits of entropy. `rand::rngs::OsRng` generating `[u8; 32]` then hex-encoded is correct. JWT signed with a startup-generated key adds complexity with no benefit — JWTs are appropriate when claims (expiry, scope, audience) need validation without a shared secret lookup, which is unnecessary for a single static token.

*Storage*: Token file written to `{data_volume}/token` with `std::fs::set_permissions(mode 0600)` immediately after creation. Volume-mounted in Docker — persists across container restarts. Token loaded into `Arc<String>` at startup and compared via constant-time equality check on every request. File must never appear in Docker image layers.

*First-run vs. restart*: On first run (token file absent): generate token, write to file, print to stdout once with a prominent label (`[UNIMATRIX TOKEN] <hex>`). On subsequent runs (token file present): read file, load into memory silently. This mirrors Jupyter's token behavior.

*Constant-time comparison*: `subtle::ConstantTimeEq` must be used for token comparison to prevent timing oracle attacks. `subtle` crate is not currently in the dependency graph — add it.

*Threat model*: Token in transit protected by TLS. Token at rest is a volume-mounted file with 0600 permissions (readable only by the container user). Primary threat vector is token file exfiltration from the volume — same threat model as SSH private keys, accepted practice. No multi-user risk (single-user by design). Token does not expire in Wave 2 — rotation procedure: stop container, delete token file, restart.

**Recommendation**: 32-byte random hex token (`rand::rngs::OsRng`, `[u8; 32]`, hex-encoded). Stored at `{data_volume}/token` with mode 0600. Generated and printed on first run; loaded silently on restart. Validated by constant-time comparison (`subtle` crate). Presented as `Authorization: Bearer <token>`. No rotation in Wave 2 — document the operational rotation procedure.

---

### Q1: Enterprise auth library selection

**Answer**: ASS-048 Q1 confirms OAuth 2.1 client credentials as enterprise M2M auth. The specific library stack: `jsonwebtoken` for JWT validation, custom JWKS fetch-and-cache via `reqwest`. No `oxide-auth`, `openidconnect`, or `josekit`.

**Evidence**:

| Library | Assessment |
|---------|------------|
| `jsonwebtoken` 9.x | Active, widely used. JWT decode + verify (RS256/ES256). Validates signature, expiry, audience, issuer. No JWKS fetch — provide separately. |
| `josekit` 0.8 | Active but less production-proven than `jsonwebtoken` in the Rust ecosystem. Full JOSE suite is more than needed. |
| `oxide-auth` 0.5 | Unmaintained (last release 2022). Full OAuth 2.0 server framework — not applicable; Unimatrix is a resource server, not an auth server. |
| `openidconnect` 3.x | Active. Includes JWKS fetch + cache + JWT validation. Uses `jsonwebtoken` internally. Couples in OIDC-specific behavior (id_token flows) unnecessary for pure M2M client credentials. |
| `axum-extra` JWT extractor | Convenience wrapper around `jsonwebtoken`. Not applicable without axum in the stack. |

*Validation flow*: (1) Extract `Authorization: Bearer <token>` header. (2) Decode JWT header to get `kid`. (3) Look up `kid` in cached JWKS — on miss, refresh JWKS from configured endpoint. (4) Verify JWT signature with resolved key (RS256 or ES256). (5) Validate `aud` claim matches configured Unimatrix audience string. (6) Validate `exp` claim. (7) Validate optional `iss` claim. (8) Extract `sub` claim as `agent_id`. (9) Look up role in control plane by `agent_id`.

*`sub` → `agent_id` → role lookup*: JWT `sub` maps to `agent_id` in the existing `AgentRegistry` (already in `unimatrix-server`). Role lookup at validation time allows role changes without re-issuing tokens. Option (a) — custom JWT claim for role — is simpler but requires IdP configuration and prevents role revocation without token re-issue. Option (b) — AgentRegistry lookup — is preferable.

*reqwest version for JWKS fetch*: The enterprise crate uses reqwest 0.13.x. No conflict since the enterprise crate is separate from `unimatrix-server`.

**Recommendation**: `jsonwebtoken` for JWT validation (RS256/ES256). Custom JWKS fetch-and-cache using `reqwest 0.13.x` on a background task with periodic refresh + per-validation-failure cache miss fallback. Validation pipeline: extract bearer → decode header → resolve key from JWKS cache → verify signature + claims → extract `sub` → AgentRegistry lookup → `ResolvedIdentity`.

---

### Q2: TLS library selection

**Answer**: `rustls` (via `tokio-rustls`) for both developer cloud and enterprise HTTPS listeners. `native-tls` must not be added as a direct dependency for the HTTPS listener.

**Evidence**:

*rustls*: Memory-safe, pure Rust, audited (Trail of Bits, 2020). Supports TLS 1.2 and 1.3. `rustls 0.23.x` is already in the dependency graph transitively. `tokio-rustls` provides `TlsAcceptor` for async TLS handshakes on `TcpListener` connections. Certificate loading: PEM file path from `config.toml`, loaded at startup via `rustls-pemfile`. No hot-reload required for Wave 2.

*native-tls*: Links against system OpenSSL on Linux — adds `libssl-dev` to the image layer (~4MB image size + separate CVE tracking surface). The ONNX runtime (`ort 2.0.0-rc.9` with `default-features = false`) does not link OpenSSL. The "ONNX already a dependency" note in the scope is not confirmed by the actual Cargo.toml — `native-tls` provides no advantage here.

*Two-listener architecture*: `rustls` supports binding multiple independent `TlsAcceptor` instances in the same process. If both listeners share the same certificate, they share one `Arc<rustls::ServerConfig>`. If they require separate certificates, they use separate `Arc<rustls::ServerConfig>` instances. Both patterns work correctly.

*Proxy-terminated TLS (per ASS-048)*: Support `tls.enabled = false` mode — binds plain HTTP when a proxy terminates TLS upstream. This is a config switch; no TLS library change required. Both modes (direct TLS + proxy-terminated) must be supported.

**Recommendation**: `rustls 0.23` via `tokio-rustls`. PEM certificate loaded at startup from `config.toml`. Support `tls.enabled = false` for proxy-terminated deployments. Do not add `native-tls` as a direct dependency for the listener.

---

### Q3: Auth middleware composition with rmcp tool dispatch

**Answer**: Tower middleware layer wraps `StreamableHttpService`. Middleware resolves identity into `ResolvedIdentity`, written to request extensions. Service layer capability checks are unchanged from the current STDIO path. Two-port architecture uses two independent `TcpListener` + `TlsAcceptor` pairs.

**Auth flow sketches for all three tiers**:

**STDIO path (OSS, local)**:
```
stdin → rmcp transport-io → UnimatrixServer (tool dispatch)
         ↑ no auth; identity = agent_id from tool params (existing behavior, unchanged)
```

**Developer cloud path (MIT, HTTPS)**:
```
TcpListener:8443 (TLS via tokio-rustls)
  → tower layer: StaticTokenAuth
      read Authorization: Bearer <hex>
      constant-time compare with in-memory token (subtle crate)
      success → insert ResolvedIdentity{agent_id: "owner", role: Admin} into extensions
      failure → HTTP 401
  → StreamableHttpService<UnimatrixServer>
      tool handler reads Extension<Parts> for identity
      no capability check beyond "authenticated" (single-user; all caps implied)
```

**Enterprise path (commercial, HTTPS)**:
```
TcpListener:8443 (TLS via tokio-rustls, or plain HTTP behind proxy)
  → tower layer: JwtBearerAuth
      read Authorization: Bearer <jwt>
      decode JWT header → resolve key from JWKS cache (refresh on miss)
      verify RS256/ES256 signature
      validate exp, aud, iss claims
      extract sub → AgentRegistry lookup → role (Admin/Operator/Auditor)
      success → insert ResolvedIdentity{agent_id, trust_level, capabilities} into extensions
      failure → HTTP 401, WWW-Authenticate: Bearer error="invalid_token"
  → StreamableHttpService<UnimatrixServer>
      tool handler reads Extension<Parts> for identity
      capability check in service layer (unchanged): identity.capabilities.contains(Write)

TcpListener:8444 (admin port — same middleware, enforces Admin role in service layer)
```

*The bearer header is shared across developer cloud and enterprise*: MCP clients send `Authorization: Bearer <value>` regardless of whether `<value>` is a static hex token or a JWT. Transport code is shared; only middleware validation logic differs. This is the critical design property enabling shared transport infrastructure.

**Recommendation**: `StaticTokenAuth` tower middleware for developer cloud (constant-time compare). `JwtBearerAuth` tower middleware for enterprise (`jsonwebtoken` + JWKS cache). Both write `ResolvedIdentity` to extensions. Service layer capability checks unchanged. Two-port: two independent `TcpListener` + `TlsAcceptor` pairs in the same tokio runtime.

---

## Q: Claude Code MCP HTTP client — Authorization header support

**Answer**: Confirmed supported. Claude Code's HTTP MCP transport client accepts `Authorization: Bearer <token>` via the `--header` CLI flag and the `headers` config field. The HTTPS transport design is unblocked. One active bug (partially mitigated) requires a specific configuration approach.

**Evidence**:

*Official support*: Claude Code's HTTP MCP transport supports custom headers including `Authorization: Bearer` both via the CLI and config file:

```bash
# CLI — stores in ~/.claude.json, works correctly
claude mcp add unimatrix \
    --transport http \
    --header "Authorization: Bearer ${UNIMATRIX_TOKEN}" \
    https://unimatrix.example.com:8443/mcp
```

```json
// Config file format (headers support env var substitution)
{
  "mcpServers": {
    "unimatrix": {
      "type": "http",
      "url": "https://unimatrix.example.com:8443/mcp",
      "headers": { "Authorization": "Bearer ${UNIMATRIX_TOKEN}" }
    }
  }
}
```

*Active bug — anthropics/claude-code#28293* (filed 2026-02-24, last active 2026-03-25, status: OPEN/stale): Headers defined in `.mcp.json` are not forwarded on tool call POST requests to `/messages` — only on the initial SSE/stream connection. This causes authentication failures mid-session. The bug affects both SSE and Streamable HTTP transports when config is sourced from `.mcp.json`.

*Workaround — confirmed working*: Headers configured via `claude mcp add -H` are stored in `~/.claude.json` rather than `.mcp.json`, and are correctly forwarded on all requests including tool call POSTs. The workaround is complete and requires no server-side changes.

*Duplicate bugs confirming breadth*: anthropics/claude-code#14976 (CLOSED Dec 2025), anthropics/claude-code#14977 (CLOSED Dec 2025), and a related issue anthropics/claude-code#2831 ("[BUG] MCP `http` transport bypasses Authorization header, attempts OAuth2 registration") suggest the bug has been partially addressed but not fully resolved for all config paths.

*Deployment implication for Unimatrix*: Unimatrix client setup documentation must instruct users to configure via `claude mcp add -H`, not via manual `.mcp.json` editing. Token should reference an environment variable (`${UNIMATRIX_TOKEN}`) rather than a plaintext value. This is already best practice and costs nothing to specify.

*OAuth path*: Claude Code also supports a full OAuth flow (`/mcp auth {server-name}`) for servers that implement the OAuth 2.1 authorization server spec. The enterprise tier's JWT validation is compatible with this flow — Claude Code can acquire a JWT via OAuth and present it as `Authorization: Bearer <jwt>`. The static token (developer cloud tier) uses the same bearer header mechanism without the OAuth acquisition step.

**Conclusion**: The HTTPS transport design is confirmed viable. The critical open risk flagged in the initial findings is resolved. The only implementation constraint is a documentation requirement: users must configure via CLI (`claude mcp add -H`), not manual `.mcp.json`, until anthropics/claude-code#28293 is resolved upstream.

---

## Unanswered Questions

**1. JWKS cache invalidation under key rotation**: How quickly the cache should refresh when an IdP rotates keys (Okta/Azure AD typically rotate every 6 hours) and how to handle the transient validation failure window (existing JWTs signed with old key while cache refreshes) needs explicit design in ASS-042. The standard pattern — background tick + per-validation-failure cache miss fallback — is assumed but not specified here.

**3. rmcp version compatibility with future MCP spec updates**: rmcp 0.16.0 was released 2026-02-17. The MCP spec is actively evolving. Whether a future rmcp version will require breaking API changes affecting the `StreamableHttpService` composition pattern is not determined. Monitor the rust-sdk releases.

---

## Out-of-Scope Discoveries

1. **Axum removed from rmcp's server-side-http feature in 0.16.0** (PR #642): rmcp's HTTP transport is now tower-native and does not require axum. If Unimatrix had been planning to use axum as the HTTP host, a simpler hyper + tower approach now suffices. Reduces the dependency footprint for the enterprise crate.

2. **reqwest version duplication risk**: If the enterprise crate ever needs rmcp's `auth` feature (client-side OAuth), it will pull in reqwest 0.13.2 while `unimatrix-server` uses 0.12.28 transitively. Cargo resolves these as separate crates (different semver), but binary size and link time increase. Flag for enterprise crate Cargo dependency strategy.

3. **`subtle` crate not in current dependency graph**: Adding it is a one-line Cargo.toml change. Load-bearing security primitive for developer cloud token comparison.

4. **UDS-over-TCP as a zero-auth option for Codespaces**: For multi-machine developer cloud use with a forwarded port, a forwarded UDS socket could eliminate the auth token requirement if the port forwarding mechanism provides access control. Not practical for general developer cloud deployment but worth noting as a zero-auth fallback for specific environments.

---

## Recommendations Summary

| Decision | Recommendation |
|----------|---------------|
| Transport | rmcp `transport-streamable-http-server` feature. Tower-native, production-stable. No axum required. |
| Developer cloud auth | 32-byte OsRng hex token. Stored at `{data_volume}/token`, mode 0600. First-run print, silent restart. Validated by `subtle` constant-time compare in `StaticTokenAuth` tower middleware. |
| Enterprise auth | OAuth 2.1 confirmed (ASS-048). `jsonwebtoken` for JWT validation (RS256/ES256). Custom JWKS fetch-and-cache via `reqwest 0.13.x`. `sub` → AgentRegistry lookup → `ResolvedIdentity`. |
| TLS | `rustls 0.23` via `tokio-rustls`. PEM from `config.toml`. Support `tls.enabled = false` for proxy-terminated deployments. |
| Middleware | Tower middleware wraps `StreamableHttpService`. Identity in request extensions. Service-layer capability checks unchanged. |
| Claude Code header support | **Resolved.** `Authorization: Bearer` confirmed supported. Configure via `claude mcp add -H` (not `.mcp.json`) until anthropics/claude-code#28293 is fixed upstream. Document this in Unimatrix client setup instructions. |
