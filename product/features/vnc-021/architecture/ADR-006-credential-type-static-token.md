## ADR-006: credential_type "static_token" for HTTP Bearer Auth

### Context

The audit log schema (v25, vnc-014) includes a `credential_type TEXT NOT NULL DEFAULT 'none'` column. Currently, all requests (stdio/UDS) write `"none"` because no credential-based auth exists. HTTP bearer token auth introduces the first real credential type.

The audit log is append-only with DDL triggers -- the schema cannot be modified (Constraint 6). The `credential_type` column accepts arbitrary text values.

Options for the credential type value:
1. `"bearer_token"` -- generic, could apply to JWT or opaque tokens
2. `"static_token"` -- specific to the static 256-bit token mechanism
3. `"http_bearer"` -- transport-specific naming

W2-3 will add JWT/OAuth with a different credential type (likely `"oauth_jwt"`). The naming must distinguish between the two.

### Decision

Use `"static_token"` as the `credential_type` value for HTTP bearer token authentication. This is specific enough to distinguish from future JWT-based auth (`"oauth_jwt"` in W2-3) while being transport-agnostic (the same static token mechanism could theoretically be used over other transports).

The value is a string constant defined in `http/auth.rs`:
```rust
pub(crate) const CREDENTIAL_TYPE_STATIC_TOKEN: &str = "static_token";
```

Audit events from HTTP-authenticated requests set:
- `credential_type`: `"static_token"`
- `agent_attribution`: populated from `clientInfo.name` via MCP initialize handshake (unchanged mechanism)
- `capability_used`: populated from the tool's required capability (unchanged mechanism)

### Consequences

Easier: Audit queries can filter by `credential_type = 'static_token'` to identify HTTP-authenticated requests. The naming is unambiguous and future-proof for W2-3 OAuth addition.

Harder: If a future feature reuses static tokens over a non-HTTP transport, the credential type value is still `"static_token"` (not transport-specific), which is the correct behavior -- the credential mechanism matters for audit, not the transport.
