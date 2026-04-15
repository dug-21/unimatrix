# ASS-041: Transport + Auth Stack Evaluation

**Date**: 2026-04-09
**Tier**: 1 (prerequisite for ASS-042)
**Feeds**: W2-2 (HTTPS transport), W2-3 (OAuth identity model)

---

## Question

Is rmcp 0.16's HTTP transport production-ready? What auth model and libraries fit each deployment tier? And what is the right simple auth mechanism for the MIT developer cloud tier (single developer, multiple machines)?

There are now **three auth paths** across three product tiers, and the library selections must serve all three:

| Tier | Transport | Auth | License |
|------|-----------|------|---------|
| Local OSS | STDIO | none | MIT |
| Developer cloud | HTTPS | startup-generated static token | MIT |
| Enterprise | HTTPS | OAuth 2.1 | commercial |

These decisions are coupled: transport choice determines where auth middleware hooks in; tier determines which auth path applies. All three must be settled before the enterprise server (ASS-042) and container (ASS-043) can be architecturally specified.

**Working hypothesis under challenge**: OAuth 2.1 client credentials is the enterprise auth model (ASS-048 Q1 confirms this). The open question is the developer cloud auth model — what is the simplest token mechanism that is secure for single-user deployment without requiring OAuth infrastructure?

---

## Why It Matters

HTTPS transport and auth decisions propagate into every downstream code path. The developer cloud tier (MIT, Docker, HTTPS) is the adoption driver for the Codespaces/multi-machine use case. Getting the auth model wrong for either tier is the most expensive mistake: too complex for developer cloud kills adoption; too simple for enterprise creates security debt.

---

## What to Explore

### 0. Developer Cloud Auth Model — Simple Token

The developer cloud tier (MIT, single-user, HTTPS) needs auth that is:
- Simple to deploy (no OAuth infrastructure, no enrollment workflow)
- Secure enough for a network-accessible daemon
- Usable without any browser-based flow

**Reference model**: Jupyter notebook auth — a token is generated at first startup, stored in the data volume (persists across container restarts), and printed once to stdout during first run. The user copies the token into their MCP client config. No rotation in Wave 2.

Evaluate:
- What is the right token format? (UUID v4, 32-byte random hex, JWT signed with a startup-generated key?)
- Where is it stored? (Volume-mounted file, not config, so it survives container restart without re-printing)
- How is it validated? (In-memory on startup, read from volume file)
- What happens on first run vs. subsequent runs? (Generate + print on first run; load silently on restart)
- How does the MCP client present it? (Bearer token in Authorization header — same header the enterprise tier uses for OAuth, so the transport code is the same)
- Is a single static token sufficient, or should there be a rotation mechanism in Wave 2? (Recommendation: single token, document rotation as "stop container, delete token file, restart")
- What are the threat model boundaries? (Token in transit protected by TLS; token at rest in the volume; no multi-user risk because single-user by design)

This path has no dependency on ASS-048 — it is a simpler, MIT-licensed mechanism independent of enterprise auth.

### 1. rmcp 0.16 HTTP Transport
- What HTTP transport modes does rmcp 0.16 actually support? (SSE, WebSocket, streaming, long-poll?)
- Is it production-stable or experimental? Are there open issues, known panics, or incomplete implementations?
- Does it expose middleware hooks for authentication? At what point in the request lifecycle?
- If rmcp HTTP is not viable: evaluate alternatives — axum-based MCP server wrapper, custom HTTP handler, or other MCP Rust implementations. What migration cost does switching incur?

### 2. Authentication Model — OAuth vs. mTLS vs. Hybrid

**Do not start by assuming OAuth.** Evaluate both models on their merits for the Unimatrix machine-to-machine use case (AI agents calling a local/hosted server):

**OAuth 2.0 client credentials**:
- Best fit when: agents are managed by an identity provider, human-facing admin UIs need token-based access, external IdP integration (Okta, Azure AD) is a requirement.
- Weaknesses: token lifetime management, secret storage on the client, requires an OAuth server component (built-in or external), more complex for container-to-container calls.

