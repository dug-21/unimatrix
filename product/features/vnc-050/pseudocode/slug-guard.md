# C3 Slug Guard + Access Audit — `http/router/slug_guard.rs` (new), `http/router.rs` (1 call)

ADR-002 (#5806). Depends on C4 (`BearerPrincipal`), C2 (inserts it). Target < 250 lines + tests in `http/router/slug_guard_tests.rs`.

## Purpose
One enforcement point for `allowed_slugs` on every `/v1/{slug}/...` route, before the observe/MCP split and before `resolve_store`; one `http_access` audit row per request.

## Interface Gap (flagged)
Architecture names `&dyn AccessAudit` but does not define it. Defined here:
```
pub(crate) trait AccessAudit: Send + Sync {
    /// Fire-and-forget; MUST NOT block or fail the request (R-16.3).
    fn record(&self, slug: &ProjectSlug, event: AuditEvent);
}

pub(crate) struct ResolverAccessAudit { resolver: Arc<dyn StoreResolver> }
impl AccessAudit for ResolverAccessAudit {
    fn record(&self, slug, event) {
        match self.resolver.resolve_store(&ProjectKey::Slug(slug.clone())):
            Err(_) -> return            // unknown slug: nothing to write; never 404-leak
            Ok(store) -> tokio::spawn(async move {
                if let Err(e) = AuditLog::new(<sqlx handle of store>).log_event_async(event).await {
                    tracing::warn!(error=%e, "http_access audit write failed")   // no token/header
                }
            })
    }
}
```
Implementer constructs `AuditLog` from the resolved store exactly as the per-slug `UnimatrixServer` does (same handle type).

`PathRouter` gains field `access_audit: Arc<dyn AccessAudit>`; `PathRouter::new(resolver, observe_ctx)` builds `ResolverAccessAudit{ resolver: Arc::clone(&resolver) }` internally (signature unchanged → main.rs untouched).

## `pub(crate) fn check<B>(req: &Request<B>, audit_sink: &dyn AccessAudit) -> Result<(), Response<BoxBody<Bytes, Infallible>>>`
```
path = req.uri().path()
key = match seam::parse_project_key(path):
    Ok(ProjectKey::Slug(s)) -> s
    Ok(_) | Err(_) -> return Ok(())          // fall through: existing 400/404 handling downstream unchanged
principal = match req.extensions().get::<BearerPrincipal>():
    Some(p) -> p
    None -> { tracing::error!("slug_guard: BearerPrincipal missing"); return Err(internal_error_response()) }  // 500, fail closed
route = if path.ends_with("/observe") { "observe" } else { "mcp" }
method = req.method().as_str()

if !principal.permits_slug(&key):
    // audit to the BOUND slug's log, never the target (R-16.2)
    if let Some(home) = principal.home_slug():
        audit_sink.record(home, access_event(principal, method, route, Outcome::Denied))
    return Err(json_error_response(StatusCode::FORBIDDEN, "token not permitted for project"))
           // body: {"error":"token not permitted for project"} — identical whether B exists or not

audit_sink.record(&key, access_event(principal, method, route, Outcome::Success))
Ok(())
```
Note: `parse_project_key` validates the slug grammar, so `%42`, `//B`, uppercase are `InvalidSlug` → fall through → existing 400 (never 200). `/v1/tools/...` → slug `tools` → guarded like any slug.

### `fn access_event(p: &BearerPrincipal, method: &str, route: &str, outcome: Outcome) -> AuditEvent`
```
AuditEvent {
    operation: "http_access",
    agent_id: p.audit_agent_id(),                 // "token:<label>" | "http-bearer"
    credential_type: p.credential_type(),         // "scoped_token" | "static_token"
    outcome,
    metadata: serde_json::json!({"token_label": p.audit_label(), "method": method, "route": route}).to_string(),
    detail: "", target_ids: [], session_id: "", capability_used: "", agent_attribution: "",
    ..AuditEvent::default()                       // event_id/timestamp assigned by log_event
}
```
Never the secret, hash, or any header.

## `PathRouter::call` change (`http/router.rs`)
```
fn call(&mut self, request) -> Future {
    path/method as today
    if !(method == GET && path == "/health"):
        if let Err(resp) = slug_guard::check(&request, &*self.access_audit) {
            return Box::pin(async move { Ok(resp) })
        }
    match (...) existing arms unchanged
}
```
`/health` exemption matches `AUTH_BYPASS_PATHS` (exact match). One row per HTTP request (an SSE GET is one request → one row).

## Error Handling
403 generic body; 500 on missing principal; audit failure logged only.

## Key Test Scenarios (test seams: OVERVIEW §Test-Only Seams)
`RESOLVE_SPY` (project_resolver.rs) proves the guard precedes `resolve_store`; `StripPrincipalLayer` drives the missing-principal 500.
- Matrix (real HTTP through auth layer + PathRouter): {MCP POST, SSE GET, DELETE session, observe POST} × {legacy, read@A, rw@A} × {A, B}: scoped on B → 403; legacy passes everywhere (AC-09, R-03.1).
- Path variants `/v1/B/`, `/v1/B`, `/v1/%42/x`, `/v1//B`, `/v1/tools/x`, `/v1/b` vs `/v1/B` → 403 or existing 400/404, never 200.
- 403 body identical for registered and unregistered B (R-03.3).
- Denied row in A's log, B's log unchanged (R-16.2); every bearer call (legacy, scoped, observe) writes one row with `token_label` (AC-11); failing sink does not fail request (R-16.3).
- Request reaching guard without principal (test layer omits it) → 500.
- Slug provisioned after boot: guard applies identically.
