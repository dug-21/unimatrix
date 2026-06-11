# SlugRouter + StoreResolver / ProjectKey / ProjectSlug seam (Wave-1 MINIMAL)

> `crates/unimatrix-server/src/http/router.rs`. Realizes C4 (ADR-003) as the isolation seam, the route grammar (ADR-005), and the `ProjectSlug` allowlist parse edge (ADR-004, SR-09/R-03). FR-X1..X5, R-01 (Critical). **Wave-1 scope: route grammar + trait + SlugRouter layer + parse edge + DefaultResolver wiring. The slug-resolver LOGIC is Wave 2 — this file models the seam only; it does NOT implement the slug resolver.**

## Purpose

Introduce the single store-resolution funnel. A new `SlugRouter` tower layer sits between `PathRouter` and the `McpAdapter`: it parses the request path into a transport-derived `ProjectKey`, calls `resolve_store(&key)` on an injected `StoreResolver`, and threads the resolved `Arc<Store>` into MCP dispatch. In Wave 1 the only resolver is `DefaultResolver` (default-resolver.md); `/v1/{slug}/...` parses but returns `RouteError::UnknownProject`. The trait + call site are built so Wave 2 swaps `DefaultResolver -> ProjectRouter` with no interface re-cut (R-01).

## Locked types + signature (downstream MUST NOT invent)

```rust
/// Transport-derived project identity. Constructible ONLY from the transport
/// (URL path here; daemon path-hash for local UDS). NEVER from a request payload (FR-X2).
pub enum ProjectKey { Default, Slug(ProjectSlug) }

/// Allowlist newtype. TryFrom enforces ^[a-z0-9][a-z0-9-]{0,62}$ at the parse edge,
/// BEFORE any filesystem use (ADR-004, SR-09).
pub struct ProjectSlug(String);

pub trait StoreResolver: Send + Sync + 'static {
    /// THE single funnel. Every read/write in the process resolves here (FR-X1).
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}

pub enum RouteError {
    UnknownProject,          // slug parsed but not registered (Wave 1: any Slug; Wave 2: unknown slug)
    InvalidSlug(String),     // failed the allowlist at the parse edge
}
```

## ProjectSlug — the parse edge (ADR-004 / SR-09 / R-03 — fix-before-merge security)

```
impl TryFrom<&str> for ProjectSlug:
    fn try_from(s) -> Result<ProjectSlug, RouteError>:
        // Allowlist: ^[a-z0-9][a-z0-9-]{0,62}$  (lowercase a-z0-9 + hyphen; start alnum; 1..=63 chars).
        if s.is_empty() or s.len() > 63:                    return Err(InvalidSlug(s))
        if first char not in [a-z0-9]:                      return Err(InvalidSlug(s))
        if any char not in [a-z0-9-]:                       return Err(InvalidSlug(s))
        // Forbidden by construction (cannot pass the charset): '.', '/', '\', '%', whitespace,
        // uppercase, ANY path separator or percent-encoding thereof -> '../', '%2f', absolute paths
        // are UNREPRESENTABLE, not merely rejected. Validation is here, BEFORE any path join.
        return Ok(ProjectSlug(s.to_owned()))
```
Because escape is structurally impossible at this edge, no slug can resolve outside `/data/.unimatrix/{slug}/` (R-03). Reserved words (`tools`, `health`, `observe`, `v1`) are handled by the route grammar position (they occupy fixed path segments), not the slug charset — see parsing below.

## Route grammar (LOCKED, ADR-005)

```
/v1/tools/...            -> ProjectKey::Default        (Wave 1: local UDS + cloud single-project alias)
/v1/{slug}/tools/...     -> ProjectKey::Slug(slug)     (Wave 2 additive; Wave 1 parses but resolver inert)
/health  (GET)           -> existing health bypass     (NOT through this seam)
/observe (POST)          -> existing vnc-022 path       (NOT through this seam)
```

### parse_project_key

```
fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError>:
    segs = path.trim_start_matches('/').split('/')
    match segs:
      ["v1", "tools", ..]           => Ok(ProjectKey::Default)         // default alias
      ["v1", maybe_slug, "tools", ..]:
            // maybe_slug is in the slug position; the literal "tools" default alias was matched above,
            // so any non-"tools" 2nd segment is a candidate slug.
            slug = ProjectSlug::try_from(maybe_slug)?                  // allowlist at the edge (R-03)
            Ok(ProjectKey::Slug(slug))
      _ => // non-/v1 MCP paths keep current behavior; default-route for backward-compat
            Ok(ProjectKey::Default)
```
Note: `/v1/tools/...` is matched as Default BEFORE the slug arm, so the reserved word `tools` in the slug position never becomes a slug. Other reserved words appearing as a slug (`health`,`observe`,`v1`) pass the charset but, in Wave 1, resolve to `UnknownProject` anyway; Wave 2's register CLI is responsible for refusing to register reserved slugs (documented seam constraint, NOT built here).

