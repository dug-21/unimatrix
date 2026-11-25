//! Bearer principal mapping (vnc-050 C4, ADR-003).
//!
//! Single home of the token-mode -> capability table and of the
//! `BearerPrincipal` -> auth-layer `ResolvedIdentity` mapping. The auth layer
//! (C2), slug guard (C3), capability ceiling (C4b) and observe gate (C6) all
//! read capabilities from `TokenMode::capabilities` — one source.

use crate::http::auth::CREDENTIAL_TYPE_STATIC_TOKEN;
use crate::http::router::ProjectSlug;
use crate::infra::registry::{Capability, TrustLevel};
use crate::mcp::identity::ResolvedIdentity;

/// Reserved agent-id prefix for scoped-token auth identities (R-13).
pub(crate) const AUTH_ID_PREFIX: &str = "token:";

/// Credential type written to audit rows for scoped-token callers.
// rationale: consumed by the C3 access-audit path (vnc-050 Wave 1).
#[allow(dead_code)]
pub(crate) const CREDENTIAL_TYPE_SCOPED_TOKEN: &str = "scoped_token";

/// Agent id of the legacy static bearer token (unchanged from vnc-021).
const LEGACY_AGENT_ID: &str = "http-bearer";

/// Scoped-token mode. Never grants `Admin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenMode {
    Read,
    ReadWrite,
}

impl TokenMode {
    /// The ONLY mode -> capability table (ADR-003).
    pub(crate) fn capabilities(self) -> Vec<Capability> {
        match self {
            TokenMode::Read => vec![Capability::Read, Capability::Search],
            TokenMode::ReadWrite => vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::SessionWrite,
            ],
        }
    }

    /// Wire form used in `tokens.json` and CLI args.
    // rationale: consumed by the C1 writers / C7 CLI (vnc-050 Wave 1-2).
    #[allow(dead_code)]
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            TokenMode::Read => "read",
            TokenMode::ReadWrite => "read-write",
        }
    }

    /// Parse the wire form; `None` for anything else.
    pub(crate) fn parse_wire(s: &str) -> Option<TokenMode> {
        match s {
            "read" => Some(TokenMode::Read),
            "read-write" => Some(TokenMode::ReadWrite),
            _ => None,
        }
    }
}

/// Authenticated bearer principal, inserted into request extensions by the
/// auth layer alongside `ResolvedIdentity`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BearerPrincipal {
    /// The legacy `{data_dir}/token` static token.
    Legacy,
    /// A `tokens.json` scoped token.
    Scoped {
        label: String,
        mode: TokenMode,
        allowed_slugs: Vec<ProjectSlug>,
    },
}

// rationale: the methods below are consumed by C3 slug guard / access audit
// (vnc-050 Wave 1); defined here so the mapping has one home.
#[allow(dead_code)]
impl BearerPrincipal {
    /// Label written to `http_access` audit rows.
    pub(crate) fn audit_label(&self) -> &str {
        match self {
            BearerPrincipal::Legacy => "legacy",
            BearerPrincipal::Scoped { label, .. } => label,
        }
    }

    /// Audit credential type.
    pub(crate) fn credential_type(&self) -> &'static str {
        match self {
            BearerPrincipal::Legacy => CREDENTIAL_TYPE_STATIC_TOKEN,
            BearerPrincipal::Scoped { .. } => CREDENTIAL_TYPE_SCOPED_TOKEN,
        }
    }

    /// Auth-layer agent id (audit only; never enrolled).
    pub(crate) fn audit_agent_id(&self) -> String {
        match self {
            BearerPrincipal::Legacy => LEGACY_AGENT_ID.to_string(),
            BearerPrincipal::Scoped { label, .. } => format!("{AUTH_ID_PREFIX}{label}"),
        }
    }

    /// Whether this principal may address `slug`. Legacy: every slug.
    pub(crate) fn permits_slug(&self, slug: &ProjectSlug) -> bool {
        match self {
            BearerPrincipal::Legacy => true,
            BearerPrincipal::Scoped { allowed_slugs, .. } => allowed_slugs.contains(slug),
        }
    }

    /// Slug whose audit log receives this principal's denial rows (ADR-002).
    pub(crate) fn home_slug(&self) -> Option<&ProjectSlug> {
        match self {
            BearerPrincipal::Legacy => None,
            BearerPrincipal::Scoped { allowed_slugs, .. } => allowed_slugs.first(),
        }
    }
}

