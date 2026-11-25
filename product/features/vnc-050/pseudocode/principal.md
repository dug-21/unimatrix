# C4 Principal Mapping — `http/principal.rs` (new)

ADR-003 (#5819). Target < 150 lines + tests. No dependencies; W0 first file.

## Purpose
Single home of the token-mode → capability table and of `BearerPrincipal` → auth-layer `ResolvedIdentity`. Used by C1 (mode type), C2 (identity), C3 (slug binding, audit label), C4b (ceiling), C6 (via `ResolvedIdentity.capabilities`).

## Types and Functions

```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenMode { Read, ReadWrite }

impl TokenMode {
    pub(crate) fn capabilities(self) -> Vec<Capability> {
        match self {
            Read      => vec![Capability::Read, Capability::Search],
            ReadWrite => vec![Capability::Read, Capability::Write, Capability::Search, Capability::SessionWrite],
        }   // never Admin
    }
    pub(crate) fn as_wire(self) -> &'static str { Read => "read", ReadWrite => "read-write" }
    pub(crate) fn parse_wire(s: &str) -> Option<TokenMode>   // "read" | "read-write"; used by C7 clap value parser
}

#[derive(Clone, Debug)]
pub(crate) enum BearerPrincipal {
    Legacy,
    Scoped { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug> },
}

impl BearerPrincipal {
    pub(crate) fn audit_label(&self) -> &str     { Legacy => "legacy", Scoped{label,..} => label }
    pub(crate) fn credential_type(&self) -> &'static str { Legacy => CREDENTIAL_TYPE_STATIC_TOKEN ("static_token"), Scoped => "scoped_token" }
    pub(crate) fn audit_agent_id(&self) -> String { Legacy => "http-bearer", Scoped{label,..} => format!("{AUTH_ID_PREFIX}{label}") }
    pub(crate) fn permits_slug(&self, slug: &ProjectSlug) -> bool { Legacy => true, Scoped{allowed_slugs,..} => allowed_slugs.contains(slug) }
    /// Slug whose audit log receives this principal's denial rows (ADR-002): first bound slug; None for Legacy.
    pub(crate) fn home_slug(&self) -> Option<&ProjectSlug> { Legacy => None, Scoped{allowed_slugs,..} => allowed_slugs.first() }
}

pub(crate) const AUTH_ID_PREFIX: &str = "token:";
pub(crate) const CREDENTIAL_TYPE_SCOPED_TOKEN: &str = "scoped_token";

pub(crate) fn auth_identity(p: &BearerPrincipal) -> ResolvedIdentity {
    match p {
        Legacy => ResolvedIdentity { agent_id: "http-bearer", trust_level: Restricted,
                                     capabilities: TokenMode::ReadWrite.capabilities() },   // identical to today's StaticTokenValidator
        Scoped{label, mode, ..} => ResolvedIdentity { agent_id: format!("token:{label}"), trust_level: Restricted,
                                                      capabilities: mode.capabilities() },
    }
}
```
`ResolvedIdentity.agent_id = "token:<label>"` is auth-layer/audit identity only (HD 7.1). It never reaches `build_context_with_external_identity` (external_identity stays `None`, server.rs:541 dormant) and is never enrolled.

## Reserved `token:` Namespace (R-13, AC-08)
```
pub(crate) fn reject_reserved_agent_id(agent_id: &Option<String>) -> Result<(), ServerError> {
    if let Some(id) = agent_id
       && id.trim().to_ascii_lowercase().starts_with(AUTH_ID_PREFIX):
        return Err(ServerError::InvalidInput{ field: "agent_id", reason: "the token: prefix is reserved" })
    Ok(())
}
```
Call site: first statement of `build_context_with_external_identity` (server.rs, +1 line), before `resolve_agent` — covers stdio and HTTP (both transports build `ToolContext` there).
Gap flagged: UDS hook dispatch does not use `build_context_with_external_identity`; if any UDS path calls `resolve_or_enroll` with a caller-supplied agent_id, the same guard must be called there. Implementer must grep `resolve_identity(`/`resolve_or_enroll(` callers and add the guard at each external-input site (no change to `bridge.rs`).

## Error Handling
Pure functions; only `reject_reserved_agent_id` fails (InvalidInput → MCP invalid params).

## Key Test Scenarios
- `capabilities`: Read = {Read, Search}; ReadWrite = 4 caps; neither contains Admin.
- `auth_identity(Legacy)` equals the pre-vnc-050 `StaticTokenValidator` identity field-for-field.
- `permits_slug`: Legacy always; Scoped only bound slugs.
- `agent_id:"token:ci"`, `"TOKEN:ci"`, `" token:ci"` → InvalidInput over stdio, HTTP-legacy, HTTP-scoped; agents table has no `token:` row after a scoped session (R-13.2).