## SlugRouter — the layer (the single funnel call site)

```
pub struct SlugRouter<ReqBody> {
    resolver: Arc<dyn StoreResolver>,          // injected; Wave 1 = DefaultResolver, Wave 2 = ProjectRouter
    project_router: ProjectRouter<ReqBody>,    // existing MCP dispatch (holds McpAdapter)
}

impl SlugRouter:
    async fn route(&mut self, request) -> Response:
        // Only MCP-bound paths reach here (PathRouter already split off /health and /observe).
        key = match parse_project_key(request.uri().path()):
                  Ok(k)  => k
                  Err(InvalidSlug(s)) => return 404/400 json error "invalid project slug"   (no panic)
        store = match self.resolver.resolve_store(&key):
                  Ok(arc) => arc                       // the SOLE write capability (FR-X3)
                  Err(UnknownProject) => return 404 json error "unknown project"  (R-01 sc.3: never the default store)
                  Err(InvalidSlug(_)) => return 400 json error
        // Thread the resolved store into MCP dispatch. Wave 1: project_router already holds the one
        // McpAdapter built over this same store; resolve_store(Default) returns that store, so the
        // seam is genuinely EXERCISED (A4 / FR-X5) — the store is served THROUGH the funnel, not around it.
        // (Wave-2 per-slug McpAdapter selection lives INSIDE the resolver/seam method — NOT a new edge.)
        return self.project_router.route_mcp(request).await
```

**Per-slug hot-path routing lives INSIDE `resolve_store` (Wave 2), not in a new edge (ADR-003, SR-07).** Wave 1 keeps `SlugRouter` thin: parse -> resolve -> dispatch. The source-assertion gate (R-01) sees ONE funnel method.

## Wiring (main.rs listener, ~L840–900 — ARCHITECTURE §6)

```
store = open_store_with_retry(db_path).await?              // existing
resolver = Arc::new(DefaultResolver::new(store.clone()))  // default-resolver.md
project_router = ProjectRouter::new(server, max_body, allowed_origins)   // existing
slug_router = SlugRouter::new(resolver, project_router)
path_router = PathRouter::new(slug_router_as_mcp_layer, observe_ctx)     // insert SlugRouter at MCP edge
```
SlugRouter is inserted at the point where `PathRouter` currently calls `project_router.route_mcp` — i.e. PathRouter -> SlugRouter -> ProjectRouter -> McpAdapter. The `/health` and `/observe` arms of PathRouter are untouched.

## Wave-2 seam points (MODELED, not implemented here)

- `resolve_store(Slug(s))` logic (slug -> per-slug `Arc<Store>`): Wave 2 `ProjectRouter` impl. **Not in this file.**
- Per-slug hot caches inside the seam method: Wave 2.
- The swap is `Arc<dyn StoreResolver> = ProjectRouter{..}` at the SAME `SlugRouter::new` call site — no change to `SlugRouter`, `parse_project_key`, `ProjectKey`, or `ProjectSlug` (R-01 scenario 2).

## Error handling

- `InvalidSlug` -> 400-class JSON error at the edge; never a path join, never a panic (R-03).
- `UnknownProject` -> 404-class JSON error; NEVER falls back to the default store (R-01 scenario 3).
- No `.unwrap()` in non-test code; resolver errors mapped to JSON responses (reuse `json_error_response` in router.rs).

## Key test scenarios (hints for tester)

- Single-funnel source assertion: Wave-1 store reached ONLY via `resolve_store(ProjectKey::Default)`; zero bypass call sites obtain `Arc<Store>` another way (AC-W1-X1, R-01 sc.1).
- Resolver swap: replacing `DefaultResolver` with a stub `ProjectRouter` needs NO change to `SlugRouter`/route grammar (R-01 sc.2).
- `ProjectKey::Slug(_)` under `DefaultResolver` -> `UnknownProject`, never panic, never default store (R-01 sc.3).
- `ProjectSlug::try_from` rejects `../`, `%2f`, `%2e`, absolute paths, `.`/`/`, >63 chars, empty, uppercase (R-03, AC-W2-R6) — corpus all rejected pre-filesystem.
- Route grammar: `/v1/tools/...` -> Default; `/v1/myproj/tools/...` -> Slug("myproj"); reserved `tools` never a slug.
- Transport-derived identity: request types carry NO project-naming payload field (FR-X2, AC-W1-X3, R-06).
- Per-slug routing resides inside the seam method, not a new edge (R-01 sc.4 — source assertion).
```
