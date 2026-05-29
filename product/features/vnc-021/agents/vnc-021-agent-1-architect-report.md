# Agent Report: vnc-021-agent-1-architect

## Task
Design the architecture for vnc-021 (HTTPS Transport + Static Bearer Token Auth).

## Deliverables

### ARCHITECTURE.md
`product/features/vnc-021/architecture/ARCHITECTURE.md`

### ADR Files
| ADR | File | Unimatrix |
|-----|------|-----------|
| ADR-001: Constant-Time Token Validation | `architecture/ADR-001-constant-time-token-validation.md` | #4665 |
| ADR-002: Health Endpoint Auth Bypass | `architecture/ADR-002-health-endpoint-auth-bypass.md` | #4666 |
| ADR-003: rmcp Adapter Boundary | `architecture/ADR-003-rmcp-adapter-boundary.md` | #4667 |
| ADR-004: Pre-TLS Connection Limiting | `architecture/ADR-004-pre-tls-connection-limiting.md` | #4668 |
| ADR-005: rustls TLS with Bypass | `architecture/ADR-005-tls-termination-with-bypass.md` | #4669 |
| ADR-006: credential_type static_token | `architecture/ADR-006-credential-type-static-token.md` | #4670 |

## Key Decisions Summary

1. **Constant-time token validation** (ADR-001): subtle::ConstantTimeEq for bearer token comparison. No custom crypto.
2. **Secure-by-default auth** (ADR-002): Path-match bypass for /health in auth middleware. All new paths authenticated by default.
3. **rmcp isolation** (ADR-003): Thin adapter in PathRouter isolates StreamableHttpService. Workarounds for SR-01/SR-02 are localized.
4. **Pre-TLS connection limiting** (ADR-004): Semaphore acquired before TLS handshake prevents resource exhaustion (SR-08).
5. **rustls with bypass** (ADR-005): Pure Rust TLS, configurable off for proxy-terminated deployments.
6. **credential_type naming** (ADR-006): "static_token" distinguishes from future "oauth_jwt" in audit queries.

## Open Questions

1. **rmcp extension propagation (SR-02)**: Must validate that StreamableHttpService propagates http::Extensions to RequestContext.extensions. ADR-003 adapter boundary provides fallback if it does not.
2. **hyper HTTP version**: rmcp may require HTTP/1.1 for SSE streaming. Start with HTTP/1.1 only; add HTTP/2 if rmcp supports it.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 17 entries returned; key inputs: #4661 (dep landscape), #4367 (rmcp traps), #4627 (W2-3 StaticTokenAuth status), #4638 (W2-2 feature entry), #1913 (ADR-003 Clone sharing), #319 (CallerId pattern)
- Stored: entry #4665 "ADR-001 vnc-021: Constant-Time Token Validation" via /uni-store-adr
- Stored: entry #4666 "ADR-002 vnc-021: Health Endpoint Auth Bypass" via /uni-store-adr
- Stored: entry #4667 "ADR-003 vnc-021: Thin Adapter Boundary" via /uni-store-adr
- Stored: entry #4668 "ADR-004 vnc-021: Pre-TLS Connection Limiting" via /uni-store-adr
- Stored: entry #4669 "ADR-005 vnc-021: rustls TLS Termination" via /uni-store-adr
- Stored: entry #4670 "ADR-006 vnc-021: credential_type static_token" via /uni-store-adr
