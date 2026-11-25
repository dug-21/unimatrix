//! Slug guard + `http_access` audit (vnc-050 ADR-002 #5806, C3).
//!
//! ONE enforcement point for a scoped token's `allowed_slugs` on every
//! `/v1/{slug}/...` route. `PathRouter::call` runs [`check`] before the
//! observe/MCP split and before any `resolve_store`, so a denied request never
//! touches the target slug's store. Every guarded request writes exactly one
//! `http_access` audit row (an SSE GET is one request → one row).

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::combinators::BoxBody;

use crate::http::principal::BearerPrincipal;
use crate::infra::audit::{AuditEvent, AuditLog, Outcome};

use super::internal_error_response;
use super::observe::json_error_response;
use super::seam::{ProjectKey, ProjectSlug, StoreResolver, parse_project_key};

/// Audit operation name for per-request bearer access rows.
pub(crate) const HTTP_ACCESS_OPERATION: &str = "http_access";

/// Fixed 403 body: identical whether the target slug exists or not (R-03.3).
pub(crate) const SLUG_DENIED_MESSAGE: &str = "token not permitted for project";

/// Sink for `http_access` rows. Fire-and-forget: MUST NOT block or fail the
/// request (R-16.3).
pub(crate) trait AccessAudit: Send + Sync {
    fn record(&self, slug: &ProjectSlug, event: AuditEvent);
}

/// Production sink: writes to the addressed slug's own audit log.
pub(crate) struct ResolverAccessAudit {
    resolver: Arc<dyn StoreResolver>,
}

impl ResolverAccessAudit {
    pub(crate) fn new(resolver: Arc<dyn StoreResolver>) -> Self {
        ResolverAccessAudit { resolver }
    }
}

impl std::fmt::Debug for ResolverAccessAudit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolverAccessAudit")
            .finish_non_exhaustive()
    }
}

impl AccessAudit for ResolverAccessAudit {
    fn record(&self, slug: &ProjectSlug, event: AuditEvent) {
        // Unknown slug: nothing to write to; the downstream 404 is unchanged.
        let Ok(store) = self.resolver.resolve_store(&ProjectKey::Slug(slug.clone())) else {
            return;
        };
        // No runtime (should not happen inside the listener): drop, never panic.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("http_access audit skipped: no tokio runtime");
            return;
        };
        handle.spawn(async move {
            if let Err(e) = AuditLog::new(store).log_event_async(event).await {
                // Never log the token, its hash, or any header.
                tracing::warn!(error = %e, "http_access audit write failed");
            }
        });
    }
}

/// Build the `http_access` row. Carries the label only — never the secret,
/// its hash, or any header.
fn access_event(p: &BearerPrincipal, method: &str, route: &str, outcome: Outcome) -> AuditEvent {
    AuditEvent {
        operation: HTTP_ACCESS_OPERATION.to_string(),
        agent_id: p.audit_agent_id(),
        credential_type: p.credential_type().to_string(),
        outcome,
        metadata: serde_json::json!({
            "token_label": p.audit_label(),
            "method": method,
            "route": route,
        })
        .to_string(),
        ..AuditEvent::default()
    }
}

/// Enforce `allowed_slugs` and record one `http_access` row.
///
/// - Non-slug or invalid-slug paths fall through (`Ok`) so the existing
///   400/404 handling downstream is unchanged.
/// - Missing `BearerPrincipal` on a slug path → 500 (fail closed).
/// - Scoped principal on a slug outside `allowed_slugs` → 403 with a fixed
///   body; the denial row goes to the principal's BOUND slug log, never the
///   target's (R-16.2).
// rationale: Err is the ready HTTP response returned as-is; boxing adds nothing.
#[allow(clippy::result_large_err)]
pub(crate) fn check<B>(
    req: &Request<B>,
    audit_sink: &dyn AccessAudit,
) -> Result<(), Response<BoxBody<Bytes, Infallible>>> {
    let path = req.uri().path();
    let key = match parse_project_key(path) {
        Ok(ProjectKey::Slug(s)) => s,
        Err(_) => return Ok(()),
    };
    let Some(principal) = req.extensions().get::<BearerPrincipal>() else {
        tracing::error!("slug_guard: BearerPrincipal missing from extensions");
        return Err(internal_error_response());
    };
    let route = if path.ends_with("/observe") {
        "observe"
    } else {
        "mcp"
    };
    let method = req.method().as_str();

    if !principal.permits_slug(&key) {
        if let Some(home) = principal.home_slug() {
            audit_sink.record(
                home,
                access_event(principal, method, route, Outcome::Denied),
            );
        }
        return Err(json_error_response(
            StatusCode::FORBIDDEN,
            SLUG_DENIED_MESSAGE,
        ));
    }

    audit_sink.record(
        &key,
        access_event(principal, method, route, Outcome::Success),
    );
    Ok(())
}

#[cfg(test)]
#[path = "slug_guard_tests.rs"]
mod tests;