**mTLS (mutual TLS)**:
- Best fit when: all callers are services (no human-facing browser OAuth flow needed), certificates are the natural trust anchor in the deployment environment, enterprise PKI is already available.
- Strengths: identity is in the TLS layer (no separate token), revocation via certificate revocation (CRL/OCSP), no secret stored in plaintext, well-understood by enterprise security teams.
- Weaknesses: certificate provisioning overhead (especially for dev/eval), no standard scope system (must be built at the application layer), harder to express "this agent has access to project X but not Y" without custom extensions.

**Hybrid**: mTLS for machine-to-machine agent calls, OAuth for the admin console (human-facing). These are not mutually exclusive.

Recommendation must take ASS-048 findings (enterprise security team preferences) as input. If ASS-048 is not complete when this spike runs, document the question as input-blocked rather than defaulting to OAuth.

### 3. OAuth / JWT Library Landscape (conditional on §2 recommending OAuth or hybrid)
- Evaluate: `jsonwebtoken`, `oxide-auth`, `axum-extra` JWT extractor, `openidconnect` crate, `josekit`.
- Required capabilities: client credentials flow validation, JWT decode + verify (RS256/ES256), JWKS endpoint fetch + cache (for external IdP support — customer Okta/Azure AD).
- Evaluate maintenance status, audit history, and test coverage for each candidate.
- How do the candidates compose with the chosen transport? (Axum extractor pattern vs. middleware layer vs. manual decode)

### 4. TLS
- `rustls` vs. `native-tls` for a long-running daemon on Linux targets.
- Trade-offs: `rustls` is pure Rust + auditable; `native-tls` links system OpenSSL (already a dependency for ONNX on some targets).
- Certificate loading: PEM file from `config.toml` path. No hot-reload required for Wave 2.
- If mTLS is recommended: evaluate how the TLS library handles client certificate extraction and validation. Can it surface the client certificate's subject/SAN as the agent identity?
- Two-listener architecture: does the TLS library support binding two independent HTTPS sockets in the same process cleanly?

### 5. Auth Middleware Composition
- How does identity resolution integrate with the existing rmcp tool dispatch?
- The non-negotiable: capability checks stay in the service layer, not the transport layer. Transport layer resolves identity only.
- Sketch the flow for all three tiers:
  - **STDIO path**: no auth; identity is implicit (single local process)
  - **Developer cloud path**: inbound HTTPS → TLS termination → bearer token extracted → compared to stored token → identity = single configured user → tool dispatch (no capability check beyond presence)
  - **Enterprise path**: inbound HTTPS → TLS termination → bearer token extracted → JWT validation → `agent_id` + role resolved from control plane → capability check in service layer → tool dispatch
- The bearer token header is the same across developer cloud and enterprise — the difference is what validates it. This means transport code is shared; only the validation middleware differs.
- Where does the enterprise JWT `sub` → `agent_id` lookup happen, and what does it query (control plane DB)?

---

## Output

1. **Transport recommendation**: rmcp HTTP or alternative, with explicit rationale and migration notes
2. **Developer cloud auth model**: token format, storage location, first-run behavior, threat model boundaries
3. **Enterprise auth model**: OAuth 2.1 confirmed (ASS-048 Q1); library selection with evaluation matrix
4. **TLS library selection** with rationale for both image types
5. **Auth flow sketches** for all three tiers showing where each component sits
6. **Known risks or open questions** to carry into ASS-042

---

## Constraints

- Must compose with existing rmcp tool definitions — no changes to the MCP tool API surface
- Capability checks remain in the service layer (`SearchService`, `StoreService`, etc.) — the transport layer resolves identity only
- Two listeners (content + admin port) must be supported in a single process
- OAuth library evaluation (§3) is conditional on auth model recommendation — do not pre-commit the library evaluation if §2 recommends mTLS

## Dependencies

- **ASS-048** (Enterprise Security Requirements) — recommended as prior input. If ASS-048 findings are available, incorporate enterprise security team preferences into §2 recommendation. If not available, flag §2 as provisional and document the open question for ASS-042 to carry.
