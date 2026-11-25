//! Token-mode capability ceiling (vnc-050 C4b, ADR-003).
//!
//! Effective capabilities = registry grants ∩ ceiling. The ceiling is carried
//! from the auth layer's `BearerPrincipal` via `http::request::Parts`
//! extensions that rmcp injects into every MCP request — never re-derived.

use rmcp::RoleServer;
use rmcp::service::RequestContext;

use crate::http::principal::BearerPrincipal;
use crate::infra::registry::Capability;

/// Capability ceiling for one MCP request. Empty = deny all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapCeiling(pub(crate) Vec<Capability>);

/// Test probe: one record per `ceiling_from` call (R-02 step 0).
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProbeRecord {
    NoParts,
    PartsNoPrincipal,
    Principal(BearerPrincipal),
}

#[cfg(test)]
pub(crate) static PROBE: std::sync::Mutex<Vec<ProbeRecord>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn probe(r: ProbeRecord) {
    PROBE.lock().unwrap_or_else(|p| p.into_inner()).push(r);
}

/// Derive the ceiling from the request context.
///
/// - No `Parts` (stdio/UDS) → `None` (no ceiling).
/// - `Parts` without `BearerPrincipal` → empty ceiling (auth-bypass guard).
/// - Legacy → `None`; Scoped → `mode.capabilities()` (single C4 table).
pub(crate) fn ceiling_from(rc: &RequestContext<RoleServer>) -> Option<CapCeiling> {
    let Some(parts) = rc.extensions.get::<http::request::Parts>() else {
        #[cfg(test)]
        probe(ProbeRecord::NoParts);
        return None;
    };
    principal_ceiling(parts.extensions.get::<BearerPrincipal>())
}

fn principal_ceiling(p: Option<&BearerPrincipal>) -> Option<CapCeiling> {
    #[cfg(test)]
    probe(match p {
        None => ProbeRecord::PartsNoPrincipal,
        Some(p) => ProbeRecord::Principal(p.clone()),
    });
    match p {
        None => Some(CapCeiling(Vec::new())),
        Some(BearerPrincipal::Legacy) => None,
        Some(BearerPrincipal::Scoped { mode, .. }) => Some(CapCeiling(mode.capabilities())),
    }
}

/// Whether the ceiling permits `cap`. `None` → always.
pub(crate) fn permits(c: Option<&CapCeiling>, cap: Capability) -> bool {
    match c {
        None => true,
        Some(CapCeiling(v)) => v.contains(&cap),
    }
}

/// Whether learning signals may be recorded (ADR-004): requires Write.
pub(crate) fn records_learning(c: Option<&CapCeiling>) -> bool {
    permits(c, Capability::Write)
}

#[cfg(test)]
#[path = "cap_ceiling_http_tests.rs"]
mod http_tests;

#[cfg(test)]
#[path = "cap_ceiling_deny_tests.rs"]
mod deny_tests;

#[cfg(test)]
#[path = "cap_ceiling_learning_tests.rs"]
mod learning_tests;

#[cfg(test)]
#[path = "legacy_token_http_tests.rs"]
mod legacy_http_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::principal::TokenMode;
    use crate::http::router::ProjectSlug;

    fn scoped(mode: TokenMode) -> BearerPrincipal {
        BearerPrincipal::Scoped {
            label: "unit".into(),
            mode,
            allowed_slugs: vec![ProjectSlug::try_from("alpha").unwrap()],
        }
    }

    #[test]
    fn legacy_principal_has_no_ceiling() {
        assert_eq!(principal_ceiling(Some(&BearerPrincipal::Legacy)), None);
    }

    #[test]
    fn ceiling_from_empty_when_parts_without_principal() {
        assert_eq!(principal_ceiling(None), Some(CapCeiling(vec![])));
    }

    #[test]
    fn scoped_ceiling_uses_mode_table() {
        for m in [TokenMode::Read, TokenMode::ReadWrite] {
            assert_eq!(
                principal_ceiling(Some(&scoped(m))),
                Some(CapCeiling(m.capabilities()))
            );
        }
    }

    #[test]
    fn observe_and_tool_ceiling_agree_per_mode() {
        for m in [TokenMode::Read, TokenMode::ReadWrite] {
            let p = scoped(m);
            let id = crate::http::principal::auth_identity(&p);
            assert_eq!(
                principal_ceiling(Some(&p)),
                Some(CapCeiling(id.capabilities))
            );
        }
    }

    #[test]
    fn permits_matrix() {
        let all = [
            Capability::Read,
            Capability::Write,
            Capability::Search,
            Capability::Admin,
            Capability::SessionWrite,
        ];
        for c in all {
            assert!(permits(None, c));
        }
        let read = CapCeiling(TokenMode::Read.capabilities());
        assert!(permits(Some(&read), Capability::Read));
        assert!(permits(Some(&read), Capability::Search));
        for c in [
            Capability::Write,
            Capability::Admin,
            Capability::SessionWrite,
        ] {
            assert!(!permits(Some(&read), c));
        }
        let rw = CapCeiling(TokenMode::ReadWrite.capabilities());
        assert!(!permits(Some(&rw), Capability::Admin));
    }

    #[test]
    fn records_learning_matrix() {
        assert!(records_learning(None));
        assert!(!records_learning(Some(&CapCeiling(
            TokenMode::Read.capabilities()
        ))));
        assert!(records_learning(Some(&CapCeiling(
            TokenMode::ReadWrite.capabilities()
        ))));
        assert!(!records_learning(Some(&CapCeiling(vec![]))));
    }
}
