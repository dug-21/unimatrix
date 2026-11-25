## ADR-002: Single Post-Parse Slug Guard in PathRouter, Plus Per-Request Bearer Access Audit

### Context
Auth runs as a tower layer before routing (`StaticTokenAuthLayer`), so it does not know the slug. `PathRouter::call` (`http/router.rs`) splits into `route_observe` (`POST /v1/{slug}/observe`) and `SlugRouter::route_mcp` (everything else under `/v1/`). Both call `parse_project_key`. SR-10: if some route skips the slug check, a token bound to A can reach B. AC-09 requires 403 on every `/v1/B/...` route. AC-11 requires a token label in the audit for every bearer call, including observe and legacy-token calls.

### Decision
- The auth layer inserts a `BearerPrincipal` into the request extensions next to the existing `ResolvedIdentity`:
  `enum BearerPrincipal { Legacy, Scoped { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug> } }`.
- `PathRouter::call` runs **one** guard, `slug_guard::check(&request) -> Result<(), Response>`, before the observe/MCP split. It covers every method and path except the auth-bypassed `GET /health`. The guard:
  1. Calls `parse_project_key(path)`, the same grammar the routes use. If parsing fails it falls through, and the existing 400/404 handling still applies. Because there is no default slug (vnc-038 ADR-004, #5083), `/v1/tools/...` resolves to slug `tools` and goes through the guard like any other slug.
  2. For `Scoped` principals, returns 403 `{"error":"token not permitted for project"}` when the slug is not in `allowed_slugs`. This happens before `resolve_store`, so a 403 does not reveal whether the slug exists.
  3. A missing `BearerPrincipal` is a 500 (fail closed). It cannot happen when the auth layer is installed.
- **Bearer access audit**: after a request passes the guard and resolves its slug, the guard emits one fire-and-forget `AuditEvent` to that slug's audit log: `operation="http_access"`, `credential_type="static_token"|"scoped_token"`, `agent_id="token:<label>"` (or `"http-bearer"` for legacy), and `metadata={"token_label":"<label>|legacy","method":..,"route":"mcp|observe"}`. This uses the existing `credential_type` and `metadata` columns. No schema change. A 403 denial is audited the same way with `outcome=Denied`, to the bound slug's log, never to the target slug's log.

### Consequences
- One enforcement point. The test is a table-driven matrix of {every route, including MCP POST, SSE GET, DELETE session, observe} x {legacy, read@A, rw@A} x {slug A, slug B}.
- The audit gets one extra row per HTTP request. The label is always recorded, the secret never is.
- New `/v1/` routes are guarded automatically. A route outside `/v1/` other than `/health` fails `parse_project_key` and is never served.