/// Map a principal to the auth-layer `ResolvedIdentity`.
///
/// Legacy is field-for-field identical to the pre-vnc-050
/// `StaticTokenValidator` output (FR-39).
pub(crate) fn auth_identity(p: &BearerPrincipal) -> ResolvedIdentity {
    match p {
        BearerPrincipal::Legacy => ResolvedIdentity {
            agent_id: LEGACY_AGENT_ID.to_string(),
            trust_level: TrustLevel::Restricted,
            capabilities: TokenMode::ReadWrite.capabilities(),
        },
        BearerPrincipal::Scoped { label, mode, .. } => ResolvedIdentity {
            agent_id: format!("{AUTH_ID_PREFIX}{label}"),
            trust_level: TrustLevel::Restricted,
            capabilities: mode.capabilities(),
        },
    }
}

/// Reject caller-supplied agent ids in the reserved `token:` namespace (R-13).
///
/// Case- and leading-whitespace-insensitive. Called first in
/// `build_context_with_external_identity` (stdio and HTTP).
pub(crate) fn reject_reserved_agent_id(
    agent_id: &Option<String>,
) -> Result<(), crate::error::ServerError> {
    if let Some(id) = agent_id
        && id.trim().to_ascii_lowercase().starts_with(AUTH_ID_PREFIX)
    {
        return Err(crate::error::ServerError::InvalidInput {
            field: "agent_id".to_string(),
            reason: "the token: prefix is reserved".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_token_prefix_rejected() {
        for id in ["token:ci", "TOKEN:ci", " token:ci", "Token:"] {
            assert!(
                reject_reserved_agent_id(&Some(id.to_string())).is_err(),
                "{id}"
            );
        }
        for id in [
            None,
            Some("human".to_string()),
            Some("tok:x".to_string()),
            Some("my-token:x".to_string()),
        ] {
            assert!(reject_reserved_agent_id(&id).is_ok());
        }
    }

    fn slug(s: &str) -> ProjectSlug {
        ProjectSlug::try_from(s).expect("valid slug")
    }

    fn scoped(label: &str, mode: TokenMode) -> BearerPrincipal {
        BearerPrincipal::Scoped {
            label: label.to_string(),
            mode,
            allowed_slugs: vec![slug("alpha")],
        }
    }

    #[test]
    fn read_mode_capabilities() {
        assert_eq!(
            TokenMode::Read.capabilities(),
            vec![Capability::Read, Capability::Search]
        );
    }

    #[test]
    fn read_write_mode_capabilities() {
        let caps = TokenMode::ReadWrite.capabilities();
        assert_eq!(
            caps,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::SessionWrite
            ]
        );
        assert!(!caps.contains(&Capability::Admin));
        assert!(!TokenMode::Read.capabilities().contains(&Capability::Admin));
    }

    #[test]
    fn wire_round_trip() {
        for m in [TokenMode::Read, TokenMode::ReadWrite] {
            assert_eq!(TokenMode::parse_wire(m.as_wire()), Some(m));
        }
        assert_eq!(TokenMode::parse_wire("READ"), None);
        assert_eq!(TokenMode::parse_wire("admin"), None);
    }

    #[test]
    fn auth_identity_scoped_is_token_label() {
        let p = scoped("ci", TokenMode::Read);
        let id = auth_identity(&p);
        assert_eq!(id.agent_id, "token:ci");
        assert_eq!(id.trust_level, TrustLevel::Restricted);
        assert_eq!(id.capabilities, TokenMode::Read.capabilities());
        assert_eq!(p.credential_type(), "scoped_token");
        assert_eq!(p.audit_agent_id(), "token:ci");
        assert_eq!(p.audit_label(), "ci");
    }

    #[test]
    fn auth_identity_legacy() {
        let id = auth_identity(&BearerPrincipal::Legacy);
        assert_eq!(id.agent_id, "http-bearer");
        assert_eq!(id.trust_level, TrustLevel::Restricted);
        assert_eq!(
            id.capabilities,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::SessionWrite
            ]
        );
        assert_eq!(BearerPrincipal::Legacy.credential_type(), "static_token");
        assert_eq!(BearerPrincipal::Legacy.audit_label(), "legacy");
    }

    #[test]
    fn permits_slug_and_home_slug() {
        let p = scoped("ci", TokenMode::ReadWrite);
        assert!(p.permits_slug(&slug("alpha")));
        assert!(!p.permits_slug(&slug("beta")));
        assert_eq!(p.home_slug(), Some(&slug("alpha")));
        assert!(BearerPrincipal::Legacy.permits_slug(&slug("beta")));
        assert_eq!(BearerPrincipal::Legacy.home_slug(), None);
    }
}
